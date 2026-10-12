// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The weekly review's preparation (src-tauri/src/weekprep.rs runs it): when it runs, what the
//! model is given for a week, and the checks on the suggestions it answers with.
//!
//! The model gets the week's dated notes and meetings in full, the open tasks with their age, the
//! Inbox, projects with no next action, waiting-fors older than a week, someday items and what's
//! dated in the next two weeks, and answers with suggestions as JSON. Each is checked: a known
//! step and action, refs to tasks and Inbox items it was given, a quote that's really in its note
//! (as an ingest's quotes are checked), and nothing that's already a task. Nothing here writes:
//! the window applies an accepted suggestion through the app's own undoable actions, and a link
//! is made as an agent change (in Changes).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use super::schedule::{self, Weekday};
use super::target::{self, Target, Window};
use crate::inbox::InboxItem;
use crate::index::Index;
use crate::proposals::normalise_quote;

/// The model's instructions.
pub const WORKFLOW: &str = include_str!("../../workflows/weekly-prep.md");

/// How long before the weekly review the preparation runs, on the review day.
pub const LEAD_HOURS: i64 = 4;
/// A waiting-for older than this many days is listed to chase.
pub const WAITING_DAYS: i64 = 7;
/// How far the `ahead` step looks.
pub const AHEAD_DAYS: i64 = 14;
/// A note's text is cut at this many characters in the prompt.
const NOTE_CHARS: usize = 8_000;
const MAX_NOTES: usize = 30;
/// At most this many next actions, someday items and Inbox items go in the prompt.
const MAX_TASKS: usize = 150;
const MAX_INBOX: usize = 40;

/// The review's steps, as src/Weekly.tsx has them.
pub const STEPS: [&str; 10] = ["loose", "inbox", "notes", "back", "next", "ahead", "projects", "waiting", "someday", "creative"];

/// What a clarified Inbox item can become, as the Inbox's own suggestions have it.
pub const BECOMES: [&str; 7] = ["next", "project", "waiting", "done", "someday", "reference", "delete"];

/// What kind of Inbox item this is, for what it can become: a thought, a task, or a capture by its
/// file: a Teams transcript, an email or chat thread, another capture ingest can read, or a file it
/// can't. src/inboxActions.ts's suggestKind is the same rule, for the window.
pub fn inbox_kind(kind: &str, path: &str) -> &'static str {
    match kind {
        "thought" => return "thought",
        "task" => return "task",
        _ => {}
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let source = path.starts_with("sources/");
    if source && name.starts_with("Teams. Transcript.") {
        "transcript"
    } else if source && (name.starts_with("Email. ") || name.starts_with("Teams. Chat. ")) {
        "thread"
    } else if crate::extract::ingestable(path) && !path.starts_with("wiki/") && !path.starts_with("Templates/") {
        "capture"
    } else {
        "file"
    }
}

/// Whether an Inbox item of this kind (`inbox_kind`) can become `becomes`, as the Inbox offers it
/// (src/inboxActions.ts's inboxChoices; both are checked against tests/fixtures/inbox-choices.json).
/// A capture isn't "done now" (Keep in Sources takes it out), only a thread is waited on or
/// answered, ingest is for a file ingest can read, and a transcript is a meeting note, kept or
/// trashed.
pub fn fits(kind: &str, becomes: &str) -> bool {
    match kind {
        "thought" | "task" => matches!(becomes, "next" | "project" | "waiting" | "done" | "someday" | "reference" | "delete"),
        "transcript" => matches!(becomes, "meeting" | "ingest" | "ask" | "reference" | "delete"),
        "thread" => matches!(becomes, "reply" | "ingest" | "ask" | "next" | "project" | "waiting" | "someday" | "reference" | "delete"),
        "capture" => matches!(becomes, "ingest" | "ask" | "next" | "project" | "someday" | "reference" | "delete"),
        "file" => matches!(becomes, "next" | "project" | "someday" | "reference" | "delete"),
        _ => false,
    }
}

/// The most suggestions a step keeps.
fn cap(step: &str) -> usize {
    match step {
        "loose" => 5,
        "creative" => 3,
        "inbox" => MAX_INBOX,
        _ => 8,
    }
}

// ---- when ------------------------------------------------------------------------------------

/// The ISO week a review done on `today` is for, by the weekly summary's rule: on a Monday or
/// Tuesday, the week just ended; otherwise this one (`reviewWeek` in src/Weekly.tsx).
pub fn review_week(today: NaiveDate) -> String {
    schedule::weekly_target(today.and_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap())).0
}

/// When the preparation runs on the review day: `LEAD_HOURS` before the review, or at midnight
/// when that would be the day before.
pub fn prep_time(review: &str) -> String {
    let r = schedule::parse_time(review).unwrap_or(NaiveTime::from_hms_opt(16, 0, 0).unwrap());
    let (t, wrapped) = r.overflowing_sub_signed(Duration::hours(LEAD_HOURS));
    let t = if wrapped != 0 { NaiveTime::MIN } else { t };
    t.format("%H:%M").to_string()
}

/// The latest preparation time at or before `now`.
pub fn last_due(now: NaiveDateTime, day: Weekday, review: &str) -> NaiveDateTime {
    schedule::last_occurrence(now, &prep_time(review), Some(day))
}

/// The next preparation time after `now`.
pub fn next_due(now: NaiveDateTime, day: Weekday, review: &str) -> NaiveDateTime {
    schedule::next_occurrence(now, &prep_time(review), Some(day))
}

/// Whether a scheduled preparation is due: its time has passed since it last fired (or since the
/// schedule was taken on, `last`). Never before a first stamp.
pub fn is_due(now: NaiveDateTime, day: Weekday, review: &str, last: Option<NaiveDateTime>) -> bool {
    last.is_some_and(|l| l < last_due(now, day, review))
}

// ---- inputs ----------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub path: String,
    pub meeting: bool,
    pub text: String,
}

/// An open task as the model sees it, with what the window needs to find it again.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    /// `T1`, `T2`… in the prompt.
    pub r#ref: String,
    pub path: String,
    /// 0-based, as the index has it.
    pub line: i64,
    pub line_text: String,
    pub text: String,
    /// next, waiting or someday.
    pub list: String,
    pub due: Option<String>,
    pub scheduled: Option<String>,
    pub start: Option<String>,
    pub project: Option<String>,
    /// Days since it was created, or since its file last changed.
    pub age: i64,
    /// Dated in the next `AHEAD_DAYS`.
    pub ahead: bool,
}

