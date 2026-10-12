// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The GTD screens' reads and edits: projects, the Inbox, and a task's contexts, effort and
//! project. Edits are refused while read-only, re-index what they wrote, and can be undone with ⌘Z
//! (`edits::undo`), as the task edits in `edits.rs` are.

use std::path::{Path, PathBuf};

use brainstead_core::inbox::{self, InboxItem};
use brainstead_core::projects::{self, ProjectRow};
use brainstead_core::write::{self, FileChange};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::edits::{edit, task_label, vault_path, writable, writing, written, EditError, Edited, FileUndo, Res, UndoEntry, UndoStack};
use crate::vault::VaultService;
use crate::AppState;

fn root(svc: &VaultService) -> Res<PathBuf> {
    Ok(svc.root().ok_or("No vault is open.".to_string())?)
}

fn invalid(message: String) -> EditError {
    write::WriteError::Invalid(message).into()
}

/// Records whole-file changes for ⌘Z and returns the toast's label.
fn push_files(app: &AppHandle, root: &Path, label: String, changes: &[FileChange]) -> String {
    let files = changes
        .iter()
        .map(|c| FileUndo {
            path: brainstead_core::vault::rel_of(root, &c.path).unwrap_or_default(),
            before: c.before.clone(),
            version: c.version.clone(),
        })
        .collect();
    app.state::<UndoStack>().push(UndoEntry::files(label.clone(), files));
    label
}

fn project_of(svc: &VaultService, rel: &str) -> Res<PathBuf> {
    if !projects::is_project_path(rel) {
        return Err(invalid(format!("Not a project note: {rel}")));
    }
    vault_path(svc, rel)
}

/// Every project note, with its counts.
#[tauri::command]
pub async fn projects_list(app: AppHandle) -> Result<Vec<ProjectRow>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<VaultService>().with_index(|ix, _| ix.projects()))
        .await
        .map_err(|e| e.to_string())?
}

/// Captures from the Outlook and Teams extensions, newest first, then the Scratchpad's thoughts,
/// then the To Do list's `#### Other` tasks.
#[tauri::command]
pub async fn inbox_list(app: AppHandle) -> Res<Vec<InboxItem>> {
    crate::blocking(move || Ok(inbox_items(&app)?)).await
}

/// What `inbox_list` lists, waited on (the weekly review's preparation reads it too).
pub fn inbox_items(app: &AppHandle) -> Result<Vec<InboxItem>, String> {
    let svc = app.state::<VaultService>();
    let root = svc.root().ok_or("No vault is open.")?;
    let mut out = captures(&svc, &root);
    out.extend(inbox::items(&root));
    Ok(out)
}

/// The captures clarified (or ingested) from the Inbox, in the app data folder.
fn captures_done_file() -> PathBuf {
    crate::platform::data_dir().join("inbox-captures.json")
}

fn captures_done() -> Vec<String> {
    std::fs::read(captures_done_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// A capture is in the Inbox while its file is there, no wiki page cites it, and it hasn't been
/// clarified here, however many captures came after it. One clarified, or whose file was deleted,
/// leaves the extensions' Inbox list for good (a file is only counted deleted while `sources/` is
/// there, so a vault that's offline for a moment doesn't empty it).
fn captures(svc: &VaultService, root: &Path) -> Vec<InboxItem> {
    let done = captures_done();
    let reachable = root.join("sources").is_dir();
    let pending = crate::nativehost::inbox(&crate::platform::data_dir(), |c| {
        c.path.as_ref().is_none_or(|p| done.contains(p) || (reachable && !root.join(p).is_file()))
    });
    let mut seen = std::collections::HashSet::new();
    let fresh: Vec<_> = pending
        .into_iter()
        .filter_map(|c| c.path.clone().map(|p| (p, c)))
        .filter(|(p, _)| seen.insert(p.clone()) && root.join(p).is_file())
        .collect();
    let paths: Vec<String> = fresh.iter().map(|(p, _)| p.clone()).collect();
    let ingested = svc.with_index(|ix, _| ix.ingested(&paths).map_err(|e| e.to_string())).unwrap_or_else(|_| vec![false; paths.len()]);
    let stem = |path: &str| path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md").to_string();
    let mut out: Vec<InboxItem> = fresh
        .into_iter()
        .zip(ingested)
        .filter(|(_, ing)| !ing)
        .map(|((path, c), _)| {
            let stamp = chrono::DateTime::parse_from_rfc3339(&c.at)
                .ok()
                .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string());
            InboxItem { kind: "capture", text: stem(&path), path, line: 0, line_text: c.what, stamp, block: None }
        })
        .collect();
    // Every other source no wiki page cites and not yet clarified here: a file dropped or imported
    // into Sources, or put in sources/ from outside the app, waits here too, newest first.
    let others = svc
        .with_index(|ix, _| {
            let files = ix.list(Some("source"))?;
            let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
            Ok(files.into_iter().zip(ix.ingested(&paths)?).collect::<Vec<_>>())
        })
        .unwrap_or_default();
    let mut others: Vec<_> =
        others.into_iter().filter(|(f, ing)| !ing && !done.contains(&f.path) && seen.insert(f.path.clone())).map(|(f, _)| f).collect();
    others.sort_by_key(|f| std::cmp::Reverse(f.mtime));
    out.extend(others.into_iter().map(|f| {
        let stamp =
            chrono::DateTime::from_timestamp_millis(f.mtime).map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string());
        InboxItem { kind: "capture", text: stem(&f.path), line_text: what_file(&f.path).into(), path: f.path, line: 0, stamp, block: None }
    }));
    out
}

