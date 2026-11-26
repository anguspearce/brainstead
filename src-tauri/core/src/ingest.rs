// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The staged ingest (§7): the code does the plumbing and the model only drafts. Brainstead reads
//! the source (its text by page, names corrected), finds the wiki pages it mentions, and asks the
//! model for structured changes (`workflows/ingest.md`): per page, a section's new text and the
//! claims behind it, each with a quote. Here the answer is checked: a page goes in the wiki
//! (`wiki/<folder>/<name>.md`, nothing outside it), a page change none of whose quotes is in the
//! source (on its page, for a PDF) is dropped, and one with some quotes missing keeps them as
//! warnings; a journal note gets no summary page. An image's text is only what OCR read, so its
//! changes are kept whether their quotes are found or not, each with a warning to check it against
//! the picture: they're flagged, and held when a scheduled run made them. What survives becomes one change per page,
//! citing the source in the text and in the page's `sources:`. Brainstead makes them as agent
//! changes (src-tauri/src/changes.rs, decided 2026-10-05), flagged by their warnings.

use serde::{Deserialize, Serialize};

use crate::extract::Extracted;
use crate::proposals::{self, Kind, Patch, Quote};

pub const WORKFLOW: &str = include_str!("../workflows/ingest.md");
/// Most of the source the model is given.
const SOURCE_CHARS: usize = 150_000;
/// What the pages it mentions share between them, and the least and most each gets
/// (`pageview::view`: a bigger page is shown as an outline with its key and newest sections).
const PAGES_CHARS: usize = 120_000;
const PAGE_MIN: usize = 6_000;
const PAGE_MAX: usize = 24_000;

/// How much of each page the model is shown, when it's given `n` of them. `plan` checks against
/// the same view, so it must be given the same.
pub fn page_budget(n: usize) -> usize {
    (PAGES_CHARS / n.max(1)).clamp(PAGE_MIN, PAGE_MAX)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub attribute: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub as_of: Option<String>,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SummaryAnswer {
    pub page: String,
    #[serde(default)]
    pub description: Option<String>,
    pub content: String,
}

/// What the source adds to a page's Timeline: the app files it by date, one entry per source.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EntryAnswer {
    /// YYYY-MM-DD, or YYYY-MM when only the month is known.
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
}

/// A topical section, rewritten whole; or, in answers before the page shape, a section's name
/// with its text in `content`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SectionAnswer {
    Topic { heading: String, content: String },
    Name(String),
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PageAnswer {
    pub page: String,
    #[serde(default)]
    pub new: bool,
    #[serde(default)]
    pub title: Option<String>,
    /// The whole Current state, when the source changes what's true now.
    #[serde(default)]
    pub current_state: Option<String>,
    #[serde(default)]
    pub entry: Option<EntryAnswer>,
    #[serde(default)]
    pub section: Option<SectionAnswer>,
    /// With `section` as a name: that section's text (answers before the page shape).
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub claims: Vec<Claim>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Answer {
    #[serde(default)]
    pub summary: Option<SummaryAnswer>,
    #[serde(default)]
    pub pages: Vec<PageAnswer>,
    /// Actions the source makes or changes, for the user's initiatives (D-20261118-09).
    #[serde(default)]
    pub actions: Vec<ActionAnswer>,
    #[serde(default)]
    pub decisions: Vec<DecisionAnswer>,
}

/// An action the source makes, or one already on an initiative it changes (by `id`): merged, done,
/// or handed on. Strings rather than enums, so one odd value drops that action, not the answer.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActionAnswer {
    /// The initiative's path or name; none for an action that fits no initiative (Unfiled).
    #[serde(default)]
    pub initiative: Option<String>,
    /// The id of an action already listed, when it's that one.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    /// me, delegated or other.
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub person: Option<String>,
    /// open, done or dropped.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DecisionAnswer {
    #[serde(default)]
    pub initiative: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub anchor: Option<String>,
}

/// What an ingest is told about the user's initiatives: those not done, Unfiled among them, and
/// who the user is, so it can tell their actions from others'.
#[derive(Debug, Clone, Default)]
pub struct Tracking {
    pub initiatives: Vec<crate::initiatives::Initiative>,
    /// The user's name, as Settings › General has it.
    pub me: Option<String>,
}

impl Tracking {
    /// The vault's initiatives that aren't done.
    pub fn read(root: &std::path::Path, me: Option<String>) -> Tracking {
        use crate::initiatives as ini;
        let mut initiatives: Vec<ini::Initiative> = std::fs::read_dir(root.join(ini::FOLDER))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let rel = format!("{}/{}", ini::FOLDER, e.file_name().to_string_lossy());
                let text = std::fs::read_to_string(e.path()).ok()?;
                ini::is_initiative(&rel).then(|| ini::parse(&rel, &text))
            })
            .filter(|i| i.status != "done")
            .collect();
        initiatives.sort_by_key(|i| (i.path == ini::UNFILED, i.name.to_lowercase()));
        Tracking { initiatives, me }
    }

    fn section(&self) -> String {
        use crate::initiatives::{Owner, Status};
        let me = self.me.as_deref().map(|m| format!("The user is {m}. ")).unwrap_or_default();
        let mut out = format!(
            "\n# The user's initiatives\n\n{me}These are the things the user is tracking, with their open actions (each with its id) and decisions. {} is where actions that fit no initiative go.\n\n",
            crate::initiatives::UNFILED
        );
        if self.initiatives.is_empty() {
            out.push_str("None yet: every action goes to Unfiled.\n");
        }
        for i in &self.initiatives {
            let about = if i.description.is_empty() { String::new() } else { format!(": {}", i.description) };
            out.push_str(&format!("## {} — {} ({}){about}\n\n", i.path, i.name, i.status));
            let open: Vec<_> = i.actions.iter().filter(|a| a.status == Status::Open && !a.id.is_empty()).collect();
            for a in &open {
                let who = match (a.owner, a.person.as_deref()) {
                    (Some(Owner::Me), _) => "the user's".to_string(),
                    (Some(Owner::Delegated), Some(p)) => format!("delegated by the user to {p}"),
                    (Some(Owner::Other), Some(p)) => format!("{p}'s"),
                    _ => "no owner".to_string(),
                };
                let due = a.due.as_deref().map(|d| format!(", due {d}")).unwrap_or_default();
                out.push_str(&format!("- [{}] {} ({who}{due})\n", a.id, a.text));
            }
            if open.is_empty() {
                out.push_str("No open actions.\n");
            }
            for d in i.decisions.iter().take(10) {
                let when = d.date.as_deref().map(|d| format!("{d}: ")).unwrap_or_default();
                out.push_str(&format!("- Decided {when}{}\n", d.text));
            }
            out.push('\n');
        }
        out
    }
}

/// The day a source is from, by its name: a `YYYY-MM-DD` in it (a note's date, a capture's), or a
/// day written out ("Week ending 2 October 2026"). Ingests wait in this order, oldest first, so a
/// newer source has the last word on a page (D-20261006-05).
pub fn source_day(name: &str) -> Option<String> {
    // Exactly dddd-dd-dd, not inside a longer number (bytes, so a name with "–" in it is fine).
    let b = name.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    for i in 0..b.len().saturating_sub(9) {
        let shape = (0..10).all(|k| if k == 4 || k == 7 { b[i + k] == b'-' } else { digit(i + k) });
        if shape && (i == 0 || !digit(i - 1)) && !digit(i + 10) {
            let w = std::str::from_utf8(&b[i..i + 10]).unwrap_or_default();
            if chrono::NaiveDate::parse_from_str(w, "%Y-%m-%d").is_ok() {
                return Some(w.to_string());
            }
        }
    }
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    for k in 0..words.len().saturating_sub(2) {
        let t = format!("{} {} {}", words[k], words[k + 1], words[k + 2]);
        for f in ["%d %B %Y", "%d %b %Y"] {
            if let Ok(d) = chrono::NaiveDate::parse_from_str(&t, f) {
                return Some(d.to_string());
            }
        }
    }
    None
}

/// The JSON in a model's answer: inside a code fence or not, from its first `{` to its last `}`.
pub fn parse_answer(text: &str) -> Result<Answer, String> {
    let t = text.trim();
    let (a, b) = (t.find('{'), t.rfind('}'));
    let json = match (a, b) {
        (Some(a), Some(b)) if b > a => &t[a..=b],
        _ => return Err("The answer had no JSON in it.".into()),
    };
    serde_json::from_str(json).map_err(|e| format!("The answer's JSON doesn't fit the ingest schema: {e}"))
}

/// The source as the ingest reads it.
pub struct Source {
    /// Vault-relative.
    pub rel: String,
    pub text: SourceText,
}

pub enum SourceText {
    /// A markdown or text file, names corrected.
    Text(String),
    /// A PDF or Office file by its parts.
    Parts(Extracted),
    /// An image, by the text OCR read from it (perhaps none).
    Image(Extracted),
}

/// The warning every change from an image carries, so it waits for review.
pub const IMAGE_WARNING: &str = "From an image: check it against the picture.";

/// The warning on a change to a page whose properties couldn't take the source in `sources`.
pub const SOURCES_WARNING: &str = "The page's properties don't read as YAML, so this source wasn't added to its sources.";

/// The page with the source cited in its `sources`, or as it is with a warning when its
/// properties can't take it.
fn with_source(text: String, cite: &str, warnings: &mut Vec<String>) -> String {
    use crate::fixname::ListValue;
    match crate::fixname::list_value_added(&text, &["sources"], cite) {
        ListValue::Changed(t) => t,
        ListValue::Already => text,
        ListValue::CannotEdit => {
            if !warnings.iter().any(|w| w == SOURCES_WARNING) {
                warnings.push(SOURCES_WARNING.into());
            }
            text
        }
    }
}

impl Source {
    /// How a page links it: by name without `.md`, with its extension otherwise.
    pub fn link(&self) -> String {
        let name = crate::lint::name_of(&self.rel);
        name.strip_suffix(".md").unwrap_or(name).to_string()
    }
    /// A `Meeting.` or `1-1.` note at the top of the vault: its own summary already.
    pub fn is_journal(&self) -> bool {
        !self.rel.contains('/') && (self.rel.starts_with("Meeting. ") || self.rel.starts_with("1-1. "))
    }
    /// A day of a Teams or Slack chat (`Teams. Chat.`, `Slack. Chat.`): short messages whose news
    /// goes on the pages they mention and whose actions on initiatives, so it gets no summary page of
    /// its own (D-20261118-21).
    pub fn is_chat_day(&self) -> bool {
        let name = crate::lint::name_of(&self.rel);
        name.starts_with("Teams. Chat. ") || name.starts_with("Slack. Chat. ")
    }
    fn body(&self) -> String {
        match &self.text {
            SourceText::Text(t) => t.clone(),
            SourceText::Parts(x) => x
                .parts
                .iter()
                .map(|p| if p.label.is_empty() { p.text.clone() } else { format!("## {}\n\n{}", p.label, p.text) })
                .collect::<Vec<_>>()
                .join("\n\n"),
            SourceText::Image(x) => {
                let t = x.text();
                let t = if t.trim().is_empty() { "(None: no text was found in it.)".to_string() } else { t };
                format!("The source is an image. If you can see it, read it from the picture. Quote words exactly as they appear in it.\n\n## Text read from the image (may be incomplete)\n\n{t}")
            }
        }
    }
    /// Whether it's an image.
    pub fn is_image(&self) -> bool {
        matches!(self.text, SourceText::Image(_))
    }
    /// Whether a quote is in the source (on `page`, when it's a PDF and one is given).
    fn holds(&self, quote: &str, anchor: Option<&str>) -> bool {
        let q = proposals::normalise_quote(quote);
        if q.is_empty() {
            return false;
        }
        let hay = match (&self.text, anchor.and_then(proposals::page_anchor)) {
            (SourceText::Parts(x), Some(n)) => match x.page(n) {
                Some(p) => p.to_string(),
                None => return false,
            },
            (SourceText::Parts(x), None) | (SourceText::Image(x), _) => x.text(),
            (SourceText::Text(t), _) => t.clone(),
        };
        proposals::normalise_quote(&hay).contains(&q)
    }
}