/// An Inbox item still to clarify.
#[derive(Debug, Clone, PartialEq)]
pub struct Inbox {
    /// `I1`, `I2`… in the prompt.
    pub r#ref: String,
    pub item: InboxRef,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stuck {
    pub path: String,
    pub name: String,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub week: String,
    pub start: String,
    pub end: String,
    /// `YYYY-MM-DD`.
    pub today: String,
    pub notes: Vec<Note>,
    pub tasks: Vec<Task>,
    pub inbox: Vec<Inbox>,
    /// Active projects with no next action.
    pub stuck: Vec<Stuck>,
    /// Notes dated in the next `AHEAD_DAYS` by their names (meetings booked ahead).
    pub ahead_notes: Vec<String>,
    /// Active projects' names.
    pub projects: Vec<String>,
    /// Every task's words, open or done, to keep suggestions that are already tasks out.
    pub known: Vec<String>,
    /// Every note's and wiki page's name, lower case, to check a link's target.
    pub pages: HashSet<String>,
    /// What the vault looked like, for `stamp`: every open task's line and every note read.
    seen: Vec<String>,
}

/// Whether a note's name carries a day (`Meeting. Plan - 2026-10-01`) from `start` to `end`.
pub fn dated_in(file: &str, start: &str, end: &str) -> bool {
    note_day(file).is_some_and(|d| start <= d.as_str() && d.as_str() <= end)
}

fn note_day(file: &str) -> Option<String> {
    let name = file.rsplit('/').next().unwrap_or(file);
    crate::filename::parse(name).date.filter(|d| d.len() == 10)
}

/// Meetings are looked back on in their own step: `Meeting.`, `1-1.` and `Interview.` notes.
pub fn is_meeting(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    ["Meeting. ", "1-1. ", "Interview. "].iter().any(|p| name.starts_with(p))
}

pub fn week_window(week: &str) -> Result<Window, String> {
    let (year, w) = target::parse_iso_week(week)?;
    Ok(target::window(Target::Week { year, week: w }))
}

fn has_tag(tags: &[String], tag: &str) -> bool {
    tags.iter().map(|t| t.trim_start_matches('#')).any(|t| t == tag || t.starts_with(&format!("{tag}/")))
}

/// Still to clarify, by the window's rule (`unclarified` in src/gtd.ts): a capture or a thought,
/// or an open To Do task with no project, context or GTD tag.
pub fn unclarified(i: &InboxItem) -> bool {
    static TICKED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*[-*+]\s+\[[^ ]\]").unwrap());
    static CLARIFIED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"#context/|\[\[Project\. |#(waiting-for|someday-maybe|followup)\b").unwrap());
    i.kind != "task" || !(TICKED.is_match(&i.line_text) || CLARIFIED.is_match(&i.line_text))
}

fn files(ix: &Index, layers: &str) -> Result<Vec<String>, String> {
    let mut st =
        ix.conn().prepare(&format!("SELECT path FROM files WHERE layer IN ({layers}) ORDER BY path")).map_err(|e| e.to_string())?;
    let rows = st.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// What the preparation of `week` reads, from the index, the notes on disk and the Inbox
/// (`inbox`: what the Inbox lists, captures included). `now_ms` dates tasks with no created date.
pub fn gather(root: &Path, ix: &Index, week: &str, today: NaiveDate, now_ms: i64, inbox: &[InboxItem]) -> Result<Inputs, String> {
    let w = week_window(week)?;
    let iso = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let (from, until) = (iso(today), iso(today + Duration::days(AHEAD_DAYS)));
    let mut seen: Vec<String> = Vec::new();

    let notes_all = files(ix, "'note'")?;
    let mut notes: Vec<Note> = notes_all
        .iter()
        .filter(|p| p.ends_with(".md") && dated_in(p, &w.start, &w.end))
        .filter_map(|p| Some(Note { path: p.clone(), meeting: is_meeting(p), text: std::fs::read_to_string(root.join(p)).ok()? }))
        .collect();
    // The newest when there are too many, shown oldest first.
    notes.sort_by(|a, b| (note_day(&b.path), &b.path).cmp(&(note_day(&a.path), &a.path)));
    notes.truncate(MAX_NOTES);
    notes.reverse();
    seen.extend(notes.iter().map(|n| format!("{}\n{}", n.path, n.text)));
    let ahead_notes: Vec<String> = notes_all
        .iter()
        .filter(|p| p.ends_with(".md") && note_day(p).is_some_and(|d| from < d && d <= until))
        .map(|p| crate::filename::stem(p).to_string())
        .collect();
    seen.extend(ahead_notes.iter().cloned());
    let pages: HashSet<String> =
        files(ix, "'note', 'wiki'")?.iter().filter(|p| p.ends_with(".md")).map(|p| crate::filename::stem(p).to_lowercase()).collect();

    let open_inbox: Vec<&InboxItem> = inbox.iter().filter(|i| unclarified(i)).take(MAX_INBOX).collect();
    seen.extend(open_inbox.iter().map(|i| format!("{}:{}", i.path, i.line_text)));
    let in_inbox: HashSet<(String, i64)> =
        open_inbox.iter().filter(|i| i.kind == "task").map(|i| (i.path.clone(), i.line as i64)).collect();

    let all = ix.all_tasks().map_err(|e| e.to_string())?;
    let projects = ix.projects().map_err(|e| e.to_string())?;
    let name_of = |p: &str| projects.iter().find(|x| x.path == p).map(|x| x.name.clone());
    let known: Vec<String> = all.iter().map(|t| task_words(&t.text)).filter(|t| !t.is_empty()).collect();
    let dated_ahead = |d: &Option<String>| d.as_deref().is_some_and(|d| from.as_str() <= d && d <= until.as_str());
    let mut open: Vec<Task> = Vec::new();
    for t in all.iter().filter(|t| !t.done && !t.path.starts_with("wiki/") && !t.path.starts_with("sources/")) {
        seen.push(format!("{}:{}", t.path, t.line_text));
        // An Inbox task is in the Inbox's list, not the next actions.
        if in_inbox.contains(&(t.path.clone(), t.line)) {
            continue;
        }
        let created = t.created.as_deref().and_then(|c| NaiveDate::parse_from_str(c, "%Y-%m-%d").ok());
        let age = match created {
            Some(c) => (today - c).num_days().max(0),
            None => (now_ms - t.mtime).max(0) / 86_400_000,
        };
        let list = if has_tag(&t.tags, "waiting-for") {
            "waiting"
        } else if has_tag(&t.tags, "someday-maybe") {
            "someday"
        } else {
            "next"
        };
        open.push(Task {
            r#ref: String::new(),
            path: t.path.clone(),
            line: t.line,
            line_text: t.line_text.clone(),
            text: t.text.clone(),
            list: list.into(),
            due: t.due.clone(),
            scheduled: t.scheduled.clone(),
            start: t.start.clone(),
            project: t.project.as_deref().and_then(name_of),
            age,
            ahead: dated_ahead(&t.due) || dated_ahead(&t.scheduled) || dated_ahead(&t.start),
        });
    }
    // Next actions the dated and the oldest first, up to a cap; waiting-fors past a week; someday
    // items up to a cap; and anything dated in the next two weeks.
    let mut next: Vec<&Task> = open.iter().filter(|t| t.list == "next").collect();
    next.sort_by_key(|t| (t.due.is_none(), std::cmp::Reverse(t.age)));
    let next: HashSet<(&str, i64)> = next.iter().take(MAX_TASKS).map(|t| (t.path.as_str(), t.line)).collect();
    let mut someday = 0;
    let mut tasks: Vec<Task> = Vec::new();
    for t in &open {
        let keep = match t.list.as_str() {
            "next" => next.contains(&(t.path.as_str(), t.line)),
            "waiting" => t.age > WAITING_DAYS,
            _ => {
                someday += 1;
                someday <= MAX_TASKS
            }
        };
        if keep || t.ahead {
            tasks.push(t.clone());
        }
    }
    for (i, t) in tasks.iter_mut().enumerate() {
        t.r#ref = format!("T{}", i + 1);
    }

    let stuck: Vec<Stuck> = projects
        .iter()
        .filter(|p| p.status == "active" && p.next == 0)
        .map(|p| Stuck { path: p.path.clone(), name: p.name.clone(), outcome: p.outcome.clone() })
        .collect();
    seen.extend(stuck.iter().map(|p| p.name.clone()));
    Ok(Inputs {
        week: w.label().to_string(),
        start: w.start.clone(),
        end: w.end.clone(),
        today: from,
        notes,
        tasks,
        inbox: open_inbox
            .iter()
            .enumerate()
            .map(|(n, i)| Inbox {
                r#ref: format!("I{}", n + 1),
                item: InboxRef {
                    kind: i.kind.to_string(),
                    path: i.path.clone(),
                    line: i.line,
                    line_text: i.line_text.clone(),
                    text: i.text.clone(),
                },
            })
            .collect(),
        stuck,
        ahead_notes,
        projects: projects.iter().filter(|p| p.status == "active").map(|p| p.name.clone()).collect(),
        known,
        pages,
        seen,
    })
}

static TASK_NOISE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"#[^\s#]+|[📅⏳🛫➕✅❌]\x{FE0F}?\s*\d{4}-\d{2}-\d{2}|[🔺⏫🔼🔽⏬🔁🏁⛔🆔]\x{FE0F}?|[\[(]\w+::[^\])]*[\])]|\^[\w-]+|\[\[([^\]|]*\|)?|\]\]|^\s*[-*+]\s+\[.\]\s*")
        .unwrap()
});

