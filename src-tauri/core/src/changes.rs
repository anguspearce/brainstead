// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Agent changes (decided 2026-10-05): what assistants and Brainstead's runs did to the vault, and
//! the few changes held for the user, kept as `changes/<id>.json` in the app data folder. Nothing
//! here writes to the vault: the app applies an instruction through the safe write and records it.
//!
//! A held change keeps the agent's instruction (replace a section, find and replace, add a task,
//! the new page's text, a rename, a move to the Trash or back), not a copy of the page: accepting runs it
//! on the page as it is then, so it never goes stale; it fails only when its target is gone. An
//! applied change keeps the page's text before and after, compressed, each text stored once by its
//! hash in `changes/text/`, for the feed's diff and for Revert.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::markdown;
use crate::proposals::{self, Edit, Kind, Origin, Quote};
use crate::write::{join_lines, split_lines, write_atomic};

/// The folder in the app data folder.
pub const DIR: &str = "changes";
/// Texts, by their hash, in `changes/`.
const TEXTS: &str = "text";
/// Unchanged lines kept on each side of a hunk in the diff, and for Revert to find it again.
const CONTEXT: usize = 2;
/// A text kept this recently isn't pruned, though nothing refers to it yet.
const FRESH: std::time::Duration = std::time::Duration::from_secs(3600);
/// History kept, by default: 90 days or 500 MB, whichever comes first.
pub const KEEP_DAYS: i64 = 90;
pub const KEEP_MB: u64 = 500;

/// What an agent asked for, run on the page as it is when it's applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Instruction {
    /// A section's new text, under its heading (added at the end when there's no such heading).
    Section { section: String, content: String },
    /// Exact find and replace pairs, each found once.
    Replace { edits: Vec<Edit> },
    /// The page's whole text: a new page, or one written over.
    Page { content: String },
    /// A task line, at the end of a project's Next actions (or its `heading`, Waiting for as the
    /// Inbox files a waiting task) or the top of the To Do list's Other.
    AddTask {
        line: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        heading: Option<String>,
    },
    /// A thought at the top of the Scratchpad, under a `## stamp` heading, as Quick capture adds it
    /// (the Scratchpad is started when it isn't there).
    AddThought { text: String, stamp: String },
    /// A line taken out: a task deleted. `at` is where it was (from 0); it's taken out there when
    /// it still reads the same, else only where it's the one line that does.
    DeleteLine {
        line: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<usize>,
    },
    /// Lines rewritten: `old`, from line `at` (from 0), become `new` (a task changed, a recurring
    /// one with its next occurrence, a thought taken out of the Scratchpad). Found as `delete_line`
    /// finds its line.
    Lines { at: usize, old: Vec<String>, new: Vec<String> },
    /// A paragraph added at the end of the page (an Inbox item filed as reference).
    Append { text: String },
    /// Properties set, or taken out with None (a project's status, area or outcome).
    Properties { set: Vec<(String, Option<String>)> },
    /// The note renamed in its folder, its links rewritten.
    Rename { to: String },
    /// The note moved to Brainstead's Trash.
    Trash,
    /// A Trash entry (`id`) put back, at the change's page.
    Restore { id: String },
    /// An image saved to `images/`, at the change's page: made when it's recorded, never held.
    Image,
}

impl Instruction {
    /// The page's text once this is run on it (`current`, None when it doesn't exist). Err says why
    /// it can't be: its target is gone, or the page isn't there. Not for a rename or a move.
    pub fn text(&self, page: &str, current: Option<&str>) -> Result<String, String> {
        let name = crate::filename::stem(page);
        let have = || current.ok_or_else(|| format!("{name} isn't in the vault any more."));
        match self {
            Instruction::Page { content } => Ok(content.replace("\r\n", "\n")),
            Instruction::Section { section, content } => crate::pageshape::with_section(page, have()?, section, content),
            Instruction::Replace { edits } => proposals::patched(have()?, &proposals::Patch::Replace { edits: edits.clone() })
                .map_err(|e| e.replace("isn't in the page", "isn't on the page any more")),
            Instruction::AddTask { line, heading } => {
                let text = have()?;
                let (mut lines, eol, trailing) = split_lines(text);
                if lines.iter().any(|l| l.trim_end() == line.trim_end()) {
                    return Err(format!("That task is in {name} already."));
                }
                if page == crate::inbox::TODO_LIST {
                    crate::write::insert_other(&mut lines, line);
                } else {
                    crate::write::append_under(&mut lines, heading.as_deref().unwrap_or(crate::projects::NEXT_ACTIONS), line);
                }
                Ok(join_lines(&lines, &eol, trailing || text.is_empty()))
            }
            Instruction::AddThought { text, stamp } => {
                if text.trim().is_empty() {
                    return Err("There's nothing to add.".into());
                }
                Ok(crate::write::with_thought(current.unwrap_or(crate::write::NEW_SCRATCHPAD), text, stamp).0)
            }
            Instruction::DeleteLine { line, at } => {
                let text = have()?;
                let (mut lines, eol, trailing) = split_lines(text);
                let at = place(&lines, *at, std::slice::from_ref(line)).ok_or_else(|| format!("That line isn't in {name} any more."))?;
                lines.remove(at);
                Ok(join_lines(&lines, &eol, trailing))
            }
            Instruction::Lines { at, old, new } => {
                let text = have()?;
                if old.is_empty() {
                    return Err("There are no lines to change.".into());
                }
                let (mut lines, eol, trailing) = split_lines(text);
                let at = place(&lines, Some(*at), old).ok_or_else(|| {
                    format!("{} isn't in {name} as it was any more.", if old.len() == 1 { "That line" } else { "That text" })
                })?;
                lines.splice(at..at + old.len(), new.iter().map(|l| l.replace("\r\n", "\n")));
                Ok(join_lines(&lines, &eol, trailing))
            }
            Instruction::Append { text: add } => {
                let text = have()?;
                let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
                let add = add.trim().replace("\r\n", "\n").replace('\n', eol);
                if add.is_empty() {
                    return Err("There's nothing to add.".into());
                }
                let body = text.trim_end();
                Ok(if body.is_empty() { format!("{add}{eol}") } else { format!("{body}{eol}{eol}{add}{eol}") })
            }
            Instruction::Properties { set } => {
                let mut text = have()?.to_string();
                for (k, v) in set {
                    text = crate::write::with_property(&text, k, v.as_deref()).map_err(|e| e.to_string())?;
                }
                Ok(text)
            }
            Instruction::Rename { .. } | Instruction::Trash | Instruction::Restore { .. } | Instruction::Image => {
                Err("A rename, a move to the Trash, a restore or an image doesn't change the text.".into())
            }
        }
    }