/// What a source file is, for its Inbox row: "Word document", "PDF"…
fn what_file(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).as_deref() {
        Some("md") => "Note",
        Some("pdf") => "PDF",
        Some("doc" | "docx") => "Word document",
        Some("ppt" | "pptx") => "PowerPoint",
        Some("xls" | "xlsx" | "csv") => "Spreadsheet",
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "heic") => "Image",
        Some("txt") => "Text",
        Some("vtt") => "Transcript",
        _ => "File",
    }
}

/// A source restored from the Trash is in the Inbox again, whatever was done with it before.
pub fn forget_clarified(path: &str) {
    let mut done = captures_done();
    let n = done.len();
    done.retain(|p| p != path);
    if done.len() != n {
        if let Ok(b) = serde_json::to_vec_pretty(&done) {
            let _ = write::write_atomic(&captures_done_file(), &b, false);
        }
    }
}

/// A capture clarified in the Inbox (made a task, ingested or kept): it leaves the Inbox.
#[tauri::command]
pub async fn inbox_capture_done(path: String) -> Res<()> {
    writing(move || {
        let mut done = captures_done();
        if !done.contains(&path) {
            done.insert(0, path);
            done.truncate(1000);
        }
        let b = serde_json::to_vec_pretty(&done).map_err(|e| EditError::from(e.to_string()))?;
        write::write_atomic(&captures_done_file(), &b, false)?;
        Ok(())
    })
    .await
}

/// Sets a task's `#context/…` tags (none clears them).
#[tauri::command]
pub async fn task_set_contexts(app: AppHandle, path: String, line: usize, line_text: String, contexts: Vec<String>) -> Res<Edited> {
    writing(move || {
        let verb = if contexts.is_empty() { "Cleared the contexts of" } else { "Set the contexts of" };
        edit(&app, &path, line, &line_text, verb, |l| write::with_contexts(l, &contexts))
    })
    .await
}

/// Sets a task's `[effort:: …]` (15m, 1h, 1h30m, 2d), or with None clears it.
#[tauri::command]
pub async fn task_set_effort(app: AppHandle, path: String, line: usize, line_text: String, effort: Option<String>) -> Res<Edited> {
    writing(move || {
        let effort = effort.filter(|e| !e.trim().is_empty());
        let verb = if effort.is_some() { "Set the effort of" } else { "Cleared the effort of" };
        edit(&app, &path, line, &line_text, verb, |l| write::with_effort(l, effort.as_deref()))
    })
    .await
}