/// A task's words, without its checkbox, tags, dates, fields, ids and link brackets, lower case:
/// the rule of `taskWords` in src/mcpActions.ts.
pub fn task_words(s: &str) -> String {
    TASK_NOISE.replace_all(s, " ").split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Whether `text` is already a task: the same words, give or take a full stop, or one's words
/// inside the other's when the shorter has three words or more.
pub fn already_a_task(text: &str, known: &[String]) -> bool {
    let w = task_words(text);
    let w = w.trim_end_matches(['.', '!']);
    if w.is_empty() {
        return false;
    }
    known.iter().any(|k| {
        let k = k.trim_end_matches(['.', '!']);
        let shorter = if k.len() < w.len() { k } else { w };
        k == w || (shorter.split_whitespace().count() >= 3 && (k.contains(w) || w.contains(k)))
    })
}

fn day_label(d: &str) -> String {
    NaiveDate::parse_from_str(d, "%Y-%m-%d").map(|d| d.format("%a %-d %b").to_string()).unwrap_or_else(|_| d.to_string())
}

fn task_line(t: &Task) -> String {
    let mut b = vec![format!("{}: {}", t.r#ref, t.text.trim())];
    if let Some(d) = &t.due {
        b.push(format!("due {d}"));
    }
    if let Some(d) = &t.scheduled {
        b.push(format!("scheduled {d}"));
    }
    if let Some(d) = &t.start {
        b.push(format!("starts {d}"));
    }
    if let Some(p) = &t.project {
        b.push(format!("project {p}"));
    }
    b.push(format!("{} days old", t.age));
    format!("- {}\n", b.join(" · "))
}

/// The inputs as the model reads them, after the workflow.
pub fn render_inputs(i: &Inputs) -> String {
    let mut s = format!("# Inputs\n\nThe review is for {} ({} to {}).\n\n", i.week, day_label(&i.start), day_label(&i.end));
    let none = |s: &mut String, empty: bool| {
        if empty {
            s.push_str("None.\n");
        }
    };
    s.push_str("## This week's notes and meetings\n\n");
    if i.notes.is_empty() {
        s.push_str("None dated this week.\n\n");
    }
    for n in &i.notes {
        // Its start and most of its end (a meeting's actions come last); the rest is in the vault.
        let text = crate::pageview::excerpt(&n.text, NOTE_CHARS);
        let cut = if text.contains("characters not shown …]") { "\n\n(read the part not shown in the vault if you need it)" } else { "" };
        s.push_str(&format!("### {} ({})\n\n{}{cut}\n\n", n.path, if n.meeting { "meeting" } else { "note" }, text.trim()));
    }
    s.push_str("## Inbox\n\n");
    none(&mut s, i.inbox.is_empty());
    for x in &i.inbox {
        s.push_str(&format!("- {} ({}): {}\n", x.r#ref, inbox_kind(&x.item.kind, &x.item.path), x.item.text.trim().replace('\n', " / ")));
    }
    for (list, head) in [
        ("next", "Open next actions".to_string()),
        ("waiting", format!("Waiting for, over {WAITING_DAYS} days")),
        ("someday", "Someday / maybe".to_string()),
    ] {
        s.push_str(&format!("\n## {head}\n\n"));
        let rows: Vec<&Task> = i.tasks.iter().filter(|t| t.list == list && (list != "waiting" || t.age > WAITING_DAYS)).collect();
        none(&mut s, rows.is_empty());
        for t in rows {
            s.push_str(&task_line(t));
        }
    }
    s.push_str(&format!("\n## Dated in the next {AHEAD_DAYS} days\n\n"));
    let ahead: Vec<&Task> = i.tasks.iter().filter(|t| t.ahead).collect();
    none(&mut s, ahead.is_empty() && i.ahead_notes.is_empty());
    for t in ahead {
        s.push_str(&task_line(t));
    }
    for n in &i.ahead_notes {
        s.push_str(&format!("- Note: {n}\n"));
    }
    s.push_str("\n## Projects with no next action\n\n");
    none(&mut s, i.stuck.is_empty());
    for p in &i.stuck {
        s.push_str(&format!("- {}{}\n", p.name, p.outcome.as_deref().map(|o| format!(" · outcome: {o}")).unwrap_or_default()));
    }
    s.push_str(&format!("\n## Active projects\n\n{}\n", if i.projects.is_empty() { "None.".to_string() } else { i.projects.join("; ") }));
    s
}

/// The whole message for the model; `today_label` as "Friday 2 October 2026".
pub fn prompt(i: &Inputs, today_label: &str) -> String {
    let until = NaiveDate::parse_from_str(&i.today, "%Y-%m-%d").map(|d| (d + Duration::days(AHEAD_DAYS)).format("%Y-%m-%d").to_string());
    format!(
        "{}\n\nToday is {today_label} ({}). Look ahead to {}.\n\n{}\n\nAnswer with the JSON array only.",
        WORKFLOW.trim(),
        i.today,
        until.unwrap_or_default(),
        render_inputs(i)
    )
}

/// What the vault looked like to the preparation, to tell whether it has changed since: a hash of
/// the week's notes, every open task's line, the Inbox, the stuck projects and the notes ahead.
/// It doesn't change with the day alone.
pub fn stamp(i: &Inputs) -> String {
    let mut seen = i.seen.clone();
    seen.sort();
    crate::write::version(format!("{}\n{}", i.week, seen.join("\n")).as_bytes())
}

// ---- the answer ------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub path: String,
    pub quote: String,
}

/// The task an action is on, as the window finds it again (`findTask` in src/mcpActions.ts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskRef {
    pub path: String,
    /// 0-based.
    pub line: i64,
    pub line_text: String,
    pub text: String,
}

/// The Inbox item a clarify is on, as the Inbox lists it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InboxRef {
    /// capture, thought or task.
    pub kind: String,
    pub path: String,
    pub line: usize,
    pub line_text: String,
    pub text: String,
}

/// What accepting a suggestion does.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "do", rename_all = "camelCase")]
pub enum Action {
    /// A new task in the To Do list (or the project's note).
    #[serde(rename_all = "camelCase")]
    Add {
        text: String,
        /// An active project's name.
        project: Option<String>,
        context: Option<String>,
        /// Without the `#`: `followup`, `waiting-for`, `someday-maybe`…
        tags: Vec<String>,
        due: Option<String>,
        scheduled: Option<String>,
    },
    Tick {
        task: TaskRef,
    },
    Defer {
        task: TaskRef,
        date: String,
    },
    Waiting {
        task: TaskRef,
    },
    Someday {
        task: TaskRef,
    },
    Edit {
        task: TaskRef,
        text: String,
    },
    Clarify {
        item: InboxRef,
        /// One of `BECOMES`.
        becomes: String,
        text: Option<String>,
        project: Option<String>,
        context: Option<String>,
        due: Option<String>,
    },
    /// The first plain mention of `phrase` in the note made a link to `target`, as a proposal.
    Link {
        path: String,
        phrase: String,
        target: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// Stable across preparations of the same week, so one skipped stays skipped.
    pub id: String,
    pub step: String,
    pub text: String,
    pub source: Option<Source>,
    pub action: Option<Action>,
}

static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!?\[\[([^\]|]*\|)?([^\]]*)\]\]").unwrap());

/// Whether `quote` is in `text`, as an ingest's quotes are checked (`normalise_quote`), with a link
/// also read as the words it shows (`[[Orbit App|the app]]` as "the app"), as a model quoting a
/// note tends to give it.
pub fn quote_in(quote: &str, text: &str) -> bool {
    let q = normalise_quote(quote);
    if q.is_empty() {
        return false;
    }
    let shown = |s: &str| normalise_quote(&LINK.replace_all(s, "$2"));
    normalise_quote(text).contains(&q) || shown(text).contains(&shown(quote))
}

/// A suggestion's id: a short hash of its step, its action and its words.
pub fn suggestion_id(step: &str, what: &str, words: &str) -> String {
    crate::write::version(format!("{step}\n{what}\n{}", normalise_quote(words)).as_bytes())[..12].to_string()
}

fn clean(o: Option<String>) -> Option<String> {
    o.map(|x| x.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|x| !x.is_empty() && x != "null")
}

fn get(v: &serde_json::Value, k: &str) -> Option<String> {
    clean(v.get(k).and_then(|x| x.as_str()).map(String::from))
}

fn date(v: &serde_json::Value, k: &str) -> Option<String> {
    get(v, k).filter(|d| d.len() == 10 && NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
}

fn context(v: &serde_json::Value) -> Option<String> {
    get(v, "context")
        .map(|c| c.trim_start_matches('@').trim_start_matches("#context/").trim_start_matches("context/").to_lowercase())
        .filter(|c| !c.is_empty() && !c.contains(char::is_whitespace))
}

/// An active project's name as the inputs have it, for what the model wrote; None when it isn't one.
fn project(v: &serde_json::Value, i: &Inputs) -> Option<String> {
    let p = get(v, "project")?;
    let p = p.trim_start_matches("[[").trim_end_matches("]]").trim_start_matches("Project. ");
    i.projects.iter().find(|x| x.eq_ignore_ascii_case(p)).cloned()
}

#[derive(Deserialize)]
struct RawSource {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    quote: Option<String>,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    step: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    source: Option<RawSource>,
    #[serde(default)]
    action: Option<serde_json::Value>,
}

/// The items of the JSON array in a model's answer, read one at a time, so an answer that stops
/// part way (cut off, or missing its closing `]`) still gives the items before the stop; the flag
/// says it stopped. A fenced ```json block is read first. Otherwise the array is the first `[`
/// that opens an item and gives at least one: a `[[link]]`, a `- [ ]` checkbox or a `sources: []`
/// before it doesn't count, nor does a `]` inside an item's text. An empty array is the answer only
/// when nothing gives an item and nothing that looked like the array failed to read.
pub fn array_items(answer: &str) -> Result<(Vec<serde_json::Value>, bool), String> {
    for block in fenced_json(answer) {
        if let Ok(got) = items_in(block) {
            return Ok(got);
        }
    }
    items_in(answer)
}

/// The contents of the answer's fenced ```json blocks (and bare ``` ones), in order.
fn fenced_json(answer: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = answer;
    while let Some(i) = rest.find("```") {
        let after = &rest[i + 3..];
        let line_end = after.find('\n').map(|n| n + 1).unwrap_or(after.len());
        let lang = after[..line_end].trim();
        let body = &after[line_end..];
        let Some(close) = body.find("```") else { break };
        if lang.is_empty() || lang.eq_ignore_ascii_case("json") {
            out.push(&body[..close]);
        }
        rest = &body[close + 3..];
    }
    out
}

/// A Markdown checkbox (`- [ ]`): a `[` with a list marker before it on its line.
fn checkbox_at(text: &str, i: usize) -> bool {
    let line = &text[text[..i].rfind('\n').map(|n| n + 1).unwrap_or(0)..i];
    let t = line.trim();
    matches!(t, "-" | "*" | "+") || (t.ends_with('.') && t.len() > 1 && t[..t.len() - 1].chars().all(|c| c.is_ascii_digit()))
}

fn items_in(answer: &str) -> Result<(Vec<serde_json::Value>, bool), String> {
    let starts = answer
        .match_indices('[')
        .map(|(i, _)| i)
        .filter(|&i| answer[i + 1..].trim_start().starts_with(['{', ']']) && !checkbox_at(answer, i));
    let (mut empty, mut error) = (false, None);
    for start in starts {
        match items_from(&answer[start + 1..]) {
            Ok((got, stopped)) if !got.is_empty() => return Ok((got, stopped)),
            Ok(_) => empty = true,
            Err(e) => {
                error.get_or_insert(e);
            }
        }
    }
    match error {
        Some(e) => Err(e),
        None if empty => Ok((vec![], false)),
        None => Err("The answer had no suggestions in it.".into()),
    }
}

/// The items after an array's `[`.
fn items_from(mut rest: &str) -> Result<(Vec<serde_json::Value>, bool), String> {
    let mut out = Vec::new();
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if rest.starts_with(']') {
            return Ok((out, false));
        }
        let mut it = serde_json::Deserializer::from_str(rest).into_iter::<serde_json::Value>();
        match it.next() {
            Some(Ok(v)) => {
                rest = &rest[it.byte_offset()..];
                out.push(v);
            }
            Some(Err(e)) if out.is_empty() => return Err(format!("The suggestions couldn't be read: {e}")),
            None if out.is_empty() => return Err("The answer stopped before its first suggestion.".into()),
            _ => return Ok((out, true)),
        }
    }
}

/// The suggestions in a model's answer, each checked against the inputs; `read` gives a note's
/// text by its path (None: not there). What fails a check is dropped, with why in the second list.
pub fn parse(answer: &str, i: &Inputs, read: impl Fn(&str) -> Option<String>) -> Result<(Vec<Suggestion>, Vec<String>), String> {
    let (raw, stopped) = array_items(answer)?;
    let tasks: HashMap<&str, &Task> = i.tasks.iter().map(|t| (t.r#ref.as_str(), t)).collect();
    let inbox: HashMap<&str, &Inbox> = i.inbox.iter().map(|x| (x.r#ref.as_str(), x)).collect();
    let refs = Refs { tasks, inbox };
    let mut out: Vec<Suggestion> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    if stopped {
        dropped.push("the rest of the answer: it stopped part way, or couldn't be read".into());
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for v in raw {
        let Ok(r) = serde_json::from_value::<Raw>(v) else {
            dropped.push("not a suggestion".into());
            continue;
        };
        match check(r, i, &refs, &read) {
            Ok(sg) => {
                let n = counts.entry(sg.step.clone()).or_default();
                if *n >= cap(&sg.step) {
                    dropped.push(format!("{}: too many in the step", sg.text));
                } else if out.iter().any(|x| x.id == sg.id) {
                    dropped.push(format!("{}: said twice", sg.text));
                } else {
                    *n += 1;
                    out.push(sg);
                }
            }
            Err(e) => dropped.push(e),
        }
    }
    Ok((out, dropped))
}

struct Refs<'a> {
    tasks: HashMap<&'a str, &'a Task>,
    inbox: HashMap<&'a str, &'a Inbox>,
}

fn check(r: Raw, i: &Inputs, refs: &Refs, read: &impl Fn(&str) -> Option<String>) -> Result<Suggestion, String> {
    let step = r.step.trim().to_lowercase();
    let text: String = r.text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect();
    if !STEPS.contains(&step.as_str()) {
        return Err(format!("{text}: no step {step:?}"));
    }
    if text.is_empty() {
        return Err(format!("an empty suggestion in {step}"));
    }
    let source = match r.source.map(|s| (clean(s.path), clean(s.quote))) {
        None | Some((None, None)) => None,
        Some((Some(path), Some(quote))) => {
            let path = path.trim_start_matches("[[").trim_end_matches("]]").to_string();
            let path = if path.ends_with(".md") { path } else { format!("{path}.md") };
            let quote = quote.trim_matches(['"', '“', '”']).to_string();
            if quote.chars().count() > 400 || !read(&path).is_some_and(|t| quote_in(&quote, &t)) {
                return Err(format!("{text}: the quote isn't in {path}"));
            }
            Some(Source { path, quote })
        }
        Some(_) => return Err(format!("{text}: a source needs a note and a quote")),
    };
    let action = match r.action.filter(|a| !a.is_null()) {
        None => None,
        Some(a) if !a.is_object() => return Err(format!("{text}: the action isn't an object")),
        Some(a) => Some(check_action(&a, i, refs).map_err(|e| format!("{text}: {e}"))?),
    };
    let (what, words) = match &action {
        None => ("note", text.clone()),
        Some(Action::Add { text, .. }) => ("add", text.clone()),
        Some(Action::Tick { task }) => ("tick", task.text.clone()),
        Some(Action::Defer { task, .. }) => ("defer", task.text.clone()),
        Some(Action::Waiting { task }) => ("waiting", task.text.clone()),
        Some(Action::Someday { task }) => ("someday", task.text.clone()),
        Some(Action::Edit { text, .. }) => ("edit", text.clone()),
        Some(Action::Clarify { item, becomes, .. }) => ("clarify", format!("{} {becomes}", item.text)),
        Some(Action::Link { path, phrase, target }) => ("link", format!("{path} {phrase} {target}")),
    };
    Ok(Suggestion { id: suggestion_id(&step, what, &words), step, text, source, action })
}

fn check_action(a: &serde_json::Value, i: &Inputs, refs: &Refs) -> Result<Action, String> {
    let what = get(a, "do").unwrap_or_default().to_lowercase();
    let task = || -> Result<&Task, String> {
        let r = get(a, "task").unwrap_or_default();
        refs.tasks.get(r.as_str()).copied().ok_or_else(|| format!("no task {r:?}"))
    };
    let task_ref = |t: &Task| TaskRef { path: t.path.clone(), line: t.line, line_text: t.line_text.clone(), text: t.text.clone() };
    let new_task = |k: &str| -> Result<String, String> {
        let text = get(a, k).ok_or("no task's words")?;
        if already_a_task(&text, &i.known) {
            return Err(format!("“{text}” is already a task"));
        }
        Ok(text)
    };
    Ok(match what.as_str() {
        "add" => {
            let mut tags: Vec<String> = Vec::new();
            for t in a.get("tags").and_then(|t| t.as_array()).into_iter().flatten().filter_map(|t| t.as_str()) {
                let t = t.trim().trim_start_matches('#').to_string();
                if !t.is_empty() && !t.contains(char::is_whitespace) && !t.starts_with("context/") && !tags.contains(&t) {
                    tags.push(t);
                }
            }
            Action::Add {
                text: new_task("text")?,
                project: project(a, i),
                context: context(a),
                tags,
                due: date(a, "due"),
                scheduled: date(a, "scheduled"),
            }
        }
        "tick" => Action::Tick { task: task_ref(task()?) },
        "waiting" | "someday" => {
            let t = task()?;
            if t.list == what {
                return Err(format!("{} is a {what} item already", t.r#ref));
            }
            let task = task_ref(t);
            if what == "waiting" {
                Action::Waiting { task }
            } else {
                Action::Someday { task }
            }
        }
        "defer" => {
            let t = task()?;
            let d = date(a, "date").ok_or("a defer with no date")?;
            if d <= i.today {
                return Err(format!("a defer to {d}, not after today"));
            }
            Action::Defer { task: task_ref(t), date: d }
        }
        "edit" => {
            let t = task()?;
            let text = get(a, "text").ok_or("an edit with no wording")?;
            if task_words(&text) == task_words(&t.text) {
                return Err("an edit that changes nothing".into());
            }
            Action::Edit { task: task_ref(t), text }
        }
        "clarify" => {
            let r = get(a, "item").unwrap_or_default();
            let x = refs.inbox.get(r.as_str()).ok_or_else(|| format!("no Inbox item {r:?}"))?;
            let becomes = get(a, "becomes").unwrap_or_default().to_lowercase();
            if !BECOMES.contains(&becomes.as_str()) {
                return Err(format!("an Inbox item can't become {becomes:?}"));
            }
            if !fits(inbox_kind(&x.item.kind, &x.item.path), &becomes) {
                return Err(format!("the Inbox doesn't offer {becomes:?} for {r}"));
            }
            let text = if matches!(becomes.as_str(), "next" | "waiting" | "someday") { Some(new_task("text")?) } else { get(a, "text") };
            Action::Clarify { item: x.item.clone(), becomes, text, project: project(a, i), context: context(a), due: date(a, "due") }
        }
        "link" => {
            let path = get(a, "path").ok_or("a link with no note")?;
            let phrase = get(a, "phrase").ok_or("a link with no words")?;
            let target = get(a, "target").ok_or("a link with no page")?;
            let target = target.trim_start_matches("[[").trim_end_matches("]]");
            let target = target.split(['|', '#']).next().unwrap_or("").trim().trim_end_matches(".md").to_string();
            let note = i.notes.iter().find(|n| n.path == path).ok_or_else(|| format!("{path} isn't one of this week's notes"))?;
            if !i.pages.contains(&target.to_lowercase()) {
                return Err(format!("there's no page {target}"));
            }
            if crate::filename::stem(&path).eq_ignore_ascii_case(&target) {
                return Err(format!("{path} would link itself"));
            }
            if note.text.to_lowercase().contains(&format!("[[{}", target.to_lowercase())) {
                return Err(format!("{path} links {target} already"));
            }
            if with_link(&note.text, &phrase, &target).is_none() {
                return Err(format!("“{phrase}” isn't in {path} outside a link"));
            }
            Action::Link { path, phrase, target }
        }
        "" => return Err("an action with nothing to do".into()),
        _ => return Err(format!("no action {what:?}")),
    })
}

/// The note with the first `phrase` outside a link made a link to `target`; None when there's no
/// such mention.
pub fn with_link(text: &str, phrase: &str, target: &str) -> Option<String> {
    if phrase.is_empty() {
        return None;
    }
    let mut from = 0;
    while let Some(at) = text[from..].find(phrase).map(|x| x + from) {
        let before = &text[..at];
        let inside = before.rfind("[[").is_some_and(|o| before.rfind("]]").is_none_or(|c| c < o));
        if !inside {
            let link = if phrase == target { format!("[[{target}]]") } else { format!("[[{target}|{phrase}]]") };
            return Some(format!("{before}{link}{}", &text[at + phrase.len()..]));
        }
        from = at + phrase.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 0).unwrap()
    }

    fn day(y: i32, mo: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, mo, d).unwrap()
    }

    // 2026-10-02 is a Friday, in 2026-W40.
    #[test]
    fn the_week_follows_the_summary_rule() {
        assert_eq!(review_week(day(2026, 10, 2)), "2026-W40");
        assert_eq!(review_week(day(2026, 10, 4)), "2026-W40");
        // Monday and Tuesday look back on the week just ended.
        assert_eq!(review_week(day(2026, 10, 5)), "2026-W40");
        assert_eq!(review_week(day(2026, 10, 6)), "2026-W40");
        assert_eq!(review_week(day(2026, 10, 7)), "2026-W41");
    }

    #[test]
    fn it_runs_four_hours_before_the_review_on_its_day() {
        assert_eq!(prep_time("16:00"), "12:00");
        assert_eq!(prep_time("09:30"), "05:30");
        assert_eq!(prep_time("04:00"), "00:00");
        assert_eq!(prep_time("02:00"), "00:00");
        let now = at(2026, 10, 2, 13, 0);
        assert_eq!(last_due(now, Weekday::Fri, "16:00"), at(2026, 10, 2, 12, 0));
        assert_eq!(last_due(at(2026, 10, 2, 11, 59), Weekday::Fri, "16:00"), at(2026, 9, 25, 12, 0));
        assert_eq!(next_due(now, Weekday::Fri, "16:00"), at(2026, 10, 9, 12, 0));
        assert!(is_due(now, Weekday::Fri, "16:00", Some(at(2026, 10, 1, 9, 0))));
        assert!(!is_due(now, Weekday::Fri, "16:00", Some(at(2026, 10, 2, 12, 5))));
        assert!(!is_due(now, Weekday::Fri, "16:00", None));
    }

    #[test]
    fn task_words_match_the_window() {
        assert_eq!(task_words("- [ ] Call [[Maya]] about the rota 📅 2026-10-09 #work ^rank-12"), "call maya about the rota");
        assert_eq!(task_words("Send [[Project. Orbit App launch|the plan]] [effort:: 30m]"), "send the plan");
        let known = vec![task_words("Confirm the launch date with Sam 📅 2026-10-03 #followup")];
        assert!(already_a_task("Confirm the launch date with Sam.", &known));
        assert!(already_a_task("confirm the launch date with sam by Friday", &known));
        assert!(!already_a_task("Book the launch party", &known));
        assert!(!already_a_task("", &known));
    }

    #[test]
    fn a_quote_is_checked_as_an_ingest_quote() {
        let note = "Discussed [[Orbit App]] and the [[OA|app]] launch date, with **Sam** – today.";
        assert!(quote_in("Discussed Orbit App and the app launch date", note));
        assert!(quote_in("the [[OA|app]] launch date", note));
        assert!(quote_in("with Sam - today", note));
        assert!(quote_in("WITH  SAM", note));
        assert!(!quote_in("Discussed the Orbit App launch", note));
        assert!(!quote_in("  ", note));
    }

    #[test]
    fn links_the_first_plain_mention() {
        assert_eq!(
            with_link("See Orbit App and [[Orbit App]].", "Orbit App", "Orbit App").unwrap(),
            "See [[Orbit App]] and [[Orbit App]]."
        );
        assert_eq!(with_link("[[Orbit App]] then Orbit App", "Orbit App", "Orbit App").unwrap(), "[[Orbit App]] then [[Orbit App]]");
        assert_eq!(with_link("the app launch", "app", "Orbit App").unwrap(), "the [[Orbit App|app]] launch");
        assert!(with_link("nothing here", "Orbit", "Orbit App").is_none());
        assert!(with_link("[[Orbit App]]", "Orbit", "Orbit").is_none());
    }

    #[test]
    fn inbox_choices_as_the_inbox_offers_them() {
        // The same table src/inboxActions.test.ts checks inboxChoices and suggestKind against.
        let table: serde_json::Value = serde_json::from_str(include_str!("../../../../tests/fixtures/inbox-choices.json")).unwrap();
        let every = ["next", "project", "waiting", "done", "someday", "reference", "delete", "meeting", "reply", "ingest", "ask"];
        for k in table["kinds"].as_array().unwrap() {
            let kind = k["kind"].as_str().unwrap();
            let item = &k["item"];
            assert_eq!(inbox_kind(item["kind"].as_str().unwrap(), item["path"].as_str().unwrap()), kind, "{item}");
            let choices: Vec<&str> = k["choices"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
            for b in every {
                assert_eq!(fits(kind, b), choices.contains(&b), "{kind} {b}");
            }
        }
    }

    #[test]
    fn inbox_items_still_to_clarify() {
        let item = |kind: &'static str, l: &str| InboxItem {
            kind,
            path: "Me. To Do List.md".into(),
            line: 3,
            line_text: l.into(),
            text: l.into(),
            stamp: None,
            block: None,
        };
        assert!(unclarified(&item("task", "- [ ] ring the bank")));
        assert!(!unclarified(&item("task", "- [ ] ring the bank #context/calls")));
        assert!(!unclarified(&item("task", "- [ ] ring the bank [[Project. Money]]")));
        assert!(!unclarified(&item("task", "- [x] ring the bank")));
        assert!(unclarified(&item("thought", "## 2026-10-01 09:00")));
    }

    fn copy_dir(from: &Path, to: &Path) {
        for e in walkdir::WalkDir::new(from).into_iter().flatten() {
            let dest = to.join(e.path().strip_prefix(from).unwrap());
            if e.file_type().is_dir() {
                std::fs::create_dir_all(&dest).unwrap();
            } else {
                std::fs::copy(e.path(), &dest).unwrap();
            }
        }
    }

    /// The fixture vault copied to a temp dir and indexed, gathered for 2026-W40 on Friday 2 October.
    fn fixture() -> (tempfile::TempDir, Inputs) {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("vault");
        copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault"), &root);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&Vault::new(root.clone(), vec![])).unwrap();
        let now = at(2026, 10, 2, 12, 0).and_utc().timestamp_millis();
        let inbox = crate::inbox::items(&root);
        let i = gather(&root, &ix, "2026-W40", day(2026, 10, 2), now, &inbox).unwrap();
        (t, i)
    }

    #[test]
    fn gathers_the_week() {
        let (_t, i) = fixture();
        assert_eq!((i.start.as_str(), i.end.as_str(), i.today.as_str()), ("2026-09-28", "2026-10-04", "2026-10-02"));
        let paths: Vec<&str> = i.notes.iter().map(|n| n.path.as_str()).collect();
        assert!(paths.contains(&"Meeting. Orbit App Steerco - 2026-09-30.md"), "{paths:?}");
        assert!(!paths.iter().any(|p| p.contains("2026-09-23")));
        assert!(i.notes.iter().all(|n| n.meeting == n.path.starts_with("Meeting.")));
        // The To Do list's open `#### Other` tasks are the Inbox, not next actions.
        assert!(i.inbox.iter().any(|x| x.r#ref == "I1"));
        assert!(i.inbox.iter().all(|x| !x.item.line_text.contains("#waiting-for")));
        let inboxed: Vec<&str> = i.inbox.iter().filter(|x| x.item.kind == "task").map(|x| x.item.line_text.as_str()).collect();
        assert!(i.tasks.iter().all(|t| !inboxed.contains(&t.line_text.as_str())));
        // Zara's waiting-for is two days old: not one to chase yet, nor dated ahead.
        assert!(!i.tasks.iter().any(|t| t.text.contains("Zara to sign off")));
        // Sam's follow-up is due tomorrow: a next action, dated ahead.
        let sam = i.tasks.iter().find(|t| t.text.contains("Confirm the launch date with Sam")).unwrap();
        assert!(sam.ahead && sam.list == "next" && sam.r#ref.starts_with('T'));
        assert!(i.projects.contains(&"Orbit App launch".to_string()));
        assert!(i.stuck.iter().all(|p| p.name != "Orbit App launch"));
        let p = prompt(&i, "Friday 2 October 2026");
        assert!(p.contains("Look ahead to 2026-10-16"));
        assert!(p.contains("### Meeting. Orbit App Steerco - 2026-09-30.md (meeting)"));
        assert!(p.contains(&format!("- {}: Confirm the launch date with Sam", sam.r#ref)));
    }

    #[test]
    fn the_stamp_changes_with_the_vault_not_the_day() {
        let (t, i) = fixture();
        let root = t.path().join("vault");
        let regather = |today: NaiveDate| {
            let mut ix = Index::open_in_memory().unwrap();
            ix.sync(&Vault::new(root.clone(), vec![])).unwrap();
            gather(&root, &ix, "2026-W40", today, at(2026, 10, 3, 12, 0).and_utc().timestamp_millis(), &crate::inbox::items(&root)).unwrap()
        };
        assert_eq!(stamp(&i), stamp(&regather(day(2026, 10, 3))));
        let steerco = root.join("Meeting. Orbit App Steerco - 2026-09-30.md");
        let text = std::fs::read_to_string(&steerco).unwrap();
        std::fs::write(&steerco, text.replace("Send the minutes", "Send the minutes to Lena")).unwrap();
        assert_ne!(stamp(&i), stamp(&regather(day(2026, 10, 2))));
    }

    #[test]
    fn checks_each_suggestion() {
        let (t, i) = fixture();
        let root = t.path().join("vault");
        let read = |p: &str| std::fs::read_to_string(root.join(p)).ok();
        let sam = i.tasks.iter().find(|t| t.text.contains("Confirm the launch date with Sam")).unwrap().r#ref.clone();
        let steerco = "Meeting. Orbit App Steerco - 2026-09-30.md";
        let todo = i.inbox.iter().find(|x| x.item.kind == "task").unwrap().r#ref.clone();
        let answer = format!(
            r##"Here they are:
[
 {{"step":"loose","text":"Anything from the steerco to capture?","source":null,"action":null}},
 {{"step":"back","text":"Sam to confirm the date.","source":{{"path":"{steerco}","quote":"the [[OA|app]] launch date"}},"action":{{"do":"add","text":"Sam: confirm the app launch date","project":"orbit app LAUNCH","context":"@calls","tags":["#waiting-for","has space"],"due":"2026-10-09","scheduled":"soon"}}}},
 {{"step":"back","text":"Made up.","source":{{"path":"{steerco}","quote":"we agreed to cancel the launch"}},"action":{{"do":"add","text":"Cancel the launch"}}}},
 {{"step":"back","text":"Already a task.","source":null,"action":{{"do":"add","text":"Confirm the launch date with Sam"}}}},
 {{"step":"next","text":"Not a task.","source":null,"action":{{"do":"tick","task":"T999"}}}},
 {{"step":"next","text":"Put it off.","source":null,"action":{{"do":"defer","task":"{sam}","date":"2026-10-20"}}}},
 {{"step":"next","text":"Bad date.","source":null,"action":{{"do":"defer","task":"{sam}","date":"2026-09-01"}}}},
 {{"step":"inbox","text":"A call.","source":null,"action":{{"do":"clarify","item":"{todo}","becomes":"next","text":"Ring Maya about the Dashboard","context":"calls"}}}},
 {{"step":"inbox","text":"No such item.","source":null,"action":{{"do":"clarify","item":"I999","becomes":"next","text":"x"}}}},
 {{"step":"notes","text":"Link the steerco.","source":null,"action":{{"do":"link","path":"{steerco}","phrase":"launch date","target":"Project. Orbit App launch"}}}},
 {{"step":"notes","text":"No such page.","source":null,"action":{{"do":"link","path":"{steerco}","phrase":"Sam","target":"Sam Carter"}}}},
 {{"step":"next","text":"Unknown action.","source":null,"action":{{"do":"delete","task":"{sam}"}}}},
 {{"step":"ghosts","text":"Unknown step."}},
 {{"step":"loose","text":"Anything from the steerco to capture?"}}
]"##
        );
        let (got, dropped) = parse(&answer, &i, read).unwrap();
        let texts: Vec<&str> = got.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            texts,
            ["Anything from the steerco to capture?", "Sam to confirm the date.", "Put it off.", "A call.", "Link the steerco."],
            "{dropped:?}"
        );
        assert_eq!(dropped.len(), 9, "{dropped:?}");
        for why in [
            "quote isn't in",
            "already a task",
            "no task \"T999\"",
            "not after today",
            "no Inbox item",
            "no page Sam Carter",
            "no action \"delete\"",
            "no step \"ghosts\"",
            "said twice",
        ] {
            assert!(dropped.iter().any(|d| d.contains(why)), "{why}: {dropped:?}");
        }
        let Some(Action::Add { project, context, tags, due, scheduled, .. }) = &got[1].action else { panic!() };
        assert_eq!(project.as_deref(), Some("Orbit App launch"));
        assert_eq!(context.as_deref(), Some("calls"));
        assert_eq!(tags, &["waiting-for"]);
        assert_eq!((due.as_deref(), scheduled.as_deref()), (Some("2026-10-09"), None));
        assert_eq!(got[1].source.as_ref().unwrap().path, steerco);
        let Some(Action::Clarify { item, becomes, .. }) = &got[3].action else { panic!() };
        assert_eq!((item.path.as_str(), becomes.as_str()), ("Me. To Do List.md", "next"));
        // As the window gets it.
        let json = serde_json::to_value(&got[2]).unwrap();
        assert_eq!(json["action"]["do"], "defer");
        assert_eq!(json["action"]["task"]["lineText"], i.tasks.iter().find(|t| t.r#ref == sam).unwrap().line_text.as_str());
        let back: Suggestion = serde_json::from_value(json).unwrap();
        assert_eq!(back, got[2]);
        // The same suggestion again has the same id.
        let (again, _) = parse(&answer, &i, |p| std::fs::read_to_string(root.join(p)).ok()).unwrap();
        assert_eq!(again[1].id, got[1].id);
        assert!(parse("no json", &i, |_| None).is_err());
    }

    #[test]
    fn array_items_survive_links_and_a_cut_off_answer() {
        let one = r#"{"step": "loose", "text": "Chase [[Orbit App]] - [ ] the date"}"#;
        // A preamble with a link, and a link inside an item, don't move the array.
        let (got, stopped) = array_items(&format!("See [[Orbit App]]:\n[\n{one},\n{one}\n]\nDone [[x]].")).unwrap();
        assert_eq!((got.len(), stopped), (2, false));
        // Cut off in the third item: the first two are kept, and it says it stopped.
        let cut = format!("[\n{one},\n{one},\n{{\"step\": \"loose\", \"text\": \"Ask [[Maya]] ab");
        let (got, stopped) = array_items(&cut).unwrap();
        assert_eq!((got.len(), stopped), (2, true));
        // No closing bracket at all.
        assert_eq!(array_items(&format!("[{one}, {one}")).unwrap(), (vec![serde_json::from_str(one).unwrap(); 2], true));
        assert_eq!(array_items("[]").unwrap(), (vec![], false));
        assert!(array_items("[{\"step\": \"loo").is_err());
        assert!(array_items("no json [[link]]").is_err());
    }

    #[test]
    fn array_items_skip_checkboxes_and_empty_lists_before_the_array() {
        let one = r#"{"step": "loose", "text": "Chase Maya"}"#;
        // A checkbox or an empty list in the preamble isn't the answer.
        let (got, _) = array_items(&format!("Looked at:\n- [ ] Ring Lena\nsources: []\n\n[{one}, {one}]")).unwrap();
        assert_eq!(got.len(), 2);
        // A checkbox alone isn't an empty answer.
        assert!(array_items("- [ ] Ring Lena\n1. [ ] and Maya").is_err());
        // A fenced block is read first, whatever comes before it.
        let (got, stopped) = array_items(&format!("Example: [{{\"step\": \"x\"}}]\n```json\n[{one}, {one}, {one}]\n```\n")).unwrap();
        assert_eq!((got.len(), stopped), (3, false));
        // An empty fenced answer is empty, even after an example with items.
        let (got, _) = array_items("Nothing to suggest.\n```json\n[]\n```").unwrap();
        assert!(got.is_empty());
        // `sources: []` then an array cut off before its first item: an error, not an empty list.
        assert!(array_items("sources: []\n[{\"step\": \"loo").is_err());
        // Nothing but an empty array: no suggestions.
        assert_eq!(array_items("Nothing this week.\n[]").unwrap(), (vec![], false));
    }
}
