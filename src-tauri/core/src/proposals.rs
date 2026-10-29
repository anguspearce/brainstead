// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What agents' changes are made of: the patch a model sends (`Patch`, `patched`), the quotes it
//! cites and their check, and the rules for new pages and notes. Changes themselves are in
//! `changes` (decided 2026-10-05).
//!
//! The old review queue's proposals (`Proposal`, `Store`, `proposals/<id>.json`) are kept only to
//! move them into Changes once (`changes::migrate`); remove them after one release.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::markdown;
use crate::write::{join_lines, split_lines, version, write_atomic};

/// The folder in the app data folder.
pub const DIR: &str = "proposals";
/// Unchanged lines kept on each side of a hunk to find it again.
const CONTEXT: usize = 2;
/// Accepted or rejected proposals are kept this long, for "Applied this week" and undo.
const KEEP_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A change to a page that exists.
    Edit,
    /// A page that doesn't exist yet.
    New,
    /// A task added to a list or project.
    Task,
    /// A note renamed in its folder (to `to`), with the links to it rewritten.
    Rename,
    /// A note moved to Brainstead's Trash.
    Trash,
    /// A note put back from Brainstead's Trash.
    Restore,
    /// An image added to `images/` (add_image).
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Applied,
    Rejected,
}

/// Where a proposal came from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    /// chat (a model in Ask or a terminal), ingest, meeting, lint (Knowledge health), review (a
    /// summary) or contradiction.
    pub kind: String,
    /// The Ask chat it came from, when Brainstead started the CLI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    /// For the list: "Ghost link"; a review's is its `log.md` detail, `daily-summary 2026-10-02`,
    /// which the window shows as "Daily summary 2026-10-02".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The run it was part of, for Changes' groups: an ingest, the daily check, a summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// `scheduled` for a scheduled run, or a terminal session that said it's unattended; else
    /// (None) the user started it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    /// A terminal session's folder on the computer (its working folder), when it's known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The project that folder belongs to, by its note's path (`projects::for_folder`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The model the session said it runs, when it said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// A passage a change rests on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    /// The source, as a vault path or a note's name.
    pub source: String,
    /// A heading or `page=6` in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    pub text: String,
    /// Some(true) when found in the source; None when the source isn't text Brainstead reads yet
    /// (a PDF until stage 7b).
    #[serde(default)]
    pub checked: Option<bool>,
    /// The source's vault path, when it was found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub id: String,
    /// Local time, `YYYY-MM-DDTHH:MM:SS`.
    pub created: String,
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Vault-relative.
    pub page: String,
    pub kind: Kind,
    /// One line for the list: "Update current state from the steerco".
    pub title: String,
    #[serde(default)]
    pub reason: String,
    /// The page's version (`write::version`) when proposed; None for a new page.
    #[serde(default)]
    pub base: Option<String>,
    /// The page's text when proposed ("" for a new page) and what it would become.
    pub before: String,
    pub after: String,
    #[serde(default)]
    pub quotes: Vec<Quote>,
    /// Things worth knowing that didn't stop it: links to pages that don't exist.
    #[serde(default)]
    pub warnings: Vec<String>,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided: Option<String>,
    /// The hunks applied, by index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied: Vec<usize>,
    /// For undo: the page's text before it was applied (None when applying created it) and its
    /// version after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<Applied>,
    /// For a rename: the new path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub before: Option<String>,
    pub version: String,
    /// `log.md` before its line was added, and its version after, when applying added one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<LogUndo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogUndo {
    pub before: String,
    pub version: String,
}

/// One change, with the unchanged lines around it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub index: usize,
    /// Where `old` starts in the page as proposed, from 0.
    pub at: usize,
    pub old: Vec<String>,
    pub new: Vec<String>,
    pub ctx_before: Vec<String>,
    pub ctx_after: Vec<String>,
    /// The heading it falls under ("frontmatter" before the body), for the diff's label.
    pub section: String,
    /// Its context reaches the start, or the end, of the page.
    pub from_start: bool,
    pub to_end: bool,
}