/// Links a task to a project note (`[[Project. Name]]`), in place of any project it links now, or
/// with None unlinks it. A task written in a project note belongs to it whatever it links.
#[tauri::command]
pub async fn task_set_project(app: AppHandle, path: String, line: usize, line_text: String, project_path: Option<String>) -> Res<Edited> {
    writing(move || {
        if let Some(p) = &project_path {
            if !projects::is_project_path(p) {
                return Err(invalid(format!("Not a project note: {p}")));
            }
        }
        let svc = app.state::<VaultService>();
        // Which of the line's links go to a project note, by the index (file name, then alias).
        let targets: Vec<String> = brainstead_core::links::parse_links(&brainstead_core::markdown::lines(&line_text, 0))
            .into_iter()
            .map(|l| l.target)
            .filter(|t| !t.is_empty())
            .collect();
        let resolved = svc.with_index(|ix, _| ix.resolve(&targets))?;
        let to_project: Vec<String> = targets
            .into_iter()
            .zip(resolved)
            .filter(|(t, r)| r.as_deref().is_some_and(projects::is_project_path) || projects::is_project_path(&format!("{t}.md")))
            .map(|(t, _)| t)
            .collect();
        let stem = project_path.as_deref().map(|p| brainstead_core::filename::stem(p).to_string());
        let verb = if project_path.is_some() { "Set the project of" } else { "Cleared the project of" };
        edit(&app, &path, line, &line_text, verb, |l| {
            Ok(write::with_project_link(l, stem.as_deref(), |t| to_project.iter().any(|x| x == &brainstead_core::links::nfc(t))))
        })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    path: String,
    undo: String,
}

/// Creates `Project. <name>.md` with its properties and headings. Undo deletes it, if it's
/// unchanged since.
#[tauri::command]
pub async fn project_create(app: AppHandle, name: String, status: String, area: Option<String>, outcome: Option<String>) -> Res<Created> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let svc = app.state::<VaultService>();
        let root = root(&svc)?;
        let (rel, ch) = projects::create_project(&root, &name, &status, area.as_deref(), outcome.as_deref())?;
        let undo = push_files(&app, &root, format!("Created the project “{}”", projects::project_name(&rel)), std::slice::from_ref(&ch));
        written(&app, &[ch.path]);
        Ok(Created { path: rel, undo })
    })
    .await
}

/// Adds `- [ ] text` (`text` without the checkbox) at the end of a project's `## heading`, Next actions when None.
#[tauri::command]
pub async fn project_add_task(app: AppHandle, path: String, text: String, heading: Option<String>) -> Res<Edited> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let svc = app.state::<VaultService>();
        let abs = project_of(&svc, &path)?;
        let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.is_empty() {
            return Err(invalid("Nothing to add.".into()));
        }
        let item = format!("- [ ] {t}");
        // Next actions unless the window names another section (Waiting for, from the Inbox).
        let heading = heading.filter(|h| !h.trim().is_empty()).unwrap_or_else(|| projects::NEXT_ACTIONS.to_string());
        let (i, ch) = write::add_task_under(&abs, &heading, &item)?;
        let label = format!("Added “{}” to {}", task_label(&item), projects::project_name(&path));
        let undo = push_files(&app, &root(&svc)?, label, std::slice::from_ref(&ch));
        written(&app, &[abs]);
        Ok(Edited { line: i, line_text: item, undo: Some(undo) })
    })
    .await
}

/// Adds `- [ ] text` at the top of the To Do list's `#### Other`, as Quick capture does, but
/// undoable with ⌘Z (the weekly review's suggestions).
#[tauri::command]
pub async fn task_add(app: AppHandle, text: String) -> Res<Edited> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let svc = app.state::<VaultService>();
        let root = root(&svc)?;
        let abs = root.join(inbox::TODO_LIST);
        let before = std::fs::read_to_string(&abs).ok();
        let item = write::capture_task(&abs, &text)?;
        let after = std::fs::read_to_string(&abs).unwrap_or_default();
        let ch = FileChange { path: abs.clone(), before, version: write::version(after.as_bytes()) };
        let undo = push_files(&app, &root, format!("Added “{}” to the To Do list", task_label(&item)), std::slice::from_ref(&ch));
        written(&app, &[abs]);
        let line = after.lines().position(|l| l == item).unwrap_or(0);
        Ok(Edited { line, line_text: item, undo: Some(undo) })
    })
    .await
}

