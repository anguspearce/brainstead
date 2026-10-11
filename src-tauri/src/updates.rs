// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks GitHub for a newer release and offers to install it (tauri-plugin-updater: the
//! endpoint, releases/latest/download/latest.json, and the public key that every package must
//! be signed with are in tauri.conf.json under plugins.updater; tools/update_files.py writes
//! the release's files).
//!
//! When: 20 seconds after launch, then once a day while the app runs (the last check's time is
//! kept, so restarting doesn't check again), and whenever the user picks Check for Updates….
//! An update found in the background waits until the main window is next focused, so the offer
//! never pops up over another app. It offers Install and Restart, Later (asked again at the next
//! day's check) or Skip This Version. Release builds only. Installing restarts the app by the
//! same road as ⌘Q (`crate::restart_for_update`), so unsaved edits are asked about first.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};
use tauri_plugin_updater::{Update, UpdaterExt};

const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const TICK: Duration = Duration::from_secs(60 * 60);

const INSTALL: &str = "Install and Restart";
const LATER: &str = "Later";
const SKIP: &str = "Skip This Version";

/// Why an update can't be installed right now ("A backup is running."), or None when it can.
pub type Busy = fn(&AppHandle) -> Option<String>;

pub struct Updates {
    pending: Mutex<Option<Update>>,
    asking: AtomicBool,
    busy: Busy,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    checked: u64,
    #[serde(default)]
    skipped: Option<String>,
}

/// In the app's data folder, so `BRAINSTEAD_DATA` keeps a dev copy's checks apart.
fn saved_path() -> PathBuf {
    crate::platform::data_dir().join("updates.json")
}

fn load() -> Saved {
    std::fs::read(saved_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(s: &Saved) {
    if let Ok(b) = serde_json::to_vec_pretty(s) {
        let _ = brainstead_core::write::write_atomic(&saved_path(), &b, false);
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn app_name(app: &AppHandle) -> String {
    app.package_info().name.clone()
}

/// Starts the background checks. Call once from setup, in the windowed app only.
pub fn start(app: &AppHandle, busy: Busy) {
    app.manage(Updates { pending: Mutex::new(None), asking: AtomicBool::new(false), busy });
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            if now().saturating_sub(load().checked) >= EVERY.as_secs() {
                tauri::async_runtime::block_on(check(&app, false));
            }
            std::thread::sleep(TICK);
        }
    });
}

/// Check for Updates…: says what it found, even when there's nothing new.
pub fn check_now(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { check(&app, true).await });
}

/// The main window gained focus: offer an update a background check found.
pub fn focused(app: &AppHandle) {
    let Some(state) = app.try_state::<Updates>() else { return };
    let Some(update) = state.pending.lock().unwrap().take() else { return };
    let app = app.clone();
    tauri::async_runtime::spawn(async move { offer(&app, update).await });
}

async fn check(app: &AppHandle, asked: bool) {
    let found = match app.updater() {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    let mut saved = load();
    match found {
        Ok(Some(update)) => {
            saved.checked = now();
            save(&saved);
            if asked {
                offer(app, update).await;
            } else if saved.skipped.as_deref() != Some(update.version.as_str()) {
                let focused = app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
                if focused {
                    offer(app, update).await;
                } else if let Some(state) = app.try_state::<Updates>() {
                    *state.pending.lock().unwrap() = Some(update);
                }
            }
        }
        Ok(None) => {
            saved.checked = now();
            save(&saved);
            if asked {
                let v = app.package_info().version.to_string();
                tell(app, "You're up to date", &format!("{} {v} is the newest version.", app_name(app))).await;
            }
        }
        Err(e) => {
            // A failed background check is tried again within the hour.
            crate::applog!("update check failed: {e}");
            if asked {
                tell(app, "Couldn't check for updates", &e.to_string()).await;
            }
        }
    }
}

async fn offer(app: &AppHandle, update: Update) {
    let Some(state) = app.try_state::<Updates>() else { return };
    if state.asking.swap(true, Ordering::SeqCst) {
        return;
    }
    let name = app_name(app);
    let mut text = format!("{name} {} is available. You have {}.", update.version, update.current_version);
    if let Some(notes) = update.body.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        text.push_str("\n\n");
        text.push_str(notes);
    }
    let dialog = app
        .dialog()
        .message(text)
        .title(format!("A new version of {name} is available"))
        .buttons(MessageDialogButtons::YesNoCancelCustom(INSTALL.into(), LATER.into(), SKIP.into()));
    let choice =
        tauri::async_runtime::spawn_blocking(move || dialog.blocking_show_with_result()).await.unwrap_or(MessageDialogResult::Cancel);
    state.asking.store(false, Ordering::SeqCst);
    // Custom buttons come back as their labels; anything else (Esc, closed) is Later.
    match choice {
        MessageDialogResult::Custom(c) if c == INSTALL => install(app, update).await,
        MessageDialogResult::Custom(c) if c == SKIP => {
            let mut saved = load();
            saved.skipped = Some(update.version.clone());
            save(&saved);
        }
        _ => {}
    }
}

async fn install(app: &AppHandle, update: Update) {
    let state = app.state::<Updates>();
    if let Some(why) = (state.busy)(app) {
        tell(app, "The update will wait", &format!("{why} {} will offer it again later.", app_name(app))).await;
        return;
    }
    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => crate::restart_for_update(app),
        Err(e) => tell(app, "The update didn't install", &e.to_string()).await,
    }
}

async fn tell(app: &AppHandle, title: &str, text: &str) {
    let dialog = app.dialog().message(text).title(title).kind(MessageDialogKind::Info);
    let _ = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show()).await;
}