/// The line-by-line changes from `before` to `after`.
pub fn hunks(before: &str, after: &str) -> Vec<Hunk> {
    let (old, _, _) = split_lines(before);
    let (new, _, _) = split_lines(&after.replace("\r\n", "\n"));
    let ops = similar::capture_diff_slices(similar::Algorithm::Myers, &old, &new);
    let mut out: Vec<Hunk> = Vec::new();
    // Runs of changes between equal runs: (old start, old end, new start, new end).
    let mut runs: Vec<(usize, usize, usize, usize)> = Vec::new();
    for op in &ops {
        if let similar::DiffOp::Equal { .. } = op {
            continue;
        }
        let (o, n) = (op.old_range(), op.new_range());
        match runs.last_mut() {
            Some(r) if r.1 == o.start && r.3 == n.start => {
                r.1 = o.end;
                r.3 = n.end;
            }
            _ => runs.push((o.start, o.end, n.start, n.end)),
        }
    }
    let sections = section_names(&old);
    for (i, &(os, oe, ns, ne)) in runs.iter().enumerate() {
        // Context never reaches into the next or previous change, so applying one hunk never
        // changes another's context.
        let prev_end = if i == 0 { 0 } else { runs[i - 1].1 };
        let next_start = runs.get(i + 1).map_or(old.len(), |r| r.0);
        let cb = os.saturating_sub(CONTEXT).max(prev_end);
        let ca = (oe + CONTEXT).min(next_start);
        out.push(Hunk {
            index: i,
            at: os,
            old: old[os..oe].to_vec(),
            new: new[ns..ne].to_vec(),
            ctx_before: old[cb..os].to_vec(),
            ctx_after: old[oe..ca].to_vec(),
            section: sections.get(os.min(old.len().saturating_sub(1))).cloned().unwrap_or_default(),
            from_start: cb == 0,
            to_end: ca == old.len(),
        });
    }
    out
}

/// The heading each line falls under; "frontmatter" for the properties, "" before any heading.
fn section_names(lines: &[String]) -> Vec<String> {
    let text = lines.join("\n");
    let parsed = markdown::lines(&text, 0);
    let fm_end = if lines.first().is_some_and(|l| l.trim_end() == "---") {
        lines.iter().skip(1).position(|l| l.trim_end() == "---" || l.trim_end() == "...").map(|i| i + 2)
    } else {
        None
    };
    let mut cur = String::new();
    let mut out = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        if fm_end.is_some_and(|e| i < e) {
            out.push("frontmatter".to_string());
            continue;
        }
        if !parsed.get(i).is_some_and(|p| p.code) {
            if let Some((_, t)) = markdown::heading(l) {
                cur = t.to_string();
            }
        }
        out.push(cur.clone());
    }
    out
}

/// Whether each hunk still fits the page as it is now: Some(true) when its old lines are there,
/// Some(false) when its new lines are (already applied), None when it's stale.
pub fn fits(current: &str, hunks: &[Hunk]) -> Vec<Option<bool>> {
    let (lines, _, _) = split_lines(current);
    hunks
        .iter()
        .map(|h| {
            // An addition (no old lines) matches wherever its context is, applied or not: when its
            // new lines are there, it's applied, or a second accept would add them twice.
            if h.old.is_empty() && locate(&lines, h, true).is_some() {
                return Some(false);
            }
            if locate(&lines, h, false).is_some() {
                Some(true)
            } else {
                locate(&lines, h, true).map(|_| false)
            }
        })
        .collect()
}

/// Whether every change in a proposal is already in `current` (none fits, none is stale): it
/// has nothing left to apply.
pub fn already_in(current: &str, before: &str, after: &str) -> bool {
    let hs = hunks(before, after);
    !hs.is_empty() && fits(current, &hs).iter().all(|f| *f == Some(false))
}

/// Where the hunk's old lines (or, with `applied`, its new ones) are, with its context around
/// them: the match nearest where it was. A hunk whose context reached the start or end of the
/// page has to be there still.
fn locate(lines: &[String], h: &Hunk, applied: bool) -> Option<usize> {
    let body = if applied { &h.new } else { &h.old };
    let (cb, ca) = (h.ctx_before.len(), h.ctx_after.len());
    let span = cb + body.len() + ca;
    if span > lines.len() {
        return None;
    }
    let mut best: Option<usize> = None;
    for s in 0..=lines.len() - span {
        if (h.from_start && s != 0) || (h.to_end && s + span != lines.len()) {
            continue;
        }
        let p = s + cb;
        if lines[s..p] != h.ctx_before[..] || lines[p..p + body.len()] != body[..] || lines[p + body.len()..s + span] != h.ctx_after[..] {
            continue;
        }
        if best.is_none_or(|b| p.abs_diff(h.at) < b.abs_diff(h.at)) {
            best = Some(p);
        }
    }
    best
}

/// The page with the chosen hunks applied to it as it is now. Returns the text and the hunks
/// that no longer fit (stale), which are left out.
pub fn apply(current: &str, hunks: &[Hunk], accept: &[usize]) -> (String, Vec<usize>) {
    let (mut lines, eol, trailing) = split_lines(current);
    let trailing = trailing || current.is_empty();
    let mut stale = Vec::new();
    let mut places: Vec<(usize, &Hunk)> = Vec::new();
    for h in hunks.iter().filter(|h| accept.contains(&h.index)) {
        match locate(&lines, h, false) {
            Some(p) => places.push((p, h)),
            None => stale.push(h.index),
        }
    }
    // Bottom up, so earlier places stay where they are.
    places.sort_by_key(|p| std::cmp::Reverse(p.0));
    for (p, h) in places {
        lines.splice(p..p + h.old.len(), h.new.iter().cloned());
    }
    (join_lines(&lines, &eol, trailing), stale)
}