    /// Rewrites the whole page: accepting it later must find the page as it was when it was held.
    pub fn whole_page(&self) -> bool {
        matches!(self, Instruction::Page { .. })
    }

    /// Moves the note (or adds an image) rather than changing its text.
    pub fn moves(&self) -> bool {
        matches!(self, Instruction::Rename { .. } | Instruction::Trash | Instruction::Restore { .. } | Instruction::Image)
    }
}

/// Where `old` is in `lines`: at `at` when it still reads the same there (line ends aside), else
/// where it's found just once; None when it's nowhere, or in more than one place.
fn place(lines: &[String], at: Option<usize>, old: &[String]) -> Option<usize> {
    let fits =
        |i: usize| i + old.len() <= lines.len() && lines[i..i + old.len()].iter().zip(old).all(|(a, b)| a.trim_end() == b.trim_end());
    if let Some(i) = at.filter(|&i| fits(i)) {
        return Some(i);
    }
    let mut hits = (0..lines.len()).filter(|&i| fits(i));
    match (hits.next(), hits.next()) {
        (Some(i), None) => Some(i),
        _ => None,
    }
}

/// An instruction that makes `after` from `before`, for changes worked out as whole texts (a
/// summary's block, an ingest's page): each change, with enough unchanged text around it to be
/// found once, as a find and replace. Run on the page later, it applies wherever that text still
/// is. A new page (or one that was empty) is its whole text.
pub fn from_texts(before: Option<&str>, after: &str) -> Instruction {
    let after = after.replace("\r\n", "\n");
    let Some(before) = before.map(|b| b.replace("\r\n", "\n")).filter(|b| !b.trim().is_empty()) else {
        return Instruction::Page { content: after };
    };
    let old: Vec<&str> = before.split('\n').collect();
    let new: Vec<&str> = after.split('\n').collect();
    let ops = similar::capture_diff_slices(similar::Algorithm::Myers, &old, &new);
    // Runs of changes: (old start, old end, new start, new end).
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
    if runs.is_empty() {
        return Instruction::Replace { edits: vec![] };
    }
    // Each run widened by unchanged lines until its text is found once in the page; windows that
    // meet become one edit, so no window cuts through another change.
    let mut windows: Vec<(usize, usize)> = Vec::new();
    for &(os, oe, _, _) in &runs {
        let (mut a, mut b) = (os, oe);
        while (a > 0 || b < old.len())
            && !{
                let find = old[a..b].join("\n");
                !find.trim().is_empty() && before.matches(&find).count() == 1
            }
        {
            a = a.saturating_sub(1);
            b = (b + 1).min(old.len());
        }
        windows.push((a, b));
    }
    windows.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (a, b) in windows {
        match merged.last_mut() {
            Some(m) if a <= m.1 => m.1 = m.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    // Where an old line boundary is in the new text: past every change before it. A window's end
    // is also past a change that starts right there at the page's end (lines added after the
    // last one), which no later window can carry: without it, what was added there was lost.
    let shift = |x: usize, end: bool| -> usize {
        let d: isize = runs
            .iter()
            .filter(|r| r.0 < x || (end && r.0 == x && x == old.len()))
            .map(|r| (r.3 - r.2) as isize - (r.1 - r.0) as isize)
            .sum();
        (x as isize + d) as usize
    };
    let edits: Vec<Edit> = merged
        .iter()
        .map(|&(a, b)| Edit { find: old[a..b].join("\n"), replace: new[shift(a, false)..shift(b, true)].join("\n") })
        .collect();
    if edits.iter().any(|e| e.find.trim().is_empty()) || (edits.len() == 1 && edits[0].find == before) {
        return Instruction::Page { content: after };
    }
    // Each find is once in the page, but edits apply in turn, and an earlier one can add a copy
    // of a later one's text (a section moved: added above, then taken out below). Then the
    // other way round, else the whole page.
    let gives = |es: &[Edit]| {
        crate::proposals::patched(&before, &crate::proposals::Patch::Replace { edits: es.to_vec() }).is_ok_and(|t| t == after)
    };
    if gives(&edits) {
        return Instruction::Replace { edits };
    }
    let rev: Vec<Edit> = edits.iter().rev().cloned().collect();
    if gives(&rev) {
        return Instruction::Replace { edits: rev };
    }
    Instruction::Page { content: after }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Waiting for the user: a scheduled change that didn't pass a check.
    Held,
    Applied,
    /// A held change the user turned down.
    Rejected,
    /// Applied, then put back.
    Reverted,
}

/// One agent change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub id: String,
    /// Local time, `YYYY-MM-DDTHH:MM:SS`.
    pub created: String,
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Vault-relative.
    pub page: String,
    pub kind: Kind,
    /// One line: "Update current state from the steerco".
    pub title: String,
    #[serde(default)]
    pub reason: String,
    pub instruction: Instruction,
    #[serde(default)]
    pub quotes: Vec<Quote>,
    /// Worth knowing, not a problem: a link to a page that doesn't exist.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Checks it didn't pass: why it's held, or what to look at in one applied anyway.
    #[serde(default)]
    pub flags: Vec<String>,
    /// Why only the user can accept it, in the app (decided 2026-10-05, D-20261005-09): it changes a
    /// template, adds code that runs, or a job the user set holds its changes. An assistant can
    /// accept a held change only when this is None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub for_user: Option<String>,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided: Option<String>,
    /// The page's text before and after, by hash (`Store::put_text`); None when there was no page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// For a rename: the new path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// For a move to the Trash: its entry, to restore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trash: Option<String>,
    /// An ingest's checked claims, written to the page's claims file with it (D-20261006-16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claims: Option<crate::claims::Update>,
}

impl Change {
    /// A new change from an instruction, not yet applied or held.
    pub fn new(
        page: &str,
        kind: Kind,
        title: &str,
        reason: &str,
        origin: Origin,
        model: Option<String>,
        instruction: Instruction,
    ) -> Change {
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let to = match &instruction {
            Instruction::Rename { to } => Some(to.clone()),
            _ => None,
        };
        Change {
            id: proposals::new_id(),
            created: proposals::now_local(),
            origin,
            model,
            page: page.to_string(),
            kind,
            title: if title.is_empty() { "Change".into() } else { title },
            reason: reason.trim().to_string(),
            instruction,
            quotes: vec![],
            warnings: vec![],
            flags: vec![],
            for_user: None,
            status: Status::Held,
            decided: None,
            before: None,
            after: None,
            to,
            trash: None,
            claims: None,
        }
    }

    /// The texts in the store it refers to: the page's before and after, and its claims file's.
    pub fn texts(&self) -> Vec<&String> {
        let c = self.claims.as_ref();
        [self.before.as_ref(), self.after.as_ref(), c.and_then(|c| c.before.as_ref()), c.and_then(|c| c.after.as_ref())]
            .into_iter()
            .flatten()
            .collect()
    }

    /// The run it belongs to, for the feed's groups: the run, else the chat, else itself.
    pub fn group(&self) -> &str {
        self.origin.run.as_deref().or(self.origin.chat.as_deref()).unwrap_or(&self.id)
    }
}

/// Whether it came from a scheduled run (or a terminal session that said it's unattended),
/// rather than something the user started.
pub fn scheduled(o: &Origin) -> bool {
    o.trigger.as_deref() == Some("scheduled")
}

/// Pages that are Brainstead's or the agents' to rewrite: the wiki, captured sources, `index.md`,
/// `log.md` and the summaries' notes. Anything else is the user's own note: a scheduled run may add
/// to it, but a change that rewrites or removes its text is held.
pub fn agents_page(rel: &str) -> bool {
    rel.starts_with("wiki/")
        || rel.starts_with("sources/")
        || rel == "index.md"
        || rel == "log.md"
        || (crate::rename::is_system(rel) && rel.starts_with("Me. ") && rel.contains(" - ") && !crate::rename::PINNED.contains(&rel))
}

/// What one change is checked on.
pub struct Facts<'a> {
    pub page: &'a str,
    pub before: Option<&'a str>,
    pub after: Option<&'a str>,
    pub quotes: &'a [Quote],
}

