// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or
// later. See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Find tasks and projects (decided 2026-10-05): an on-demand run over the user's notes from the
//! last 90 days (core/src/find.rs), whose suggestions show at the top of Tasks and Projects. An
//! accepted one is made as an agent change (src/changes.rs), revertable in Changes; what's
//! accepted or skipped isn't suggested again. Kept in `find/state.json` in the app data folder.

use std::path::PathBuf;
use std::sync::Mutex;

use brainstead_core::find::{self, Suggestion};
use brainstead_core::proposals::{Kind, Origin};
use brainstead_core::{changes::Instruction, write};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::edits::{EditError, Res};
use crate::vault::VaultService;
use crate::{ask, platform};

const CHANGED: &str = "find-changed";
/// Decided ids kept, so a skipped suggestion stays skipped for a long while.
const KEEP_DECIDED: usize = 2000;

fn dir() -> PathBuf {
    platform::data_dir().join("find")
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// The last run, as the screens show it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Run {
    pub id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// running, done, error or stopped.
    pub status: String,
    pub error: Option<String>,
    pub model: String,
    /// Notes read, and the batches they went in.
    pub notes: usize,
    pub batches: usize,
    /// Batches done so far.
    pub done: usize,
    /// Suggestions found, and left out by the checks.
    pub found: usize,
    pub dropped: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub run: Option<Run>,
    /// Waiting to be accepted, edited or skipped.
    pub suggestions: Vec<Suggestion>,
    /// Ids accepted or skipped, oldest first: never suggested again.
    pub decided: Vec<String>,
}

pub struct Find {
    state: Mutex<State>,
    stopped: Mutex<bool>,
}