/// What a model sends as its change.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Patch {
    /// A section's new text, under its heading (added at the end when there's no such heading).
    Section { section: String, content: String },
    /// Exact find and replace pairs, each found once.
    Replace { edits: Vec<Edit> },
    /// The whole page.
    Content { content: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub find: String,
    pub replace: String,
}

/// The page with the patch applied.
pub fn patched(before: &str, patch: &Patch) -> Result<String, String> {
    let text = before.replace("\r\n", "\n");
    match patch {
        Patch::Content { content } => Ok(content.replace("\r\n", "\n")),
        Patch::Replace { edits } => {
            if edits.is_empty() {
                return Err("No edits given.".into());
            }
            let mut t = text;
            for e in edits {
                let find = e.find.replace("\r\n", "\n");
                if find.is_empty() {
                    return Err("An edit's find text is empty.".into());
                }
                match t.matches(&find).count() {
                    1 => t = t.replacen(&find, &e.replace.replace("\r\n", "\n"), 1),
                    0 => return Err(format!("This text isn't in the page: {}", short(&find))),
                    n => return Err(format!("This text is in the page {n} times; give more of it so it's found once: {}", short(&find))),
                }
            }
            Ok(t)
        }
        Patch::Section { section, content } => Ok(with_section(&text, section, content)),
    }
}

fn short(s: &str) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 80 {
        format!("“{}…”", one.chars().take(80).collect::<String>())
    } else {
        format!("“{one}”")
    }
}

/// Replaces the body of the section under `heading` (to the next heading of the same or a higher
/// level), or adds the section at the end: above a closing See also (Related, References), which
/// stays last.
fn with_section(text: &str, heading: &str, content: &str) -> String {
    let (mut lines, eol, trailing) = split_lines(text);
    let want = heading.trim().trim_start_matches('#').trim();
    let joined = lines.join("\n");
    let code: Vec<bool> = markdown::lines(&joined, 0).iter().map(|l| l.code).collect();
    let head = |i: usize| if code.get(i).copied().unwrap_or(false) { None } else { markdown::heading(&lines[i]) };
    let body: Vec<String> = content.replace("\r\n", "\n").trim_matches('\n').split('\n').map(str::to_string).collect();
    let found = (0..lines.len()).find(|&i| head(i).is_some_and(|(_, t)| t.eq_ignore_ascii_case(want)));
    match found {
        Some(h) => {
            let level = head(h).map(|(n, _)| n).unwrap_or(2);
            let end = (h + 1..lines.len()).find(|&i| head(i).is_some_and(|(n, _)| n <= level)).unwrap_or(lines.len());
            let mut fill = Vec::new();
            if lines.get(h + 1).is_some_and(|l| l.trim().is_empty()) || h + 1 == end {
                fill.push(String::new());
            }
            fill.extend(body);
            if end < lines.len() {
                fill.push(String::new());
            }
            lines.splice(h + 1..end, fill);
        }
        None => {
            // A summing-up section goes first, under the page's opening text; any other goes above
            // a closing See also (Related, References), wherever that is.
            let top = (0..lines.len()).find(|&i| head(i).is_some_and(|(n, _)| n == 2));
            let summing = ["current state", "summary", "overview", "status"].contains(&want.to_lowercase().as_str());
            let closing = (0..lines.len()).rev().find(|&i| {
                head(i).is_some_and(|(n, t)| n <= 2 && ["see also", "related", "references"].contains(&t.trim().to_lowercase().as_str()))
            });
            let at = if summing { top.or(closing) } else { closing };
            let mut tail = at.map(|i| lines.split_off(i)).unwrap_or_default();
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!("## {want}"));
            lines.push(String::new());
            lines.extend(body);
            if !tail.is_empty() {
                lines.push(String::new());
                lines.append(&mut tail);
            }
        }
    }
    join_lines(&lines, &eol, trailing || text.is_empty())
}

/// What a source's text is, for checking quotes.
pub enum SourceText {
    Text(String),
    /// A PDF or Office file's text by page, slide or sheet.
    Parts(crate::extract::Extracted),
    /// A file Brainstead doesn't read the text of yet (PDF, Office).
    Unchecked,
    Missing,
}