/// The checks every change gets that need only its text: a quote that isn't in its source, a
/// change that rewrites or takes out text in the user's own note, properties that wouldn't read.
/// The app adds its own (unsaved edits, a contradiction, a job set to hold). For a scheduled change
/// any of them holds it; for one the user started they only flag it.
pub fn checks(f: &Facts) -> Vec<String> {
    let mut out = Vec::new();
    for q in f.quotes.iter().filter(|q| q.checked == Some(false)) {
        out.push(format!("This quote isn't in {}: “{}”", q.path.as_deref().unwrap_or(&q.source), short(&q.text)));
    }
    if let (Some(b), Some(a)) = (f.before, f.after) {
        if !agents_page(f.page) && crate::ingest::removes_text(b, a) {
            out.push("It changes or takes out text in your own note.".into());
        }
        if crate::frontmatter::split(a).error.is_some() && crate::frontmatter::split(b).error.is_none() {
            out.push("The page's properties wouldn't read any more.".into());
        }
    }
    out
}

/// Why a change must wait for the user whoever started it: it changes a template (whose code runs
/// when it's used) or adds code that runs when the note is drawn (a dataviewjs block, inline `$=`,
/// a Tasks `by function` line, Templater's `<% %>`). Scripts run with the app's full access, and an
/// assistant's text may carry what a captured source told it to write.
pub fn needs_the_user(page: &str, before: Option<&str>, after: &str) -> Option<String> {
    if in_templates(page) {
        return Some("It changes a template, whose code runs when you use it.".into());
    }
    // The system-note callout other features look for: changed or taken out.
    let callout = |t: &str| crate::rename::system_callout(t).map(|(a, b)| t[a..b].replace("\r\n", "\n"));
    if let Some(had) = before.and_then(callout) {
        if callout(after).as_deref() != Some(had.as_str()) {
            return Some("It changes the note's system-note header, which other parts of Brainstead rely on.".into());
        }
    }
    (runnable(after) > before.map_or(0, runnable)).then(|| "It adds code that runs when the note is shown (a script or a function).".into())
}

/// Templater's tags: `<% … %>`, `<%* … %>`, with `-` or `_` to trim around them.
static TEMPLATER: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(?s)<%[*_-]?.*?[_-]?%>").unwrap());

/// How many pieces of runnable code a text has: dataviewjs blocks, inline `$=` queries, `by
/// function` lines inside a Tasks query block, and Templater tags. The same words in prose aren't
/// code.
pub fn runnable(text: &str) -> usize {
    let text = text.replace("\r\n", "\n");
    let mut n = text.matches("`$=").count() + TEMPLATER.find_iter(&text).count();
    // The fence open (its marker and info), as code blocks are read.
    let mut fence: Option<(String, String)> = None;
    for line in text.lines() {
        let t = line.trim_start().trim_start_matches('>').trim_start();
        let mark: String = t.chars().take_while(|c| *c == '`' || *c == '~').collect();
        let is_fence = mark.len() >= 3 && mark.chars().all(|c| c == mark.chars().next().unwrap_or('`'));
        match &fence {
            Some((open, _)) if is_fence && mark.starts_with(open.as_str()) && t[mark.len()..].trim().is_empty() => fence = None,
            Some((_, info)) => {
                if info == "tasks" && t.to_lowercase().contains("by function") {
                    n += 1;
                }
            }
            None if is_fence => {
                let info = t[mark.len()..].split_whitespace().next().unwrap_or("").to_lowercase();
                if info == "dataviewjs" {
                    n += 1;
                }
                fence = Some((mark, info));
            }
            None => {}
        }
    }
    n
}

/// Why a rename or a move to the Trash must wait for the user (D-20261005-11): the note is a
/// template, the rename would put it among them, or the links it rewrites are in a template.
pub fn move_needs_the_user<'a>(page: &str, to: Option<&str>, rewrites: impl IntoIterator<Item = &'a str>) -> Option<String> {
    if in_templates(page) || to.is_some_and(in_templates) {
        return Some("It moves a template, whose code runs when you use it.".into());
    }
    rewrites.into_iter().any(in_templates).then(|| "It rewrites links in a template, whose code runs when you use it.".into())
}

