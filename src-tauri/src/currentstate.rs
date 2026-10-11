// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Write Current state (D-20261006-10): for wiki pages with a Timeline but no Current state, the
//! cheap model writes one from the page's opening and newest entries only (core pageshape), each
//! checked and made as a change in one Changes run, with one `log.md` line for the run. A sample
//! of a few pages first, then the rest; it runs in the background, four pages at once, and can be stopped.

use std::sync::atomic::{AtomicBool, Ordering};

use brainstead_core::pageshape;
use brainstead_core::proposals::{self, Kind, Origin};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::edits::{writable, Res};
use crate::vault::VaultService;
use crate::AppState;

/// The start of a run's group in Changes.
pub const RUN: &str = "current-state-";
/// What the model sees of a page.
const BUDGET: usize = 6_000;
/// Pages written at once: each is its own short model call.
const AT_ONCE: usize = 4;

static RUNNING: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);

/// The run going now or the last one, for Knowledge health.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Last {
    pub running: bool,
    pub run: String,
    pub total: usize,
    pub done: usize,
    pub written: usize,
    /// Pages the model had nothing current for.
    pub nothing: usize,
    /// Pages it couldn't do, and why.
    pub failed: Vec<String>,
    pub error: Option<String>,
    pub model: String,
}

/// Whether Write Current state is going.
pub fn going() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

fn last_path() -> std::path::PathBuf {
    crate::platform::data_dir().join("current-state.json")
}