/// Most of the catalogue of wiki pages the model is given.
const CATALOGUE_CHARS: usize = 20_000;

/// The catalogue within `n`: as it is if it fits, else without the summaries (the model writes
/// only its own), then without descriptions, so every entity and concept is still named and none
/// is made twice for want of seeing it.
fn fit_catalogue(c: &str, n: usize) -> String {
    if c.chars().count() <= n {
        return c.to_string();
    }
    let mut keep = Vec::new();
    let mut in_summaries = false;
    for l in c.lines() {
        if l.starts_with("## ") {
            in_summaries = l.starts_with("## Summaries");
        }
        if !in_summaries {
            keep.push(l);
        }
    }
    let c = keep.join("\n");
    if c.chars().count() <= n {
        return c;
    }
    c.lines().map(|l| l.split(" — ").next().unwrap_or(l)).collect::<Vec<_>>().join("\n")
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}\n\n[… cut here: {} characters in all]", s.chars().take(n).collect::<String>(), s.chars().count())
    }
}

/// Added to the prompt of an actions-only ingest (D-20261118-23): a source the wiki already has,
/// read again only for its actions and decisions, so its pages aren't written twice.
pub const ACTIONS_ONLY: &str = "\n# Actions and decisions only\n\nThis source is in the wiki already: its pages have what it says. Give only `actions` and `decisions` (step 6); leave `pages` empty and `summary` null.\n";

/// The message to the model: the workflow, the source, the pages it mentions with their text,
/// and every wiki page by name and description (`catalogue`, `catalogue::render`), for a source
/// that doesn't name what it's about.
pub fn prompt(src: &Source, pages: &[(String, String)], catalogue: &str, today: &str, tracking: &Tracking) -> String {
    prompt_since(src, pages, catalogue, today, tracking, None)
}

/// [`prompt`] for a source read before: `seen` holds the [`message_keys`] of the messages that
/// read had, so the ones new since are marked and the earlier ones cut down (D-20261120-09).
pub fn prompt_since(
    src: &Source,
    pages: &[(String, String)],
    catalogue: &str,
    today: &str,
    tracking: &Tracking,
    seen: Option<&std::collections::HashSet<String>>,
) -> String {
    let mut out = format!("{}\n\nToday is {today}.\n\n# The source: {}\n\nLink it as [[{}]].\n\n", WORKFLOW.trim(), src.rel, src.link());
    let since = seen.and_then(|seen| since_seen(&src.body(), seen));
    out.push_str(&cut(since.as_ref().map_or(&src.body(), |s| &s.text), SOURCE_CHARS));
    if since.is_some() {
        out.push_str(READ_BEFORE);
    }
    out.push_str("\n\n# Wiki pages the source mentions\n\n");
    if pages.is_empty() {
        out.push_str("None yet.\n");
    }
    let budget = page_budget(pages.len());
    for (rel, text) in pages {
        out.push_str(&format!("## {rel}\n\n{}\n\n", crate::pageview::view(text, budget).text));
    }
    if !catalogue.trim().is_empty() {
        out.push_str("\n# Every wiki page\n\nA source may not name what it's about (\"the steerco\", \"the launch\"): these are the pages it could change.\n\n");
        out.push_str(&cut(&fit_catalogue(catalogue, CATALOGUE_CHARS), CATALOGUE_CHARS));
        out.push('\n');
    }
    out.push_str(&tracking.section());
    if src.is_journal() {
        out.push_str("\nThis source is a journal note: give no summary page.\n");
    }
    if src.is_chat_day() {
        out.push_str("\nThis source is a day of a chat: give no summary page.\n");
    }
    out
}

/// Told to the model when the source's new messages are marked ([`since_seen`]).
pub const READ_BEFORE: &str = "\n\n# Read before\n\nThis source was ingested before and has grown since. Each message new since then starts with the line `[New since the last ingest]`. The other messages are there for context only: the wiki has them already, and some are left out. Take only what the new messages add. Don't restate what the earlier messages said, and leave a page alone when the new messages add nothing to it. Quote the source word for word, never that line or a left-out note.\n";

/// The line before each message new since a source was last read.
pub const NEW_MARK: &str = "[New since the last ingest]";

/// How much of the earlier messages is kept, as context, beside the new ones.
const EARLIER_CHARS: usize = 20_000;

/// A synced thread's parts: the header, then each message, split at its `---` lines.
fn message_parts(text: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    for line in text.split_inclusive('\n') {
        if line.trim_end() == "---" {
            parts.push(String::new());
        } else {
            parts.last_mut().unwrap().push_str(line);
        }
    }
    parts
}

fn message_key(part: &str) -> String {
    crate::write::version(part.trim().as_bytes())[..16].to_string()
}

/// The keys of a thread's messages (not its header, which changes with each capture), to tell
/// next time which are new.
pub fn message_keys(text: &str) -> Vec<String> {
    message_parts(text).iter().skip(1).filter(|p| !p.trim().is_empty()).map(|p| message_key(p)).collect()
}

/// A source shown with its new messages marked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinceSeen {
    pub text: String,
    pub new: usize,
    pub left_out: usize,
}

/// `text` with each message not in `seen` marked as new, and the earlier messages farthest
/// back left out past [`EARLIER_CHARS`]. None when none was read before or none is new: then the
/// whole source is read as usual.
pub fn since_seen(text: &str, seen: &std::collections::HashSet<String>) -> Option<SinceSeen> {
    let parts = message_parts(text);
    let msgs: Vec<(&str, bool)> =
        parts.iter().skip(1).filter(|p| !p.trim().is_empty()).map(|p| (p.as_str(), !seen.contains(&message_key(p)))).collect();
    let new = msgs.iter().filter(|m| m.1).count();
    if new == 0 || new == msgs.len() {
        return None;
    }
    // The earlier messages kept: the nearest the end first, within the budget.
    let mut keep = vec![true; msgs.len()];
    let mut room = EARLIER_CHARS;
    for i in (0..msgs.len()).rev().filter(|&i| !msgs[i].1) {
        let n = msgs[i].0.chars().count();
        if n <= room {
            room -= n;
        } else {
            room = 0;
            keep[i] = false;
        }
    }
    let mut out = parts[0].clone();
    let mut left_out = 0;
    let mut gap = 0;
    for (i, (m, is_new)) in msgs.iter().enumerate() {
        if !keep[i] {
            gap += 1;
            continue;
        }
        if gap > 0 {
            out.push_str(&format!("---\n\n[{gap} earlier message{} left out: read before]\n\n", if gap == 1 { "" } else { "s" }));
            left_out += gap;
            gap = 0;
        }
        out.push_str("---\n");
        if *is_new {
            out.push_str(&format!("\n{NEW_MARK}\n"));
        }
        out.push_str(m);
    }
    if gap > 0 {
        out.push_str(&format!("---\n\n[{gap} earlier message{} left out: read before]\n", if gap == 1 { "" } else { "s" }));
        left_out += gap;
    }
    Some(SinceSeen { text: out, new, left_out })
}

/// The messages of each synced thread an ingest has read (`sources-read-messages.json` in the
/// app data folder, path → [`message_keys`]).
pub fn read_messages_file(data: &std::path::Path) -> std::path::PathBuf {
    data.join("sources-read-messages.json")
}

pub fn read_messages(data: &std::path::Path, rel: &str) -> Option<std::collections::HashSet<String>> {
    let all: std::collections::BTreeMap<String, Vec<String>> =
        std::fs::read(read_messages_file(data)).ok().and_then(|b| serde_json::from_slice(&b).ok())?;
    all.get(rel).map(|v| v.iter().cloned().collect())
}

/// Notes the messages an ingest of `rel` read, replacing what it read before.
pub fn note_read_messages(data: &std::path::Path, rel: &str, keys: &[String]) -> std::io::Result<()> {
    let f = read_messages_file(data);
    let mut all: std::collections::BTreeMap<String, Vec<String>> =
        std::fs::read(&f).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    all.insert(rel.to_string(), keys.to_vec());
    let json = serde_json::to_vec(&all).map_err(std::io::Error::other)?;
    crate::write::write_atomic(&f, &json, false).map_err(|e| std::io::Error::other(e.to_string()))
}

/// One page change ready to become a proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Planned {
    pub page: String,
    pub kind: Kind,
    pub title: String,
    pub before: Option<String>,
    pub after: String,
    pub quotes: Vec<Quote>,
    /// The claims kept for the page's claims file: those whose quotes are in the source (all of
    /// them for an image, whose text is only what OCR read).
    pub claims: Vec<Claim>,
    /// The Timeline entry's heading it made ("2026-10-02 — Steerco"), for its claims.
    pub entry: Option<String>,
    /// Claims whose quotes weren't found in the source: the change waits for review.
    pub warnings: Vec<String>,
}

/// What the checks took out, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dropped {
    pub page: String,
    pub reason: String,
}

fn md(rel: &str) -> String {
    if rel.ends_with(".md") {
        rel.to_string()
    } else {
        format!("{rel}.md")
    }
}

/// The wiki page a model named, normalised: `\` as `/`, empty and `.` segments gone, `.md` added.
/// Err unless it's `wiki/<folder>/<name>.md` with no `..` or hidden segment, so a change never
/// reaches outside the wiki (a skill, a note, a dotfile).
pub fn wiki_page(named: &str) -> Result<String, String> {
    let segs: Vec<&str> = named.trim().split(['/', '\\']).filter(|s| !s.is_empty() && *s != ".").collect();
    let rel = md(&segs.join("/"));
    if segs.contains(&"..") {
        return Err(format!("A page goes in the wiki, not {}.", named.trim()));
    }
    proposals::valid_new_page(&rel).map(|_| rel)
}

fn short(q: &str) -> String {
    let one = q.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 80 {
        format!("“{}…”", one.chars().take(80).collect::<String>())
    } else {
        format!("“{one}”")
    }
}

