// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scheduled daily and weekly reviews, run here in Rust (a hidden webview throttles its
//! timers). Only when Settings › Jobs & schedule says Brainstead runs them: one app at a time.
//!
//! A run gathers the review's inputs itself (brainstead_core::reviews, replacing the previous app's
//! scripts), asks the model with read-only tools to compose the block from them and the workflow,
//! checks the block has its heading, then writes it: upserted into the month's review note through
//! Changes (with its line in `log.md`), whose checks hold a scheduled run's block when one fails.
//! The model never writes. Each run is also a chat
//! (kind "review"), saved like any other, which the window opens as a tab while it runs; its
//! write can be undone while the files are as the run left them.

use std::path::PathBuf;
use std::sync::Mutex;

use brainstead_core::ask::Event;
use brainstead_core::chats::{Chat, Item, Summary};
use brainstead_core::proposals::{Kind, Origin};
use brainstead_core::reviews::{self, schedule, ReviewKind, Target, Upsert, Window};
use brainstead_core::{trash, write};
use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::edits::{writable, written};
use crate::vault::VaultService;
use crate::{ask, platform, AppState};

const KEEP_RUNS: usize = 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub kind: ReviewKind,
    /// 2026-10-01 or 2026-W40.
    pub target: String,
    /// "schedule", "manual" or "unattended".
    pub trigger: String,
    pub model: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// running, done or error.
    pub status: String,
    pub error: Option<String>,
    /// The review note written, vault-relative.
    pub file: Option<String>,
    pub replaced: bool,
    pub undone: bool,
    /// The run's chat file.
    pub chat: Option<String>,
}

/// What a run changed, to undo it: each file's text before (None: it didn't exist) and the
/// version it was left at.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Written {
    path: String,
    before: Option<String>,
    after: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Saved {
    /// When each kind last ran (or its schedule changed), local `YYYY-MM-DDTHH:MM:SS`.
    last_daily: Option<String>,
    last_weekly: Option<String>,
    runs: Vec<Run>,
}

impl Saved {
    fn last(&self) -> schedule::LastRun {
        let p = |s: &Option<String>| s.as_deref().and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok());
        schedule::LastRun { daily: p(&self.last_daily), weekly: p(&self.last_weekly) }
    }
    fn set_last(&mut self, k: ReviewKind, t: NaiveDateTime) {
        match k {
            ReviewKind::Daily => self.last_daily = Some(stamp(t)),
            ReviewKind::Weekly => self.last_weekly = Some(stamp(t)),
        }
    }
}

pub struct Reviews {
    path: PathBuf,
    state: Mutex<Saved>,
    running: Mutex<Vec<ReviewKind>>,
    /// Runs Stop was pressed on.
    stopped: Mutex<Vec<String>>,
}

fn dir() -> PathBuf {
    platform::data_dir().join("summaries")
}

/// `automated.json`: the user's own app-generated session signatures (stage 6 notes).
pub fn automated_file() -> PathBuf {
    dir().join("automated.json")
}