impl Find {
    pub fn load() -> Find {
        let mut state: State =
            std::fs::read(dir().join("state.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        // A run the app quit during isn't going any more.
        if let Some(r) = state.run.as_mut().filter(|r| r.status == "running") {
            r.status = "stopped".into();
            r.error = Some("Stopped when Brainstead closed.".into());
        }
        Find { state: Mutex::new(state), stopped: Mutex::new(false) }
    }

    fn save(&self) {
        let s = crate::lock(&self.state);
        if let Ok(json) = serde_json::to_vec_pretty(&*s) {
            let _ = std::fs::create_dir_all(dir());
            let _ = write::write_atomic(&dir().join("state.json"), &json, false);
        }
    }

    fn update(&self, app: &AppHandle, f: impl FnOnce(&mut State)) {
        f(&mut crate::lock(&self.state));
        self.save();
        let _ = app.emit(CHANGED, ());
    }

    pub fn running(&self) -> bool {
        crate::lock(&self.state).run.as_ref().is_some_and(|r| r.status == "running")
    }
}

#[tauri::command]
pub fn find_status(app: AppHandle) -> State {
    crate::lock(&app.state::<Find>().state).clone()
}

/// Starts a scan; returns its id. Off the main thread: the notes are read once the index is let go.
#[tauri::command]
pub async fn find_run(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || start(&app)).await.map_err(|e| e.to_string())?
}

fn start(app: &AppHandle) -> Result<String, String> {
    const BUSY: &str = "Brainstead is already looking for tasks and projects.";
    let f = app.state::<Find>();
    if f.running() {
        return Err(BUSY.into());
    }
    let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
    let id = format!("find-{}", chrono::Local::now().format("%Y%m%d%H%M%S"));
    let model = ask::job_model(app, "find");
    // The index is held only for the paths and the tasks and projects; the notes are read after.
    let (mut inputs, paths) = app.state::<VaultService>().with_index(|ix, _| find::gather(ix, chrono::Local::now().date_naive()))?;
    inputs.notes = find::read_notes(&root, paths);
    let batches = find::batches(&inputs).len();
    // Looking again: suggestions still waiting are kept unless they've lapsed (made by hand since,
    // or their note no longer says it); those kept aren't suggested twice.
    let read = |p: &str| brainstead_core::trash::safe_rel(p).ok().and_then(|_| std::fs::read_to_string(root.join(p)).ok());
    {
        // Checked again under the lock: two starts at once make one run.
        let mut s = crate::lock(&f.state);
        if s.run.as_ref().is_some_and(|r| r.status == "running") {
            return Err(BUSY.into());
        }
        *crate::lock(&f.stopped) = false;
        s.suggestions.retain(|x| find::still_holds(x, &inputs, read));
        s.run = Some(Run {
            id: id.clone(),
            started_at: now(),
            status: "running".into(),
            model: model.clone(),
            notes: inputs.notes.len(),
            batches,
            ..Default::default()
        });
    }
    f.save();
    let _ = app.emit(CHANGED, ());
    let (app2, id2) = (app.clone(), id.clone());
    std::thread::spawn(move || {
        let result = scan(&app2, &id2, &model, &inputs, &root);
        let f = app2.state::<Find>();
        let stopped = *crate::lock(&f.stopped);
        f.update(&app2, |s| {
            if let Some(r) = s.run.as_mut() {
                r.finished_at = Some(now());
                match &result {
                    Ok(()) => r.status = "done".into(),
                    Err(_) if stopped => r.status = "stopped".into(),
                    Err(e) => {
                        r.status = "error".into();
                        r.error = Some(e.clone());
                    }
                }
            }
        });
        let _ = tauri_plugin_notification::NotificationExt::notification(&app2)
            .builder()
            .title("Find tasks and projects")
            .body(match &result {
                Ok(()) => {
                    let s = crate::lock(&f.state);
                    let (t, p) = (
                        s.suggestions.iter().filter(|x| x.kind == "task").count(),
                        s.suggestions.iter().filter(|x| x.kind == "project").count(),
                    );
                    format!("{t} tasks and {p} projects suggested: see Tasks and Projects")
                }
                Err(e) => e.clone(),
            })
            .show();
    });
    Ok(id)
}

/// Each batch of notes to the model, its suggestions checked and added as they come.
fn scan(app: &AppHandle, id: &str, model: &str, inputs: &find::Inputs, root: &std::path::Path) -> Result<(), String> {
    let f = app.state::<Find>();
    let system = brainstead_core::ask::system_prompt(&ask::today(), false);
    let read = |p: &str| {
        if brainstead_core::trash::safe_rel(p).is_err() {
            return None;
        }
        std::fs::read_to_string(root.join(p)).ok()
    };
    for batch in find::batches(inputs) {
        if *crate::lock(&f.stopped) {
            return Err("Stopped.".into());
        }
        let out = ask::run(app, id, &find::prompt(&batch, inputs, &ask::today()), model, None, &system, None, &mut |_| {});
        if let Some(e) = out.error {
            return Err(e);
        }
        let seen: Vec<String> = {
            let s = crate::lock(&f.state);
            s.decided.iter().chain(s.suggestions.iter().map(|x| &x.id)).cloned().collect()
        };
        // One answer that can't be read loses that batch, not the scan.
        let (found, dropped) = match find::parse(&out.text, inputs, read, &seen) {
            Ok(r) => r,
            Err(e) => {
                crate::applog!("find tasks and projects skipped a batch it couldn't read: {e}");
                (Vec::new(), vec![format!("a batch of notes: {e}")])
            }
        };
        if !dropped.is_empty() {
            crate::applog!("find tasks and projects left out {}: {}", dropped.len(), dropped.join("; "));
        }
        f.update(app, |s| {
            if let Some(r) = s.run.as_mut() {
                r.done += 1;
                r.found += found.len();
                r.dropped += dropped.len();
            }
            s.suggestions.extend(found);
        });
    }
    Ok(())
}

#[tauri::command]
pub fn find_stop(app: AppHandle) {
    let f = app.state::<Find>();
    let id = crate::lock(&f.state).run.as_ref().filter(|r| r.status == "running").map(|r| r.id.clone());
    if let Some(id) = id {
        *crate::lock(&f.stopped) = true;
        app.state::<std::sync::Arc<ask::Running>>().cancel(&id);
    }
}

/// What the user changed before accepting.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Edit {
    /// The task, or the project's name.
    pub text: Option<String>,
    /// A task's project by name; Some("") for the To Do list.
    pub project: Option<String>,
    pub outcome: Option<String>,
    pub tasks: Option<Vec<String>>,
}

/// Accepts a suggestion (made as an agent change, revertable in Changes) or skips it; either way
/// it's never suggested again. The change's message when accepted. `unattended`: an assistant
/// nobody is watching accepted it, so the change is held when a check fails.
#[tauri::command]
pub async fn find_decide(app: AppHandle, id: String, action: String, edit: Option<Edit>, unattended: Option<bool>) -> Res<Option<String>> {
    let trigger = unattended.unwrap_or(false).then(|| "scheduled".to_string());
    tauri::async_runtime::spawn_blocking(move || decide(&app, &id, &action, edit.unwrap_or_default(), trigger))
        .await
        .map_err(|e| EditError::from(e.to_string()))?
}