/// Whether a change takes away anything the page had: a body line that isn't there after it (a
/// changed line counts), or a property other than `updated:` changed or a `sources:` entry gone.
pub fn removes_text(before: &str, after: &str) -> bool {
    let (b, a) = (before.replace("\r\n", "\n"), after.replace("\r\n", "\n"));
    let (fb, fa) = (crate::frontmatter::split(&b), crate::frontmatter::split(&a));
    if fb.error.is_some() || fa.error.is_some() {
        return true;
    }
    let props = |d: &serde_json::Value| {
        let mut d = d.clone();
        if let Some(m) = d.as_object_mut() {
            m.remove("sources");
            m.remove("updated");
            if m.is_empty() {
                return serde_json::Value::Null;
            }
        }
        d
    };
    if props(&fb.data) != props(&fa.data) {
        return true;
    }
    let has = crate::frontmatter::list(&fa.data, &["sources"], false);
    if crate::frontmatter::list(&fb.data, &["sources"], false).iter().any(|s| !has.contains(s)) {
        return true;
    }
    let old: Vec<&str> = b[fb.body_start..].lines().collect();
    let new: Vec<&str> = a[fa.body_start..].lines().collect();
    similar::capture_diff_slices(similar::Algorithm::Myers, &old, &new)
        .iter()
        .any(|op| !matches!(op, similar::DiffOp::Equal { .. }) && old[op.old_range()].iter().any(|l| !l.trim().is_empty()))
}

/// The new page's properties and body.
fn new_page(rel: &str, kind: &str, description: &str, source_link: &str, body: &str) -> String {
    let name = crate::lint::stem(crate::lint::name_of(rel));
    let desc = crate::write::yaml_scalar(description.trim());
    let src = crate::write::yaml_scalar(&format!("[[{source_link}]]"));
    format!("---\ntype: {kind}\nname: {}\ndescription: {desc}\nsources: [{src}]\n---\n\n{}\n", crate::write::yaml_scalar(name), body.trim())
}

/// What a summary page's name starts with. A summary named like its source would share the
/// source's link key, and `[[name]]` goes to the shorter path (the source), so the summary couldn't
/// be linked by name. Summaries made before this carry their source's name; they still count as
/// the source's summary (`plan` finds them by what their `sources:` cite, then by name).
pub const SUMMARY_PREFIX: &str = "Summary. ";

/// A summary page's path with `SUMMARY_PREFIX` on its name, as it is when it already has it.
fn prefixed_summary(rel: &str) -> String {
    match rel.rsplit_once('/') {
        Some((dir, name)) if !name.starts_with(SUMMARY_PREFIX) => format!("{dir}/{SUMMARY_PREFIX}{name}"),
        _ => rel.to_string(),
    }
}

/// What `plan` checks a new summary page against.
pub struct SummaryCheck<'a> {
    /// Every page in `wiki/summaries/` with the entries of its `sources:` property as written
    /// (`"[[Plan.pdf]]"`): which source each one summarises, whatever it's called.
    pub citing: &'a [(String, Vec<String>)],
    /// The page of any layer that a name's link key reaches, if one does: a summary may not share
    /// its link key with another page.
    pub taken: &'a dyn Fn(&str) -> Option<String>,
}

fn no_page(_: &str) -> Option<String> {
    None
}

impl SummaryCheck<'static> {
    /// No summaries and no pages: nothing to clash with.
    pub const NONE: SummaryCheck<'static> = SummaryCheck { citing: &[], taken: &no_page };
}

static CITED: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\[\[([^\]|#\n]+)").unwrap());

impl SummaryCheck<'_> {
    /// The summary page that cites this source in its `sources:`, by the source's file name with
    /// or without its extension: the rule Sources' Ingested status and Wiki health's
    /// uningested check use. It holds when the source is renamed (the citing page's link is
    /// rewritten with it) and for summaries named before the `Summary. ` prefix, with or without
    /// the source's extension.
    fn summary_of(&self, src: &Source) -> Option<&str> {
        let base = crate::links::nfc(crate::lint::name_of(&src.rel)).to_lowercase();
        let stem = base.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| base.clone());
        self.citing
            .iter()
            .find(|(_, entries)| {
                entries.iter().any(|e| {
                    CITED.captures_iter(e).any(|c| {
                        let name = c[1].trim();
                        let n = crate::links::nfc(name.rsplit('/').next().unwrap_or(name)).to_lowercase();
                        n == base || n == stem
                    })
                })
            })
            .map(|(p, _)| p.as_str())
    }
}

/// Checks the answer and turns it into page changes. `read` gives a page's text by vault path;
/// `resolve` a name's wiki page; `summaries` what a new summary page is checked against.
pub fn plan(
    src: &Source,
    answer: &Answer,
    read: impl Fn(&str) -> Option<String>,
    resolve: impl Fn(&str) -> Option<String>,
    summaries: &SummaryCheck,
    today: &str,
    budget: usize,
) -> (Vec<Planned>, Vec<Dropped>) {
    let mut out = Vec::new();
    let mut dropped = Vec::new();
    let link = src.link();
    let cite = format!("[[{link}]]");

    if let Some(s) = &answer.summary {
        let named = s.page.trim();
        let asked = wiki_page(named).unwrap_or_else(|_| md(named));
        // Named from the source (`Summary. <its name>`), not by the model: whatever the model
        // wrote, a name that is the source's own would share its link key.
        let stem = crate::lint::stem(crate::lint::name_of(&src.rel));
        let rel = format!("wiki/summaries/{SUMMARY_PREFIX}{stem}.md");
        if src.is_journal() {
            dropped.push(Dropped { page: rel, reason: "A journal note gets no summary page.".into() });
        } else if src.is_chat_day() {
            dropped.push(Dropped { page: rel, reason: "A day of a chat gets no summary page.".into() });
        } else if let Err(e) = wiki_page(named).and_then(|_| {
            if asked.starts_with("wiki/summaries/") {
                Ok(())
            } else {
                Err("A summary goes in wiki/summaries/.".into())
            }
        }) {
            dropped.push(Dropped { page: asked, reason: e });
        } else if let Some(have) = summaries.summary_of(src).map(String::from).or_else(|| {
            // No page cites it: by name, as this one would be named, as the model named it, and
            // as summaries were named before the prefix (the source's name, with or without its
            // extension).
            [
                rel.clone(),
                asked.clone(),
                prefixed_summary(&asked),
                format!("wiki/summaries/{stem}.md"),
                format!("wiki/summaries/{link}.md"),
                format!("wiki/summaries/{SUMMARY_PREFIX}{link}.md"),
            ]
            .into_iter()
            .find(|p| read(p).is_some())
        }) {
            dropped.push(Dropped {
                page: have,
                reason: "There's a summary page already; update it by hand or re-ingest it as a page.".into(),
            });
        } else if let Some(other) = (summaries.taken)(&format!("{SUMMARY_PREFIX}{stem}"))
            .filter(|p| p != &rel && p != &src.rel && crate::links::key(p) == crate::links::key(&rel))
        {
            dropped.push(Dropped {
                page: rel,
                reason: format!("{other} already has this name, so a [[link]] to the summary would reach it instead. A summary needs a name of its own."),
            });
        } else {
            let after = new_page(&rel, "summary", s.description.as_deref().unwrap_or(""), &link, &s.content);
            out.push(Planned {
                page: rel,
                kind: Kind::New,
                title: format!("Summary of {link}"),
                before: None,
                after,
                quotes: vec![],
                claims: vec![],
                entry: None,
                warnings: if src.is_image() { vec![IMAGE_WARNING.into()] } else { vec![] },
            });
        }
    }

    for p in &answer.pages {
        let named = p.page.trim().trim_start_matches("[[").trim_end_matches("]]").to_string();
        let rel = if named.contains(['/', '\\']) {
            wiki_page(&named)
        } else {
            resolve(&named).map_or_else(|| wiki_page(&format!("wiki/entities/{named}")), |r| wiki_page(&r))
        };
        let rel = match rel {
            Ok(r) => r,
            Err(e) => {
                dropped.push(Dropped { page: named, reason: e });
                continue;
            }
        };
        let found: Vec<&Claim> = p.claims.iter().filter(|c| src.holds(&c.quote, c.anchor.as_deref())).collect();
        // An image's text is what OCR made of it: a quote the model read from the picture may not
        // be in it, so the change is kept for review rather than dropped.
        if p.claims.is_empty() || (found.is_empty() && !src.is_image()) {
            let why = if p.claims.is_empty() { "No claims backed the change." } else { "None of its quotes is in the source." };
            dropped.push(Dropped { page: rel, reason: why.into() });
            continue;
        }
        // A claim whose quote isn't there stays, as a warning: the change waits for review.
        let mut warnings: Vec<String> = if src.is_image() { vec![IMAGE_WARNING.into()] } else { vec![] };
        warnings.extend(p.claims.iter().filter(|c| !src.holds(&c.quote, c.anchor.as_deref())).map(|c| {
            if src.is_image() {
                format!("This quote isn't in the text read from the image, so its claim is unchecked: {}", short(&c.quote))
            } else {
                format!("This quote isn't in the source, so its claim is unchecked: {}", short(&c.quote))
            }
        }));
        let claims: Vec<Claim> = if src.is_image() { p.claims.clone() } else { found.iter().map(|c| (*c).clone()).collect() };
        let quotes: Vec<Quote> = found
            .iter()
            .map(|c| Quote {
                source: link.clone(),
                anchor: c.anchor.clone(),
                text: c.quote.clone(),
                checked: Some(true),
                path: Some(src.rel.clone()),
            })
            .collect();
        let title = p.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| format!("Update from {link}"));
        // The entry's date: as given, else the source's own day.
        let entry_date = p.entry.as_ref().map(|e| {
            crate::pageshape::Date::parse_iso(e.date.trim())
                .or_else(|| source_day(&src.rel).and_then(|d| crate::pageshape::Date::parse_iso(&d)))
        });
        if entry_date == Some(None) {
            dropped.push(Dropped { page: rel, reason: "Its timeline entry has no date the app can read.".into() });
            continue;
        }
        let entry_date = entry_date.flatten();
        let entry = p.entry.as_ref().zip(entry_date).map(|(e, d)| {
            let t = e.title.trim();
            if t.is_empty() {
                d.iso()
            } else {
                format!("{} — {t}", d.iso())
            }
        });
        let current_state = p.current_state.as_deref().map(str::trim).filter(|t| !t.is_empty());
        // Two entries that come to one page ("Orbit App" and its path): the second builds on what
        // the first made of the page, in the one change, so neither's text or claims are lost.
        let earlier = out.iter().position(|o| o.page == rel);
        match earlier.map(|i| out[i].after.clone()).or_else(|| read(&rel)) {
            Some(before) => {
                let view = crate::pageview::view(&before, budget);
                let mut after = before.clone();
                let mut fail = None;
                // The app places what the model gives (D-20261006-06): the Current state after the
                // opening, the entry in the Timeline by date, a topical section above the Timeline.
                // A section is replaced only when the model saw all of it.
                if let Some(cs) = current_state {
                    if crate::pageshape::has_current_state(&before) && !view.can_rewrite(&before, crate::pageshape::CURRENT_STATE) {
                        fail = Some("It would rewrite “Current state”, which it was shown only part of.".to_string());
                    } else {
                        match crate::pageshape::with_current_state_text(&after, cs) {
                            Ok(t) => after = t,
                            Err(e) => fail = Some(e),
                        }
                    }
                }
                if let (Some(e), Some(d), None) = (&p.entry, entry_date, &fail) {
                    after = crate::pageshape::with_entry(&after, d, &e.title, &link, &e.body).0;
                }
                // An answer in the old form with no section named: the Current state's text.
                let legacy = SectionAnswer::Name(crate::pageshape::CURRENT_STATE.into());
                let section = match &p.section {
                    None if current_state.is_none() && p.entry.is_none() && !p.content.trim().is_empty() => Some(&legacy),
                    s => s.as_ref(),
                };
                match (section, &fail) {
                    (Some(SectionAnswer::Topic { heading, content }), None) => {
                        if !view.can_rewrite(&before, heading) {
                            fail = Some(format!(
                                "It would rewrite “{}”, which it was shown only part of.",
                                heading.trim().trim_start_matches('#').trim()
                            ));
                        } else {
                            match crate::pageshape::with_topic(&after, heading, content) {
                                Ok(t) => after = t,
                                Err(e) => fail = Some(e),
                            }
                        }
                    }
                    // An answer in the old form: one section's new text.
                    (Some(SectionAnswer::Name(section)), None) if current_state.is_none() && p.entry.is_none() => {
                        if !view.can_rewrite(&before, section) {
                            fail = Some(format!(
                                "It would rewrite “{}”, which it was shown only part of.",
                                section.trim().trim_start_matches('#').trim()
                            ));
                        } else {
                            match proposals::patched(&after, &Patch::Section { section: section.clone(), content: p.content.clone() }) {
                                Ok(t) => after = t,
                                Err(_) => fail = Some("The change didn't apply to the page.".into()),
                            }
                        }
                    }
                    _ => {}
                }
                if let Some(reason) = fail {
                    dropped.push(Dropped { page: rel, reason });
                    continue;
                }
                after = with_source(after, &cite, &mut warnings);
                after = proposals::bump_updated(&before, &after, today);
                if after.replace("\r\n", "\n") == before.replace("\r\n", "\n") {
                    dropped.push(Dropped { page: rel, reason: "It changed nothing.".into() });
                    continue;
                }
                match earlier {
                    Some(i) => {
                        let o = &mut out[i];
                        o.after = after;
                        o.quotes.extend(quotes);
                        o.claims.extend(claims);
                        for w in warnings {
                            if !o.warnings.contains(&w) {
                                o.warnings.push(w);
                            }
                        }
                        o.entry = o.entry.take().or(entry);
                    }
                    None => out.push(Planned {
                        page: rel,
                        kind: Kind::Edit,
                        title,
                        before: Some(before),
                        after,
                        quotes,
                        claims,
                        entry,
                        warnings,
                    }),
                }
            }
            None => {
                let kind = if rel.starts_with("wiki/concepts/") { "concept" } else { "entity" };
                let cs = current_state.unwrap_or(p.content.trim());
                let body = if cs.is_empty() { String::new() } else { format!("## Current state\n\n{cs}") };
                let mut after = new_page(&rel, kind, p.description.as_deref().unwrap_or(""), &link, &body);
                if let (Some(e), Some(d)) = (&p.entry, entry_date) {
                    after = crate::pageshape::with_entry(&after, d, &e.title, &link, &e.body).0;
                }
                out.push(Planned { page: rel, kind: Kind::New, title, before: None, after, quotes, claims, entry, warnings });
            }
        }
    }
    (out, dropped)
}

