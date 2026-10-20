// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Prepare the weekly review: a background job that reads the week and saves suggestions for each
//! of the review's steps (brainstead_core::reviews::prep gathers, prompts and checks). It runs on
//! the review day, `prep::LEAD_HOURS` before the review (Settings › Jobs & schedule), when
//! Brainstead runs the jobs and the week has changed since it was last prepared; when the review
//! is started and the week has no preparation or has changed since (`weekprep_ensure`); and on
//! Run now (`weekprep_run`).
//!
//! The model reads with read-only tools, as the summaries' runs do, and its run opens as a tab in
//! Ask. What it suggests is kept per week in `weekly-prep/` in the app data folder, with when it
//! was made and a stamp of the vault it saw. Nothing is written to the vault until a suggestion is
//! accepted in the window, through the app's own undoable actions; a link is made as an agent change, revertable in Changes
//! (`weekprep_link`).

use std::path::PathBuf;
use std::sync::Mutex;

use brainstead_core::chats::{Chat, Item, Summary};
use brainstead_core::proposals::{Kind, Origin};
use brainstead_core::reviews::prep::{self, Suggestion};
use brainstead_core::reviews::Weekday;
use brainstead_core::{ask::Event, write};
use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::vault::VaultService;
use crate::{ask, gtd, platform, AppState};

const KEEP_RUNS: usize = 30;
/// Weeks of suggestions kept.
const KEEP_WEEKS: usize = 12;
/// The event the window listens for.
const CHANGED: &str = "weekprep-changed";

