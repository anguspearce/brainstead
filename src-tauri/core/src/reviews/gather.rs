// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Everything a review is written from, gathered in one pass, and rendered as the text the model
//! is given. The selection rules are the previous app's: `scripts/week_inputs.py` for the vault side
//! (notes, wiki pages and sources by modification day, wiki pages split into created and edited by
//! their own `created:` property, never the file's birth time), `scripts/lint_wiki.py` for ghost
//! links and pending sources, `scripts/stuck_tasks.py` and the BFF's task queries for tasks, and
//! `scripts/session_logs.py` for Claude Code sessions. Notes, wiki pages and sources are the
//! index's, so they follow Brainstead's rules for what a note is (no conflict copies, no excluded
//! folders, no templates, not `log.md`); the files' text is read from disk.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use chrono::{Duration, NaiveDateTime};
use regex::Regex;
use serde::Serialize;

use super::sessions::{sessions_in, Automation, Session};
use super::target::{self, Window};
use super::tasks::{review_blocks, someday, stuck, ReviewTask, Someday, Stuck};
use super::Zone;
use crate::index::{Index, Result};
use crate::lint::{fm_field, frontmatter_block, is_wiki_page, lint_key, name_of, names, sources_block, stem, WIKILINK};
use crate::taskquery::{Sort, Status, TaskQuery};

const SCRATCHPAD: &str = "Me. Scratchpad.md";
const LOG: &str = "log.md";
const TODO: &str = "Me. To Do List.md";
/// How many ghost links the weekly review surfaces.
const TOP_GHOSTS: usize = 3;
/// User messages shown per session, and how much of each, as `session_logs.py day` prints them.
const MESSAGES: usize = 6;
const MESSAGE_CHARS: usize = 200;