/// The initiative an answer names: its page, by path or name, or Unfiled for none.
fn initiative_rel(named: Option<&str>, tracking: &Tracking) -> Result<String, String> {
    use crate::initiatives as ini;
    let named = named
        .map(|n| n.trim().trim_start_matches("[[").trim_end_matches("]]").trim())
        .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("null"));
    let Some(n) = named else { return Ok(ini::UNFILED.to_string()) };
    let key = |s: &str| s.trim_end_matches(".md").rsplit('/').next().unwrap_or("").to_lowercase();
    let want = key(n);
    tracking
        .initiatives
        .iter()
        .find(|i| key(&i.path) == want || i.name.to_lowercase() == want)
        .map(|i| i.path.clone())
        .or_else(|| (want == "unfiled").then(|| ini::UNFILED.to_string()))
        .ok_or_else(|| format!("There's no initiative called {n}."))
}

fn owner_of(s: Option<&str>) -> Result<Option<crate::initiatives::Owner>, String> {
    use crate::initiatives::Owner;
    match s.map(|s| s.trim().to_lowercase()).as_deref() {
        None | Some("") => Ok(None),
        Some("me") => Ok(Some(Owner::Me)),
        Some("delegated") => Ok(Some(Owner::Delegated)),
        Some("other") => Ok(Some(Owner::Other)),
        Some(o) => Err(format!("“{o}” isn't an owner (me, delegated or other).")),
    }
}

fn status_of(s: Option<&str>) -> Result<Option<crate::initiatives::Status>, String> {
    use crate::initiatives::Status;
    match s.map(|s| s.trim().to_lowercase()).as_deref() {
        None | Some("") => Ok(None),
        Some("open") => Ok(Some(Status::Open)),
        Some("done") => Ok(Some(Status::Done)),
        Some("dropped") => Ok(Some(Status::Dropped)),
        Some(o) => Err(format!("“{o}” isn't a status (open, done or dropped).")),
    }
}

/// An initiative page as `plan_tracking` changes it: its text so far, the change's quotes and
/// warnings, and what was done to it.
struct Edited {
    rel: String,
    text: String,
    quotes: Vec<Quote>,
    warnings: Vec<String>,
    done: Vec<String>,
}