/// A week's suggestions, as saved in `weekly-prep/<week>.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prep {
    pub week: String,
    /// When it was made, local `YYYY-MM-DDTHH:MM:SS`.
    pub prepared_at: String,
    pub model: String,
    /// The vault it saw (`prep::stamp`), to tell when the week has changed since.
    pub stamp: String,
    pub suggestions: Vec<Suggestion>,
    /// How many of the model's suggestions failed the checks.
    pub dropped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Run {
    id: String,
    week: String,
    /// schedule, start (the review was started) or manual.
    trigger: String,
    model: String,
    started_at: String,
    finished_at: Option<String>,
    /// running, done, error or stopped.
    status: String,
    error: Option<String>,
    count: usize,
    chat: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Saved {
    /// When the schedule last fired, or was taken on: it fires at the next time after this.
    last: Option<String>,
    runs: Vec<Run>,
}

#[derive(Default)]
pub struct WeekPrep {
    state: Mutex<Saved>,
    /// The run going now: its chat id and week.
    going: Mutex<Option<(String, String)>>,
    /// Runs stopped with Stop, so their end reads as stopped rather than failed.
    stopped: Mutex<Vec<String>>,
}

fn dir() -> PathBuf {
    platform::data_dir().join("weekly-prep")
}

fn week_file(week: &str) -> PathBuf {
    dir().join(format!("{}.json", week.replace(['/', '\\', '.'], "")))
}

fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

fn stamp(t: NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M:%S").to_string()
}

impl WeekPrep {
    pub fn load() -> WeekPrep {
        let mut state: Saved =
            std::fs::read(dir().join("state.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        // A run the app quit during isn't going any more.
        for r in state.runs.iter_mut().filter(|r| r.status == "running") {
            r.status = "stopped".into();
            r.error = Some("Stopped when Brainstead closed.".into());
        }
        WeekPrep { state: Mutex::new(state), going: Mutex::new(None), stopped: Mutex::new(Vec::new()) }
    }

    fn save(&self) {
        let s = crate::lock(&self.state);
        if let Ok(json) = serde_json::to_vec_pretty(&*s) {
            let _ = std::fs::create_dir_all(dir());
            let _ = write::write_atomic(&dir().join("state.json"), &json, false);
        }
    }

    fn update_run(&self, id: &str, f: impl FnOnce(&mut Run)) {
        if let Some(r) = crate::lock(&self.state).runs.iter_mut().find(|r| r.id == id) {
            f(r);
        }
        self.save();
    }

    /// The review's day or time changed, or Brainstead took the jobs over: the schedule first
    /// fires at its next time rather than catching up at once.
    pub fn stamp_now(&self) {
        crate::lock(&self.state).last = Some(stamp(now()));
        self.save();
    }

    pub fn running(&self) -> bool {
        crate::lock(&self.going).is_some()
    }
}

/// The weekly review's day and time (Settings › Jobs & schedule).
fn review_schedule(app: &AppHandle) -> (Weekday, String) {
    let st = app.state::<AppState>();
    let s = crate::lock(&st.settings);
    (s.weekly_review.day, s.weekly_review.time.clone())
}

/// The job runs on schedule (Settings › Jobs & schedule, `weekprepEnabled`, on unless switched off).
fn enabled(app: &AppHandle) -> bool {
    let st = app.state::<AppState>();
    let s = crate::lock(&st.settings);
    s.ui.get("weekprepEnabled").and_then(|v| v.as_bool()).unwrap_or(true)
}

/// The job's model: its own in Settings › AI assistants (`jobModels.weekprep`), else the
/// summaries'.
fn model(app: &AppHandle) -> String {
    let own = {
        let st = app.state::<AppState>();
        let s = crate::lock(&st.settings);
        s.ui.get("jobModels").and_then(|m| m.get("weekprep")).and_then(|v| v.as_str()).filter(|v| !v.is_empty()).map(String::from)
    };
    own.unwrap_or_else(|| ask::job_model(app, "summaries"))
}

fn read_prep(week: &str) -> Option<Prep> {
    std::fs::read(week_file(week)).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

/// What the window shows for a week.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    prep: Option<Prep>,
    /// The week is being prepared now.
    running: bool,
    /// The last preparation of the week failed: why.
    error: Option<String>,
}

#[tauri::command]
pub fn weekprep_status(app: AppHandle, week: String) -> Status {
    let wp = app.state::<WeekPrep>();
    let running = crate::lock(&wp.going).as_ref().is_some_and(|(_, w)| *w == week);
    let last = crate::lock(&wp.state).runs.iter().rev().find(|r| r.week == week && r.status != "running").cloned();
    Status { prep: read_prep(&week), running, error: last.filter(|r| r.status == "error").and_then(|r| r.error) }
}

/// The job's row in Settings › Jobs & schedule: its last run, and when it next runs on schedule.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    /// The week being prepared now.
    running: Option<String>,
    last: Option<Run>,
    /// Local `YYYY-MM-DDTHH:MM:SS`; none when it won't run on schedule (switched off, or Brainstead
    /// doesn't run the jobs).
    next: Option<String>,
    enabled: bool,
}

#[tauri::command]
pub fn weekprep_job(app: AppHandle) -> JobStatus {
    let wp = app.state::<WeekPrep>();
    let running = crate::lock(&wp.going).as_ref().map(|(_, w)| w.clone());
    let last = crate::lock(&wp.state).runs.iter().rev().find(|r| r.status != "running").cloned();
    let on = enabled(&app);
    let here = crate::lock(&app.state::<AppState>().settings).summaries_here;
    let (day, time) = review_schedule(&app);
    let next = (on && here).then(|| stamp(prep::next_due(now(), day, &time)));
    JobStatus { running, last, next, enabled: on }
}

/// Stops the run going now (Stop, in Settings › Jobs & schedule).
#[tauri::command]
pub fn weekprep_stop(app: AppHandle) {
    let wp = app.state::<WeekPrep>();
    let id = crate::lock(&wp.going).as_ref().map(|(id, _)| id.clone());
    if let Some(id) = id {
        crate::lock(&wp.stopped).push(id.clone());
        app.state::<std::sync::Arc<ask::Running>>().cancel(&id);
    }
}

/// Prepares `week` now (Run now, Prepare again, Retry); without one, the week a review started
/// today is for. Returns the run's chat id.
#[tauri::command]
pub fn weekprep_run(app: AppHandle, week: Option<String>) -> Result<String, String> {
    let week = week.filter(|w| !w.trim().is_empty()).unwrap_or_else(|| prep::review_week(Local::now().date_naive()));
    prep::week_window(&week)?;
    start(&app, &week, "manual")
}

/// The review was started: prepares its week when there's no preparation for it, or the vault has
/// changed since. Returns whether a run started.
#[tauri::command]
pub async fn weekprep_ensure(app: AppHandle, week: String) -> Result<bool, String> {
    prep::week_window(&week)?;
    // Screenshot mode runs no assistant.
    if crate::scene().is_some() {
        return Ok(false);
    }
    tauri::async_runtime::spawn_blocking(move || ensure(&app, &week, "start")).await.map_err(|e| e.to_string())?
}

fn ensure(app: &AppHandle, week: &str, trigger: &str) -> Result<bool, String> {
    if app.state::<WeekPrep>().running() {
        return Ok(false);
    }
    if let Some(have) = read_prep(week) {
        // Opening the review never prepares again once the week has one: working through it
        // changes the vault, and that's no reason to start over (Prepare again does it on purpose).
        // The scheduled run before the review still refreshes one the vault has moved past.
        if trigger == "start" || prep::stamp(&gather(app, week)?) == have.stamp {
            return Ok(false);
        }
    }
    start(app, week, trigger).map(|_| true)
}

/// An accepted link suggestion: the note with the words linked, made as an agent change (in
/// Changes, revertable), or held there for the user. Returns what became of it.
#[tauri::command]
pub fn weekprep_link(
    app: AppHandle,
    week: String,
    path: String,
    phrase: String,
    target: String,
    unattended: Option<bool>,
) -> Result<crate::changes::Outcome, String> {
    let abs = app.state::<VaultService>().resolve_path(&path)?;
    let before = std::fs::read_to_string(&abs).map_err(|_| format!("{path} isn't there any more."))?;
    let after = prep::with_link(&before, &phrase, &target).ok_or_else(|| format!("“{phrase}” isn't in {path} any more."))?;
    let title = format!("Link {target} in {}", brainstead_core::filename::stem(&path));
    // An assistant nobody is watching: held when a check fails.
    let trigger = unattended.unwrap_or(false).then(|| "scheduled".to_string());
    let origin = Origin { kind: "review".into(), label: Some(format!("Weekly review {week}")), trigger, ..Default::default() };
    let mut s = crate::changes::Submit::new(
        &path,
        Kind::Edit,
        &title,
        "Suggested while preparing the weekly review.",
        origin,
        brainstead_core::changes::from_texts(Some(&before), &after),
    );
    s.model = Some(model(&app));
    crate::changes::submit(&app, s).map_err(|e| e.message().to_string())
}

/// Checks every minute whether the preparation is due; runs nothing unless Brainstead runs the
/// jobs.
pub fn start_scheduler(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        tick(&app);
        std::thread::sleep(std::time::Duration::from_secs(60));
    });
}