/// Whether a vault-relative path is in `Templates/`. The app gives the rules each page as the
/// file system spells it (`paths::vault_rel`, which also refuses the names Win32 reads as another
/// file's: a trailing dot or space, a stream); a path that reaches them spelled otherwise is still
/// caught when it differs in case or separator. An 8.3 short name (`TEMPLA~1`) only `vault_rel`
/// can read back.
fn in_templates(p: &str) -> bool {
    p.split(['/', '\\']).next().is_some_and(|f| f.eq_ignore_ascii_case("Templates"))
}

fn short(s: &str) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 80 {
        format!("{}…", one.chars().take(80).collect::<String>())
    } else {
        one
    }
}

/// One change, with the unchanged lines around it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub index: usize,
    /// Where `old` starts in the page before, from 0.
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
    let mut out = Vec::new();
    for (i, &(os, oe, ns, ne)) in runs.iter().enumerate() {
        // Context never reaches into the next or previous change.
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

/// Where a hunk's old lines are in `lines`, with its context around them: the one place they
/// are. None when they're nowhere, or in more than one place, so it can't be told which to undo.
fn locate(lines: &[String], h: &Hunk) -> Option<usize> {
    let (cb, ca) = (h.ctx_before.len(), h.ctx_after.len());
    let span = cb + h.old.len() + ca;
    if span > lines.len() {
        return None;
    }
    let mut best: Option<usize> = None;
    for s in 0..=lines.len() - span {
        if (h.from_start && s != 0) || (h.to_end && s + span != lines.len()) {
            continue;
        }
        let p = s + cb;
        if lines[s..p] != h.ctx_before[..] || lines[p..p + h.old.len()] != h.old[..] || lines[p + h.old.len()..s + span] != h.ctx_after[..]
        {
            continue;
        }
        if best.is_some() {
            return None;
        }
        best = Some(p);
    }
    best
}

/// The page as it is now (`current`) with a change undone: its lines from `after` put back to
/// what they were in `before`, wherever they are now. None when some of them have been edited
/// since (or the page is empty of them), so it can't be undone line by line.
pub fn revert_text(before: &str, after: &str, current: &str) -> Option<String> {
    let norm = |s: &str| s.replace("\r\n", "\n");
    if norm(current) == norm(after) {
        return Some(before.to_string());
    }
    let inverse = hunks(after, before);
    let (mut lines, eol, trailing) = split_lines(current);
    let mut places = Vec::new();
    for h in &inverse {
        places.push((locate(&lines, h)?, h));
    }
    places.sort_by_key(|p| p.0);
    // Two that land on the same lines (or touch) can't both be undone.
    if places.windows(2).any(|w| w[1].0 < w[0].0 + w[0].1.old.len() + 1) {
        return None;
    }
    places.reverse();
    for (p, h) in places {
        lines.splice(p..p + h.old.len(), h.new.iter().cloned());
    }
    Some(join_lines(&lines, &eol, trailing))
}

/// The changes folder.
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(data_dir: &Path) -> Self {
        Store { dir: data_dir.join(DIR) }
    }

    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(format!("Not a change id: {id}"));
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    pub fn save(&self, c: &Change) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(c).map_err(|e| e.to_string())?;
        write_atomic(&self.path(&c.id)?, &json, false).map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Result<Change, String> {
        let b = std::fs::read(self.path(id)?).map_err(|_| format!("There's no change {id} any more."))?;
        serde_json::from_slice(&b).map_err(|e| e.to_string())
    }

    /// Every change, newest first.
    pub fn list(&self) -> Vec<Change> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return vec![] };
        let mut out: Vec<Change> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .filter_map(|p| std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Change>(&b).ok()))
            .collect();
        out.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| b.id.cmp(&a.id)));
        out
    }

    pub fn held(&self) -> usize {
        self.list().iter().filter(|c| c.status == Status::Held).count()
    }

    /// Keeps a text, compressed, once; its hash.
    pub fn put_text(&self, text: &str) -> Result<String, String> {
        let hash = crate::write::version(text.as_bytes());
        let dir = self.dir.join(TEXTS);
        let p = dir.join(format!("{hash}.gz"));
        if p.exists() {
            // Fresh again, so a prune running now leaves it for the change about to refer to it.
            if let Ok(f) = std::fs::File::options().write(true).open(&p) {
                let _ = f.set_modified(std::time::SystemTime::now());
            }
        } else {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gz.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            write_atomic(&p, &gz.finish().map_err(|e| e.to_string())?, false).map_err(|e| e.to_string())?;
        }
        Ok(hash)
    }

    pub fn text(&self, hash: &str) -> Option<String> {
        if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let b = std::fs::read(self.dir.join(TEXTS).join(format!("{hash}.gz"))).ok()?;
        let mut s = String::new();
        flate2::read::GzDecoder::new(&b[..]).read_to_string(&mut s).ok()?;
        Some(s)
    }

    /// Drops history older than `days`, then the oldest until it all takes no more than `max`
    /// bytes, then texts nothing refers to. Held changes stay however old, and so do texts kept in
    /// the last hour: a change being recorded keeps its texts before it's saved. How many went.
    pub fn prune(&self, today: chrono::NaiveDate, days: i64, max: u64) -> usize {
        let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let mut all = self.list();
        let mut gone = 0;
        let old = |c: &Change| {
            chrono::NaiveDate::parse_from_str(c.created.get(..10).unwrap_or(""), "%Y-%m-%d").is_ok_and(|d| (today - d).num_days() > days)
        };
        all.retain(|c| {
            if c.status != Status::Held && old(c) {
                let _ = self.path(&c.id).map(std::fs::remove_file);
                gone += 1;
                false
            } else {
                true
            }
        });
        // Each text counted once, with the first (newest) change that needs it.
        let mut seen = std::collections::HashSet::new();
        let mut cost: Vec<u64> = Vec::with_capacity(all.len());
        for c in &all {
            let mut n = self.path(&c.id).map(|p| size(&p)).unwrap_or(0);
            for h in c.texts() {
                if seen.insert(h.clone()) {
                    n += size(&self.dir.join(TEXTS).join(format!("{h}.gz")));
                }
            }
            cost.push(n);
        }
        let mut total: u64 = cost.iter().sum();
        let mut keep = all.len();
        while total > max && keep > 0 {
            keep -= 1;
            if all[keep].status == Status::Held {
                continue;
            }
            let _ = self.path(&all[keep].id).map(std::fs::remove_file);
            total -= cost[keep];
            gone += 1;
            all.remove(keep);
        }
        let used: std::collections::HashSet<String> = all.iter().flat_map(|c| c.texts().into_iter().cloned()).collect();
        if let Ok(rd) = std::fs::read_dir(self.dir.join(TEXTS)) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().trim_end_matches(".gz").to_string();
                let fresh = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_none_or(|age| age < FRESH);
                if !used.contains(&name) && !fresh {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        gone
    }
}

