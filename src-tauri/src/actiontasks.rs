// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! An action's task moves the action (D-20261118-26). An action made a task in the Inbox records
//! where the task went (`task::`), and the task links the action's block. After a note changes,
//! each such action's task is read: ticked, the action is done; cancelled, dropped; opened again,
//! open. Only a change in the task's state moves the action (the last state seen is kept in
//! `action-tasks.json`), so an action an ingest marked done isn't reopened by its open task. Each
//! change is recorded in Changes, as the app's own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use brainstead_core::initiatives::{self, ActionEdit, TaskState};
use brainstead_core::proposals::{Kind, Origin};
use tauri::{AppHandle, Manager};

use crate::vault::VaultService;

/// A look is waiting to run: notes changing in a burst are read once.
static PENDING: AtomicBool = AtomicBool::new(false);

fn state_file() -> PathBuf {
    crate::platform::data_dir().join("action-tasks.json")
}

fn load() -> BTreeMap<String, TaskState> {
    std::fs::read(state_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(s: &BTreeMap<String, TaskState>) {
    if let Ok(b) = serde_json::to_vec_pretty(s) {
        let _ = brainstead_core::write::write_atomic(&state_file(), &b, false);
    }
}

/// Some notes changed: the actions' tasks are read again shortly, once for the burst. A change only
/// to wiki pages (an action itself) doesn't start one.
pub fn vault_changed(app: &AppHandle, changed: &[String]) {
    if !changed.iter().any(|p| !p.starts_with("wiki/") && p.ends_with(".md")) {
        return;
    }
    if PENDING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        PENDING.store(false, Ordering::SeqCst);
        follow(&app);
    });
}

/// The note a `task::` field names (`[[Me. To Do List]]`, a project's note), as a vault path.
fn note_of(app: &AppHandle, field: &str) -> Option<String> {
    let name = field.trim().trim_start_matches("[[").trim_end_matches("]]").split(['|', '#']).next()?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    app.state::<VaultService>().with_index(|ix, _| brainstead_core::names::resolve(ix, &name)).ok()?.exact.map(|(p, _)| p)
}

/// Reads each action's task and moves the actions whose task changed.
pub fn follow(app: &AppHandle) {
    let Some(root) = app.state::<VaultService>().root() else { return };
    if crate::edits::writable(&app.state::<crate::AppState>()).is_err() {
        return;
    }
    let mut seen = load();
    let mut notes: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut moved = false;
    for page in initiatives::all(&root) {
        for a in page.actions.iter().filter(|a| a.task.is_some() && !a.id.is_empty()) {
            let Some(note) = note_of(app, a.task.as_deref().unwrap_or("")) else { continue };
            let text = notes.entry(note.clone()).or_insert_with(|| std::fs::read_to_string(root.join(&note)).ok());
            let Some(now) = text.as_deref().and_then(|t| initiatives::task_state(t, &a.id)) else { continue };
            let key = format!("{}#{}", page.path, a.id);
            let last = seen.insert(key, now);
            moved |= last != Some(now);
            if let Some(status) = initiatives::follow_task(a.status, last, now) {
                if let Err(e) = set_status(app, &root, &page.path, &a.id, status, &note) {
                    crate::applog!("action tasks: {} {}: {e}", page.path, a.id);
                }
            }
        }
    }
    if moved {
        save(&seen);
    }
}

fn set_status(app: &AppHandle, root: &Path, page: &str, id: &str, status: initiatives::Status, note: &str) -> Result<(), String> {
    let before = std::fs::read_to_string(root.join(page)).map_err(|e| e.to_string())?;
    let edit = ActionEdit {
        id: Some(id.to_string()),
        text: None,
        owner: None,
        person: None,
        status: Some(status),
        due: None,
        quotes: vec![],
        task: None,
        on: None,
    };
    let (after, _) = initiatives::with_action(&before, &edit, id)?;
    if after == before {
        return Ok(());
    }
    let task = brainstead_core::filename::stem(note);
    let (title, why) = match status {
        initiatives::Status::Done => ("Marked an action done: its task was ticked", format!("Its task in {task} was ticked.")),
        initiatives::Status::Dropped => ("Dropped an action: its task was cancelled", format!("Its task in {task} was cancelled.")),
        initiatives::Status::Open => ("Opened an action again: its task was reopened", format!("Its task in {task} was opened again.")),
    };
    let origin = Origin { kind: "tasks".into(), label: Some("Its task".into()), ..Default::default() };
    let s = crate::changes::Submit::new(page, Kind::Edit, title, &why, origin, brainstead_core::changes::from_texts(Some(&before), &after));
    crate::changes::submit(app, s).map(|_| ()).map_err(|e| e.message().to_string())
}