/// Quote matching: case, accents' composition, curly quotes and dashes, emphasis marks and
/// spacing don't count.
pub fn normalise_quote(s: &str) -> String {
    let t: String = s
        .nfkc()
        .map(|c| match c {
            '‘' | '’' | '‚' | '′' => '\'',
            '“' | '”' | '„' | '″' => '"',
            '–' | '—' | '‐' | '‑' | '−' => '-',
            '\u{a0}' => ' ',
            c => c,
        })
        .filter(|c| !matches!(c, '*' | '_' | '`'))
        .collect();
    t.replace('…', "...").split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Checks each quote against its source, marking it `checked` (Some(false) when it isn't there),
/// and says what's wrong with each that isn't: a change resting on it still applies, flagged
/// (decided 2026-10-05). Err only for a quote with no text.
pub fn mark_quotes(quotes: &mut [Quote], source: impl Fn(&str) -> (Option<String>, SourceText)) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for q in quotes.iter_mut() {
        if q.text.trim().is_empty() {
            return Err(format!("A quote from {} is empty.", q.source));
        }
        let (path, text) = source(&q.source);
        q.path = path;
        let name = q.path.clone().unwrap_or_else(|| q.source.clone());
        let missing = match text {
            SourceText::Missing => Some(format!("There's no source called {} in the vault.", q.source)),
            SourceText::Unchecked => {
                q.checked = None;
                continue;
            }
            SourceText::Parts(x) => {
                let page = q.anchor.as_deref().and_then(page_anchor);
                match page.map(|n| x.page(n).map(str::to_string).ok_or(n)).unwrap_or_else(|| Ok(x.text())) {
                    Err(n) => Some(format!("{name} has no page {n}.")),
                    Ok(hay) if !normalise_quote(&hay).contains(&normalise_quote(&q.text)) => {
                        let where_ = page.map(|n| format!(" on page {n}")).unwrap_or_default();
                        Some(format!("This quote isn't in {name}{where_}: {}. Quote the source word for word.", short(&q.text)))
                    }
                    Ok(_) => None,
                }
            }
            SourceText::Text(t) => (!normalise_quote(&t).contains(&normalise_quote(&q.text)))
                .then(|| format!("This quote isn't in {name}: {}. Quote the source word for word.", short(&q.text))),
        };
        q.checked = Some(missing.is_none());
        problems.extend(missing);
    }
    Ok(problems)
}

/// `page=6`, `page 6`, `Page 6` or `p. 6` as a page number.
pub fn page_anchor(a: &str) -> Option<usize> {
    let t = a.trim().to_ascii_lowercase();
    let n =
        t.strip_prefix("page=").or_else(|| t.strip_prefix("page ")).or_else(|| t.strip_prefix("p. ")).or_else(|| t.strip_prefix("p."))?;
    n.trim().parse().ok().filter(|n: &usize| *n > 0)
}

static WIKILINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]|#\n]+?)(?:[|#][^\]\n]*)?\]\]").unwrap());

/// Links in `after` that aren't in `before` and go nowhere.
pub fn new_ghost_links(before: &str, after: &str, resolves: impl Fn(&str) -> bool) -> Vec<String> {
    let had: std::collections::HashSet<String> = WIKILINK.captures_iter(before).map(|c| c[1].trim().to_string()).collect();
    let mut out: Vec<String> = Vec::new();
    for c in WIKILINK.captures_iter(after) {
        let t = c[1].trim().to_string();
        if !had.contains(&t) && !out.contains(&t) && !resolves(&t) {
            out.push(t);
        }
    }
    out
}

/// `updated:` set to `today` when the page has one and the change didn't set it.
pub fn bump_updated(before: &str, after: &str, today: &str) -> String {
    let fm = crate::frontmatter::split(before);
    let had = fm.data.get("updated").is_some();
    let now = crate::frontmatter::split(after);
    if !had || now.data.get("updated") != fm.data.get("updated") {
        return after.to_string();
    }
    crate::write::with_property(after, "updated", Some(today)).unwrap_or_else(|_| after.to_string())
}

/// Whether `rel` is a sensible new wiki page: `wiki/<folder>/<Name>.md`, nothing odd in it.
pub fn valid_new_page(rel: &str) -> Result<(), String> {
    crate::trash::safe_rel(rel).map_err(|e| e.to_string())?;
    let segs: Vec<&str> = rel.split('/').collect();
    let ok = segs.len() >= 3
        && segs[0] == "wiki"
        && rel.ends_with(".md")
        && segs.iter().all(|s| !s.is_empty() && !s.starts_with('.'))
        && !rel.contains(['\\', ':', '*', '?', '"', '<', '>', '|', '\n']);
    if !ok {
        return Err(format!(
            "A new wiki page goes in wiki/entities/<Name>.md, wiki/concepts/<Name>.md or wiki/summaries/<Name>.md, not {rel}. A new note is create_note."
        ));
    }
    Ok(())
}

/// Folders a new note can't go in: captured sources, the wiki (its pages have their own rules,
/// `valid_new_page`) and the templates.
pub const NOT_FOR_NOTES: [&str; 3] = ["sources", "wiki", "Templates"];