/// The user's own tools (Settings › Jobs & schedule): the signatures in `automated.json`, not
/// the built-in ones. A file that doesn't read gives none.
#[tauri::command]
pub fn automated_list() -> Vec<reviews::sessions::Automation> {
    std::fs::read(automated_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Saves the user's tools, each checked to have a name and something to match on.
#[tauri::command]
pub fn automated_save(list: Vec<reviews::sessions::Automation>) -> Result<(), String> {
    let list: Vec<_> = list
        .into_iter()
        .map(|a| reviews::sessions::Automation {
            label: a.label.trim().into(),
            cwd_contains: a.cwd_contains.trim().into(),
            opening: a.opening.trim().to_lowercase(),
        })
        .collect();
    reviews::sessions::check_automations(&list)?;
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?;
    write::write_atomic(&automated_file(), &json, false).map_err(|e| e.to_string())
}

/// Folders whose Claude Code sessions of the last two weeks look like a tool's, as tools to list.
/// Not the vault, where Ask's chats run (the user's own work, however short), nor temporary
/// folders, whose one-off paths would never match again.
#[tauri::command]
pub async fn automated_suggest(app: AppHandle) -> Result<Vec<reviews::sessions::Suggestion>, String> {
    let vault = app.state::<VaultService>().root().map(|r| r.to_string_lossy().into_owned());
    tauri::async_runtime::spawn_blocking(move || {
        let projects = dirs::home_dir().unwrap_or_default().join(".claude").join("projects");
        let listed = std::fs::read_to_string(automated_file()).ok();
        let automations = reviews::sessions::automations(listed.as_deref());
        let end = now();
        let sessions = reviews::sessions::sessions_for_window(&projects, end - chrono::Duration::days(14), end, &automations);
        // Folders are compared with `/` for `\` and no trailing separator; Windows' are case-blind.
        let norm = |p: &str| {
            let p = p.replace('\\', "/").trim_end_matches('/').to_string();
            if cfg!(windows) {
                p.to_lowercase()
            } else {
                p
            }
        };
        let (vault, temp) = (vault.as_deref().map(norm), norm(&std::env::temp_dir().to_string_lossy()));
        let skip = |cwd: &str| {
            let cwd = norm(cwd);
            vault.as_deref().is_some_and(|v| cwd == v)
                || cwd.starts_with(&temp)
                || cwd.starts_with("/tmp/")
                || cwd.starts_with("/private/")
                || cwd.starts_with("/var/folders/")
        };
        reviews::sessions::suggestions(&sessions).into_iter().filter(|g| !skip(&g.cwd)).take(20).collect()
    })
    .await
    .map_err(|e| e.to_string())
}

fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

fn stamp(t: NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M:%S").to_string()
}

impl Reviews {
    /// Whether a summary is being written.
    pub fn running(&self) -> bool {
        !self.running.lock().unwrap().is_empty()
    }

    pub fn load() -> Reviews {
        let path = dir().join("state.json");
        let mut state: Saved = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        // A run the app quit during isn't going any more.
        for r in state.runs.iter_mut().filter(|r| r.status == "running") {
            r.status = "stopped".into();
            r.error = Some("Stopped when Brainstead closed.".into());
        }
        Reviews { path, state: Mutex::new(state), running: Mutex::new(Vec::new()), stopped: Mutex::new(Vec::new()) }
    }

    fn save(&self) {
        let s = self.state.lock().unwrap();
        if let Ok(json) = serde_json::to_vec_pretty(&*s) {
            let _ = std::fs::create_dir_all(dir());
            let _ = write::write_atomic(&self.path, &json, false);
        }
    }

    fn update_run(&self, id: &str, f: impl FnOnce(&mut Run)) {
        if let Some(r) = self.state.lock().unwrap().runs.iter_mut().find(|r| r.id == id) {
            f(r);
        }
        self.save();
    }

    /// A schedule changed (or Brainstead took the reviews over): those kinds count as run now, so
    /// they first fire at their next time rather than at once.
    pub fn stamp(&self, kinds: &[ReviewKind]) {
        let t = now();
        {
            let mut s = self.state.lock().unwrap();
            for k in kinds {
                s.set_last(*k, t);
            }
        }
        self.save();
    }
}

#[derive(Serialize)]
pub struct Status {
    runs: Vec<Run>,
    next: schedule::NextDue,
    running: Vec<ReviewKind>,
}

#[tauri::command]
pub fn reviews_status(st: State<AppState>, rv: State<Reviews>, vs: State<VaultService>) -> Status {
    let s = st.settings.lock().unwrap().clone();
    let mut runs = rv.state.lock().unwrap().runs.clone();
    runs.reverse();
    // A run from before the rename names the old file: once that's been renamed, the new one.
    if let Some(root) = vs.root() {
        for r in &mut runs {
            r.file = r.file.take().map(|f| current_file(&f, |p| root.join(p).exists()));
        }
    }
    let next = if s.summaries_here { schedule::next_due(&s.summaries, now()) } else { schedule::next_due(&Default::default(), now()) };
    Status { runs, next, running: rv.running.lock().unwrap().clone() }
}

/// The latest daily or weekly summary block on file, for Today's cards; None when there's none
/// (or no vault).
#[tauri::command]
pub async fn review_latest(app: AppHandle, kind: ReviewKind) -> Result<Option<reviews::Latest>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(root) = app.state::<VaultService>().root() else { return Ok(None) };
        Ok(reviews::latest(kind, Local::now().date_naive(), |p| std::fs::read_to_string(root.join(p)).ok()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Starts a review now, whatever the schedule. Returns the run's chat id.
#[tauri::command]
/// `day` (YYYY-MM-DD) picks what it covers: that day for a daily summary, that day's week for a
/// weekly one; without it, what a run due now would cover.
pub fn review_run_now(app: AppHandle, kind: ReviewKind, day: Option<String>, unattended: Option<bool>) -> Result<String, String> {
    if app.state::<Reviews>().running.lock().unwrap().contains(&kind) {
        return Err("That summary is already running.".into());
    }
    let target = match day.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        None => schedule::target_for(kind, now()),
        Some(d) => {
            let d = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| format!("Expected a date like 2026-10-02, got {d:?}."))?;
            if d >= now().date() {
                return Err("A summary can only cover a day that has ended.".into());
            }
            match kind {
                ReviewKind::Daily => Target::Day(d),
                // As Run now, which only does the week just ended: not this week, though its day has.
                ReviewKind::Weekly if !Target::week_of(d).ended(now().date()) => {
                    return Err("A weekly summary can only cover a week that has ended, and this week runs to Sunday.".into())
                }
                ReviewKind::Weekly => Target::week_of(d),
            }
        }
    };
    // "unattended": an assistant nobody is watching asked, so it counts as scheduled but isn't retried.
    Ok(start_for(&app, kind, target, if unattended.unwrap_or(false) { "unattended" } else { "manual" }))
}

/// Checks every half minute whether a review is due; runs nothing unless Brainstead runs them.
pub fn start_scheduler(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        tick(&app);
        std::thread::sleep(std::time::Duration::from_secs(30));
    });
}