fn decide(app: &AppHandle, id: &str, action: &str, edit: Edit, trigger: Option<String>) -> Res<Option<String>> {
    if action != "skip" && action != "accept" {
        return Err("action is accept or skip.".to_string().into());
    }
    let f = app.state::<Find>();
    // Taken out before it's made, under one lock, so a second accept (a double click) finds it gone
    // rather than adding the task twice.
    let (at, sg) = {
        let mut s = crate::lock(&f.state);
        let at = s.suggestions.iter().position(|x| x.id == id).ok_or_else(|| "That suggestion isn't there any more.".to_string())?;
        (at, s.suggestions.remove(at))
    };
    let message = if action == "accept" {
        match accept(app, &sg, edit, trigger) {
            Ok(m) => Some(m),
            Err(e) => {
                // Not made: it's back, where it was, to edit and try again.
                let mut s = crate::lock(&f.state);
                if !s.suggestions.iter().any(|x| x.id == sg.id) {
                    let at = at.min(s.suggestions.len());
                    s.suggestions.insert(at, sg);
                }
                return Err(e);
            }
        }
    } else {
        None
    };
    f.update(app, |s| {
        s.decided.push(id.to_string());
        let n = s.decided.len();
        if n > KEEP_DECIDED {
            s.decided.drain(..n - KEEP_DECIDED);
        }
    });
    Ok(message)
}

fn accept(app: &AppHandle, sg: &Suggestion, edit: Edit, trigger: Option<String>) -> Res<String> {
    let run = crate::lock(&app.state::<Find>().state).run.as_ref().map(|r| r.id.clone());
    let origin = Origin { kind: "find".into(), label: Some("Find tasks and projects".into()), run, trigger, ..Default::default() };
    let reason = sg.sources.first().map(|s| format!("From {}: “{}”", s.path.trim_end_matches(".md"), s.quote)).unwrap_or_default();
    let clean = |s: String| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = edit.text.map(clean).filter(|t| !t.is_empty()).unwrap_or_else(|| sg.text.clone());
    let s = if sg.kind == "project" {
        if !find::good_name(&text) {
            return Err(format!(
                "“{text}” can't be a project's name: leave out / \\ : * ? \" < > | [ ] # ^, and don't start it with a dot."
            )
            .into());
        }
        let outcome = edit.outcome.map(clean).or_else(|| sg.outcome.clone()).filter(|o| !o.is_empty());
        let tasks: Vec<String> = edit.tasks.unwrap_or_else(|| sg.tasks.clone()).into_iter().map(clean).filter(|t| !t.is_empty()).collect();
        let page = format!("{}{text}.md", brainstead_core::projects::PREFIX);
        crate::changes::Submit::new(
            &page,
            Kind::New,
            &format!("New project: {text}"),
            &reason,
            origin,
            Instruction::Page { content: find::project_text(outcome.as_deref(), &tasks) },
        )
    } else {
        let project = match edit.project {
            Some(p) if p.trim().is_empty() => None,
            Some(p) => Some(p),
            None => sg.project.clone(),
        };
        let page = match &project {
            Some(name) => {
                let projects = app.state::<VaultService>().with_index(|ix, _| ix.projects().map_err(|e| e.to_string()))?;
                projects
                    .into_iter()
                    .find(|p| p.name.eq_ignore_ascii_case(name))
                    .map(|p| p.path)
                    .ok_or_else(|| format!("There's no project called {name} any more."))?
            }
            None => brainstead_core::inbox::TODO_LIST.to_string(),
        };
        let mut line = format!("- [ ] {text}");
        if sg.waiting {
            line.push_str(" #waiting-for");
        }
        if let Some(d) = &sg.due {
            line = write::with_date(&line, write::DateKind::Due, Some(d)).map_err(|e| e.to_string())?;
        }
        crate::changes::Submit::new(
            &page,
            Kind::Task,
            &format!("Add “{text}”"),
            &reason,
            origin,
            Instruction::AddTask { line, heading: None },
        )
    };
    let mut s = s;
    s.quotes = sg
        .sources
        .iter()
        .map(|x| brainstead_core::proposals::Quote {
            source: x.path.clone(),
            anchor: None,
            text: x.quote.clone(),
            checked: Some(true),
            path: Some(x.path.clone()),
        })
        .collect();
    Ok(crate::changes::submit(app, s)?.message)
}