/// The old review queue's `proposals/` folder, moved into Changes once (decided 2026-10-05): a
/// waiting proposal already in its page is dropped, the others are held, applied ones from the
/// last `days` become history that can be reverted, rejected ones are dropped. The folder is kept
/// as `proposals-backup/` for one release. How many changes it made.
pub fn migrate(data_dir: &Path, vault: &Path, today: chrono::NaiveDate, days: i64) -> Result<usize, String> {
    let old = data_dir.join(proposals::DIR);
    if !old.is_dir() {
        return Ok(0);
    }
    let st = Store::new(data_dir);
    let mut n = 0;
    for p in proposals::Store::new(data_dir).list(today) {
        let recent = chrono::NaiveDate::parse_from_str(p.decided.as_deref().unwrap_or(&p.created).get(..10).unwrap_or(""), "%Y-%m-%d")
            .is_ok_and(|d| (today - d).num_days() <= days);
        let instruction = match p.kind {
            Kind::Rename => Instruction::Rename { to: p.to.clone().unwrap_or_default() },
            Kind::Trash => Instruction::Trash,
            _ => from_texts((!p.before.is_empty() || p.kind != Kind::New).then_some(p.before.as_str()), &p.after),
        };
        let mut c = Change::new(&p.page, p.kind, &p.title, &p.reason, p.origin.clone(), p.model.clone(), instruction);
        c.id = p.id.clone();
        c.created = p.created.clone();
        c.quotes = p.quotes.clone();
        c.warnings = p.warnings.clone();
        c.decided = p.decided.clone();
        match p.status {
            proposals::Status::Pending => {
                let current = std::fs::read_to_string(vault.join(&p.page)).ok();
                if p.kind == Kind::Edit && current.as_deref().is_some_and(|t| proposals::already_in(t, &p.before, &p.after)) {
                    continue;
                }
                c.status = Status::Held;
                c.flags = vec!["Waiting in the old review queue when Brainstead moved to Changes.".into()];
                // The page it was worked out from, so accepting a whole-page rewrite sees later edits.
                if c.instruction.whole_page() && !p.before.is_empty() {
                    c.before = Some(st.put_text(&p.before)?);
                }
            }
            proposals::Status::Applied if recent => {
                let Some(u) = &p.undo else { continue };
                // What was written: the kept hunks applied to the page as it was then.
                let before = u.before.clone().unwrap_or_default();
                let all = proposals::hunks(&p.before, &p.after);
                let (after, _) = proposals::apply(&before, &all, &p.applied);
                c.status = Status::Applied;
                c.before = u.before.as_deref().map(|b| st.put_text(b)).transpose()?;
                c.after = Some(st.put_text(&after)?);
            }
            _ => continue,
        }
        st.save(&c)?;
        n += 1;
    }
    let backup = data_dir.join("proposals-backup");
    if backup.exists() {
        let _ = std::fs::remove_dir_all(&backup);
    }
    std::fs::rename(&old, &backup).map_err(|e| e.to_string())?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies the instruction's edits to `before`, as the change would.
    fn applied(before: &str, i: &Instruction) -> String {
        match i {
            Instruction::Replace { edits } => edits.iter().fold(before.to_string(), |t, e| t.replacen(&e.find, &e.replace, 1)),
            Instruction::Page { content } => content.clone(),
            _ => panic!("not a text change"),
        }
    }

    #[test]
    fn a_moved_section_applies() {
        let before = "# Orbit App\n\n## History\n\nPilot in August.\n\n## See also\n\n- [[Lena]]\n\n## Oct 2026\n\nRecon moves to 16 Oct.\n\n## Nov 2026\n\nLaunch on 28 November.\n";
        let after = "# Orbit App\n\n## History\n\nPilot in August.\n\n## Oct 2026\n\nRecon moves to 16 Oct.\n\n## Nov 2026\n\nLaunch on 28 November.\n\n## See also\n\n- [[Lena]]\n";
        let i = from_texts(Some(before), after);
        let p = match i {
            Instruction::Replace { edits } => crate::proposals::Patch::Replace { edits },
            Instruction::Page { content } => crate::proposals::Patch::Content { content },
            _ => panic!(),
        };
        assert_eq!(crate::proposals::patched(before, &p).unwrap(), after);
    }

    #[test]
    fn a_section_added_at_the_end_is_kept() {
        let before = "---\nupdated: 2026-10-01\n---\n# Orbit App\n\n## Oct 2026\n\n- **Pending**: the pen test, due 16 Oct.\n";
        for after in [
            format!("{before}\n## Oct 2026 roadmap\n\n- Origination is in build.\n"),
            format!("{}\n\n## Oct 2026 roadmap\n\n- Origination is in build.", before.trim_end()),
            format!("{before}## Oct 2026 roadmap\n- Origination is in build.\n"),
        ] {
            let i = from_texts(Some(before), &after);
            assert_eq!(applied(before, &i), after, "{i:?}");
        }
        // Without a newline at the end of the page.
        let before = before.trim_end();
        let after = format!("{before}\n\n## Oct 2026 roadmap\n\n- Origination is in build.\n");
        assert_eq!(applied(before, &from_texts(Some(before), &after)), after);
    }

    const PAGE: &str = "---\ntype: entity\nupdated: 2026-09-01\n---\n\n# Orbit App\n\n## Current state\n\nIn build.\nLaunch planned for 14 November.\n\n## History\n\n- Started in May.\n";

    #[test]
    fn instructions_from_texts_apply_to_the_page_as_it_is() {
        let after = PAGE.replace("In build.", "In test.").replace("- Started in May.", "- Started in May.\n- Pen test booked.");
        let i = from_texts(Some(PAGE), &after);
        let Instruction::Replace { edits } = &i else { panic!("{i:?}") };
        assert_eq!(edits.len(), 2);
        assert_eq!(i.text("Orbit App.md", Some(PAGE)).unwrap(), after);
        // Someone wrote elsewhere on the page since: both still apply.
        let now = PAGE.replace("# Orbit App\n", "# Orbit App\n\nOwned by Maya.\n");
        let t = i.text("Orbit App.md", Some(&now)).unwrap();
        assert!(t.contains("Owned by Maya.") && t.contains("In test.") && t.contains("- Pen test booked."));
        // Its line changed: it says so.
        let edited = PAGE.replace("In build.", "In design.");
        assert!(i.text("Orbit App.md", Some(&edited)).unwrap_err().contains("isn't on the page any more"));
        // No page: its whole text.
        assert_eq!(from_texts(None, "# New\n"), Instruction::Page { content: "# New\n".into() });
    }

    #[test]
    fn a_repeated_line_gets_context_until_it_is_found_once() {
        let before = "# A\n\n- x\n\n# B\n\n- x\n";
        let after = "# A\n\n- x\n\n# B\n\n- y\n";
        let i = from_texts(Some(before), after);
        assert_eq!(i.text("p.md", Some(before)).unwrap(), after);
        // Changes close together become one edit.
        let after2 = "# A\n\n- z\n\n# B\n\n- y\n";
        let i2 = from_texts(Some(before), after2);
        assert_eq!(i2.text("p.md", Some(before)).unwrap(), after2);
    }

    #[test]
    fn several_task_additions_to_one_page_all_apply() {
        let page = "Project. Orbit App launch.md";
        let before = "---\nstatus: active\n---\n\n## Next actions\n\n## Waiting for\n";
        let a = Instruction::AddTask { line: "- [ ] Book the venue".into(), heading: None };
        let b = Instruction::AddTask { line: "- [ ] Email Maya".into(), heading: None };
        let one = a.text(page, Some(before)).unwrap();
        let two = b.text(page, Some(&one)).unwrap();
        assert!(two.contains("## Next actions\n\n- [ ] Book the venue\n- [ ] Email Maya\n\n## Waiting for"));
        assert!(a.text(page, Some(&two)).unwrap_err().contains("already"));
        let d = Instruction::DeleteLine { line: "- [ ] Email Maya".into(), at: None };
        assert_eq!(d.text(page, Some(&two)).unwrap(), one);
        assert!(d.text(page, Some(&one)).unwrap_err().contains("isn't in"));
        // Under the heading given: a waiting task in the project's Waiting for, as the Inbox files it.
        let w = Instruction::AddTask { line: "- [ ] Lena's quote #waiting-for".into(), heading: Some("Waiting for".into()) };
        assert!(w.text(page, Some(&two)).unwrap().ends_with("## Waiting for\n\n- [ ] Lena's quote #waiting-for\n"));
    }

    #[test]
    fn a_captured_thought_goes_below_the_scratchpads_header() {
        let page = "Me. Scratchpad.md";
        let i = Instruction::AddThought { text: "Ask Lena about\nthe venue".into(), stamp: "2026-10-07 09:30".into() };
        let before = "# Me. Scratchpad\n\n## 2026-10-06 18:00\n\nOlder.\n";
        assert_eq!(
            i.text(page, Some(before)).unwrap(),
            "# Me. Scratchpad\n\n## 2026-10-07 09:30\n\nAsk Lena about\nthe venue\n\n## 2026-10-06 18:00\n\nOlder.\n"
        );
        // Not there yet: started, as Quick capture starts it.
        assert!(i.text(page, None).unwrap().starts_with("# Me. Scratchpad\n\n## 2026-10-07 09:30\n\n"));
        let empty = Instruction::AddThought { text: " ".into(), stamp: "2026-10-07 09:30".into() };
        assert!(empty.text(page, Some(before)).is_err());
    }

    #[test]
    fn a_deleted_line_goes_where_it_was_and_only_there() {
        let page = "Me. To Do List.md";
        let text = "# To do\n\n- [ ] Email Maya\n- [ ] Book the room\n- [ ] Email Maya\n";
        // Two lines the same: the one it was at goes.
        let d = Instruction::DeleteLine { line: "- [ ] Email Maya".into(), at: Some(4) };
        assert_eq!(d.text(page, Some(text)).unwrap(), "# To do\n\n- [ ] Email Maya\n- [ ] Book the room\n");
        // Moved since, with another the same: it can't tell which, so it doesn't guess.
        let moved = Instruction::DeleteLine { line: "- [ ] Email Maya".into(), at: Some(3) };
        assert!(moved.text(page, Some(text)).unwrap_err().contains("isn't in"));
        // Moved since, and the only one: found.
        let once = Instruction::DeleteLine { line: "- [ ] Book the room".into(), at: Some(0) };
        assert!(!once.text(page, Some(text)).unwrap().contains("Book the room"));
    }

    #[test]
    fn lines_append_and_properties_run_on_the_page_as_it_is() {
        let page = "Project. Orbit App launch.md";
        let text = "---\nstatus: active\n---\n\n## Next actions\n\n- [ ] Call Sam\n";
        let l = Instruction::Lines {
            at: 6,
            old: vec!["- [ ] Call Sam".into()],
            new: vec!["- [ ] Call Sam 🔁 every week".into(), "- [x] Call Sam".into()],
        };
        assert!(l.text(page, Some(text)).unwrap().ends_with("- [ ] Call Sam 🔁 every week\n- [x] Call Sam\n"));
        let gone = Instruction::Lines { at: 6, old: vec!["- [ ] Call Maya".into()], new: vec![] };
        assert!(gone.text(page, Some(text)).unwrap_err().contains("as it was"));
        let a = Instruction::Append { text: "- Venue holds 80".into() };
        assert!(a.text(page, Some(text)).unwrap().ends_with("- [ ] Call Sam\n\n- Venue holds 80\n"));
        let p = Instruction::Properties { set: vec![("status".into(), Some("done".into())), ("area".into(), None)] };
        assert!(p.text(page, Some(text)).unwrap().starts_with("---\nstatus: done\n---"));
    }

    #[test]
    fn revert_wont_guess_between_two_places() {
        // The change added a line between two that repeat further down.
        let before = "# A\n\n- x\n- y\n\n# B\n\n- z\n";
        let after = "# A\n\n- x\n- new\n- y\n\n# B\n\n- z\n";
        assert_eq!(revert_text(before, after, after).unwrap(), before);
        // Copied, context and all, further down since: which one to undo is anyone's guess.
        let now = format!("{after}\n- x\n- new\n- y\n\n# C\n");
        assert!(revert_text(before, after, &now).is_none());
    }

    #[test]
    fn revert_undoes_its_lines_after_later_edits() {
        let after = PAGE.replace("In build.", "In test.");
        // Unchanged since: back to before.
        assert_eq!(revert_text(PAGE, &after, &after).unwrap(), PAGE);
        // Edited elsewhere since: only its line goes back.
        let now = after.replace("- Started in May.", "- Started in May.\n- Kick-off with Lena.");
        let back = revert_text(PAGE, &after, &now).unwrap();
        assert!(back.contains("In build.") && back.contains("Kick-off with Lena.") && !back.contains("In test."));
        // Its own line edited since: it can't.
        assert!(revert_text(PAGE, &after, &after.replace("In test.", "In UAT.")).is_none());
        // CRLF pages keep CRLF.
        let crlf = now.replace('\n', "\r\n");
        assert!(revert_text(PAGE, &after, &crlf).unwrap().contains("In build.\r\n"));
    }

    #[test]
    fn checks_flag_quotes_own_notes_and_properties() {
        let q = Quote { source: "Steerco".into(), text: "in production".into(), checked: Some(false), ..Default::default() };
        let after = PAGE.replace("In build.", "In test.");
        let f = |page: &'static str, quotes: &'static [Quote]| Facts { page, before: Some(PAGE), after: None, quotes };
        assert!(checks(&f("wiki/entities/Orbit App.md", &[])).is_empty());
        let qs = vec![q];
        let fx = Facts { page: "wiki/entities/Orbit App.md", before: Some(PAGE), after: Some(&after), quotes: &qs };
        assert!(checks(&fx)[0].contains("isn't in Steerco"));
        // A rewrite in the user's own note, but not an addition.
        let own = Facts { page: "Orbit notes.md", before: Some(PAGE), after: Some(&after), quotes: &[] };
        assert!(checks(&own)[0].contains("your own note"));
        let added = PAGE.replace("- Started in May.", "- Started in May.\n- More.");
        assert!(checks(&Facts { page: "Orbit notes.md", before: Some(PAGE), after: Some(&added), quotes: &[] }).is_empty());
        let broken = PAGE.replace("type: entity", "type: [entity");
        assert!(checks(&Facts { page: "wiki/x.md", before: Some(PAGE), after: Some(&broken), quotes: &[] })[0].contains("properties"));
        assert!(
            agents_page("Me. Daily Summaries - 2026-10.md")
                && !agents_page("Me. To Do List.md")
                && !agents_page("Me. Weekly Review - 2026-W40.md")
        );
    }

    #[test]
    fn a_restore_is_a_move_and_says_its_entry() {
        let r = Instruction::Restore { id: "abc-123456".into() };
        assert_eq!(serde_json::to_value(&r).unwrap(), serde_json::json!({"op": "restore", "id": "abc-123456"}));
        assert!(r.moves() && !r.whole_page());
        assert!(r.text("Notes.md", None).is_err());
        // Checked as a new page with the trashed file's text: into Templates/, or code that runs, is held.
        assert!(needs_the_user("Templates/Meeting.md", None, "# Meeting\n").is_some());
        assert!(needs_the_user("Notes.md", None, "```dataviewjs\ndv.paragraph(1)\n```\n").is_some());
        assert!(needs_the_user("Notes.md", None, "# Notes\n").is_none());
    }

    #[test]
    fn scripts_and_templates_wait_for_the_user() {
        assert!(needs_the_user("Templates/Meeting.md", Some("a"), "b").is_some());
        assert!(needs_the_user("Notes.md", Some("# A\n"), "# A\n\n```dataviewjs\ndv.paragraph(1)\n```\n").is_some());
        assert!(needs_the_user("Notes.md", Some("x `$= 1`"), "y `$= 1`").is_none());
        assert!(needs_the_user("Notes.md", None, "<% tp.date.now() %>").is_some());
        assert!(needs_the_user("wiki/x.md", Some("a"), "b").is_none());
        let sys = "> **This is a system note** - Tasks.\n\n- [ ] a\n";
        assert!(needs_the_user("Me. To Do List.md", Some(sys), "- [ ] a\n").is_some(), "header taken out");
        assert!(needs_the_user("Me. To Do List.md", Some(sys), &sys.replace("Tasks.", "Stuff.")).is_some(), "header changed");
        assert!(needs_the_user("Me. To Do List.md", Some(sys), &format!("{sys}- [ ] b\n")).is_none(), "below it is fine");
        // The same words in prose aren't code.
        assert!(needs_the_user("Notes.md", Some("a"), "Sort the list by function, not by name. Use <% for the old syntax.").is_none());
        assert!(needs_the_user("Notes.md", Some("a"), "```js\nsort by function x\n```\n").is_none());
        assert!(needs_the_user("Notes.md", Some("a"), "```tasks\nnot done\nsort by function task.urgency\n```\n").is_some());
        assert!(needs_the_user("Notes.md", Some("a"), "<%* tR += 1 %>").is_some());
    }

    #[test]
    fn moves_touching_templates_wait_for_the_user() {
        assert!(move_needs_the_user("Templates/Meeting.md", None, []).is_some());
        assert!(move_needs_the_user("Idea. A.md", Some("Templates/Idea. A.md"), []).is_some());
        assert!(move_needs_the_user("Idea. A.md", Some("Idea. B.md"), ["Notes.md", "Templates/Weekly.md"]).unwrap().contains("links"));
        assert!(move_needs_the_user("Idea. A.md", Some("Idea. B.md"), ["Notes.md"]).is_none());
    }

    #[test]
    fn a_template_spelled_another_way_still_waits() {
        for page in ["templates/Meeting.md", "TEMPLATES/Meeting.md", "Templates\\Meeting.md", "templates\\sub\\Meeting.md"] {
            assert!(needs_the_user(page, Some("a"), "b").is_some(), "{page}");
            assert!(move_needs_the_user(page, None, []).is_some(), "{page}");
            assert!(move_needs_the_user("Idea. A.md", Some(page), []).is_some(), "{page}");
            assert!(move_needs_the_user("Idea. A.md", Some("Idea. B.md"), [page]).is_some(), "{page}");
        }
        assert!(needs_the_user("Templates archive/Meeting.md", Some("a"), "b").is_none());
        assert!(needs_the_user("Notes/Templates/Meeting.md", Some("a"), "b").is_none());
    }

    /// Each spelling Windows takes for `Templates/Meeting.md`, as `paths::vault_rel` gives it to
    /// the rules: read back as the file's own path, or refused.
    #[cfg(windows)]
    #[test]
    fn every_windows_spelling_of_a_template_is_held() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("Templates")).unwrap();
        std::fs::write(d.path().join("Templates/Meeting.md"), "# Meeting\n").unwrap();
        let mut spellings = vec!["Templates\\Meeting.md".to_string(), "templates/meeting.md".into(), "TEMPLATES\\New.md".into()];
        match crate::paths::short_name(&d.path().join("Templates")) {
            Some(short) => spellings.extend([format!("{short}/Meeting.md"), format!("{short}\\New.md")]),
            None => eprintln!("This volume makes no 8.3 names, so the short spelling isn't tried."),
        }
        for s in &spellings {
            let page = crate::paths::vault_rel(d.path(), s).unwrap();
            assert!(page.starts_with("Templates/"), "{s}: {page}");
            assert!(needs_the_user(&page, Some("a"), "b").is_some(), "{s}");
            assert!(move_needs_the_user(&page, None, []).is_some(), "{s}");
            assert!(move_needs_the_user("Idea. A.md", Some(&crate::paths::vault_rel_new(d.path(), s).unwrap()), []).is_some(), "{s}");
        }
        for s in ["Templates./Meeting.md", "Templates /Meeting.md", "Templates::$INDEX_ALLOCATION/Meeting.md"] {
            assert!(crate::paths::vault_rel(d.path(), s).is_err(), "{s}");
        }
    }

    #[test]
    fn store_keeps_texts_once_and_prunes() {
        let t = tempfile::tempdir().unwrap();
        let s = Store::new(t.path());
        let mut c =
            Change::new("wiki/entities/Orbit App.md", Kind::Edit, " Update  state ", "", Origin::default(), None, Instruction::Trash);
        assert_eq!(c.title, "Update state");
        c.status = Status::Applied;
        c.before = Some(s.put_text(PAGE).unwrap());
        c.after = Some(s.put_text(&PAGE.replace("In build.", "In test.")).unwrap());
        assert_eq!(s.put_text(PAGE).unwrap(), c.before.clone().unwrap());
        s.save(&c).unwrap();
        assert_eq!(s.get(&c.id).unwrap(), c);
        assert_eq!(s.text(c.before.as_deref().unwrap()).unwrap(), PAGE);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        assert_eq!(s.prune(today, 90, u64::MAX), 0);
        // Too big: the oldest goes, and its texts with it. A held one stays.
        let mut held = Change::new("a.md", Kind::Edit, "x", "", Origin::default(), None, Instruction::Trash);
        held.created = "2020-01-01T00:00:00".into();
        s.save(&held).unwrap();
        assert_eq!(s.prune(today, 90, 1), 1);
        assert!(s.get(&c.id).is_err() && s.get(&held.id).is_ok());
        // Its texts were kept in the last hour, as a change being recorded keeps its own before
        // its JSON is saved: they stay until they're older.
        assert!(s.text(c.before.as_deref().unwrap()).is_some());
        for e in std::fs::read_dir(s.dir.join(TEXTS)).unwrap().flatten() {
            let f = std::fs::File::options().write(true).open(e.path()).unwrap();
            f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(7200)).unwrap();
        }
        s.prune(today, 90, 1);
        assert!(s.text(c.before.as_deref().unwrap()).is_none());
        // Keeping a text again makes it fresh, so a prune now leaves it.
        let h = s.put_text(PAGE).unwrap();
        s.prune(today, 90, 1);
        assert!(s.text(&h).is_some());
        assert!(s.get("../x").is_err());
    }

    #[test]
    fn the_old_queue_moves_into_changes() {
        let t = tempfile::tempdir().unwrap();
        let (data, vault) = (t.path().join("data"), t.path().join("vault"));
        std::fs::create_dir_all(&vault).unwrap();
        let after = PAGE.replace("In build.", "In test.");
        std::fs::write(vault.join("Done.md"), &after).unwrap();
        std::fs::write(vault.join("Waiting.md"), PAGE).unwrap();
        let old = proposals::Store::new(&data);
        let mk = |page: &str| {
            proposals::make(proposals::Draft {
                page,
                kind: Kind::Edit,
                title: "t",
                reason: "",
                origin: Origin::default(),
                model: None,
                before: Some(PAGE),
                after: after.clone(),
                quotes: vec![],
                warnings: vec![],
            })
            .unwrap()
        };
        old.save(&mk("Done.md")).unwrap();
        let waiting = mk("Waiting.md");
        old.save(&waiting).unwrap();
        let mut applied = mk("Applied.md");
        applied.status = proposals::Status::Applied;
        applied.decided = Some("2026-10-01T10:00:00".into());
        applied.applied = vec![0];
        applied.undo = Some(proposals::Applied { before: Some(PAGE.into()), version: String::new(), log: None });
        old.save(&applied).unwrap();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        assert_eq!(migrate(&data, &vault, today, 90).unwrap(), 2);
        let st = Store::new(&data);
        let held = st.get(&waiting.id).unwrap();
        assert_eq!(held.status, Status::Held);
        assert_eq!(held.instruction.text("Waiting.md", Some(PAGE)).unwrap(), after);
        let a = st.get(&applied.id).unwrap();
        assert_eq!(st.text(a.after.as_deref().unwrap()).unwrap(), after);
        assert!(data.join("proposals-backup").is_dir() && !data.join("proposals").exists());
        assert_eq!(migrate(&data, &vault, today, 90).unwrap(), 0);
    }
}
