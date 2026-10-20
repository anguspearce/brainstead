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

/// The message to the model: the workflow, the source, the pages it mentions with their text,
/// and every wiki page by name and description (`catalogue`, `catalogue::render`), for a source
/// that doesn't name what it's about.
pub fn prompt(src: &Source, pages: &[(String, String)], catalogue: &str, today: &str) -> String {
    let mut out = format!("{}\n\nToday is {today}.\n\n# The source: {}\n\nLink it as [[{}]].\n\n", WORKFLOW.trim(), src.rel, src.link());
    out.push_str(&cut(&src.body(), SOURCE_CHARS));
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
    if src.is_journal() {
        out.push_str("\nThis source is a journal note: give no summary page.\n");
    }
    out
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

/// Checks the answer and turns it into page changes. `read` gives a page's text by vault path;
/// `resolve` a name's page.
pub fn plan(
    src: &Source,
    answer: &Answer,
    read: impl Fn(&str) -> Option<String>,
    resolve: impl Fn(&str) -> Option<String>,
    today: &str,
    budget: usize,
) -> (Vec<Planned>, Vec<Dropped>) {
    let mut out = Vec::new();
    let mut dropped = Vec::new();
    let link = src.link();
    let cite = format!("[[{link}]]");

    if let Some(s) = &answer.summary {
        let named = s.page.trim();
        let rel = wiki_page(named).unwrap_or_else(|_| md(named));
        if src.is_journal() {
            dropped.push(Dropped { page: rel, reason: "A journal note gets no summary page.".into() });
        } else if let Err(e) = wiki_page(named).and_then(|_| {
            if rel.starts_with("wiki/summaries/") {
                Ok(())
            } else {
                Err("A summary goes in wiki/summaries/.".into())
            }
        }) {
            dropped.push(Dropped { page: rel, reason: e });
        } else if read(&rel).is_some() {
            dropped
                .push(Dropped { page: rel, reason: "There's a summary page already; update it by hand or re-ingest it as a page.".into() });
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
        match read(&rel) {
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
                if let Some(t) = crate::fixname::with_list_value(&after, &["sources"], &cite) {
                    after = t;
                }
                after = proposals::bump_updated(&before, &after, today);
                if after.replace("\r\n", "\n") == before.replace("\r\n", "\n") {
                    dropped.push(Dropped { page: rel, reason: "It changed nothing.".into() });
                    continue;
                }
                out.push(Planned { page: rel, kind: Kind::Edit, title, before: Some(before), after, quotes, claims, entry, warnings });
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

#[cfg(test)]
mod tests {
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
        let (planned, dropped) = plan(&pdf(), &answer(), read, resolve, "2026-10-02", page_budget(1));
        let pages: Vec<&str> = planned.iter().map(|p| p.page.as_str()).collect();
        assert_eq!(
            pages,
            ["wiki/summaries/Programme Update Steerco 2026-09-28.md", "wiki/entities/Orbit App.md", "wiki/concepts/Launch gate.md"]
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
        let (planned, dropped) = plan(&pdf(), &shaped_answer(""), read, resolve, "2026-10-02", page_budget(1));
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
        let (p2, _) = plan(&pdf(), &a2, read2, resolve, "2026-10-02", page_budget(1));
        assert_eq!(p2[0].after.matches("Source: [[Programme Update Steerco 2026-09-28.pdf]]").count(), 1);
        assert!(p2[0].after.contains("### 2026-09-28 — Steerco: launch on 28 November confirmed\nSource: [[Programme Update Steerco 2026-09-28.pdf]]\n\nConfirmed.\n"));
    }

    #[test]
    fn a_topical_section_is_rewritten_only_when_shown_whole() {
        let read = |r: &str| (r == "wiki/entities/Orbit App.md").then(|| SHAPED.to_string());
        let a = shaped_answer(r#""section": {"heading": "Risks", "content": "- The pen test gates the launch."},"#);
        let (planned, _) = plan(&pdf(), &a, read, resolve, "2026-10-02", page_budget(1));
        assert!(planned[0].after.contains("On the Hub.\n\n## Risks\n\n- The pen test gates the launch.\n\n## Timeline"));
        // A long page whose Architecture the model saw only the heading of.
        let long = SHAPED.replace("On the Hub.", &"On the Hub, with many details. ".repeat(2000));
        let read = move |r: &str| (r == "wiki/entities/Orbit App.md").then(|| long.clone());
        let a = shaped_answer(r#""section": {"heading": "Architecture", "content": "Rewritten."},"#);
        let (planned, dropped) = plan(&pdf(), &a, read, resolve, "2026-10-02", page_budget(1));
        assert!(planned.is_empty() && dropped[0].reason.contains("Architecture"), "{dropped:?}");
    }

    #[test]
    fn a_new_page_gets_its_current_state_and_timeline() {
        let a = parse_answer(r#"{"pages": [{"page": "wiki/entities/Pen test.md", "new": true, "description": "The launch gate",
            "current_state": "Gates the launch.", "entry": {"date": "", "title": "Pending", "body": "Pending closure."},
            "claims": [{"subject": "Pen test", "attribute": "status", "value": "pending", "quote": "pending pen-test closure", "anchor": "page=2"}]}]}"#).unwrap();
        let (planned, _) = plan(&pdf(), &a, |_| None, |_| None, "2026-10-02", page_budget(1));
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
        let (planned, dropped) = plan(&note, &answer(), read, resolve, "2026-10-02", page_budget(1));
        assert!(planned.iter().all(|p| !p.page.starts_with("wiki/summaries/")));
        assert_eq!(dropped[0].reason, "A journal note gets no summary page.");
        // Without anchors to check, a markdown source's quotes are found anywhere in it.
        assert!(planned.iter().any(|p| p.page == "wiki/concepts/Pen test.md"));
        assert!(prompt(&note, &[], "", "Friday").contains("give no summary page"));
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
        let (planned, dropped) = plan(&src, &a, read, resolve, "2026-10-04", page_budget(1));
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
        assert!(prompt(&src, &[], "", "Sunday").contains("## Text read from the image (may be incomplete)\n\nOrbit App launch"));
        assert!(prompt(&image(""), &[], "", "Sunday").contains("no text was found in it"));
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
        let (planned, dropped) = plan(&pdf(), &a, read, |_| Some("Projects/Hub.md".into()), "2026-10-03", page_budget(1));
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
        let (planned, dropped) = plan(&pdf(), &one(content, quotes), read, resolve, "2026-10-03", page_budget(1));
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