fn tick(app: &AppHandle) {
    let s = app.state::<AppState>().settings.lock().unwrap().clone();
    if !s.summaries_here || app.state::<VaultService>().root().is_none() {
        return;
    }
    let rv = app.state::<Reviews>();
    let last = rv.state.lock().unwrap().last();
    let due = schedule::compute_due(&s.summaries, &last, now());
    let running = rv.running.lock().unwrap().clone();
    for d in schedule::to_start(&due, &running) {
        start(app, d.kind, d.due_at, "schedule");
    }
}

/// A run's file as it is now: its new name when the old one is gone and the new one is there.
fn current_file(file: &str, exists: impl Fn(&str) -> bool) -> String {
    match reviews::target::new_name(file) {
        Some(new) if !exists(file) && exists(&new) => new,
        _ => file.to_string(),
    }
}

fn label(w: &Window) -> String {
    w.label().to_string()
}

fn kind_name(k: ReviewKind) -> &'static str {
    match k {
        ReviewKind::Daily => "Daily",
        ReviewKind::Weekly => "Weekly",
    }
}

/// What the UI calls the run: the scheduled weekly one is the "weekly summary", so it isn't
/// taken for the guided Weekly review screen.
fn run_name(k: ReviewKind) -> &'static str {
    match k {
        ReviewKind::Daily => "Daily summary",
        ReviewKind::Weekly => "Weekly summary",
    }
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
fn start(app: &AppHandle, kind: ReviewKind, due: NaiveDateTime, trigger: &str) -> String {
    start_for(app, kind, schedule::target_for(kind, due), trigger)
}

fn start_for(app: &AppHandle, kind: ReviewKind, target: Target, trigger: &str) -> String {
    let rv = app.state::<Reviews>();
    let t = now();
    let id = format!("summary-{}-{}", t.format("%Y%m%d%H%M%S"), kind_name(kind).to_lowercase());
    let window = reviews::target::window(target);
    let model = crate::ask::job_model(app, "summaries");
    rv.running.lock().unwrap().push(kind);
    {
        let mut s = rv.state.lock().unwrap();
        s.set_last(kind, t);
        s.runs.push(Run {
            id: id.clone(),
            kind,
            target: label(&window),
            trigger: trigger.into(),
            model: model.clone(),
            started_at: stamp(t),
            finished_at: None,
            status: "running".into(),
            error: None,
            file: None,
            replaced: false,
            undone: false,
            chat: None,
        });
        let n = s.runs.len();
        if n > KEEP_RUNS {
            s.runs.drain(..n - KEEP_RUNS);
        }
    }
    rv.save();
    let _ = app.emit("reviews-changed", ());
    let app2 = app.clone();
    let id2 = id.clone();
    // A scheduled run that fails is tried once more, 15 minutes later (the notification says so).
    let retry = trigger == "schedule";
    let scheduled = trigger != "manual";
    std::thread::spawn(move || {
        if !run(&app2, &id2, kind, window, &model, retry, scheduled) && retry {
            std::thread::sleep(std::time::Duration::from_secs(RETRY_AFTER_SECS));
            // Not when one is going already (Run now, or the next scheduled one).
            let busy = app2.state::<Reviews>().running.lock().unwrap().contains(&kind);
            if !busy && app2.state::<AppState>().settings.lock().unwrap().summaries_here {
                start_for(&app2, kind, target, "retry");
            }
        }
    });
    id
}

