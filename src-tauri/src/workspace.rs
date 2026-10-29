// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Workspace's folders (roots and their git repos, pinned, recent) and past Claude Code
//! sessions, the Claude Code settings file its sessions start with, and the hook those settings
//! run (`Brainstead --hook`, before any window, as `--mcp`). The sessions themselves are in
//! terminal.rs; the rules (the guard, the folder list, the events) in brainstead_core::workspace.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;

use brainstead_core::projects::{self, FolderClaim, ProjectLink};
use brainstead_core::reviews::sessions::{self, FolderSession};
use brainstead_core::workspace::{self as ws, Folder};
use tauri::{AppHandle, Manager};

use crate::settings::Settings;
use crate::terminal::{self, folder_path};
use crate::vault::VaultService;
use crate::AppState;

/// How many folders the recent list keeps.
const RECENT_MAX: usize = 20;
/// The most past sessions listed for a folder.
const PAST_MAX: usize = 50;
/// How long `git status` may take for one repo before it's left unknown.
const GIT_TIMEOUT: Duration = Duration::from_secs(2);

/// Changes the settings from this side, saved as the window's are; a screenshot scene keeps them
/// in memory only.
fn update_settings(app: &AppHandle, f: impl FnOnce(&mut Settings)) -> Result<(), String> {
    let st = app.state::<AppState>();
    let mut cur = crate::lock(&st.settings);
    let mut next = cur.clone();
    f(&mut next);
    if crate::scene().is_none() {
        crate::settings::save(&st.settings_path, &next)?;
    }
    *cur = next;
    Ok(())
}

/// The Workspace's lists are kept here, not by the window, which sends its whole set of settings
/// back whenever one changes: what it sends for these is replaced with what's here, so a list
/// changed while the window held an older copy isn't put back.
pub fn keep_lists(cur: &Settings, next: &mut Settings) {
    next.workspace_roots = cur.workspace_roots.clone();
    next.workspace_pinned = cur.workspace_pinned.clone();
    next.workspace_recent = cur.workspace_recent.clone();
}

/// Writes `<data>/workspace/claude-settings.json` for the vault open now, when it has changed.
/// With no vault, only the hooks (which then allow everything).
pub fn write_claude_settings(app: &AppHandle) {
    write_settings_for(app.state::<VaultService>().root().as_deref());
}

pub fn write_settings_for(vault: Option<&Path>) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut v = ws::claude_settings(vault.unwrap_or(Path::new("/")), &exe);
    if vault.is_none() {
        if let Some(o) = v.as_object_mut() {
            o.remove("permissions");
        }
    }
    let text = serde_json::to_string_pretty(&v).unwrap_or_default();
    let file = terminal::settings_file();
    if std::fs::read_to_string(&file).ok().as_deref() == Some(text.as_str()) {
        return;
    }
    let r = std::fs::create_dir_all(terminal::dir()).and_then(|_| {
        brainstead_core::write::write_atomic(&file, text.as_bytes(), false).map_err(|e| std::io::Error::other(e.to_string()))
    });
    if let Err(e) = r {
        crate::applog!("workspace: couldn't write {}: {e}", file.display());
    }
}

/// The project notes' `folders`, from the index when it's open, else read from the files.
fn claims(app: &AppHandle) -> Vec<FolderClaim> {
    let svc = app.state::<VaultService>();
    let Some(root) = svc.root() else { return Vec::new() };
    svc.with_index(|ix, _| ix.folder_claims()).unwrap_or_else(|_| projects::folder_claims(&root))
}

/// The project a folder belongs to, as the MCP server's `project_for_folder` finds it.
pub fn project_for(app: &AppHandle, folder: &Path) -> Option<ProjectLink> {
    projects::pick_project(&claims(app), folder, dirs::home_dir().as_deref())
}

/// When a session last started in each recent folder, beside the settings' list.
fn recent_file() -> PathBuf {
    terminal::dir().join("recent.json")
}

fn recent_times() -> HashMap<String, i64> {
    std::fs::read(recent_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// A session started in `folder`: first in the recent list, which keeps twenty.
pub fn note_recent(app: &AppHandle, folder: &Path) {
    let path = folder.to_string_lossy().into_owned();
    let mut kept = Vec::new();
    let r = update_settings(app, |s| {
        s.workspace_recent = ws::bump(&s.workspace_recent, &path, RECENT_MAX);
        kept = s.workspace_recent.clone();
    });
    if let Err(e) = r {
        crate::applog!("workspace: couldn't save the recent folders: {e}");
    }
    let mut times = recent_times();
    times.insert(path, chrono::Utc::now().timestamp_millis());
    times.retain(|p, _| kept.contains(p));
    if crate::scene().is_none() {
        let _ = std::fs::create_dir_all(terminal::dir());
        let _ = brainstead_core::write::write_atomic(&recent_file(), serde_json::to_string(&times).unwrap_or_default().as_bytes(), false);
    }
}

/// Whether a repo has uncommitted changes, by `git status`; None when git isn't there or takes too
/// long. Only the first byte of its answer is needed.
fn dirty(repo: &Path) -> Option<bool> {
    let mut child = crate::platform::command("git")
        .args(["status", "--porcelain"])
        .current_dir(repo)
        // Never takes the index lock, so it can't get in the way of the user's own git.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut b = [0u8; 1];
        let _ = tx.send(out.read(&mut b).ok().map(|n| n > 0));
    });
    let got = rx.recv_timeout(GIT_TIMEOUT).ok().flatten();
    let _ = child.kill();
    let ok = child.wait().is_ok_and(|s| s.success() || got == Some(true));
    got.filter(|_| ok)
}