fn tick(app: &AppHandle) {
    let here = crate::lock(&app.state::<AppState>().settings).summaries_here;
    if !here || !enabled(app) || app.state::<VaultService>().root().is_none() {
        return;
    }
    let wp = app.state::<WeekPrep>();
    let last = crate::lock(&wp.state).last.as_deref().and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok());
    if last.is_none() {
        wp.stamp_now();
        return;
    }
    let (day, time) = review_schedule(app);
    if wp.running() || !prep::is_due(now(), day, &time, last) {
        return;
    }
    wp.stamp_now();
    // The week the review that's due is for, prepared unless it's prepared already as it is.
    let week = prep::review_week(prep::last_due(now(), day, &time).date());
    if let Err(e) = ensure(app, &week, "schedule") {
        crate::applog!("weekly review preparation: {e}");
    }
}

fn gather(app: &AppHandle, week: &str) -> Result<prep::Inputs, String> {
    let today = Local::now().date_naive();
    let ms = chrono::Utc::now().timestamp_millis();
    let inbox = gtd::inbox_items(app)?;
    app.state::<VaultService>().with_index(|ix, v| prep::gather(&v.root, ix, week, today, ms, &inbox))
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Started {
    chat_id: String,
    title: String,
    model: String,
    prompt: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Finished {
    chat_id: String,
    filename: Option<String>,
}

/// Starts a run on its own thread; returns its chat id.
fn start(app: &AppHandle, week: &str, trigger: &str) -> Result<String, String> {
    let wp = app.state::<WeekPrep>();
    let t = now();
    let id = format!("weekprep-{}", t.format("%Y%m%d%H%M%S"));
    {
        let mut g = crate::lock(&wp.going);
        if g.is_some() {
            return Err("The weekly review is being prepared already.".into());
        }
        *g = Some((id.clone(), week.into()));
    }
    let model = model(app);
    {
        let mut s = crate::lock(&wp.state);
        s.runs.push(Run {
            id: id.clone(),
            week: week.into(),
            trigger: trigger.into(),
            model: model.clone(),
            started_at: stamp(t),
            finished_at: None,
            status: "running".into(),
            error: None,
            count: 0,
            chat: None,
        });
        let n = s.runs.len();
        if n > KEEP_RUNS {
            s.runs.drain(..n - KEEP_RUNS);
        }
    }
    wp.save();
    let _ = app.emit(CHANGED, ());
    let (app2, id2, week2) = (app.clone(), id.clone(), week.to_string());
    std::thread::spawn(move || run(&app2, &id2, &week2, &model));
    Ok(id)
}

fn run(app: &AppHandle, id: &str, week: &str, model: &str) {
    let wp = app.state::<WeekPrep>();
    let title = format!("Weekly review preparation · {week}");
    let ask_line =
        format!("Suggestions for each step of the weekly review of {week}, from the notes, tasks and Inbox Brainstead gathered.");
    let _ = app.emit("review-started", Started { chat_id: id.into(), title: title.clone(), model: model.into(), prompt: ask_line.clone() });
    let mut looked: Vec<String> = Vec::new();
    let mut answer = String::new();
    let result = prepare(app, id, week, model, &mut looked, &mut answer);

    // The run as a chat: what was asked, what it looked at, and what came of it.
    let mut transcript = vec![Item { id: "1".into(), role: "user".into(), text: ask_line, meta: None }];
    transcript.extend(looked.iter().enumerate().map(|(i, t)| Item {
        id: format!("t{i}"),
        role: "tool".into(),
        text: t.clone(),
        meta: None,
    }));
    match &result {
        Ok(p) => {
            let by_step = prep::STEPS
                .iter()
                .map(|s| (s, p.suggestions.iter().filter(|x| x.step == *s).count()))
                .filter(|(_, n)| *n > 0)
                .map(|(s, n)| format!("{s} {n}"))
                .collect::<Vec<_>>()
                .join(", ");
            transcript.push(Item {
                id: "a".into(),
                role: "assistant".into(),
                text: format!(
                    "{} suggestions for the weekly review of {week}{}{}. Nothing is changed until you accept one in the Weekly review.",
                    p.suggestions.len(),
                    if by_step.is_empty() { String::new() } else { format!(" ({by_step})") },
                    if p.dropped > 0 { format!("; {} left out because they failed Brainstead's checks", p.dropped) } else { String::new() }
                ),
                meta: Some(model.into()),
            });
            let n = p.suggestions.len();
            wp.update_run(id, |r| {
                r.status = "done".into();
                r.count = n;
            });
        }
        Err(e) => {
            let stopped = crate::lock(&wp.stopped).iter().any(|s| s == id);
            let e = if stopped { "Stopped.".to_string() } else { e.clone() };
            if !answer.trim().is_empty() {
                transcript.push(Item { id: "a".into(), role: "assistant".into(), text: answer.clone(), meta: Some(model.into()) });
            }
            transcript.push(Item { id: "e".into(), role: "error".into(), text: e.clone(), meta: None });
            wp.update_run(id, |r| {
                r.status = if stopped { "stopped".into() } else { "error".into() };
                r.error = Some(e.clone());
            });
        }
    }
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let chat = Chat {
        summary: Summary {
            id: id.into(),
            title,
            model: Some(model.into()),
            created_at: ts.clone(),
            updated_at: ts,
            state: "archived".into(),
            kind: Some("review".into()),
            ..Default::default()
        },
        transcript,
    };
    let filename = ask::save_chat(app, chat, false).ok().map(|s| s.filename);
    wp.update_run(id, |r| {
        r.finished_at = Some(stamp(now()));
        r.chat = filename.clone();
    });
    *crate::lock(&wp.going) = None;
    let _ = app.emit("review-done", Finished { chat_id: id.into(), filename });
    let _ = app.emit(CHANGED, ());
}

/// Whether a source path the model quotes is read to check its suggestion: vault-relative or
/// absolute, as the model was shown it, but never climbing with `..`. On Windows a drive or `\\?\`
/// prefix is refused too, since joining one onto the vault's root replaces the root (prefixes are
/// parsed only on Windows).
fn readable_source(p: &str) -> bool {
    std::path::Path::new(p).components().all(|c| !matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_)))
}