/// The answer's actions and decisions, put on their initiatives' pages (D-20261118-09): one change
/// per page, added to the page's change from `plan` when it has one. An action or decision whose
/// quote isn't in the source is dropped (kept, flagged, from an image); an action named by `id` is
/// that action changed (merged, done, handed on), never a second one. Unfiled is made the first
/// time it's needed.
pub fn plan_tracking(
    src: &Source,
    answer: &Answer,
    tracking: &Tracking,
    read: impl Fn(&str) -> Option<String>,
    seed: &str,
    today: &str,
    planned: (Vec<Planned>, Vec<Dropped>),
) -> (Vec<Planned>, Vec<Dropped>) {
    use crate::initiatives as ini;
    let (mut out, mut dropped) = planned;
    let link = src.link();
    let cite = format!("[[{link}]]");
    let quoted = |q: &str, anchor: Option<&str>| ini::Quote {
        text: q.trim().to_string(),
        source: match anchor.map(str::trim).filter(|a| !a.is_empty()) {
            Some(a) => format!("{link}#{a}"),
            None => link.clone(),
        },
    };
    let mut pages: Vec<Edited> = Vec::new();
    let page_text = |rel: &str, pages: &mut Vec<Edited>| -> Option<usize> {
        if let Some(i) = pages.iter().position(|p| p.rel == rel) {
            return Some(i);
        }
        let text =
            out.iter().find(|p| p.page == rel).map(|p| p.after.clone()).or_else(|| read(rel)).or_else(|| {
                (rel == ini::UNFILED).then(|| ini::new_page("Unfiled", "Actions from your sources that fit no initiative yet."))
            })?;
        pages.push(Edited { rel: rel.to_string(), text, quotes: vec![], warnings: vec![], done: vec![] });
        Some(pages.len() - 1)
    };
    let check = |what: &str, quote: &str, anchor: Option<&str>| -> Result<Option<String>, String> {
        if quote.trim().is_empty() {
            return Err(format!("{what} has no quote."));
        }
        if src.holds(quote, anchor) {
            Ok(None)
        } else if src.is_image() {
            Ok(Some(format!("This quote isn't in the text read from the image, so “{what}” is unchecked: {}", short(quote))))
        } else {
            Err(format!("The quote for “{what}” isn't in the source."))
        }
    };
    for a in &answer.actions {
        let what = a.text.clone().or_else(|| a.id.clone()).unwrap_or_else(|| "an action".into());
        let made = (|| -> Result<(String, Option<String>, Quote), String> {
            let rel = initiative_rel(a.initiative.as_deref(), tracking)?;
            let warning = check(&what, &a.quote, a.anchor.as_deref())?;
            let edit = ini::ActionEdit {
                id: a.id.clone().filter(|i| !i.trim().is_empty()),
                text: a.text.clone(),
                owner: owner_of(a.owner.as_deref())?,
                person: a.person.clone().filter(|p| !p.trim().is_empty()),
                status: status_of(a.status.as_deref())?,
                due: a.due.clone().filter(|d| !d.trim().is_empty()),
                quotes: vec![quoted(&a.quote, a.anchor.as_deref())],
                task: None,
                // Delegated on the day the source is from.
                on: Some(source_day(&src.rel).unwrap_or_else(|| today.to_string())),
            };
            let i = page_text(&rel, &mut pages).ok_or_else(|| format!("{rel} isn't there."))?;
            let (after, _) = ini::with_action(&pages[i].text, &edit, &format!("{seed}\n{rel}"))?;
            pages[i].text = after;
            pages[i].done.push(if edit.id.is_some() { format!("updated “{what}”") } else { format!("added “{what}”") });
            let q = Quote {
                source: link.clone(),
                anchor: a.anchor.clone(),
                text: a.quote.clone(),
                checked: Some(warning.is_none()),
                path: Some(src.rel.clone()),
            };
            Ok((rel, warning, q))
        })();
        match made {
            Ok((rel, warning, q)) => {
                let p = pages.iter_mut().find(|p| p.rel == rel).expect("made above");
                p.quotes.push(q);
                p.warnings.extend(warning);
            }
            Err(reason) => dropped.push(Dropped {
                page: a.initiative.clone().unwrap_or_else(|| ini::UNFILED.into()),
                reason: format!("Action “{what}”: {reason}"),
            }),
        }
    }
    for d in &answer.decisions {
        let what = d.text.trim().to_string();
        let made = (|| -> Result<(String, Option<String>, Quote), String> {
            let rel = initiative_rel(d.initiative.as_deref(), tracking)?;
            let warning = check(&what, &d.quote, d.anchor.as_deref())?;
            let i = page_text(&rel, &mut pages).ok_or_else(|| format!("{rel} isn't there."))?;
            let have = ini::parse(&rel, &pages[i].text);
            if have.decisions.iter().any(|x| x.text.eq_ignore_ascii_case(&what)) {
                return Err("It's there already.".into());
            }
            let date = d.date.clone().filter(|x| !x.trim().is_empty()).or_else(|| source_day(&src.rel));
            let (after, _) = ini::with_decision(
                &pages[i].text,
                date.as_deref(),
                &what,
                &[quoted(&d.quote, d.anchor.as_deref())],
                &format!("{seed}\n{rel}"),
            )?;
            pages[i].text = after;
            pages[i].done.push(format!("decided “{what}”"));
            let q = Quote {
                source: link.clone(),
                anchor: d.anchor.clone(),
                text: d.quote.clone(),
                checked: Some(warning.is_none()),
                path: Some(src.rel.clone()),
            };
            Ok((rel, warning, q))
        })();
        match made {
            Ok((rel, warning, q)) => {
                let p = pages.iter_mut().find(|p| p.rel == rel).expect("made above");
                p.quotes.push(q);
                p.warnings.extend(warning);
            }
            Err(reason) => dropped.push(Dropped {
                page: d.initiative.clone().unwrap_or_else(|| ini::UNFILED.into()),
                reason: format!("Decision “{what}”: {reason}"),
            }),
        }
    }
    for Edited { rel, text, quotes, mut warnings, done } in pages {
        if done.is_empty() {
            continue;
        }
        let text = with_source(text, &cite, &mut warnings);
        let summary = done.join(", ");
        match out.iter_mut().find(|p| p.page == rel) {
            Some(p) => {
                p.after = text;
                p.title = format!("{}; {summary}", p.title);
                p.quotes.extend(quotes);
                p.warnings.extend(warnings);
            }
            None => {
                let before = read(&rel);
                let after = match &before {
                    Some(b) => proposals::bump_updated(b, &text, today),
                    None => text,
                };
                if before.as_deref().is_some_and(|b| b.replace("\r\n", "\n") == after.replace("\r\n", "\n")) {
                    continue;
                }
                let kind = if before.is_some() { Kind::Edit } else { Kind::New };
                let name = crate::lint::stem(crate::lint::name_of(&rel)).to_string();
                let title = format!("{name}: {summary}");
                out.push(Planned { page: rel, kind, title, before, after, quotes, claims: vec![], entry: None, warnings });
            }
        }
    }
    (out, dropped)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_grown_thread_marks_its_new_messages_and_cuts_the_earlier_ones() {
        let msg = |who: &str, body: &str| format!("## {who} — 2026-11-19 09:00\n\n{body}\n\n---\n\n");
        let head = |at: &str| format!("# Launch\n\n**Captured:** {at}\n\n---\n\n");
        let before = format!("{}{}{}", head("09:00"), msg("Maya", "Launch is on the 14th."), msg("Lena", "Agreed."));
        let seen: std::collections::HashSet<String> = super::message_keys(&before).into_iter().collect();
        // Nothing new (only the header's capture time moved): read as usual.
        let same = format!("{}{}{}", head("10:00"), msg("Maya", "Launch is on the 14th."), msg("Lena", "Agreed."));
        assert_eq!(super::since_seen(&same, &seen), None);
        // A reply: only it is marked.
        let grown = format!("{same}{}", msg("Sam", "Can we move it to the 21st?"));
        let s = super::since_seen(&grown, &seen).unwrap();
        assert_eq!((s.new, s.left_out), (1, 0));
        assert_eq!(s.text.matches(super::NEW_MARK).count(), 1);
        assert!(s.text.contains(&format!("{}\n\n## Sam", super::NEW_MARK)));
        assert!(s.text.contains("Launch is on the 14th.") && s.text.starts_with("# Launch"));
        // A long history: the earliest messages are left out, the latest kept beside the new one.
        let long: String = (0..60).map(|i| msg("Maya", &format!("Update {i}: {}", "x".repeat(500)))).collect();
        let seen: std::collections::HashSet<String> = super::message_keys(&format!("{}{long}", head("09:00"))).into_iter().collect();
        let s = super::since_seen(&format!("{}{long}{}", head("10:00"), msg("Sam", "Done.")), &seen).unwrap();
        assert!(s.left_out > 0 && s.text.contains("earlier messages left out") && s.text.contains("Update 59"));
        assert!(!s.text.contains("Update 0:"));
        // Nothing read before: None.
        assert_eq!(super::since_seen(&grown, &Default::default()), None);
    }

    use super::*;
    use crate::extract::Part;

    const PAGE: &str = "---\ntype: entity\nsources:\n  - \"[[Roadmap Update 2026-09-18]]\"\n---\n# Orbit App\n\n## Current state\n\nSoft launch to staff is planned for 14 November. ^q3\n";

    fn pdf() -> Source {
        Source {
            rel: "sources/Programme Update Steerco 2026-09-28.pdf".into(),
            text: SourceText::Parts(Extracted {
                kind: "pdf".into(),
                parts: vec![
                    Part { label: "Page 1".into(), text: "Programme update".into() },
                    Part { label: "Page 2".into(), text: "Staff launch now targeted for 28 November, pending pen-test closure.".into() },
                ],
            }),
        }
    }

    fn answer() -> Answer {
        parse_answer(
            r#"Here you go:
```json
{"summary": {"page": "wiki/summaries/Programme Update Steerco 2026-09-28", "description": "The September steerco update", "content": "Launch moves to 28 November."},
 "pages": [
  {"page": "Orbit App", "title": "Launch date moves", "content": "Soft launch to staff is now planned for 28 November ([[Programme Update Steerco 2026-09-28.pdf#page=2]]). ^q3",
   "claims": [{"subject": "Orbit App", "attribute": "go_live_date", "value": "28 November", "as_of": "2026-09-28", "quote": "Staff launch now targeted for 28 November", "anchor": "page=2"}]},
  {"page": "wiki/entities/Northwind.md", "new": true, "description": "A supplier", "content": "Supplies hardware.",
   "claims": [{"subject": "Northwind", "attribute": "vendor", "value": "hardware", "quote": "Northwind supplies the hardware", "anchor": "page=2"}]},
  {"page": "wiki/concepts/Pen test.md", "new": true, "content": "Gates the launch.",
   "claims": [{"subject": "Pen test", "attribute": "status", "value": "gating", "quote": "pending pen-test closure", "anchor": "page=1"}]},
  {"page": "wiki/concepts/Launch gate.md", "new": true, "content": "Gates the launch.",
   "claims": [{"subject": "Launch gate", "attribute": "status", "value": "open", "quote": "pending pen-test closure", "anchor": "page=2"}]}
 ]}
```"#,
        )
        .unwrap()
    }

    fn read(rel: &str) -> Option<String> {
        (rel == "wiki/entities/Orbit App.md").then(|| PAGE.to_string())
    }

    fn resolve(name: &str) -> Option<String> {
        (name == "Orbit App").then(|| "wiki/entities/Orbit App.md".to_string())
    }

    #[test]
    fn quotes_decide_what_survives() {
        let (planned, dropped) = plan(&pdf(), &answer(), read, resolve, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        let pages: Vec<&str> = planned.iter().map(|p| p.page.as_str()).collect();
        assert_eq!(
            pages,
            [
                "wiki/summaries/Summary. Programme Update Steerco 2026-09-28.md",
                "wiki/entities/Orbit App.md",
                "wiki/concepts/Launch gate.md"
            ]
        );
        // Northwind's quote isn't in the source; the pen test's is, but not on page 1.
        assert_eq!(
            dropped.iter().map(|d| d.page.as_str()).collect::<Vec<_>>(),
            ["wiki/entities/Northwind.md", "wiki/concepts/Pen test.md"]
        );
        let orbit = &planned[1];
        assert_eq!(orbit.kind, Kind::Edit);
        assert!(orbit.after.contains("now planned for 28 November") && orbit.after.contains("^q3"));
        assert!(orbit.after.contains("  - \"[[Programme Update Steerco 2026-09-28.pdf]]\"\n"), "{}", orbit.after);
        assert_eq!(orbit.quotes[0].checked, Some(true));
        assert_eq!(orbit.quotes[0].anchor.as_deref(), Some("page=2"));
        let gate = &planned[2];
        assert!(gate.after.starts_with("---\ntype: concept\nname: Launch gate\ndescription: \"\"\nsources: [\"[[Programme Update Steerco 2026-09-28.pdf]]\"]\n---\n\n## Current state\n\nGates the launch.\n"));
        let summary = &planned[0];
        assert!(summary.after.contains("type: summary") && summary.after.ends_with("Launch moves to 28 November.\n"));
    }

    /// An answer with only a summary, named as given.
    fn summary_only(page: &str) -> Answer {
        let page = page.replace('\\', "\\\\");
        parse_answer(&format!(r#"{{"summary": {{"page": "{page}", "description": "d", "content": "Launch moves."}}, "pages": []}}"#))
            .unwrap()
    }

    const SUMMARY: &str = "wiki/summaries/Summary. Programme Update Steerco 2026-09-28.md";

    fn plan_summary(src: &Source, a: &Answer, read: impl Fn(&str) -> Option<String>, s: &SummaryCheck) -> (Vec<Planned>, Vec<Dropped>) {
        plan(src, a, read, |_| None, s, "2026-10-02", page_budget(1))
    }

    fn citing(path: &str, sources: &[&str]) -> (String, Vec<String>) {
        (path.to_string(), sources.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn a_summary_is_named_from_its_source_with_the_summary_prefix() {
        for named in [
            "wiki/summaries/Programme Update Steerco 2026-09-28",
            "wiki/summaries/Programme Update Steerco 2026-09-28.md",
            "wiki/summaries/Summary. Programme Update Steerco 2026-09-28",
            "wiki/summaries/Something the model liked better",
            "wiki\\summaries\\Programme Update Steerco 2026-09-28.md",
        ] {
            let (planned, dropped) = plan_summary(&pdf(), &summary_only(named), |_| None, &SummaryCheck::NONE);
            assert!(dropped.is_empty(), "{named}: {dropped:?}");
            assert_eq!(planned[0].page, SUMMARY, "{named}");
            // The page's own name property carries it too; it cites the source by the source's name.
            assert!(planned[0].after.contains("name: Summary. Programme Update Steerco 2026-09-28\n"), "{}", planned[0].after);
            assert!(planned[0].after.contains("sources: [\"[[Programme Update Steerco 2026-09-28.pdf]]\"]"));
        }
        // Not the summaries folder: still refused, whatever the prefix.
        let a = summary_only("wiki/entities/Summary. Orbit");
        let (planned, dropped) = plan_summary(&pdf(), &a, |_| None, &SummaryCheck::NONE);
        assert!(planned.is_empty() && dropped[0].reason.contains("wiki/summaries/"), "{dropped:?}");
    }

    #[test]
    fn a_source_called_summary_still_gets_a_summary() {
        // Its own name already starts with the prefix: the summary is `Summary. Summary. Q3`, not a
        // page with the source's own link key.
        let src =
            Source { rel: "sources/Summary. Q3.md".into(), text: SourceText::Text("Staff launch now targeted for 28 November.".into()) };
        let a = summary_only("wiki/summaries/Summary. Q3");
        // The source itself is what its own name reaches; that isn't a clash.
        let itself = |name: &str| (crate::links::key(name) == "summary. q3").then(|| "sources/Summary. Q3.md".to_string());
        let (planned, dropped) = plan_summary(&src, &a, |_| None, &SummaryCheck { citing: &[], taken: &itself });
        assert!(dropped.is_empty(), "{dropped:?}");
        assert_eq!(planned[0].page, "wiki/summaries/Summary. Summary. Q3.md");
        assert!(planned[0].after.contains("sources: [\"[[Summary. Q3]]\"]"));
    }

    #[test]
    fn a_summary_may_not_share_a_link_key_with_any_page() {
        let a = summary_only("wiki/summaries/Programme Update Steerco 2026-09-28");
        // Any layer: the key is the lower-cased name without .md, wherever the file is.
        let clash = |name: &str| {
            (crate::links::key(name) == "summary. programme update steerco 2026-09-28")
                .then(|| "Notes/summary. Programme Update Steerco 2026-09-28.md".to_string())
        };
        let (planned, dropped) = plan_summary(&pdf(), &a, |_| None, &SummaryCheck { citing: &[], taken: &clash });
        assert!(planned.is_empty());
        assert_eq!(dropped[0].page, SUMMARY);
        assert!(
            dropped[0].reason.contains("Notes/summary. Programme Update Steerco 2026-09-28.md already has this name"),
            "{}",
            dropped[0].reason
        );
        // A page of another name (reached by an alias, say) is no clash.
        let other = |_: &str| Some("sources/Something else.md".to_string());
        let (planned, _) = plan_summary(&pdf(), &a, |_| None, &SummaryCheck { citing: &[], taken: &other });
        assert_eq!(planned.len(), 1);
    }

    #[test]
    fn a_source_with_a_summary_gets_no_second_one_whatever_the_summary_is_called() {
        let a = summary_only("wiki/summaries/Programme Update Steerco 2026-09-28");
        let exists = |c: &[(String, Vec<String>)]| {
            let (planned, dropped) = plan_summary(&pdf(), &a, |_| None, &SummaryCheck { citing: c, taken: &no_page });
            assert!(planned.is_empty(), "no second summary: {c:?}");
            assert!(dropped[0].reason.contains("summary page already"), "{dropped:?}");
            dropped[0].page.clone()
        };
        // Old form: named like the source without its extension (what the app made before the
        // prefix for a PDF), citing it by its name with or without the extension.
        let old = "wiki/summaries/Programme Update Steerco 2026-09-28.md";
        assert_eq!(exists(&[citing(old, &["[[Programme Update Steerco 2026-09-28.pdf]]"])]), old);
        assert_eq!(exists(&[citing(old, &["\"[[Programme Update Steerco 2026-09-28]]\""])]), old);
        // The new form, and a page that cites it among others, with a folder in the link.
        let new = "wiki/summaries/Summary. Programme Update Steerco 2026-09-28.md";
        assert_eq!(
            exists(&[
                citing("wiki/summaries/Other.md", &["[[Other.pdf]]"]),
                citing(new, &["[[Notes/Other]]", "[[sources/Programme Update Steerco 2026-09-28.pdf]]"])
            ]),
            new
        );
    }

    #[test]
    fn a_renamed_source_keeps_its_summary() {
        // The source was `Steerco update.pdf` when it was summarised; renaming it rewrote the
        // summary's link, and the summary kept its first name.
        let src = Source { rel: "sources/Steerco notes.pdf".into(), ..pdf() };
        let kept = [citing("wiki/summaries/Summary. Steerco update.md", &["[[Steerco notes.pdf]]"])];
        let (planned, dropped) =
            plan_summary(&src, &summary_only("wiki/summaries/Steerco notes"), |_| None, &SummaryCheck { citing: &kept, taken: &no_page });
        assert!(planned.is_empty());
        assert_eq!(dropped[0].page, "wiki/summaries/Summary. Steerco update.md");
        // A summary citing some other source doesn't count.
        let other = [citing("wiki/summaries/Summary. Steerco update.md", &["[[Steerco update.pdf]]"])];
        let (planned, _) =
            plan_summary(&src, &summary_only("wiki/summaries/Steerco notes"), |_| None, &SummaryCheck { citing: &other, taken: &no_page });
        assert_eq!(planned.len(), 1);
    }

    #[test]
    fn a_summary_found_only_by_name_still_counts() {
        // No `sources:` to go by: the page at the old name (with the extension, or without), and at
        // the new one.
        for path in [
            "wiki/summaries/Programme Update Steerco 2026-09-28.pdf.md",
            "wiki/summaries/Programme Update Steerco 2026-09-28.md",
            "wiki/summaries/Summary. Programme Update Steerco 2026-09-28.pdf.md",
            SUMMARY,
        ] {
            let there = |rel: &str| (rel == path).then(|| "---\ntype: summary\n---\n".to_string());
            let (planned, dropped) = plan_summary(&pdf(), &summary_only("wiki/summaries/Anything"), there, &SummaryCheck::NONE);
            assert!(planned.is_empty() && dropped[0].reason.contains("summary page already"), "{path}: {dropped:?}");
        }
    }

    #[test]
    fn the_workflow_names_summaries_with_the_prefix() {
        assert!(WORKFLOW.contains("wiki/summaries/Summary. <Source name>.md"));
    }

    /// A page in the shape, and an answer in the page-shape form for it.
    const SHAPED: &str = "---\ntype: entity\n---\n# Orbit App\n\nThe staff app.\n\n## Current state\n\nSoft launch on 14 November.\n\n## Architecture\n\nOn the Hub.\n\n## Timeline\n\n### 2026-09-18 — Roadmap\nSource: [[Roadmap Update 2026-09-18]]\n\nRoadmap shared.\n\n## See also\n\n- [[Lena]]\n";

    fn shaped_answer(extra: &str) -> Answer {
        parse_answer(&format!(
            r#"{{"pages": [{{"page": "Orbit App", "title": "Launch moves",
              "current_state": "Soft launch to staff on 28 November ([[Programme Update Steerco 2026-09-28.pdf#page=2]]).",
              "entry": {{"date": "2026-09-28", "title": "Steerco: launch moves to 28 November", "body": "Staff launch now 28 November, pending the pen test."}},
              {extra}
              "claims": [{{"subject": "Orbit App", "attribute": "go_live_date", "value": "28 November", "as_of": "2026-09-28", "quote": "Staff launch now targeted for 28 November", "anchor": "page=2"}}]}}]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn the_app_places_the_entry_and_current_state_and_reingesting_replaces_the_entry() {
        let read = |r: &str| (r == "wiki/entities/Orbit App.md").then(|| SHAPED.to_string());
        let (planned, dropped) = plan(&pdf(), &shaped_answer(""), read, resolve, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        assert!(dropped.is_empty(), "{dropped:?}");
        let after = &planned[0].after;
        assert!(after.contains("## Current state\n\nSoft launch to staff on 28 November"));
        assert!(after.contains("## Timeline\n\n### 2026-09-28 — Steerco: launch moves to 28 November\nSource: [[Programme Update Steerco 2026-09-28.pdf]]\n\nStaff launch now 28 November, pending the pen test.\n\n### 2026-09-18 — Roadmap"), "{after}");
        assert!(crate::pageshape::in_shape(after, &|t| t.starts_with("Roadmap") || t.ends_with(".pdf")));
        // The same source again: its entry is replaced, not repeated.
        let again = after.clone();
        let read2 = move |r: &str| (r == "wiki/entities/Orbit App.md").then(|| again.clone());
        let a2 = parse_answer(&serde_json::to_string(&serde_json::json!({"pages": [{"page": "Orbit App",
            "entry": {"date": "2026-09-28", "title": "Steerco: launch on 28 November confirmed", "body": "Confirmed."},
            "claims": [{"subject": "Orbit App", "attribute": "go_live_date", "value": "28 November", "quote": "Staff launch now targeted for 28 November", "anchor": "page=2"}]}]})).unwrap()).unwrap();
        let (p2, _) = plan(&pdf(), &a2, read2, resolve, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        assert_eq!(p2[0].after.matches("Source: [[Programme Update Steerco 2026-09-28.pdf]]").count(), 1);
        assert!(p2[0].after.contains("### 2026-09-28 — Steerco: launch on 28 November confirmed\nSource: [[Programme Update Steerco 2026-09-28.pdf]]\n\nConfirmed.\n"));
    }

    #[test]
    fn a_topical_section_is_rewritten_only_when_shown_whole() {
        let read = |r: &str| (r == "wiki/entities/Orbit App.md").then(|| SHAPED.to_string());
        let a = shaped_answer(r#""section": {"heading": "Risks", "content": "- The pen test gates the launch."},"#);
        let (planned, _) = plan(&pdf(), &a, read, resolve, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        assert!(planned[0].after.contains("On the Hub.\n\n## Risks\n\n- The pen test gates the launch.\n\n## Timeline"));
        // A long page whose Architecture the model saw only the heading of.
        let long = SHAPED.replace("On the Hub.", &"On the Hub, with many details. ".repeat(2000));
        let read = move |r: &str| (r == "wiki/entities/Orbit App.md").then(|| long.clone());
        let a = shaped_answer(r#""section": {"heading": "Architecture", "content": "Rewritten."},"#);
        let (planned, dropped) = plan(&pdf(), &a, read, resolve, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        assert!(planned.is_empty() && dropped[0].reason.contains("Architecture"), "{dropped:?}");
    }

    #[test]
    fn two_entries_for_one_page_make_one_change() {
        let read = |r: &str| (r == "wiki/entities/Orbit App.md").then(|| SHAPED.to_string());
        // The page by name, then by path: one planned change, with both entries' text and claims.
        let a = parse_answer(
            r#"{"pages": [
              {"page": "Orbit App", "title": "Launch moves",
               "current_state": "Soft launch to staff on 28 November.",
               "claims": [{"subject": "Orbit App", "attribute": "go_live_date", "value": "28 November", "quote": "Staff launch now targeted for 28 November", "anchor": "page=2"}]},
              {"page": "wiki/entities/Orbit App.md", "title": "The pen test gates it",
               "section": {"heading": "Risks", "content": "- The pen test gates the launch."},
               "entry": {"date": "2026-09-28", "title": "Steerco", "body": "Pending the pen test."},
               "claims": [{"subject": "Orbit App", "attribute": "status", "value": "gated", "quote": "pending pen-test closure", "anchor": "page=2"}]}]}"#,
        )
        .unwrap();
        let (planned, dropped) = plan(&pdf(), &a, read, resolve, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        assert!(dropped.is_empty(), "{dropped:?}");
        assert_eq!(planned.len(), 1);
        let p = &planned[0];
        assert_eq!((p.page.as_str(), p.kind, p.before.as_deref()), ("wiki/entities/Orbit App.md", Kind::Edit, Some(SHAPED)));
        assert!(p.after.contains("## Current state\n\nSoft launch to staff on 28 November."), "{}", p.after);
        assert!(
            p.after.contains("## Risks\n\n- The pen test gates the launch.") && p.after.contains("### 2026-09-28 — Steerco"),
            "{}",
            p.after
        );
        assert_eq!(p.claims.iter().map(|c| c.attribute.as_str()).collect::<Vec<_>>(), ["go_live_date", "status"]);
        assert_eq!(p.quotes.len(), 2);
        assert_eq!(p.entry.as_deref(), Some("2026-09-28 — Steerco"));
        // A second entry for a page the first one made.
        let a = parse_answer(
            r#"{"pages": [
              {"page": "wiki/entities/Pen test.md", "current_state": "Gates the launch.",
               "claims": [{"subject": "Pen test", "attribute": "status", "value": "pending", "quote": "pending pen-test closure", "anchor": "page=2"}]},
              {"page": "[[wiki/entities/Pen test]]", "entry": {"date": "2026-09-28", "title": "Pending", "body": "Pending closure."},
               "claims": [{"subject": "Pen test", "attribute": "owner", "value": "Acme", "quote": "Staff launch now targeted", "anchor": "page=2"}]}]}"#,
        )
        .unwrap();
        let (planned, dropped) = plan(&pdf(), &a, |_| None, |_| None, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        assert!(dropped.is_empty(), "{dropped:?}");
        assert_eq!(planned.len(), 1);
        let p = &planned[0];
        assert_eq!((p.kind, p.before.as_deref(), p.claims.len()), (Kind::New, None, 2));
        assert!(p.after.contains("Gates the launch.") && p.after.contains("### 2026-09-28 — Pending"), "{}", p.after);
    }

    #[test]
    fn properties_that_cant_take_the_source_are_a_warning() {
        // Properties that don't read as YAML (a list never closed).
        let page = "---\ntype: entity\nsources: [Roadmap\n---\n# Orbit App\n\n## Current state\n\nSoft launch on 14 November.\n";
        assert_eq!(
            crate::fixname::list_value_added(page, &["sources"], "[[x]]"),
            crate::fixname::ListValue::CannotEdit,
            "the page for this test"
        );
        let read = move |r: &str| (r == "wiki/entities/Orbit App.md").then(|| page.to_string());
        let (planned, dropped) = plan(&pdf(), &shaped_answer(""), read, resolve, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        assert!(dropped.is_empty(), "{dropped:?}");
        assert_eq!(planned[0].warnings, [SOURCES_WARNING]);
        assert!(planned[0].after.contains("sources: [Roadmap\n---\n") && !planned[0].after.contains(".pdf]]\"]"), "{}", planned[0].after);
        // A page that takes it has no warning.
        let (planned, _) = plan(&pdf(), &answer(), super::tests::read, resolve, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        assert!(planned[1].warnings.is_empty());
    }

    #[test]
    fn a_new_page_gets_its_current_state_and_timeline() {
        let a = parse_answer(r#"{"pages": [{"page": "wiki/entities/Pen test.md", "new": true, "description": "The launch gate",
            "current_state": "Gates the launch.", "entry": {"date": "", "title": "Pending", "body": "Pending closure."},
            "claims": [{"subject": "Pen test", "attribute": "status", "value": "pending", "quote": "pending pen-test closure", "anchor": "page=2"}]}]}"#).unwrap();
        let (planned, _) = plan(&pdf(), &a, |_| None, |_| None, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        // No date given: the source's own day.
        assert!(planned[0].after.ends_with("## Current state\n\nGates the launch.\n\n## Timeline\n\n### 2026-09-28 — Pending\nSource: [[Programme Update Steerco 2026-09-28.pdf]]\n\nPending closure.\n"), "{}", planned[0].after);
    }

    #[test]
    fn journal_notes_get_no_summary() {
        let note = Source {
            rel: "Meeting. Orbit App Steerco - 2026-09-30.md".into(),
            text: SourceText::Text("Staff launch now targeted for 28 November, pending pen-test closure.".into()),
        };
        assert!(note.is_journal());
        let (planned, dropped) = plan(&note, &answer(), read, resolve, &SummaryCheck::NONE, "2026-10-02", page_budget(1));
        assert!(planned.iter().all(|p| !p.page.starts_with("wiki/summaries/")));
        assert_eq!(dropped[0].reason, "A journal note gets no summary page.");
        // Without anchors to check, a markdown source's quotes are found anywhere in it.
        assert!(planned.iter().any(|p| p.page == "wiki/concepts/Pen test.md"));
        assert!(prompt(&note, &[], "", "Friday", &Tracking::default()).contains("give no summary page"));
    }

    #[test]
    fn answers_that_dont_parse() {
        assert!(parse_answer("No JSON here").unwrap_err().contains("no JSON"));
        assert!(parse_answer("{\"pages\": 3}").unwrap_err().contains("schema"));
        assert!(parse_answer("{}").unwrap().pages.is_empty());
    }

    #[test]
    fn a_source_is_dated_by_its_name() {
        assert_eq!(source_day("Meeting. Orbit App Steerco - 2026-09-30.md").as_deref(), Some("2026-09-30"));
        assert_eq!(source_day("sources/Roadmap Update 2026-10-02-061026-052837.pdf").as_deref(), Some("2026-10-02"));
        assert_eq!(source_day("sources/Milestones (Product & Tech) - Week ending 2 October 2026.pdf").as_deref(), Some("2026-10-02"));
        assert_eq!(source_day("sources/TOM v2.1.pdf"), None);
    }

    #[test]
    fn a_big_catalogue_keeps_every_entity_named() {
        let mut c = String::from("# Catalogue\n\n## Entities (300)\n\n");
        for i in 0..300 {
            c.push_str(&format!("- [[Person {i}]] — {}\n", "Works on the Orbit App. ".repeat(4)));
        }
        c.push_str("\n## Concepts (1)\n\n- [[Launch plan]] — The plan.\n\n## Summaries (1)\n\n- [[Steerco 2026-09-30]] — Notes.\n");
        let f = fit_catalogue(&c, CATALOGUE_CHARS);
        assert!(f.contains("- [[Person 299]]") && f.contains("- [[Launch plan]]"));
        assert!(!f.contains("Steerco 2026-09-30") && !f.contains("Works on"));
    }

    #[test]
    fn the_prompt() {
        let p = prompt(
            &pdf(),
            &[("wiki/entities/Orbit App.md".into(), PAGE.into())],
            "- [[Orbit App]] — The staff app.",
            "Friday 2 October 2026",
            &Tracking::default(),
        );
        assert!(p.contains("# Every wiki page") && p.contains("- [[Orbit App]] — The staff app."));
        assert!(p.starts_with("# Ingest a source into the wiki"));
        assert!(p.contains("Link it as [[Programme Update Steerco 2026-09-28.pdf]]."));
        assert!(p.contains("## Page 2\n\nStaff launch now targeted"));
        assert!(p.contains("## wiki/entities/Orbit App.md\n\n---\ntype: entity"));
    }

    fn image(text: &str) -> Source {
        let parts = if text.is_empty() { vec![] } else { vec![Part { label: String::new(), text: text.into() }] };
        Source { rel: "sources/Whiteboard photo.png".into(), text: SourceText::Image(Extracted { kind: "image".into(), parts }) }
    }

    #[test]
    fn an_image_keeps_its_changes_for_review() {
        let src = image("Orbit App launch: 14 Oct\nOwner: Maya");
        let a = parse_answer(
            r#"{"pages": [
              {"page": "Orbit App", "content": "Launch on 14 October ([[Whiteboard photo.png]]).",
               "claims": [{"subject": "Orbit App", "attribute": "go_live_date", "value": "14 Oct", "quote": "Orbit App launch: 14 Oct"}]},
              {"page": "wiki/entities/Lena.md", "new": true, "content": "Signs off the pen test.",
               "claims": [{"subject": "Lena", "attribute": "role", "value": "pen test", "quote": "Pen test signed off by Lena"}]},
              {"page": "wiki/entities/Theo.md", "new": true, "content": "Nothing backs this.", "claims": []}
            ]}"#,
        )
        .unwrap();
        let (planned, dropped) = plan(&src, &a, read, resolve, &SummaryCheck::NONE, "2026-10-04", page_budget(1));
        assert_eq!(planned.iter().map(|p| p.page.as_str()).collect::<Vec<_>>(), ["wiki/entities/Orbit App.md", "wiki/entities/Lena.md"]);
        assert_eq!(dropped.iter().map(|d| d.page.as_str()).collect::<Vec<_>>(), ["wiki/entities/Theo.md"]);
        // The OCR'd quote counts as found; the one read from the picture alone is a warning.
        let orbit = &planned[0];
        assert_eq!(orbit.quotes.len(), 1);
        assert_eq!(orbit.quotes[0].checked, Some(true));
        assert_eq!(orbit.warnings, [IMAGE_WARNING]);
        assert!(planned[1].quotes.is_empty());
        assert!(planned[1].warnings[1].contains("text read from the image"));
        // Flagged as resting on an image, even with every quote found (src-tauri/src/ingest.rs).
        assert!(planned.iter().all(|p| p.warnings.contains(&IMAGE_WARNING.to_string())));
        assert!(orbit.after.contains("[[Whiteboard photo.png]]"));
        // The prompt labels the text; with none, it says so.
        assert!(prompt(&src, &[], "", "Sunday", &Tracking::default())
            .contains("## Text read from the image (may be incomplete)\n\nOrbit App launch"));
        assert!(prompt(&image(""), &[], "", "Sunday", &Tracking::default()).contains("no text was found in it"));
    }

    #[test]
    fn pages_stay_in_the_wiki() {
        assert_eq!(wiki_page("wiki/entities/Orbit App").unwrap(), "wiki/entities/Orbit App.md");
        assert_eq!(wiki_page("/wiki//entities/./Orbit App.md").unwrap(), "wiki/entities/Orbit App.md");
        assert_eq!(wiki_page("wiki\\concepts\\Pen test.md").unwrap(), "wiki/concepts/Pen test.md");
        for bad in [
            "wiki/../.claude/skills/x.md",
            "wiki/entities/../../Note.md",
            ".claude/skills/x.md",
            "Note.md",
            "wiki/x.md",
            "wiki/.hidden/x.md",
        ] {
            assert!(wiki_page(bad).is_err(), "{bad}");
        }
        // A name outside the wiki, from the model or from resolving it, is dropped, never planned.
        let a = parse_answer(
            r#"{"pages": [
              {"page": "wiki/../.claude/skills/evil.md", "content": "x", "claims": [{"quote": "Staff launch now targeted for 28 November"}]},
              {"page": "Notes/Orbit App.md", "content": "x", "claims": [{"quote": "Staff launch now targeted for 28 November"}]},
              {"page": "Hub", "content": "x", "claims": [{"quote": "Staff launch now targeted for 28 November"}]}
            ]}"#,
        )
        .unwrap();
        let (planned, dropped) =
            plan(&pdf(), &a, read, |_| Some("Projects/Hub.md".into()), &SummaryCheck::NONE, "2026-10-03", page_budget(1));
        assert!(planned.is_empty(), "{planned:?}");
        assert_eq!(dropped.len(), 3);
    }

    const LIST_PAGE: &str = "---\ntype: entity\nsources:\n  - \"[[Roadmap Update 2026-09-18]]\"\n---\n# Orbit App\n\n## Current state\n\n- Soft launch to staff planned. ^q3\n\n## History\n\n- Started in May.\n";

    fn one(content: &str, quotes: &[&str]) -> Answer {
        let claims: Vec<serde_json::Value> = quotes.iter().map(|q| serde_json::json!({"quote": q, "anchor": "page=2"})).collect();
        let json = serde_json::json!({"pages": [{"page": "Orbit App", "content": content, "claims": claims}]});
        parse_answer(&json.to_string()).unwrap()
    }

    fn plan_one(content: &str, quotes: &[&str]) -> Planned {
        let read = |rel: &str| (rel == "wiki/entities/Orbit App.md").then(|| LIST_PAGE.to_string());
        let (planned, dropped) = plan(&pdf(), &one(content, quotes), read, resolve, &SummaryCheck::NONE, "2026-10-03", page_budget(1));
        assert!(dropped.is_empty(), "{dropped:?}");
        planned.into_iter().next().unwrap()
    }

    #[test]
    fn changes_say_what_isnt_backed_and_what_they_take_away() {
        let keep = "- Soft launch to staff planned. ^q3\n- Staff launch now targeted for 28 November.";
        let takes = |p: &Planned| removes_text(p.before.as_deref().unwrap(), &p.after);
        // Every quote found, nothing taken away.
        let p = plan_one(keep, &["Staff launch now targeted for 28 November"]);
        assert!(p.warnings.is_empty() && !takes(&p), "{}", p.after);
        // One of two quotes missing: the claim stays, as a warning (a flag in Changes), and only
        // the checked one is kept for the page's claims file.
        let p = plan_one(keep, &["Staff launch now targeted for 28 November", "Northwind supplies the hardware"]);
        assert_eq!(p.claims.len(), 1);
        assert_eq!(p.quotes.len(), 1);
        assert!(p.warnings[0].contains("Northwind supplies the hardware"));
        // The section's text replaced, or a line changed: the user's line would go.
        assert!(takes(&plan_one("- Staff launch now targeted for 28 November.", &["Staff launch now targeted for 28 November"])));
        assert!(takes(&plan_one("- Soft launch to staff planned for 28 November. ^q3", &["Staff launch now targeted for 28 November"])));
    }

    #[test]
    fn what_counts_as_taking_text_away() {
        let page = "---\ntype: entity\nupdated: 2026-09-01\nsources: [\"[[A]]\"]\n---\n# X\n\n- one\n\n- two\n";
        // A line added, a source added, the date bumped, a blank line gone: nothing taken.
        let ok = "---\ntype: entity\nupdated: 2026-10-03\nsources: [\"[[A]]\", \"[[B]]\"]\n---\n# X\n\n- one\n- two\n- three\n";
        assert!(!removes_text(page, ok));
        assert!(!removes_text(&page.replace('\n', "\r\n"), ok));
        assert!(removes_text(page, &page.replace("- two\n", "")));
        assert!(removes_text(page, &page.replace("- two", "- 2")));
        assert!(removes_text(page, &page.replace("type: entity", "type: concept")));
        assert!(removes_text(page, &page.replace("sources: [\"[[A]]\"]", "sources: [\"[[B]]\"]")));
    }
}

#[cfg(test)]
mod tracking_tests {
    use super::*;
    use crate::initiatives as ini;

    const LAUNCH: &str = "---\ntype: initiative\nname: Orbit launch\nstatus: active\nsources: []\n---\n\nGetting the Orbit App out.\n\n## Current state\n\nOn track.\n\n## Actions\n\n- Draft the support FAQ [owner:: me] [status:: open] ^a-1111\n\n## Decisions\n\n- 2026-09-18 — Staff first ^d-1111\n";
    const MAIL: &str = "Maya: Sam, can you hand the support FAQ to Lena? Lena: Happy to take the FAQ, I'll have it by Friday.\nMaya: Also, please book the launch room for 14 November.\nWe agreed to launch on 14 November.\nSomeone should water the office plants.\n";

    fn src() -> Source {
        Source { rel: "sources/Email. Launch prep - 2026-10-02.md".into(), text: SourceText::Text(MAIL.into()) }
    }

    fn tracking() -> Tracking {
        Tracking { initiatives: vec![ini::parse("wiki/initiatives/Orbit launch.md", LAUNCH)], me: Some("Sam".into()) }
    }

    fn read(rel: &str) -> Option<String> {
        (rel == "wiki/initiatives/Orbit launch.md").then(|| LAUNCH.to_string())
    }

    fn planned(json: &str) -> (Vec<Planned>, Vec<Dropped>) {
        let a = parse_answer(json).unwrap();
        plan_tracking(&src(), &a, &tracking(), read, "run-1", "2026-10-02", (vec![], vec![]))
    }

    #[test]
    fn a_chat_day_gets_no_summary_page() {
        let chat = Source {
            rel: "sources/slack/Slack. Chat. Lena - 2026-11-16.md".into(),
            text: SourceText::Text("Lena: the launch moves.".into()),
        };
        assert!(
            chat.is_chat_day()
                && Source { rel: "sources/Teams. Chat. Standup - 2026-11-16.md".into(), text: SourceText::Text(String::new()) }
                    .is_chat_day()
        );
        assert!(!src().is_chat_day());
        assert!(prompt(&chat, &[], "", "Friday", &Tracking::default()).contains("give no summary page"));
        let a =
            parse_answer(r#"{"summary": {"page": "wiki/summaries/Slack. Chat. Lena - 2026-11-16", "content": "x"}, "pages": []}"#).unwrap();
        let (out, dropped) = plan(&chat, &a, |_| None, |_| None, &SummaryCheck::NONE, "2026-11-18", 6000);
        assert!(out.is_empty());
        assert_eq!(dropped[0].reason, "A day of a chat gets no summary page.");
    }

    #[test]
    fn the_prompt_names_the_user_and_lists_open_actions_by_id() {
        let p = prompt(&src(), &[], "", "Friday 2 October 2026", &tracking());
        assert!(p.contains("The user is Sam."), "{p}");
        assert!(p.contains("## wiki/initiatives/Orbit launch.md — Orbit launch (active)"), "{p}");
        assert!(p.contains("- [a-1111] Draft the support FAQ (the user's)"), "{p}");
        assert!(p.contains("- Decided 2026-09-18: Staff first"), "{p}");
    }

    #[test]
    fn a_handed_on_action_is_reassigned_and_a_new_one_added() {
        let (out, dropped) = planned(
            r#"{"actions": [
              {"initiative": "wiki/initiatives/Orbit launch.md", "id": "a-1111", "owner": "delegated", "person": "Lena", "due": "2026-10-09", "quote": "Lena: Happy to take the FAQ, I'll have it by Friday."},
              {"initiative": "Orbit launch", "text": "Book the launch room", "owner": "me", "quote": "please book the launch room for 14 November"}
            ], "decisions": [
              {"initiative": "wiki/initiatives/Orbit launch.md", "date": "2026-10-02", "text": "Launch on 14 November", "quote": "We agreed to launch on 14 November."},
              {"initiative": "wiki/initiatives/Orbit launch.md", "text": "Staff first", "quote": "We agreed to launch on 14 November."}
            ]}"#,
        );
        assert_eq!(out.len(), 1, "{out:?}");
        let p = &out[0];
        assert_eq!((p.page.as_str(), p.kind), ("wiki/initiatives/Orbit launch.md", Kind::Edit));
        let i = ini::parse(&p.page, &p.after);
        assert_eq!(i.actions.len(), 2, "{}", p.after);
        let faq = &i.actions[0];
        assert_eq!(
            (faq.id.as_str(), faq.owner, faq.person.as_deref(), faq.due.as_deref()),
            ("a-1111", Some(ini::Owner::Delegated), Some("Lena"), Some("2026-10-09"))
        );
        assert_eq!(faq.quotes[0].source, "Email. Launch prep - 2026-10-02");
        assert_eq!((i.actions[1].text.as_str(), i.actions[1].owner), ("Book the launch room", Some(ini::Owner::Me)));
        assert_eq!(i.decisions[0].text, "Launch on 14 November");
        assert_eq!(i.decisions.len(), 2, "the decision already there isn't added again");
        assert!(p.after.contains("sources: [\"[[Email. Launch prep - 2026-10-02]]\"]"), "{}", p.after);
        assert!(p.quotes.iter().all(|q| q.checked == Some(true)) && p.quotes.len() == 3, "{:?}", p.quotes);
        assert_eq!(dropped.len(), 1);
        assert!(dropped[0].reason.contains("It's there already"), "{dropped:?}");
        assert!(p.title.contains("updated “a-1111”") && p.title.contains("added “Book the launch room”"), "{}", p.title);
    }

    #[test]
    fn an_action_that_fits_no_initiative_goes_to_unfiled_and_a_bad_quote_is_dropped() {
        let (out, dropped) = planned(
            r#"{"actions": [
              {"initiative": null, "text": "Water the office plants", "owner": "other", "person": "Someone", "quote": "Someone should water the office plants."},
              {"initiative": "wiki/initiatives/Orbit launch.md", "text": "Order cake", "owner": "me", "quote": "Sam will order the cake"},
              {"initiative": "Nowhere", "text": "x", "owner": "me", "quote": "Someone should water"},
              {"initiative": "Orbit launch", "text": "y", "owner": "boss", "quote": "Someone should water"}
            ]}"#,
        );
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!((out[0].page.as_str(), out[0].kind), (ini::UNFILED, Kind::New));
        let u = ini::parse(ini::UNFILED, &out[0].after);
        assert_eq!((u.name.as_str(), u.actions[0].text.as_str()), ("Unfiled", "Water the office plants"));
        let why: Vec<&str> = dropped.iter().map(|d| d.reason.as_str()).collect();
        assert!(why[0].contains("isn't in the source"), "{why:?}");
        assert!(why[1].contains("no initiative called Nowhere"), "{why:?}");
        assert!(why[2].contains("isn't an owner"), "{why:?}");
    }

    #[test]
    fn a_page_change_and_its_actions_are_one_change() {
        let a = parse_answer(
            r#"{"actions": [{"initiative": "Orbit launch", "id": "a-1111", "status": "done", "quote": "Happy to take the FAQ"}]}"#,
        )
        .unwrap();
        let earlier = Planned {
            page: "wiki/initiatives/Orbit launch.md".into(),
            kind: Kind::Edit,
            title: "Launch date set".into(),
            before: Some(LAUNCH.into()),
            after: LAUNCH.replace("On track.", "Launching 14 November."),
            quotes: vec![],
            claims: vec![],
            entry: None,
            warnings: vec![],
        };
        let (out, _) = plan_tracking(&src(), &a, &tracking(), read, "r", "2026-10-02", (vec![earlier], vec![]));
        assert_eq!(out.len(), 1);
        assert!(out[0].after.contains("Launching 14 November.") && out[0].after.contains("[status:: done] ^a-1111"), "{}", out[0].after);
        assert_eq!(out[0].title, "Launch date set; updated “a-1111”");
    }
}