/// Where a new note may go: a `.md` file outside `sources/`, `wiki/`, `Templates/` and hidden
/// folders.
pub fn valid_new_note(rel: &str) -> Result<(), String> {
    crate::trash::safe_rel(rel).map_err(|e| e.to_string())?;
    let segs: Vec<&str> = rel.split('/').collect();
    let first = segs.first().copied().unwrap_or("");
    if segs.len() > 1 && NOT_FOR_NOTES.iter().any(|n| n.eq_ignore_ascii_case(first)) {
        return Err(format!("A note can't go in {first}/: not in sources/, wiki/ (edit_page makes wiki pages) or Templates/."));
    }
    if !rel.ends_with(".md")
        || segs.iter().any(|s| s.is_empty() || s.starts_with('.'))
        || rel.contains(['\\', ':', '*', '?', '"', '<', '>', '|', '\n'])
    {
        return Err(format!("{rel} isn't a name a note can have."));
    }
    Ok(())
}

/// The proposals folder.
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(data_dir: &Path) -> Self {
        Store { dir: data_dir.join(DIR) }
    }

    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(format!("Not a proposal id: {id}"));
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    pub fn save(&self, p: &Proposal) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(p).map_err(|e| e.to_string())?;
        write_atomic(&self.path(&p.id)?, &json, false).map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Result<Proposal, String> {
        let b = std::fs::read(self.path(id)?).map_err(|_| format!("There's no proposal {id} any more."))?;
        serde_json::from_slice(&b).map_err(|e| e.to_string())
    }

    /// Every proposal, newest first. Decided ones older than 30 days are deleted on the way.
    pub fn list(&self, today: chrono::NaiveDate) -> Vec<Proposal> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return vec![] };
        let mut out: Vec<Proposal> = Vec::new();
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let Some(prop) = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Proposal>(&b).ok()) else { continue };
            let old = prop.status != Status::Pending
                && prop
                    .decided
                    .as_deref()
                    .and_then(|d| chrono::NaiveDate::parse_from_str(d.get(..10)?, "%Y-%m-%d").ok())
                    .is_some_and(|d| (today - d).num_days() > KEEP_DAYS);
            if old {
                let _ = std::fs::remove_file(&p);
                continue;
            }
            out.push(prop);
        }
        out.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| b.id.cmp(&a.id)));
        out
    }
}

/// A new id: the time, then enough to tell two from one moment apart.
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("{ms}-{}-{}", std::process::id() % 100_000, SEQ.fetch_add(1, Ordering::Relaxed))
}

/// The local time now, as proposals keep it.
pub fn now_local() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// A proposal ready to save, from the page as it is (`before`, None when it's new) and the text it
/// would become. Refuses a change that changes nothing or breaks the properties.
pub struct Draft<'a> {
    pub page: &'a str,
    pub kind: Kind,
    pub title: &'a str,
    pub reason: &'a str,
    pub origin: Origin,
    pub model: Option<String>,
    pub before: Option<&'a str>,
    pub after: String,
    pub quotes: Vec<Quote>,
    pub warnings: Vec<String>,
}

/// The property a new note from an assistant carries, so its scripts don't run when it's drawn
/// (src/md/scripts.ts). Wiki pages don't need it: no script runs in `wiki/`.
pub const ASSISTANT_MARK: (&str, &str) = ("created-by", "assistant");

/// Whether a new page from an assistant gets the mark: not a wiki page, and not a template, whose
/// notes would all carry it.
pub fn gets_mark(page: &str) -> bool {
    !page.starts_with("wiki/") && !page.starts_with("Templates/")
}