/// `1-1.`, `Meeting.`, `Me.`: the text before the first `. `.
static PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([^.]+)\.\s").unwrap());
static LOG_ENTRY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^##\s*\[(\d{4}-\d{2}-\d{2}) \d{2}:\d{2}\]\s*(\S+)\s*\|\s*(.+)$").unwrap());
static SCRATCH_BLOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^##\s*(\d{4}-\d{2}-\d{2})\s+\d{2}:\d{2}\s*$").unwrap());
/// A daily block's heading, `## Daily summary 2026-09-30` or, before the rename, `## Daily review …`.
static DAILY_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^##[ \t]*Daily (?:summary|review)\s+(\d{4}-\d{2}-\d{2})[ \t\r]*$").unwrap());
static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{4}-\d{2}-\d{2})").unwrap());

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    pub file: String,
    pub modified: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WikiRow {
    /// Path inside `wiki/`: `entities/Orbit App.md`.
    pub id: String,
    pub created: Option<String>,
    pub modified: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct WikiChanges {
    pub created: Vec<WikiRow>,
    pub edited: Vec<WikiRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceRow {
    pub file: String,
    pub imported: String,
    /// Not yet cited by any wiki page's `sources:`.
    pub pending: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub date: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Scratchpad {
    pub count: usize,
    pub dates: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DailyCoverage {
    pub present: Vec<String>,
    /// Days to reconstruct from the session logs.
    pub missing: Vec<String>,
    /// The week's daily summary files on file, under their new or old names.
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GhostLink {
    pub target: String,
    /// Wiki pages linking to it.
    pub pages: usize,
}

/// What only the weekly review gathers.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyInputs {
    pub daily_reviews: DailyCoverage,
    /// Every open task, as the To Do list's queries see them (manual order).
    pub open: Vec<ReviewTask>,
    /// The open tasks typed under `#### Other` on the To Do list.
    pub other: Vec<ReviewTask>,
    /// Done in the week, newest first.
    pub done: Vec<ReviewTask>,
    pub someday: Someday,
    pub stuck: Stuck,
    /// The ghost links with the most wiki pages linking to them.
    pub ghost_links: Vec<GhostLink>,
    pub ghost_link_count: usize,
    /// Every source not yet ingested, not only this week's.
    pub pending_sources: Vec<String>,
    /// Last week's block, for closing its loops.
    pub previous_review: Option<String>,
    /// The user's own review of the week (`Me. Weekly Review - YYYY-Www.md`), if they've finished
    /// the guided weekly review: its path and text.
    pub your_review: Option<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewInputs {
    pub window: Window,
    #[serde(serialize_with = "super::ser_time")]
    pub now: NaiveDateTime,
    /// Notes changed in the window, by name prefix (`1-1`, `Meeting`, `Other`…).
    pub journals: BTreeMap<String, Vec<FileRow>>,
    pub wiki: WikiChanges,
    /// Imported in the window and still in `sources/`.
    pub sources: Vec<SourceRow>,
    pub log: Vec<LogEntry>,
    pub scratchpad: Scratchpad,
    pub sessions: Vec<Session>,
    /// App-generated sessions counted by what they were.
    pub automated: BTreeMap<String, usize>,
    /// Every project note, done ones too, with its area: what decides Work or Personal.
    pub projects: Vec<ProjectArea>,
    pub weekly: Option<WeeklyInputs>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectArea {
    pub name: String,
    pub area: Option<String>,
}

impl ProjectArea {
    /// `Personal` for an area named Personal, `Work` for any other area, none without one.
    pub fn domain(&self) -> Option<&'static str> {
        let a = self.area.as_deref()?.trim();
        if a.is_empty() {
            None
        } else if a.eq_ignore_ascii_case("personal") {
            Some("Personal")
        } else {
            Some("Work")
        }
    }
}

/// The project a session's folder is named after (`~/Projects/orbit-app` for `Orbit App`),
/// comparing letters and digits only.
pub fn project_of<'a>(cwd: &str, projects: &'a [ProjectArea]) -> Option<&'a ProjectArea> {
    let key = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    let dir = key(cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or(""));
    if dir.is_empty() {
        return None;
    }
    projects.iter().find(|p| key(&p.name) == dir)
}

impl ReviewInputs {
    pub fn set_sessions(&mut self, sessions: Vec<Session>) {
        self.automated.clear();
        for s in &sessions {
            if let Some(a) = s.automated() {
                *self.automated.entry(a.to_string()).or_default() += 1;
            }
        }
        self.sessions = sessions;
    }
}

/// The window as local instants: [first day 00:00, the day after the last 00:00).
pub fn session_bounds(w: &Window) -> (NaiveDateTime, NaiveDateTime) {
    let start = w.first_day().and_hms_opt(0, 0, 0).unwrap_or_default();
    let end = (w.last_day() + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap_or_default();
    (start, end)
}

/// Everything for the review of `window`, at `now`. `projects` is Claude Code's projects folder
/// (normally `~/.claude/projects`); `automations` say which sessions an app made. Reading the session logs is most of the time; to hold the
/// index for less of it, call [`gather_vault`] and then [`ReviewInputs::set_sessions`] yourself.
pub fn gather(
    vault_root: &Path,
    index: &Index,
    projects: &Path,
    automations: &[Automation],
    window: &Window,
    now: NaiveDateTime,
) -> Result<ReviewInputs> {
    gather_in(vault_root, index, projects, automations, window, now, Zone::Local)
}

/// Everything but the sessions.
pub fn gather_vault(vault_root: &Path, index: &Index, window: &Window, now: NaiveDateTime) -> Result<ReviewInputs> {
    vault_in(vault_root, index, window, now, Zone::Local)
}

pub(crate) fn gather_in(
    vault_root: &Path,
    index: &Index,
    projects: &Path,
    automations: &[Automation],
    w: &Window,
    now: NaiveDateTime,
    zone: Zone,
) -> Result<ReviewInputs> {
    let mut inputs = vault_in(vault_root, index, w, now, zone)?;
    let (start, end) = session_bounds(w);
    inputs.set_sessions(sessions_in(projects, start, end, zone, automations));
    Ok(inputs)
}

struct FileInfo {
    path: String,
    layer: String,
    size: u64,
    day: String,
    /// The day the note is about, from its name or `date` property, when it names a whole day.
    date: Option<String>,
}

impl FileInfo {
    /// The day a journal note belongs to: its own date when it has one (a meeting note written up
    /// after midnight is still the meeting's day), else the day it last changed.
    fn journal_day(&self) -> &str {
        self.date.as_deref().filter(|d| d.len() == 10).unwrap_or(&self.day)
    }
}

fn files(index: &Index, zone: Zone) -> Result<Vec<FileInfo>> {
    let mut st = index.conn().prepare("SELECT path, layer, size, mtime, date FROM files ORDER BY path").map_err(|e| e.to_string())?;
    let rows = st
        .query_map([], |r| {
            Ok(FileInfo {
                path: r.get(0)?,
                layer: r.get(1)?,
                size: r.get::<_, Option<i64>>(2)?.unwrap_or(0) as u64,
                day: zone.ms_to_local(r.get::<_, Option<i64>>(3)?.unwrap_or(0)).format("%Y-%m-%d").to_string(),
                date: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<std::result::Result<_, _>>().map_err(|e| e.to_string())
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read(root.join(rel)).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// Whether a log entry belongs in the window's summary. One about a note that carries a day in its
/// name (`Meeting. Plan - 2026-10-02`) belongs to that day, so writing it up after midnight isn't
/// the next day's work; nor does one about a note that has since been renamed or deleted.
fn about_this_window(detail: &str, stems: &std::collections::HashSet<&str>, w: &Window) -> bool {
    let name = detail.trim_end_matches(".md");
    let Some(day) = crate::filename::parse(name).date.filter(|d| d.len() == 10) else { return true };
    if !name.contains(". ") {
        return true;
    }
    stems.contains(name) && in_window(&day, w)
}

fn in_window(day: &str, w: &Window) -> bool {
    w.start.as_str() <= day && day <= w.end.as_str()
}

fn vault_in(root: &Path, index: &Index, w: &Window, now: NaiveDateTime, zone: Zone) -> Result<ReviewInputs> {
    let all = files(index, zone)?;
    let wiki_pages: Vec<&FileInfo> = all.iter().filter(|f| f.layer == "wiki" && is_wiki_page(&f.path)).collect();
    let wiki_texts: Vec<(&FileInfo, String)> = wiki_pages.iter().map(|f| (*f, read(root, &f.path))).collect();

    let mut journals: BTreeMap<String, Vec<FileRow>> = BTreeMap::new();
    for f in all.iter().filter(|f| f.layer == "note" && f.path.ends_with(".md") && in_window(f.journal_day(), w)) {
        let key = PREFIX.captures(name_of(&f.path)).map_or("Other".to_string(), |c| c[1].to_string());
        journals.entry(key).or_default().push(FileRow { file: f.path.clone(), modified: f.day.clone(), size_bytes: f.size });
    }
    for rows in journals.values_mut() {
        rows.sort_by(|a, b| (&a.modified, &a.file).cmp(&(&b.modified, &b.file)));
    }

    let mut wiki = WikiChanges::default();
    for (f, text) in wiki_texts.iter().filter(|(f, _)| in_window(&f.day, w)) {
        let created = DATE.captures(fm_field(frontmatter_block(text), "created")).map(|c| c[1].to_string());
        let row = WikiRow { id: f.path["wiki/".len()..].to_string(), created: created.clone(), modified: f.day.clone() };
        // No `created:` counts as an old page edited: claiming an edit as new is the worse error.
        if created.is_some_and(|c| in_window(&c, w)) {
            wiki.created.push(row);
        } else {
            wiki.edited.push(row);
        }
    }
    wiki.created.sort_by(|a, b| a.id.cmp(&b.id));
    wiki.edited.sort_by(|a, b| a.id.cmp(&b.id));

    let cited: HashSet<String> = wiki_texts
        .iter()
        .flat_map(|(_, t)| WIKILINK.captures_iter(sources_block(frontmatter_block(t))).map(|c| lint_key(c[1].trim())).collect::<Vec<_>>())
        .collect();
    let top_sources: Vec<&FileInfo> =
        all.iter().filter(|f| f.layer == "source" && f.path.matches('/').count() == 1 && !name_of(&f.path).starts_with('.')).collect();
    let pending = |f: &FileInfo| {
        let name = name_of(&f.path);
        !cited.contains(&lint_key(stem(name))) && !cited.contains(&lint_key(name))
    };
    let sources = top_sources
        .iter()
        .filter(|f| in_window(&f.day, w))
        .map(|f| SourceRow { file: name_of(&f.path).to_string(), imported: f.day.clone(), pending: pending(f) })
        .collect();

    // The vault's notes by name, to tell which log entries are about a note dated another day.
    let stems: std::collections::HashSet<&str> =
        all.iter().filter(|f| f.path.ends_with(".md")).map(|f| name_of(&f.path).trim_end_matches(".md")).collect();
    let mut log: Vec<LogEntry> = read(root, LOG)
        .lines()
        .filter_map(|l| LOG_ENTRY.captures(l))
        .filter(|c| in_window(&c[1], w) && about_this_window(c[3].trim(), &stems, w))
        .map(|c| LogEntry { date: c[1].to_string(), kind: c[2].to_string(), detail: c[3].to_string() })
        .collect();
    log.sort_by(|a, b| a.date.cmp(&b.date));

    let dates: Vec<String> = read(root, SCRATCHPAD)
        .lines()
        .filter_map(|l| SCRATCH_BLOCK.captures(l.trim()))
        .map(|c| c[1].to_string())
        .filter(|d| in_window(d, w))
        .collect();
    let scratchpad = Scratchpad { count: dates.len(), dates };

    let weekly = if w.is_week() {
        let pending_sources = top_sources.iter().filter(|f| pending(f)).map(|f| f.path.clone()).collect();
        Some(weekly(root, index, w, &all, &wiki_texts, pending_sources)?)
    } else {
        None
    };
    Ok(ReviewInputs {
        window: w.clone(),
        now,
        journals,
        wiki,
        sources,
        log,
        scratchpad,
        sessions: Vec::new(),
        automated: BTreeMap::new(),
        projects: index.projects()?.into_iter().map(|p| ProjectArea { name: p.name, area: p.area }).collect(),
        weekly,
    })
}

/// A note by file name, anywhere in the vault (the root first), as the scripts looked for the
/// monthly files.
fn find_note<'a>(all: &'a [FileInfo], name: &str) -> Vec<&'a FileInfo> {
    let mut v: Vec<&FileInfo> = all.iter().filter(|f| f.layer == "note" && name_of(&f.path) == name).collect();
    v.sort_by_key(|f| f.path.matches('/').count());
    v
}

fn weekly(
    root: &Path,
    index: &Index,
    w: &Window,
    all: &[FileInfo],
    wiki: &[(&FileInfo, String)],
    pending_sources: Vec<String>,
) -> Result<WeeklyInputs> {
    // The week's daily summary files under their new names and their old: the ones on file, or
    // the new names when there are none yet.
    let names = w.daily_files.clone().unwrap_or_default();
    let mut files = Vec::new();
    let mut present = BTreeSet::new();
    for name in names.iter().flat_map(|n| target::with_old(n)) {
        let found = find_note(all, &name);
        if !found.is_empty() {
            files.push(name);
        }
        for f in found {
            present.extend(DAILY_HEADING.captures_iter(&read(root, &f.path)).map(|c| c[1].to_string()));
        }
    }
    if files.is_empty() {
        files = names;
    }
    let daily_reviews = DailyCoverage {
        present: w.days.iter().filter(|d| present.contains(*d)).cloned().collect(),
        missing: w.days.iter().filter(|d| !present.contains(*d)).cloned().collect(),
        files,
    };

    let task = |q: TaskQuery| -> Result<Vec<ReviewTask>> { Ok(index.tasks(&q)?.iter().map(ReviewTask::from).collect()) };
    let open = task(TaskQuery { status: Status::NotDone, sort: Sort::Manual, ..Default::default() })?;
    let other = open.iter().filter(|t| t.path == TODO && t.heading.as_deref() == Some("Other")).cloned().collect();
    let done = task(TaskQuery { status: Status::Done, sort: Sort::DoneDesc, ..Default::default() })?
        .into_iter()
        .filter(|t| t.done_on.as_deref().is_some_and(|d| in_window(d, w)))
        .collect();
    let week_no: u32 = w.label().rsplit('W').next().and_then(|n| n.parse().ok()).unwrap_or(0);
    let pool =
        task(TaskQuery { status: Status::NotDone, tags_include: vec!["#someday-maybe".into()], sort: Sort::Manual, ..Default::default() })?;

    // This month's weekly summaries and last week's file, which is last month's at a boundary,
    // under their new names and their old.
    let mut review_files = target::with_old(&w.file);
    if w.prev_file != w.file {
        review_files.extend(target::with_old(&w.prev_file));
    }
    let texts: Vec<String> = review_files.iter().filter_map(|n| find_note(all, n).first().map(|f| read(root, &f.path))).collect();
    let stuck = stuck(&open, &review_blocks(&texts), w.label(), 2, 5);

    // Last week's block, wherever it is: the new file first, under either heading.
    let previous_review = target::with_old(&w.prev_file).iter().find_map(|name| {
        let text = read(root, &find_note(all, name).first()?.path).replace("\r\n", "\n");
        let lines: Vec<&str> = text.split('\n').collect();
        target::with_old(&w.prev_heading)
            .iter()
            .find_map(|h| super::write::block_bounds(&lines, h))
            .map(|(a, b)| lines[a..b].join("\n").trim_end().to_string())
    });

    let your_review = find_note(all, &super::note::file_name(w.label()))
        .first()
        .map(|f| (f.path.clone(), read(root, &f.path).replace("\r\n", "\n").trim_end().to_string()));

    let (ghost_links, ghost_link_count) = ghosts(root, wiki);
    Ok(WeeklyInputs {
        daily_reviews,
        other,
        done,
        someday: someday(&pool, week_no, 3),
        stuck,
        open,
        ghost_links,
        ghost_link_count,
        pending_sources,
        previous_review,
        your_review,
    })
}

/// Wikilinks in wiki pages to nothing in the vault: the most-linked first, and how many there are.
fn ghosts(root: &Path, wiki: &[(&FileInfo, String)]) -> (Vec<GhostLink>, usize) {
    let names = names(root);
    let mut missing: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for (f, text) in wiki {
        for c in WIKILINK.captures_iter(text) {
            let t = c[1].trim();
            if !names.contains(&lint_key(t)) {
                missing.entry(t.to_string()).or_default().insert(&f.path);
            }
        }
    }
    let count = missing.len();
    let mut v: Vec<GhostLink> = missing.into_iter().map(|(target, pages)| GhostLink { target, pages: pages.len() }).collect();
    v.sort_by(|a, b| b.pages.cmp(&a.pages).then_with(|| a.target.cmp(&b.target)));
    v.truncate(TOP_GHOSTS);
    (v, count)
}

fn short(t: NaiveDateTime) -> String {
    t.format("%m-%d %H:%M").to_string()
}

fn task_line(t: &ReviewTask) -> String {
    let mut s = format!("- {} — {}", t.text, t.path);
    if let Some(h) = &t.heading {
        s.push_str(&format!(" › {h}"));
    }
    s
}

/// The inputs as the text the model is given after the workflow's instructions.
pub fn render(i: &ReviewInputs) -> String {
    let w = &i.window;
    let mut o = String::new();
    let mut line = |s: &str| {
        o.push_str(s);
        o.push('\n');
    };
    let kind = if w.is_week() { "weekly" } else { "daily" };
    line(&format!("# Inputs for the {kind} review {}", w.label()));
    line("");
    line(&format!("- Now: {} (local time)", i.now.format("%Y-%m-%d %H:%M")));
    line(&format!("- Window: {} to {}", w.start, w.end));
    line(&format!("- Heading: `{}`", w.heading));
    line(&format!("- File: [[{}]]", w.file.trim_end_matches(".md")));
    line(&format!("- Previous: `{}` in [[{}]]", w.prev_heading, w.prev_file.trim_end_matches(".md")));

    line("");
    line("## Notes changed in the window");
    if i.journals.is_empty() {
        line("None.");
    }
    for (prefix, rows) in &i.journals {
        line(&format!("### {prefix}"));
        for r in rows {
            line(&format!("- {} (modified {}, {} bytes)", r.file, r.modified, r.size_bytes));
        }
    }

    line("");
    line(&format!("## Scratchpad: {} entries", i.scratchpad.count));
    if !i.scratchpad.dates.is_empty() {
        line(&format!("Blocks dated {} in [[Me. Scratchpad]].", i.scratchpad.dates.join(", ")));
    }

    line("");
    line(&format!("## Wiki pages: {} created, {} edited", i.wiki.created.len(), i.wiki.edited.len()));
    for (label, rows) in [("Created", &i.wiki.created), ("Edited", &i.wiki.edited)] {
        for r in rows.iter() {
            let created = r.created.as_deref().map(|c| format!(", created {c}")).unwrap_or_default();
            line(&format!("- {label}: wiki/{} (modified {}{created})", r.id, r.modified));
        }
    }

    line("");
    line("## Sources imported in the window");
    if i.sources.is_empty() {
        line("None.");
    }
    for s in &i.sources {
        line(&format!("- sources/{} (imported {}, {})", s.file, s.imported, if s.pending { "pending" } else { "ingested" }));
    }

    line("");
    line("## log.md entries in the window");
    if i.log.is_empty() {
        line("None.");
    }
    for l in &i.log {
        line(&format!("- {} {} | {}", l.date, l.kind, l.detail));
    }

    line("");
    line("## Projects and their areas");
    line("A session or note that serves one of these projects takes its domain: area Personal is Personal, any other area is Work. Label it with the project's name.");
    if i.projects.is_empty() {
        line("None.");
    }
    for p in &i.projects {
        match (p.area.as_deref(), p.domain()) {
            (Some(a), Some(d)) => line(&format!("- {}: area {a} ({d})", p.name)),
            _ => line(&format!("- {}: no area", p.name)),
        }
    }

    line("");
    let automated: usize = i.automated.values().sum();
    line(&format!("## Claude Code sessions: {} with real activity, {automated} of them automated", i.sessions.len()));
    // App-generated runs are only counted, below.
    for s in i.sessions.iter().filter(|s| s.automated().is_none()) {
        line("");
        line(&format!("### {}/{} [{}]", s.project, s.short_id(), s.kind));
        line(&format!(
            "- In window: {} -> {} ({} of {} user msgs)",
            short(s.first_in_window),
            short(s.last_in_window),
            s.user_msgs_in_window,
            s.user_msgs_total
        ));
        if s.spans_days {
            line(&format!(
                "- Full span: {} -> {} (session spans days: continued, not started)",
                short(s.session_start),
                short(s.session_end)
            ));
        }
        line(&format!("- cwd: {}", s.cwd));
        if let Some(d) = project_of(&s.cwd, &i.projects).and_then(|p| p.domain().map(|d| (p, d))) {
            line(&format!("- Project: {} ({})", d.0.name, d.1));
        }
        line(&format!("- Log: {}", s.path));
        for m in s.messages.iter().take(MESSAGES) {
            let m: String = m.chars().take(MESSAGE_CHARS).collect();
            line(&format!("  - {}", m.replace('\n', " ")));
        }
        if s.messages.len() > MESSAGES {
            line(&format!("  - … {} more", s.messages.len() - MESSAGES));
        }
    }
    if !i.automated.is_empty() {
        line("");
        line("Automated runs by type (not work; count them, don't write them up):");
        for (k, n) in &i.automated {
            line(&format!("- {k}: {n}"));
        }
    }

    if let Some(wk) = &i.weekly {
        line("");
        line("## Daily summaries for the week");
        line(&format!(
            "- Present: {}",
            if wk.daily_reviews.present.is_empty() { "none".into() } else { wk.daily_reviews.present.join(", ") }
        ));
        line(&format!(
            "- Missing (reconstruct these days from the sessions, and say so): {}",
            if wk.daily_reviews.missing.is_empty() { "none".into() } else { wk.daily_reviews.missing.join(", ") }
        ));
        line(&format!("- In: {}", wk.daily_reviews.files.join(", ")));

        line("");
        line(&format!("## Open tasks: {}", wk.open.len()));
        line("");
        line(&format!("### `#### Other` on the To Do list: {}", wk.other.len()));
        for t in &wk.other {
            line(&task_line(t));
        }
        line("");
        line(&format!("### Closed this week: {}", wk.done.len()));
        for t in &wk.done {
            line(&format!("{} (✅ {})", task_line(t), t.done_on.as_deref().unwrap_or("")));
        }
        line("");
        line(&format!("### Someday/Maybe picks ({} in the pool)", wk.someday.pool));
        for t in &wk.someday.picks {
            line(&task_line(t));
        }
        line("");
        if wk.stuck.enough {
            line(&format!("### Stuck: open, and in each of the weekly reviews {} ({})", wk.stuck.prior_reviews.join(", "), wk.stuck.count));
            for t in &wk.stuck.stuck {
                line(&task_line(t));
            }
        } else {
            line("### Stuck");
            line(wk.stuck.note.as_deref().unwrap_or(""));
        }
        line("");
        line("### Every open task");
        for t in &wk.open {
            line(&task_line(t));
        }

        line("");
        line(&format!("## Missing pages (ghost links in wiki pages): {}", wk.ghost_link_count));
        for g in &wk.ghost_links {
            line(&format!("- [[{}]] — linked from {} wiki page(s)", g.target, g.pages));
        }
        line("");
        line(&format!("## Pending sources (all): {}", wk.pending_sources.len()));
        for s in &wk.pending_sources {
            line(&format!("- {s}"));
        }
        line("");
        line(&format!("## Previous review: `{}` in [[{}]]", w.prev_heading, w.prev_file.trim_end_matches(".md")));
        match &wk.previous_review {
            Some(b) => {
                line("");
                line("````markdown");
                line(b);
                line("````");
            }
            None => line("None on file: this is the first review."),
        }
        line("");
        match &wk.your_review {
            Some((path, text)) => {
                line(&format!("## The user's own review of the week: [[{}]]", path.trim_end_matches(".md")));
                line("");
                line("````markdown");
                line(text);
                line("````");
            }
            None => line("## The user's own review of the week: not finished yet."),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reviews::target::{window, Target};
    use crate::reviews::{fixture, tasks};
    use crate::vault::Vault;

    fn setup(tmp: &Path) -> (std::path::PathBuf, Index) {
        let root = fixture::vault(tmp);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&Vault::new(&root, vec!["archived".into()])).unwrap();
        (root, ix)
    }

    fn now() -> NaiveDateTime {
        NaiveDateTime::parse_from_str("2026-10-05 07:30", "%Y-%m-%d %H:%M").unwrap()
    }

    /// Where Brainstead's notes differ from the script's `rglob("*.md")`: `log.md` is the vault's
    /// schema, `… 2.md` beside `….md` is a OneDrive conflict copy, and `Templates/` holds templates
    /// (the script skipped only a lower-case `templates`).
    /// `1-1. Maya - 2026-09-23` was only edited in W40: it belongs to its own day's week now.
    const NOT_NOTES: [&str; 4] =
        ["log.md", "Meeting. Orbit App Steerco - 2026-09-30 2.md", "Templates/Meeting.md", "1-1. Maya - 2026-09-23.md"];

    fn journals_without(want: &serde_json::Value) -> serde_json::Value {
        let mut out = serde_json::Map::new();
        for (k, rows) in want.as_object().unwrap() {
            let rows: Vec<_> =
                rows.as_array().unwrap().iter().filter(|r| !NOT_NOTES.contains(&r["file"].as_str().unwrap())).cloned().collect();
            if !rows.is_empty() {
                out.insert(k.clone(), rows.into());
            }
        }
        out.into()
    }

    #[test]
    fn log_entries_about_notes_dated_elsewhere_are_left_out() {
        let w = window(Target::parse("2026-10-03").unwrap());
        let stems: std::collections::HashSet<&str> =
            ["Meeting. Orbit App launch - 2026-10-02", "Idea. Pricing", "Meeting. Acme - 2026-10-03"].into();
        // Written up after midnight: the meeting's own day, not this one.
        assert!(!about_this_window("Meeting. Orbit App launch - 2026-10-02", &stems, &w));
        // Renamed since (the log keeps the old name).
        assert!(!about_this_window("Meeting. Orbit App launch - 2026-10-03", &stems, &w));
        assert!(about_this_window("Meeting. Acme - 2026-10-03", &stems, &w));
        assert!(about_this_window("Idea. Pricing", &stems, &w));
        assert!(about_this_window("daily-summary 2026-10-02", &stems, &w));
        assert!(about_this_window("Me. Daily Summaries - 2026-10", &stems, &w));
    }

    #[test]
    fn a_dated_note_belongs_to_its_own_day() {
        let f = |day: &str, date: Option<&str>| FileInfo {
            path: "x.md".into(),
            layer: "note".into(),
            size: 1,
            day: day.into(),
            date: date.map(Into::into),
        };
        // Written up after midnight: still the meeting's day.
        assert_eq!(f("2026-10-03", Some("2026-10-02")).journal_day(), "2026-10-02");
        // No date, or only a month: the day it changed.
        assert_eq!(f("2026-10-03", None).journal_day(), "2026-10-03");
        assert_eq!(f("2026-10-03", Some("2026-10")).journal_day(), "2026-10-03");
    }

    #[test]
    fn week_matches_week_inputs_py() {
        let tmp = tempfile::tempdir().unwrap();
        let (root, ix) = setup(tmp.path());
        let projects = fixture::projects(tmp.path());
        let w = window(Target::parse("2026-W40").unwrap());
        let got = gather_in(&root, &ix, &projects, &fixture::automations(), &w, now(), fixture::zone()).unwrap();
        let want = fixture::expected("week-inputs.json");

        // The script wrote the old names; the fixture's review files still have them, so this is
        // also the test that the old files and headings are read.
        assert_eq!(serde_json::to_value(&got.window).unwrap(), fixture::renamed(want["window"].clone()));
        assert_eq!(serde_json::to_value(&got.journals).unwrap(), journals_without(&want["journals"]));
        assert_eq!(serde_json::to_value(&got.wiki).unwrap(), want["wiki"]);
        assert_eq!(serde_json::to_value(&got.sources).unwrap(), want["sources"]["all"]);
        assert_eq!(serde_json::to_value(&got.log).unwrap(), want["log"]);
        assert_eq!(serde_json::to_value(&got.scratchpad).unwrap(), want["scratchpad"]);
        let wk = got.weekly.as_ref().unwrap();
        assert_eq!(serde_json::to_value(&wk.daily_reviews).unwrap(), want["dailyReviews"]);

        // Ghost links and pending sources, as lint_wiki.py lists them.
        let lint = fixture::expected("lint.json");
        let missing = lint["missingPages"].as_array().unwrap();
        assert_eq!(wk.ghost_link_count, missing.len());
        for g in &wk.ghost_links {
            let l = format!("[[{}]]  — linked from {} wiki page(s)", g.target, g.pages);
            assert!(missing.iter().any(|m| m == l.as_str()), "{l}");
        }
        assert_eq!(wk.ghost_links[0].target, "Missing Concept");
        assert_eq!(serde_json::to_value(&wk.pending_sources).unwrap(), lint["uningestedSources"]);

        // Tasks from the index, as the BFF's queries and stuck_tasks.py gave them.
        let input = fixture::json("tasks.json");
        let open: Vec<tasks::ReviewTask> = serde_json::from_value(input["open"].clone()).unwrap();
        assert_eq!(wk.open, open);
        let stuck = fixture::expected("stuck.json");
        assert_eq!(wk.stuck.prior_reviews, ["2026-W39", "2026-W38"]);
        assert_eq!(wk.stuck.stuck.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), [stuck["stuck"][0]["id"].as_str().unwrap()]);
        let someday = fixture::expected("someday.json");
        assert_eq!(
            wk.someday.picks.iter().map(|t| serde_json::Value::from(t.id.clone())).collect::<Vec<_>>(),
            someday["picks"].as_array().unwrap().iter().map(|t| t["id"].clone()).collect::<Vec<_>>()
        );
        assert_eq!(wk.done.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["Send the minutes ✅ 2026-09-30"]);
        assert_eq!(wk.other.len(), 8);
        assert!(wk.previous_review.as_deref().unwrap().starts_with("## Weekly review 2026-W39\n"));
        assert!(wk.previous_review.as_deref().unwrap().ends_with("- Land the Orbit App soft-launch date."));

        // The sessions are session_logs.py's for Monday to Monday.
        assert_eq!(got.sessions.len(), fixture::expected("sessions-week.json").as_array().unwrap().len());
        assert_eq!(got.automated["batchbot record categorisation"], 1);

        let text = render(&got);
        for s in [
            "# Inputs for the weekly review 2026-W40",
            "- Missing (reconstruct these days from the sessions, and say so): 2026-09-30, 2026-10-02, 2026-10-03, 2026-10-04",
            "### Stuck: open, and in each of the weekly reviews 2026-W39, 2026-W38 (1)",
            "- [[Missing Concept]] — linked from 3 wiki page(s)",
            "- batchbot pending summary: 1",
        ] {
            assert!(text.contains(s), "{s}\n\n{text}");
        }
    }

    #[test]
    fn day_matches_the_scripts_selection() {
        let tmp = tempfile::tempdir().unwrap();
        let (root, ix) = setup(tmp.path());
        let projects = fixture::projects(tmp.path());
        let w = window(Target::parse("2026-09-30").unwrap());
        let got = gather_in(&root, &ix, &projects, &fixture::automations(), &w, now(), fixture::zone()).unwrap();
        let want = fixture::expected("day-inputs.json");
        assert_eq!(serde_json::to_value(&got.journals).unwrap(), journals_without(&want["journals"]));
        assert_eq!(serde_json::to_value(&got.wiki).unwrap(), want["wiki"]);
        assert_eq!(serde_json::to_value(&got.sources).unwrap(), want["sources"]);
        assert_eq!(serde_json::to_value(&got.log).unwrap(), want["log"]);
        assert_eq!(serde_json::to_value(&got.scratchpad).unwrap(), want["scratchpad"]);
        assert!(got.weekly.is_none());
        assert_eq!(got.sessions.len(), fixture::expected("sessions-day.json").as_array().unwrap().len());
        let text = render(&got);
        assert!(text.contains("- Heading: `## Daily summary 2026-09-30`"));
        assert!(text.contains("- sources/Roadmap Update 2026-09-18.md (imported 2026-09-30, ingested)"));
        assert!(!text.contains("## Open tasks"));
        assert!(text.contains("- Full span: 09-29 22:00 -> 10-01 00:00 (session spans days: continued, not started)"));
    }

    #[test]
    fn a_projects_area_decides_its_domain() {
        let tmp = tempfile::tempdir().unwrap();
        let (root, ix) = setup(tmp.path());
        let w = window(Target::parse("2026-09-30").unwrap());
        let mut got = gather_vault(&root, &ix, &w, now()).unwrap();
        let garden = got.projects.iter().find(|p| p.name == "Garden").unwrap();
        assert_eq!(garden.domain(), Some("Personal"));
        assert_eq!(project_of("/home/me/Projects/orbit-app-launch/", &got.projects).unwrap().domain(), Some("Work"));
        assert_eq!(project_of("/home/me/Projects/garden", &got.projects).unwrap().name, "Garden");
        assert_eq!(project_of(r"C:\Users\me\Projects\garden\", &got.projects).unwrap().name, "Garden");
        assert!(project_of("/home/me/Projects", &got.projects).is_none());
        let text = render(&got);
        assert!(text.contains("- Garden: area Personal (Personal)"), "{text}");
        assert!(text.contains("- Orbit App launch: area Work (Work)"), "{text}");
        got.projects.push(ProjectArea { name: "Alpha".into(), area: Some("Personal".into()) });
        let sessions = sessions_in(
            &fixture::projects(tmp.path()),
            session_bounds(&w).0,
            session_bounds(&w).1,
            fixture::zone(),
            &fixture::automations(),
        );
        got.set_sessions(sessions);
        assert!(render(&got).contains("- cwd: /home/me/Projects/alpha\n- Project: Alpha (Personal)\n"));
    }

    #[test]
    fn reads_the_new_names_beside_the_old() {
        let tmp = tempfile::tempdir().unwrap();
        let root = fixture::vault(tmp.path());
        // October's daily file and September's weekly file under the new names, beside the old ones.
        std::fs::write(root.join("Me. Daily Summaries - 2026-10.md"), "> note\n\n## Daily summary 2026-10-02\n\n- a\n").unwrap();
        std::fs::write(
            root.join("Me. Weekly Summaries - 2026-09.md"),
            "> note\n\n## Weekly summary 2026-W39\n\n### Next week\n\n- From the new file.\n",
        )
        .unwrap();
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&Vault::new(&root, vec!["archived".into()])).unwrap();
        let w = window(Target::parse("2026-W40").unwrap());
        let got = gather_vault(&root, &ix, &w, now()).unwrap();
        let wk = got.weekly.as_ref().unwrap();
        // 09-28 and 09-29 from the old September file, 10-01 from the old October one, 10-02 from the new.
        assert_eq!(wk.daily_reviews.present, ["2026-09-28", "2026-09-29", "2026-10-01", "2026-10-02"]);
        assert_eq!(
            wk.daily_reviews.files,
            ["Me. Daily Reviews - 2026-09.md", "Me. Daily Summaries - 2026-10.md", "Me. Daily Reviews - 2026-10.md"]
        );
        // The new file's block is last week's, ahead of the old file's.
        assert!(wk.previous_review.as_deref().unwrap().ends_with("- From the new file."));
        // Both W39 blocks are read for the stuck check, once.
        assert_eq!(wk.stuck.prior_reviews, ["2026-W39", "2026-W38"]);
        assert!(wk.your_review.is_none());
        assert!(render(&got).contains("## The user's own review of the week: not finished yet."));
    }

    #[test]
    fn reads_the_weeks_own_review_note() {
        let tmp = tempfile::tempdir().unwrap();
        let root = fixture::vault(tmp.path());
        std::fs::write(root.join("Me. Weekly Review - 2026-W40.md"), "# Weekly review 2026-W40\r\n\r\nA good week.\r\n").unwrap();
        std::fs::write(root.join("Me. Weekly Review - 2026-W39.md"), "# Weekly review 2026-W39\n\nLast week.\n").unwrap();
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&Vault::new(&root, vec!["archived".into()])).unwrap();
        let w = window(Target::parse("2026-W40").unwrap());
        let got = gather_vault(&root, &ix, &w, now()).unwrap();
        let (path, text) = got.weekly.as_ref().unwrap().your_review.clone().unwrap();
        assert_eq!((path.as_str(), text.as_str()), ("Me. Weekly Review - 2026-W40.md", "# Weekly review 2026-W40\n\nA good week."));
        let r = render(&got);
        assert!(r.contains("## The user's own review of the week: [[Me. Weekly Review - 2026-W40]]\n\n````markdown\n# Weekly review 2026-W40\n\nA good week.\n````"), "{r}");
        assert!(!r.contains("Last week."));
    }

    #[test]
    fn script_helpers() {
        assert_eq!(stem("a.tar.gz"), "a.tar");
        assert_eq!(stem(".hidden"), ".hidden");
        assert_eq!(stem("file."), "file.");
        let fm = "title: x\nsources:\n  - \"[[A]]\"\n- [[B]]\ntype: y\nalso: [[C]]";
        assert_eq!(sources_block(fm), "sources:\n  - \"[[A]]\"\n- [[B]]\n");
        assert_eq!(sources_block("sources: [[A]]"), "sources: [[A]]");
        assert_eq!(fm_field("created: 2026-09-30 10:00\n", "created"), "2026-09-30 10:00");
        assert_eq!(frontmatter_block("---\na: 1\n---\nbody"), "a: 1");
        assert_eq!(frontmatter_block("---\na: 1\n---"), "");
    }
}