/// Changes a project's `status` (active, on-hold, someday, done), `area` or `outcome`; None or
/// empty removes the property. Returns the toast's undo label.
#[tauri::command]
pub async fn project_set(app: AppHandle, path: String, key: String, value: Option<String>) -> Res<String> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let svc = app.state::<VaultService>();
        let abs = project_of(&svc, &path)?;
        let value = value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let value = match (key.as_str(), value) {
            ("status", Some(v)) => {
                Some(projects::parse_status(&v).ok_or_else(|| invalid(format!("Not a project status: {v}")))?.to_string())
            }
            ("area" | "outcome" | "status", v) => v,
            _ => return Err(invalid(format!("Projects here have a status, area and outcome, not “{key}”."))),
        };
        let ch = write::set_property(&abs, &key, value.as_deref())?;
        let name = projects::project_name(&path);
        let label = match (key.as_str(), value.as_deref()) {
            ("status", Some("done")) => format!("Marked “{name}” complete"),
            ("status", Some("on-hold")) => format!("Put “{name}” on hold"),
            ("status", Some("someday")) => format!("Moved “{name}” to someday"),
            ("status", _) => format!("Made “{name}” active"),
            (k, Some(_)) => format!("Set the {k} of “{name}”"),
            (k, None) => format!("Cleared the {k} of “{name}”"),
        };
        let undo = push_files(&app, &root(&svc)?, label, std::slice::from_ref(&ch));
        written(&app, &[abs]);
        Ok(undo)
    })
    .await
}

/// Renames an area: sets `area` on each project note in `paths` (the area's projects, as the
/// screen lists them), all undone together. Returns the toast's undo label.
#[tauri::command]
pub async fn projects_set_area(app: AppHandle, paths: Vec<String>, from: String, to: String) -> Res<String> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let svc = app.state::<VaultService>();
        let to = to.trim().to_string();
        if to.is_empty() {
            return Err(invalid("Give the area a name.".into()));
        }
        if to.contains('\n') {
            return Err(invalid("An area's name is one line.".into()));
        }
        let root = root(&svc)?;
        let abs = paths.iter().map(|p| project_of(&svc, p)).collect::<Res<Vec<_>>>()?;
        let mut changes = Vec::new();
        let mut failed = None;
        for a in &abs {
            match write::set_property(a, "area", Some(&to)) {
                Ok(ch) => changes.push(ch),
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        // What was written stays undoable even when a later note refused.
        let undo = if changes.is_empty() {
            String::new()
        } else {
            push_files(&app, &root, format!("Renamed the area “{from}” to “{to}”"), &changes)
        };
        written(&app, &changes.iter().map(|c| c.path.clone()).collect::<Vec<_>>());
        match failed {
            Some(e) => Err(e.into()),
            None => Ok(undo),
        }
    })
    .await
}

/// Removes a thought from the Scratchpad: `line` is its heading's line and `text` its whole block
/// (`InboxItem.block`), refused as stale if it's changed. Returns the toast's undo label.
#[tauri::command]
pub async fn inbox_remove_thought(app: AppHandle, line: usize, text: String) -> Res<String> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let svc = app.state::<VaultService>();
        let root = root(&svc)?;
        let abs = root.join(inbox::SCRATCHPAD);
        let ch = inbox::remove_block(&abs, line, &text)?;
        let undo = push_files(&app, &root, "Removed a thought from the Scratchpad".into(), std::slice::from_ref(&ch));
        written(&app, &[abs]);
        Ok(undo)
    })
    .await
}

/// Clarifies a To Do list task into a project: takes its line out of the To Do list and adds
/// `new_line` at the end of the project's `## heading` (Next actions, Waiting for), both or neither.
/// Returns where it is in the project.
#[tauri::command]
pub async fn inbox_move_task(
    app: AppHandle,
    line: usize,
    line_text: String,
    to_path: String,
    heading: String,
    new_line: String,
) -> Res<Edited> {
    writing(move || {
        writable(&app.state::<AppState>())?;
        let svc = app.state::<VaultService>();
        let root = root(&svc)?;
        let from = root.join(inbox::TODO_LIST);
        let to = vault_path(&svc, &to_path)?;
        let (i, chs) = write::move_task(&from, line, &line_text, &to, &heading, &new_line)?;
        let label = format!("Moved “{}” to {}", task_label(&new_line), brainstead_core::filename::stem(&to_path));
        let undo = push_files(&app, &root, label, &chs);
        written(&app, &[to, from]);
        Ok(Edited { line: i, line_text: new_line, undo: Some(undo) })
    })
    .await
}