#[tauri::command]
pub async fn workspace_folders(app: AppHandle) -> Result<Vec<Folder>, String> {
    crate::blocking(move || {
        let s = crate::lock(&app.state::<AppState>().settings).clone();
        let times = recent_times();
        let recent: Vec<(String, i64)> = s.workspace_recent.iter().map(|p| (p.clone(), times.get(p).copied().unwrap_or(0))).collect();
        let vault = app.state::<VaultService>().root();
        let mut out = ws::discover_with(&s.workspace_roots, &s.workspace_pinned, &recent, vault.as_deref(), &claims(&app));
        // Eight repos' git status at a time.
        for chunk in out.chunks_mut(8) {
            std::thread::scope(|sc| {
                for f in chunk.iter_mut().filter(|f| f.kind == "repo") {
                    sc.spawn(move || f.dirty = dirty(Path::new(&f.path)));
                }
            });
        }
        Ok(out)
    })
    .await
}

/// A folder the user chose to add, by its full path.
fn chosen(path: &str) -> Result<PathBuf, String> {
    let p = folder_path(path);
    if !p.is_absolute() {
        return Err(format!("Choose “{path}” by its full path."));
    }
    if !p.is_dir() {
        return Err(format!("{} isn't there, or isn't a folder.", p.display()));
    }
    Ok(p)
}

#[tauri::command]
pub fn workspace_add_root(app: AppHandle, path: String) -> Result<(), String> {
    let p = chosen(&path)?;
    if app.state::<VaultService>().root().is_some_and(|v| ws::in_vault(&v, &p)) {
        return Err(ws::IN_VAULT.into());
    }
    let p = p.to_string_lossy().into_owned();
    update_settings(&app, |s| {
        if !s.workspace_roots.contains(&p) {
            s.workspace_roots.push(p);
        }
    })
}

#[tauri::command]
pub fn workspace_remove_root(app: AppHandle, path: String) -> Result<(), String> {
    let p = folder_path(&path).to_string_lossy().into_owned();
    update_settings(&app, |s| s.workspace_roots.retain(|r| folder_path(r).to_string_lossy() != p))
}

#[tauri::command]
pub fn workspace_pin(app: AppHandle, path: String, pinned: bool) -> Result<(), String> {
    let p = if pinned { chosen(&path)? } else { folder_path(&path) };
    let p = p.to_string_lossy().into_owned();
    update_settings(&app, |s| {
        s.workspace_pinned.retain(|x| folder_path(x).to_string_lossy() != p);
        if pinned {
            s.workspace_pinned.push(p);
        }
    })
}

/// Claude Code's sessions in a folder, to resume: newest first, fifty at most.
#[tauri::command]
pub async fn workspace_past_sessions(folder: String) -> Result<Vec<FolderSession>, String> {
    crate::blocking(move || {
        let projects = dirs::home_dir().unwrap_or_default().join(".claude").join("projects");
        Ok(sessions::sessions_for_folder(&projects, &folder_path(&folder).to_string_lossy(), PAST_MAX))
    })
    .await
}

// --- the hook ------------------------------------------------------------------------------

/// `Brainstead --hook`: Claude Code's hook for a guarded session. Reads its input, adds a line
/// to the events file and answers it (a refusal, or the project's context). Always exits 0 and
/// prints nothing on failure: a hook must never block the user's session.
pub fn hook_main() -> i32 {
    let data = crate::platform::data_dir();
    let r = std::panic::catch_unwind(|| hook(&data));
    let err = match r {
        Ok(Ok(())) => return 0,
        Ok(Err(e)) => e,
        Err(_) => "the hook stopped unexpectedly".into(),
    };
    log(&data, &format!("workspace hook: {err}"));
    0
}