pub fn make(mut d: Draft) -> Result<Proposal, String> {
    if d.kind == Kind::New && gets_mark(d.page) {
        d.after = crate::write::with_property(&d.after, ASSISTANT_MARK.0, Some(ASSISTANT_MARK.1)).map_err(|e| e.to_string())?;
    }
    let before = d.before.unwrap_or("");
    // A rename or a move to the Trash leaves the text as it is.
    let moves = matches!(d.kind, Kind::Rename | Kind::Trash);
    if !moves && d.after.replace("\r\n", "\n") == before.replace("\r\n", "\n") {
        return Err("That changes nothing in the page.".into());
    }
    let fm = crate::frontmatter::split(&d.after);
    if let Some(err) = fm.error {
        if crate::frontmatter::split(before).error.is_none() {
            return Err(format!("The page's properties wouldn't read any more: {err}"));
        }
    }
    let title = d.title.split_whitespace().collect::<Vec<_>>().join(" ");
    Ok(Proposal {
        id: new_id(),
        created: now_local(),
        origin: d.origin,
        model: d.model,
        page: d.page.to_string(),
        kind: d.kind,
        title: if title.is_empty() { "Change".into() } else { title },
        reason: d.reason.trim().to_string(),
        base: d.before.map(|b| version(b.as_bytes())),
        before: before.to_string(),
        after: d.after,
        quotes: d.quotes,
        warnings: d.warnings,
        status: Status::Pending,
        decided: None,
        applied: vec![],
        undo: None,
        to: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "---\ntype: entity\nupdated: 2026-09-01\n---\n\n# Orbit App\n\n## Current state\n\nIn build.\nLaunch planned for 14 November.\n\n## History\n\n- Started in May.\n";

    #[test]
    fn section_patch_replaces_its_body() {
        let p = Patch::Section { section: "Current state".into(), content: "In test.\nLaunch planned for 28 November.\n".into() };
        let after = patched(PAGE, &p).unwrap();
        assert!(after.contains("## Current state\n\nIn test.\nLaunch planned for 28 November.\n\n## History"));
        // A heading that isn't there is added at the end.
        let p = Patch::Section { section: "## Risks".into(), content: "- Pen test.".into() };
        assert!(patched(PAGE, &p).unwrap().ends_with("- Started in May.\n\n## Risks\n\n- Pen test.\n"));
        // Above a closing See also, which stays last.
        let page = format!("{PAGE}\n## See also\n\n- [[Lena]]\n");
        assert!(patched(&page, &p).unwrap().ends_with("- Started in May.\n\n## Risks\n\n- Pen test.\n\n## See also\n\n- [[Lena]]\n"));
        // Above See also even when older sections were left below it.
        let page = format!("{PAGE}\n## See also\n\n- [[Lena]]\n\n## Old note\n\nKept.\n");
        assert!(patched(&page, &p).unwrap().contains("## Risks\n\n- Pen test.\n\n## See also\n\n- [[Lena]]\n\n## Old note"));
        // A new Current state goes under the opening text, not at the end.
        let page = "# Lena\n\nLeads design.\n\n## Sep 2026\n\nJoined.\n\n## See also\n\n- [[Maya]]\n";
        let cs = Patch::Section { section: "Current state".into(), content: "Leads design at Acme.".into() };
        assert_eq!(
            patched(page, &cs).unwrap(),
            "# Lena\n\nLeads design.\n\n## Current state\n\nLeads design at Acme.\n\n## Sep 2026\n\nJoined.\n\n## See also\n\n- [[Maya]]\n"
        );
    }

    #[test]
    fn replace_patch_needs_one_match() {
        let ok = Patch::Replace { edits: vec![Edit { find: "In build.".into(), replace: "In test.".into() }] };
        assert!(patched(PAGE, &ok).unwrap().contains("In test."));
        let none = Patch::Replace { edits: vec![Edit { find: "In flight".into(), replace: "x".into() }] };
        assert!(patched(PAGE, &none).unwrap_err().contains("isn't in the page"));
        let two = Patch::Replace { edits: vec![Edit { find: "## ".into(), replace: "### ".into() }] };
        assert!(patched(PAGE, &two).unwrap_err().contains("2 times"));
    }

    #[test]
    fn patch_json_shapes() {
        let s: Patch = serde_json::from_str(r#"{"section":"A","content":"b"}"#).unwrap();
        assert!(matches!(s, Patch::Section { .. }));
        let r: Patch = serde_json::from_str(r#"{"edits":[{"find":"a","replace":"b"}]}"#).unwrap();
        assert!(matches!(r, Patch::Replace { .. }));
        let c: Patch = serde_json::from_str(r#"{"content":"x"}"#).unwrap();
        assert!(matches!(c, Patch::Content { .. }));
    }

    #[test]
    fn hunks_apply_one_at_a_time() {
        let after = PAGE.replace("In build.", "In test.").replace("- Started in May.", "- Started in May.\n- Pen test booked.");
        let hs = hunks(PAGE, &after);
        assert_eq!(hs.len(), 2);
        assert_eq!(hs[0].section, "Current state");
        assert_eq!(hs[0].old, ["In build."]);
        assert_eq!(hs[1].new, ["- Pen test booked."]);
        // Only the second.
        let (t, stale) = apply(PAGE, &hs, &[1]);
        assert!(stale.is_empty());
        assert_eq!(t, PAGE.replace("- Started in May.", "- Started in May.\n- Pen test booked."));
        // Both give the whole change.
        assert_eq!(apply(PAGE, &hs, &[0, 1]).0, after);
        assert_eq!(fits(PAGE, &hs), [Some(true), Some(true)]);
        assert_eq!(fits(&after, &hs), [Some(false), Some(false)]);
    }

    #[test]
    fn hunks_follow_the_page_when_it_changed_elsewhere() {
        let after = PAGE.replace("In build.", "In test.");
        let hs = hunks(PAGE, &after);
        // Someone added lines above it since.
        let now = PAGE.replace("# Orbit App\n", "# Orbit App\n\nA customer app.\nOwned by Maya.\n");
        let (t, stale) = apply(&now, &hs, &[0]);
        assert!(stale.is_empty());
        assert!(t.contains("Owned by Maya.") && t.contains("In test.") && !t.contains("In build."));
        // Someone changed the line itself: stale, nothing applied.
        let edited = PAGE.replace("In build.", "In design.");
        let (t, stale) = apply(&edited, &hs, &[0]);
        assert_eq!(stale, [0]);
        assert_eq!(t, edited);
        assert_eq!(fits(&edited, &hs), [None]);
    }

    #[test]
    fn crlf_pages_keep_crlf() {
        let crlf = PAGE.replace('\n', "\r\n");
        let after = PAGE.replace("In build.", "In test.");
        let hs = hunks(&crlf, &after);
        assert_eq!(hs.len(), 1);
        let (t, _) = apply(&crlf, &hs, &[0]);
        assert_eq!(t, after.replace('\n', "\r\n"));
    }

    #[test]
    fn new_page_is_one_hunk() {
        let hs = hunks("", "---\ntype: entity\n---\n\n# Northwind\n");
        assert_eq!(hs.len(), 1);
        let (t, stale) = apply("", &hs, &[0]);
        assert!(stale.is_empty());
        assert_eq!(t, "---\ntype: entity\n---\n\n# Northwind\n");
        // A page made meanwhile with other text: stale.
        assert_eq!(apply("Other\n", &hs, &[0]).1, [0]);
    }

    #[test]
    fn quotes_are_checked() {
        let src = "The **origination** journey is now in build — portfolio APIs follow.";
        let look = |s: &str| match s {
            "Steerco" => (Some("sources/Steerco.md".to_string()), SourceText::Text(src.into())),
            "Deck.pdf" => (Some("sources/Deck.pdf".to_string()), SourceText::Unchecked),
            _ => (None, SourceText::Missing),
        };
        let mut qs = vec![
            Quote { source: "Steerco".into(), text: "origination journey is now in build - portfolio".into(), ..Default::default() },
            Quote { source: "Deck.pdf".into(), anchor: Some("page=6".into()), text: "anything".into(), ..Default::default() },
        ];
        assert!(mark_quotes(&mut qs, look).unwrap().is_empty());
        assert_eq!(qs[0].checked, Some(true));
        assert_eq!(qs[0].path.as_deref(), Some("sources/Steerco.md"));
        assert_eq!(qs[1].checked, None);
        let mut bad = vec![Quote { source: "Steerco".into(), text: "in production".into(), ..Default::default() }];
        assert!(mark_quotes(&mut bad, look).unwrap()[0].contains("isn't in sources/Steerco.md"));
        // A PDF's pages: the anchor's page must hold it.
        let pdf = crate::extract::Extracted {
            kind: "pdf".into(),
            parts: vec![
                crate::extract::Part { label: "Page 1".into(), text: "Agenda".into() },
                crate::extract::Part { label: "Page 2".into(), text: "Pen test closure by 30/10 is the gating item.".into() },
            ],
        };
        let look_pdf = |_: &str| (Some("sources/Steerco.pdf".to_string()), SourceText::Parts(pdf.clone()));
        let mut on2 = vec![Quote {
            source: "Steerco.pdf".into(),
            anchor: Some("page=2".into()),
            text: "pen test closure by 30/10".into(),
            ..Default::default()
        }];
        assert!(mark_quotes(&mut on2, look_pdf).unwrap().is_empty());
        assert_eq!(on2[0].checked, Some(true));
        let mut on1 = vec![Quote {
            source: "Steerco.pdf".into(),
            anchor: Some("page=1".into()),
            text: "pen test closure".into(),
            ..Default::default()
        }];
        assert!(mark_quotes(&mut on1, look_pdf).unwrap()[0].contains("on page 1"));
        let mut on9 = vec![Quote { source: "Steerco.pdf".into(), anchor: Some("p. 9".into()), text: "x".into(), ..Default::default() }];
        assert!(mark_quotes(&mut on9, look_pdf).unwrap()[0].contains("no page 9"));
        let mut gone = vec![Quote { source: "Nope".into(), text: "x".into(), ..Default::default() }];
        assert!(mark_quotes(&mut gone, look).unwrap()[0].contains("no source called Nope"));
    }

    #[test]
    fn updated_is_bumped_once() {
        let after = PAGE.replace("In build.", "In test.");
        let b = bump_updated(PAGE, &after, "2026-10-02");
        assert!(b.contains("updated: 2026-10-02\n") && b.contains("In test."));
        // Set by the change itself, or not a property of the page: left alone.
        let set = after.replace("updated: 2026-09-01", "updated: 2026-09-30");
        assert_eq!(bump_updated(PAGE, &set, "2026-10-02"), set);
        assert_eq!(bump_updated("# A\n", "# B\n", "2026-10-02"), "# B\n");
    }

    #[test]
    fn checks_on_making() {
        let base = || Draft {
            page: "wiki/entities/Orbit App.md",
            kind: Kind::Edit,
            title: " Update  state ",
            reason: "",
            origin: Origin { kind: "chat".into(), ..Default::default() },
            model: None,
            before: Some(PAGE),
            after: PAGE.to_string(),
            quotes: vec![],
            warnings: vec![],
        };
        assert!(make(base()).unwrap_err().contains("changes nothing"));
        let broken = Draft { after: PAGE.replace("type: entity", "type: [entity"), ..base() };
        assert!(make(broken).unwrap_err().contains("properties"));
        let ok = make(Draft { after: PAGE.replace("In build.", "In test."), ..base() }).unwrap();
        assert_eq!(ok.title, "Update state");
        assert_eq!(ok.base.as_deref(), Some(version(PAGE.as_bytes()).as_str()));
        assert_eq!(ok.status, Status::Pending);
    }

    #[test]
    fn applying_through_the_safe_write_and_undoing() {
        use crate::write::{restore_files, save_file, FileChange};
        let t = tempfile::tempdir().unwrap();
        let page = t.path().join("Orbit App.md");
        std::fs::write(&page, PAGE).unwrap();
        let after = PAGE.replace("In build.", "In test.").replace("- Started in May.", "- Started in May.\n- Pen test booked.");
        let hs = hunks(PAGE, &after);
        // Someone edits another line meanwhile; only the kept hunk lands, nothing else moves.
        let now = PAGE.replace("# Orbit App", "# Orbit App (staff)");
        std::fs::write(&page, &now).unwrap();
        let (text, stale) = apply(&now, &hs, &[1]);
        assert!(stale.is_empty());
        let v = save_file(&page, &text, &version(now.as_bytes())).unwrap();
        assert_eq!(std::fs::read_to_string(&page).unwrap(), now.replace("- Started in May.", "- Started in May.\n- Pen test booked."));
        // The safe write refuses a stale base.
        assert!(save_file(&page, "x", &version(PAGE.as_bytes())).is_err());
        restore_files(&[FileChange { path: page.clone(), before: Some(now.clone()), version: v }]).unwrap();
        assert_eq!(std::fs::read_to_string(&page).unwrap(), now);
    }

    #[test]
    fn safe_fixes_keep_the_modification_time() {
        let t = tempfile::tempdir().unwrap();
        let page = t.path().join("p.md");
        std::fs::write(&page, "a\n").unwrap();
        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::File::options().write(true).open(&page).unwrap().set_modified(old).unwrap();
        crate::write::save_file_as(&page, "b\n", &version(b"a\n"), true).unwrap();
        assert_eq!(std::fs::metadata(&page).unwrap().modified().unwrap(), old);
        assert_eq!(std::fs::read_to_string(&page).unwrap(), "b\n");
    }

    #[test]
    fn new_pages_go_in_the_wiki() {
        assert!(valid_new_page("wiki/entities/Northwind.md").is_ok());
        assert!(gets_mark("Idea. Plan.md") && !gets_mark("wiki/entities/Northwind.md") && !gets_mark("Templates/Retro.md"));
        assert!(valid_new_page("Northwind.md").is_err());
        assert!(valid_new_page("wiki/../x.md").is_err());
        assert!(valid_new_page("wiki/entities/.hidden.md").is_err());
    }

    #[test]
    fn ghost_links_in_the_change() {
        let g =
            new_ghost_links("See [[Orbit App]].", "See [[Orbit App]] and [[Northwind]] and [[Hub Platform|hub]].", |t| t != "Northwind");
        assert_eq!(g, ["Northwind"]);
    }

    #[test]
    fn store_round_trip_and_pruning() {
        let t = tempfile::tempdir().unwrap();
        let s = Store::new(t.path());
        let mut p = make(Draft {
            page: "wiki/entities/Orbit App.md",
            kind: Kind::Edit,
            title: "x",
            reason: "",
            origin: Origin::default(),
            model: None,
            before: Some(PAGE),
            after: PAGE.replace("In build.", "In test."),
            quotes: vec![],
            warnings: vec![],
        })
        .unwrap();
        s.save(&p).unwrap();
        assert_eq!(s.get(&p.id).unwrap(), p);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 2).unwrap();
        assert_eq!(s.list(today).len(), 1);
        p.status = Status::Rejected;
        p.decided = Some("2026-08-01T10:00:00".into());
        s.save(&p).unwrap();
        assert!(s.list(today).is_empty());
        assert!(s.get(&p.id).is_err());
        assert!(s.get("../x").is_err());
    }

    #[test]
    fn an_addition_already_made_is_applied_not_fitting() {
        let before = "# Day\n\n## Work\n- A\n\n## Personal\n- B\n";
        let after = "# Day\n\n## Work\n- A\n\n## Personal\n- B\n- Brainstead\n";
        let hs = hunks(before, after);
        assert_eq!(fits(before, &hs), vec![Some(true)]);
        assert_eq!(fits(after, &hs), vec![Some(false)]);
        assert!(already_in(after, before, after));
    }

    #[test]
    fn a_proposal_already_in_the_page_has_nothing_left() {
        let before = "# Log\n\nOne\nTwo\n";
        let after = "# Log\n\nOne\nTwo and a half\n";
        assert!(already_in(after, before, after));
        assert!(!already_in(before, before, after));
        assert!(!already_in("# Other\n", before, after));
        assert!(!already_in(before, before, before));
    }
}