/// Gathers the week, asks the model, checks its suggestions and saves them.
/// `answer` gets the model's answer, to show in the run's chat when it couldn't be read.
fn prepare(app: &AppHandle, id: &str, week: &str, model: &str, looked: &mut Vec<String>, answer: &mut String) -> Result<Prep, String> {
    let inputs = gather(app, week)?;
    let system = brainstead_core::ask::system_prompt(&ask::today(), false);
    let mut on = |e: &Event| {
        if let Event::Status { text } = e {
            if !looked.contains(text) {
                looked.push(text.clone());
            }
        }
    };
    let out = ask::run(app, id, &prep::prompt(&inputs, &ask::today()), model, None, &system, None, &mut on);
    if let Some(e) = out.error {
        return Err(e);
    }
    answer.clone_from(&out.text);
    let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
    let (suggestions, dropped) = prep::parse(&out.text, &inputs, |p| {
        if !readable_source(p) {
            return None;
        }
        std::fs::read_to_string(root.join(p)).ok()
    })?;
    if !dropped.is_empty() {
        crate::applog!("weekly review preparation left out {}: {}", dropped.len(), dropped.join("; "));
    }
    let p = Prep {
        week: week.into(),
        prepared_at: stamp(now()),
        model: model.into(),
        stamp: prep::stamp(&inputs),
        suggestions,
        dropped: dropped.len(),
    };
    let _ = std::fs::create_dir_all(dir());
    write::write_atomic(&week_file(week), &serde_json::to_vec_pretty(&p).map_err(|e| e.to_string())?, false).map_err(|e| e.to_string())?;
    prune();
    Ok(p)
}

/// Keeps the latest weeks' suggestions.
fn prune() {
    let Ok(rd) = std::fs::read_dir(dir()) else { return };
    let mut weeks: Vec<PathBuf> =
        rd.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().contains("-W"))).collect();
    weeks.sort();
    if weeks.len() > KEEP_WEEKS {
        for p in &weeks[..weeks.len() - KEEP_WEEKS] {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::readable_source;

    #[test]
    fn a_source_is_read_unless_it_climbs_or_names_a_drive() {
        for ok in ["wiki/x.md", "Meetings/2026-10-05 Orbit App.md", "/Users/maya/Vault/wiki/x.md"] {
            assert!(readable_source(ok), "{ok}");
        }
        for bad in ["../x.md", "wiki/../../x.md", ".."] {
            assert!(!readable_source(bad), "{bad}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_or_verbatim_source_is_not_read_on_windows() {
        for bad in [r"C:\Users\maya\x.md", "C:x.md", "c:/x.md", r"\\?\C:\x.md", r"wiki\..\..\x.md"] {
            assert!(!readable_source(bad), "{bad}");
        }
    }
}