pub fn load_last() -> Last {
    let mut l: Last = std::fs::read(last_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    // A run the app quit during can't still be going.
    l.running &= RUNNING.load(Ordering::SeqCst);
    l
}

fn save(app: &AppHandle, l: &Last) {
    if let Ok(j) = serde_json::to_vec_pretty(l) {
        let _ = brainstead_core::write::write_atomic(&last_path(), &j, false);
    }
    let _ = app.emit("current-state-changed", l);
}

/// The cheap model when Claude is the assistant (the evals rule), else the job's model.
fn model(app: &AppHandle) -> String {
    let m = crate::ask::job_model(app, "contradictions");
    if m.starts_with("claude") {
        "claude:haiku".into()
    } else {
        m
    }
}

/// Starts a run on `pages` (by path or name), or on every page that wants one, at most `limit`
/// of them; false when one is going already. Unattended, a change that fails a check is held.
pub fn start(app: &AppHandle, pages: Option<Vec<String>>, limit: Option<usize>, unattended: bool) -> Res<bool> {
    writable(&app.state::<AppState>())?;
    let root = app.state::<VaultService>().root().ok_or("No vault is open.".to_string())?;
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    STOP.store(false, Ordering::SeqCst);
    let mut todo: Vec<String> = pageshape::vault_files(&root)
        .into_iter()
        .filter(|p| pageshape::shaped(p))
        .filter(|p| {
            pages.as_ref().is_none_or(|ps| ps.iter().any(|x| x == p || brainstead_core::filename::stem(p).eq_ignore_ascii_case(x.trim())))
        })
        .filter(|p| std::fs::read_to_string(root.join(p)).is_ok_and(|t| pageshape::wants_current_state(&t)))
        .collect();
    todo.sort();
    if let Some(n) = limit {
        todo.truncate(n);
    }
    let l =
        Last { running: true, run: format!("{RUN}{}", proposals::new_id()), total: todo.len(), model: model(app), ..Default::default() };
    save(app, &l);
    let (run, model) = (l.run.clone(), l.model.clone());
    let last = std::sync::Arc::new(std::sync::Mutex::new(l));
    let queue = std::sync::Arc::new(std::sync::Mutex::new(todo.into_iter().rev().collect::<Vec<_>>()));
    let app = app.clone();
    std::thread::spawn(move || {
        let workers: Vec<_> = (0..AT_ONCE)
            .map(|w| {
                let (app, root, run, model, last, queue) =
                    (app.clone(), root.clone(), run.clone(), model.clone(), last.clone(), queue.clone());
                std::thread::spawn(move || loop {
                    if STOP.load(Ordering::SeqCst) {
                        break;
                    }
                    let Some(page) = queue.lock().unwrap().pop() else { break };
                    let got = one(&app, &root, &page, &run, &model, w, unattended);
                    let mut l = last.lock().unwrap();
                    match got {
                        Ok(true) => l.written += 1,
                        Ok(false) => l.nothing += 1,
                        Err(e) => l.failed.push(format!("{page}: {e}")),
                    }
                    l.done += 1;
                    save(&app, &l);
                })
            })
            .collect();
        for w in workers {
            let _ = w.join();
        }
        let mut l = last.lock().unwrap();
        if STOP.load(Ordering::SeqCst) && l.done < l.total {
            l.error = Some("Stopped.".into());
        }
        if l.written > 0 {
            let n = l.written;
            let detail = format!("wrote Current state on {n} page{}", if n == 1 { "" } else { "s" });
            let entry =
                brainstead_core::reviews::log_entry("lint-fix", "Knowledge health", Some(&detail), chrono::Local::now().naive_local());
            if let Err(e) = crate::edits::add_log(&root, &entry) {
                crate::applog!("current state log line: {e}");
            }
            crate::edits::written(&app, &[root.join("log.md")]);
        }
        l.running = false;
        save(&app, &l);
        RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(true)
}

/// One page: asks the model, checks its answer and makes the change. False when the model had
/// nothing to say about how things stand now.
fn one(
    app: &AppHandle,
    root: &std::path::Path,
    page: &str,
    run: &str,
    model: &str,
    worker: usize,
    unattended: bool,
) -> Result<bool, String> {
    let before = std::fs::read_to_string(root.join(page)).map_err(|e| e.to_string())?;
    if !pageshape::wants_current_state(&before) {
        return Ok(false);
    }
    let out = crate::ask::run(
        app,
        &format!("current-state-{worker}"),
        &pageshape::current_state_prompt(page, &before, BUDGET),
        model,
        None,
        &brainstead_core::ask::system_prompt(&crate::ask::today(), false),
        None,
        &mut |_| {},
    );
    if let Some(e) = out.error {
        return Err(e);
    }
    let files = pageshape::vault_files(root);
    let names: std::collections::HashSet<String> = files.iter().map(|f| brainstead_core::links::key(f)).collect();
    let exists = |t: &str| names.contains(&brainstead_core::links::key(t));
    let text = match pageshape::current_state_answer(&before, &out.text, &exists) {
        Ok(t) => t,
        Err(why) if why.starts_with("the model found nothing") => return Ok(false),
        Err(why) => return Err(why),
    };
    let after = pageshape::with_current_state(&before, &text);
    let origin = Origin {
        kind: "lint".into(),
        label: Some("Write Current state".into()),
        run: Some(run.into()),
        trigger: unattended.then(|| "scheduled".into()),
        ..Default::default()
    };
    let mut s = crate::changes::Submit::new(
        page,
        Kind::Edit,
        "Wrote a Current state",
        "Written by the model from the page's opening and newest Timeline entries only.",
        origin,
        brainstead_core::changes::from_texts(Some(&before), &after),
    );
    s.model = Some(model.into());
    crate::changes::submit(app, s).map_err(|e| e.message().to_string())?;
    Ok(true)
}

/// Stops the run going: it ends after the page it's on.
pub fn stop(app: &AppHandle) {
    if RUNNING.load(Ordering::SeqCst) {
        STOP.store(true, Ordering::SeqCst);
        for w in 0..AT_ONCE {
            app.state::<std::sync::Arc<crate::ask::Running>>().cancel(&format!("current-state-{w}"));
        }
    }
}

#[tauri::command]
pub fn current_state_start(app: AppHandle, pages: Option<Vec<String>>, limit: Option<usize>, unattended: Option<bool>) -> Res<bool> {
    start(&app, pages, limit, unattended.unwrap_or(false))
}

#[tauri::command]
pub fn current_state_status() -> Last {
    load_last()
}

#[tauri::command]
pub fn current_state_stop(app: AppHandle) {
    stop(&app);
}