fn hook(data: &Path) -> Result<(), String> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).map_err(|e| format!("couldn't read its input: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&input).map_err(|e| format!("its input isn't JSON: {e}"))?;
    let term = std::env::var("BRAINSTEAD_TERM").ok();
    let ev = ws::event_from_input(&v, term, chrono::Utc::now().timestamp_millis());
    let appended = append_event(data, &ev);
    let vault = ws::vault_from_settings(data);
    if vault.is_none() && ev.event == "PreToolUse" {
        no_vault(data);
    }
    if vault.is_some() {
        let _ = std::fs::remove_file(no_vault_marker(data));
    }
    if let Some(out) = ws::hook_reply(vault.as_deref(), &v) {
        let mut o = std::io::stdout().lock();
        let _ = writeln!(o, "{out}");
        let _ = o.flush();
    }
    appended
}

/// The marker that the hook has logged finding no vault, so it says so once rather than at every
/// tool call; taken away when there's a vault again.
fn no_vault_marker(data: &Path) -> PathBuf {
    data.join("workspace").join("hook-no-vault")
}

/// A PreToolUse with no vault in the settings is allowed (a hook never blocks a session), and the
/// log says so the first time. Returns whether it logged.
fn no_vault(data: &Path) -> bool {
    let marker = no_vault_marker(data);
    if marker.exists() {
        return false;
    }
    let _ = std::fs::create_dir_all(data.join("workspace")).and_then(|_| std::fs::write(&marker, b""));
    log(data, "workspace hook: no vault in the settings, so tool calls aren't guarded until one is chosen");
    true
}

fn append_event(data: &Path, ev: &ws::HookEvent) -> Result<(), String> {
    let dir = data.join("workspace");
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    let mut line = serde_json::to_string(ev).map_err(|e| e.to_string())?;
    line.push('\n');
    // One write in append mode, so lines from sessions at once don't interleave.
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("events.jsonl"))
        .and_then(|mut f| f.write_all(line.as_bytes()))
        .map_err(|e| format!("couldn't add to the events file: {e}"))
}

/// A line in the app's log from the hook's own process, which has no log of its own open.
fn log(data: &Path, line: &str) {
    let dir = crate::applog::dir(data);
    let now = chrono::Local::now();
    let _ = std::fs::create_dir_all(&dir).and_then(|_| {
        let mut f =
            std::fs::OpenOptions::new().create(true).append(true).open(dir.join(format!("brainstead-{}.log", now.format("%Y-%m-%d"))))?;
        writeln!(f, "{} {line}", now.format("%H:%M:%S"))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_cant_put_back_the_lists() {
        let cur = Settings { workspace_roots: vec!["/r".into()], workspace_recent: vec!["/r/orbit-app".into()], ..Default::default() };
        let mut next = Settings { workspace_shell: Some("/bin/zsh".into()), ..Default::default() };
        keep_lists(&cur, &mut next);
        assert_eq!((next.workspace_roots, next.workspace_recent), (vec!["/r".to_string()], vec!["/r/orbit-app".to_string()]));
        assert_eq!(next.workspace_shell.as_deref(), Some("/bin/zsh"));
    }

    #[test]
    fn the_hook_writes_an_event_line() {
        let t = tempfile::tempdir().unwrap();
        let ev = ws::HookEvent { t: 5, event: "Stop".into(), session_id: Some("s1".into()), ..Default::default() };
        append_event(t.path(), &ev).unwrap();
        append_event(t.path(), &ev).unwrap();
        let text = std::fs::read_to_string(t.path().join("workspace/events.jsonl")).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert_eq!(ws::parse_event(text.lines().next().unwrap()), Some(ev));
        log(t.path(), "workspace hook: test");
        assert!(std::fs::read_dir(crate::applog::dir(t.path())).unwrap().count() == 1);
    }

    #[test]
    fn no_vault_is_logged_once_and_blocks_nothing() {
        let t = tempfile::tempdir().unwrap();
        assert!(no_vault(t.path()));
        assert!(!no_vault(t.path()));
        let logs = std::fs::read_dir(crate::applog::dir(t.path()))
            .unwrap()
            .flatten()
            .map(|e| std::fs::read_to_string(e.path()).unwrap())
            .collect::<String>();
        assert_eq!(logs.matches("no vault").count(), 1, "{logs}");
        // Without a vault the answer to a PreToolUse is nothing: allowed.
        let input = serde_json::json!({ "hook_event_name": "PreToolUse", "cwd": "/r", "tool_name": "Bash", "tool_input": { "command": "rm -rf /r/x" } });
        assert!(ws::hook_reply(ws::vault_from_settings(t.path()).as_deref(), &input).is_none());
    }

    #[test]
    fn a_repo_with_changes_is_dirty() {
        let t = tempfile::tempdir().unwrap();
        let ok =
            |args: &[&str]| std::process::Command::new("git").args(args).current_dir(t.path()).output().is_ok_and(|o| o.status.success());
        if !ok(&["init", "-q"]) {
            return;
        }
        assert_eq!(dirty(t.path()), Some(false));
        std::fs::write(t.path().join("a.txt"), "x").unwrap();
        assert_eq!(dirty(t.path()), Some(true));
    }
}