/// How long after a failed scheduled review it's tried again (once).
const RETRY_AFTER_SECS: u64 = 15 * 60;

/// Runs the review; true when it was written. `retrying`: a failure will be tried again.
/// `scheduled`: the scheduler started it, not Run now.
fn run(app: &AppHandle, id: &str, kind: ReviewKind, w: Window, model: &str, retrying: bool, scheduled: bool) -> bool {
    let title = format!("{} · {}", run_name(kind), label(&w));
    let ask_line = format!("{} for {}, from the notes, tasks and Claude Code sessions Brainstead gathered.", run_name(kind), label(&w));
    let _ = app.emit("review-started", Started { chat_id: id.into(), title: title.clone(), model: model.into(), prompt: ask_line.clone() });

    let mut looked: Vec<String> = Vec::new();
    let queued = app.state::<AppState>().settings.lock().unwrap().ui.get("holdSummaries").and_then(|v| v.as_bool()).unwrap_or(false);
    let result = compose(app, id, kind, &w, model, &mut looked).and_then(|block| {
        let origin = Origin {
            kind: "review".into(),
            label: Some(w.log_detail()),
            run: Some(id.into()),
            trigger: scheduled.then(|| "scheduled".into()),
            ..Default::default()
        };
        if queued {
            hold(app, &w, &block, &title, model, origin).map(|f| (block, f, None))
        } else {
            put(app, id, &w, &block, &title, model, origin).map(|(f, how)| (block, f, how))
        }
    });

    // The run as a chat: what was asked, what it looked at, and the block or what went wrong.
    let mut transcript = vec![Item { id: "1".into(), role: "user".into(), text: ask_line, meta: None }];
    transcript.extend(looked.iter().enumerate().map(|(i, t)| Item {
        id: format!("t{i}"),
        role: "tool".into(),
        text: t.clone(),
        meta: None,
    }));
    let rv = app.state::<Reviews>();
    match &result {
        Ok((block, file, how)) => {
            let note = file.trim_end_matches(".md");
            let what = match how {
                Some(Upsert::Created) => "started",
                Some(Upsert::Replaced) => "replaced its block in",
                Some(Upsert::Prepended) => "added to the top of",
                None => "",
            };
            transcript.push(Item {
                id: "a".into(),
                role: "assistant".into(),
                text: if how.is_none() {
                    format!("{block}\n\n---\n\nBrainstead held this in Changes for you to accept, for [[{note}]].")
                } else {
                    format!("{block}\n\n---\n\nBrainstead {what} [[{note}]].")
                },
                meta: Some(model.into()),
            });
            rv.update_run(id, |r| {
                r.status = "done".into();
                r.file = Some(file.clone());
                r.replaced = matches!(how, Some(Upsert::Replaced));
            });
        }
        Err(e) => {
            let stopped = rv.stopped.lock().unwrap().contains(&id.to_string());
            let e = if stopped { "Stopped.".to_string() } else { e.clone() };
            transcript.push(Item { id: "e".into(), role: "error".into(), text: e.clone(), meta: None });
            rv.update_run(id, |r| {
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
    rv.update_run(id, |r| {
        r.finished_at = Some(stamp(now()));
        r.chat = filename.clone();
    });
    rv.running.lock().unwrap().retain(|k| *k != kind);
    let _ = app.emit("review-done", Finished { chat_id: id.into(), filename });
    let _ = app.emit("reviews-changed", ());
    let ok = result.is_ok();
    if rv.stopped.lock().unwrap().contains(&id.to_string()) {
        // Stopped by the user: no failure notice, no retry.
        return true;
    }
    notify(app, kind, &w, &result.map(|(_, f, _)| f), retrying);
    ok
}

/// Stops the review of this kind that's running (Stop, beside Run now).
#[tauri::command]
pub fn reviews_stop(app: AppHandle, kind: ReviewKind) {
    let rv = app.state::<Reviews>();
    let ids: Vec<String> =
        rv.state.lock().unwrap().runs.iter().filter(|r| r.kind == kind && r.status == "running").map(|r| r.id.clone()).collect();
    for id in ids {
        rv.stopped.lock().unwrap().push(id.clone());
        app.state::<std::sync::Arc<ask::Running>>().cancel(&id);
    }
}

/// Gathers the inputs and asks the model for the block.
fn compose(app: &AppHandle, id: &str, kind: ReviewKind, w: &Window, model: &str, looked: &mut Vec<String>) -> Result<String, String> {
    let projects = dirs::home_dir().unwrap_or_default().join(".claude").join("projects");
    // The user's own app-generated sessions, if they've listed any.
    let listed = std::fs::read_to_string(automated_file()).ok();
    let automations = reviews::sessions::automations(listed.as_deref());
    let t = now();
    let inputs = app.state::<VaultService>().with_index(|ix, v| reviews::gather(&v.root, ix, &projects, &automations, w, t))?;
    let workflow = match kind {
        ReviewKind::Daily => reviews::DAILY_REVIEW,
        ReviewKind::Weekly => reviews::WEEKLY_REVIEW,
    };
    let prompt = format!(
        "{workflow}\n\n# Inputs\n\n{}\n\nReply with the block only, starting with the line `{}`.",
        reviews::render(&inputs),
        w.heading.trim()
    );
    let system = brainstead_core::ask::system_prompt(&ask::today(), false);
    let mut on = |e: &Event| {
        if let Event::Status { text } = e {
            if !looked.contains(text) {
                looked.push(text.clone());
            }
        }
    };
    let out = ask::run(app, id, &prompt, model, None, &system, None, &mut on);
    if let Some(e) = out.error {
        return Err(e);
    }
    reviews::extract_block(&out.text, &w.heading)
}

/// Writes the block into the month's note through Changes (which adds the `log.md` line and runs
/// the checks: a scheduled run whose change fails one is held instead), and keeps what's needed to
/// undo it. `None` for the how: it was held, not written.
fn put(
    app: &AppHandle,
    id: &str,
    w: &Window,
    block: &str,
    title: &str,
    model: &str,
    origin: Origin,
) -> Result<(String, Option<Upsert>), String> {
    let scheduled = brainstead_core::changes::scheduled(&origin);
    if !scheduled {
        writable(&app.state::<AppState>())
            .map_err(|_| "Brainstead is read-only, so the summary wasn't written. Turn read-only off in Settings › Vault.".to_string())?;
    }
    let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;

    // A re-run replaces the block where it is, even in a file from before the rename.
    let (file, heading) = w.locate(|f| std::fs::read_to_string(root.join(f)).ok());
    let note = root.join(&file);
    let before = std::fs::read_to_string(&note).ok();
    let (text, how) = reviews::upsert(before.as_deref().unwrap_or(""), &heading, block, Some(&w.header()));
    let kind = if before.is_some() { Kind::Edit } else { Kind::New };
    let mut s = crate::changes::Submit::new(&file, kind, title, "", origin, brainstead_core::changes::from_texts(before.as_deref(), &text));
    s.model = Some(model.into());
    let o = crate::changes::submit(app, s).map_err(|e| e.message().to_string())?;
    if !o.applied {
        crate::applog!("review held in Changes: {}", o.flags.join(" "));
        return Ok((file, None));
    }
    let after = std::fs::read(&note).map(|b| write::version(&b)).map_err(|e| e.to_string())?;
    let done = vec![Written { path: file.clone(), before, after }];
    let undo = serde_json::to_vec(&done).map_err(|e| e.to_string())?;
    let _ = std::fs::create_dir_all(dir().join("undo"));
    write::write_atomic(&dir().join("undo").join(format!("{id}.json")), &undo, false).map_err(|e| e.to_string())?;
    Ok((file, Some(how)))
}

/// The block held in Changes for the user to accept ("Hold the summaries for me", Settings › Jobs
/// & schedule), instead of a write.
fn hold(app: &AppHandle, w: &Window, block: &str, title: &str, model: &str, origin: Origin) -> Result<String, String> {
    let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
    let (file, heading) = w.locate(|f| std::fs::read_to_string(root.join(f)).ok());
    let before = std::fs::read_to_string(root.join(&file)).ok();
    let (text, _) = reviews::upsert(before.as_deref().unwrap_or(""), &heading, block, Some(&w.header()));
    let kind = if before.is_some() { Kind::Edit } else { Kind::New };
    let mut s = crate::changes::Submit::new(&file, kind, title, "", origin, brainstead_core::changes::from_texts(before.as_deref(), &text));
    s.model = Some(model.into());
    s.hold = Some("You asked for the summaries to be held for you (Settings › Jobs & schedule).".into());
    crate::changes::submit(app, s).map_err(|e| e.message().to_string())?;
    Ok(file)
}

/// Puts the files back as they were before the run, if they're still as it left them.
#[tauri::command]
pub fn review_undo(app: AppHandle, id: String) -> Result<(), String> {
    writable(&app.state::<AppState>()).map_err(|_| "Brainstead is read-only: turn it off in Settings › Vault to undo.".to_string())?;
    let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
    let file = dir().join("undo").join(format!("{id}.json"));
    let done: Vec<Written> =
        std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).ok_or("There's nothing to undo for that run.")?;
    // Check them all first, so nothing is half undone.
    for d in &done {
        let now = std::fs::read(root.join(&d.path)).map_err(|_| format!("{} isn't there any more.", d.path))?;
        if write::version(&now) != d.after {
            return Err(format!("{} has changed since the summary was written, so it wasn't undone.", d.path));
        }
    }
    let mut paths = Vec::new();
    for d in &done {
        let abs = root.join(&d.path);
        match &d.before {
            Some(b) => {
                write::save_file(&abs, b, &d.after).map_err(|e| e.to_string())?;
            }
            None => {
                trash::undo_created(&root, &d.path, "note").map_err(|e| e.to_string())?;
            }
        }
        paths.push(abs);
    }
    written(&app, &paths);
    let _ = std::fs::remove_file(file);
    crate::changes::mark_reverted(&app, &id);
    app.state::<Reviews>().update_run(&id, |r| r.undone = true);
    let _ = app.emit("reviews-changed", ());
    Ok(())
}

/// A summary's change reverted in Changes: the run is undone, as its own Undo leaves it, and has
/// nothing left to undo.
pub fn reverted_in_changes(app: &AppHandle, run: &str) {
    let _ = std::fs::remove_file(dir().join("undo").join(format!("{run}.json")));
    app.state::<Reviews>().update_run(run, |r| r.undone = true);
    let _ = app.emit("reviews-changed", ());
}

fn notify(app: &AppHandle, kind: ReviewKind, w: &Window, result: &Result<String, String>, retrying: bool) {
    use tauri_plugin_notification::NotificationExt;
    let (title, body) = match result {
        Ok(f) => (format!("{} written", run_name(kind)), format!("{} is in {}.", label(w), f.trim_end_matches(".md"))),
        Err(e) => (
            format!("{} failed", run_name(kind)),
            format!("{}{}", e.chars().take(180).collect::<String>(), if retrying { " Brainstead tries again in 15 minutes." } else { "" }),
        ),
    };
    let _ = app.notification().builder().title(title).body(body).show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_runs_file_follows_the_rename() {
        let renamed = |p: &str| p == "Me. Daily Summaries - 2026-09.md";
        assert_eq!(current_file("Me. Daily Reviews - 2026-09.md", renamed), "Me. Daily Summaries - 2026-09.md");
        // Still there under the old name, or not renamed: as it was.
        assert_eq!(current_file("Me. Daily Reviews - 2026-09.md", |_| true), "Me. Daily Reviews - 2026-09.md");
        assert_eq!(current_file("Me. Daily Reviews - 2026-09.md", |_| false), "Me. Daily Reviews - 2026-09.md");
        assert_eq!(current_file("Me. Daily Summaries - 2026-10.md", |_| false), "Me. Daily Summaries - 2026-10.md");
    }
}
