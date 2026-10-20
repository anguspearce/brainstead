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
//! day's check) or Skip This Version. Release builds only, on a system releases carry a package
//! for (`platform::release_updates`). Installing restarts the app by the
//! same road as ⌘Q (`crate::restart_for_update`), so unsaved edits are asked about first.
//!
//! The offer and what follows are shown in the main window (src/Update.tsx): `update-offer`,
//! then `update-progress` while it downloads, and `update-message` for what it has to say; the
//! window answers with update_install, update_later or update_skip. Without a main window the
//! system's own dialogs ask instead.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
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
    /// Found by a background check, waiting for the window's focus.
    pending: Mutex<Option<Update>>,
    /// On the window, waiting for an answer.
    offered: Mutex<Option<Update>>,
    /// An offer is up or an update is installing: no second offer meanwhile.
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
    app.manage(Updates { pending: Mutex::new(None), offered: Mutex::new(None), asking: AtomicBool::new(false), busy });
    if cfg!(debug_assertions) || !crate::platform::release_updates() {
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

/// What the window's update dialog shows.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    name: String,
    version: String,
    current: String,
    notes: Option<String>,
    /// When the release was published (RFC 3339), if latest.json says.
    date: Option<String>,
}

impl Offer {
    fn of(app: &AppHandle, update: &Update) -> Offer {
        Offer {
            name: app_name(app),
            version: update.version.clone(),
            current: update.current_version.clone(),
            notes: update.body.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string),
            date: update.raw_json.get("pub_date").and_then(serde_json::Value::as_str).map(str::to_string),
        }
    }
}

/// How the download is going; `total` is None when the server doesn't say. `phase` is
/// "downloading", "installing" or "restarting".
#[derive(Clone, Serialize)]
struct Progress {
    phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

#[derive(Clone, Serialize)]
struct Message {
    title: String,
    text: String,
}

/// The main window, shown and focused, to ask in.
fn window(app: &AppHandle) -> Option<WebviewWindow> {
    let w = app.get_webview_window("main")?;
    let _ = w.show();
    let _ = w.set_focus();
    Some(w)
}

async fn offer(app: &AppHandle, update: Update) {
    let Some(state) = app.try_state::<Updates>() else { return };
    if state.asking.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(w) = window(app) else { return offer_natively(app, update).await };
    let offer = Offer::of(app, &update);
    *state.offered.lock().unwrap() = Some(update);
    if w.emit_to("main", "update-offer", offer).is_err() {
        state.offered.lock().unwrap().take();
        state.asking.store(false, Ordering::SeqCst);
    }
}

/// The offer on the window, if one is waiting for an answer: for a window that loaded after it was made.
#[tauri::command]
pub fn update_offered(app: AppHandle) -> Option<Offer> {
    let state = app.try_state::<Updates>()?;
    let offered = state.offered.lock().unwrap();
    offered.as_ref().map(|u| Offer::of(&app, u))
}

/// Install and Restart, from the window's dialog.
#[tauri::command]
pub async fn update_install(app: AppHandle) {
    let Some(state) = app.try_state::<Updates>() else { return };
    let Some(update) = state.offered.lock().unwrap().take() else { return };
    install(&app, update).await;
    state.asking.store(false, Ordering::SeqCst);
}

/// Later (or the dialog closed): asked again at the next day's check.
#[tauri::command]
pub fn update_later(app: AppHandle) {
    let Some(state) = app.try_state::<Updates>() else { return };
    state.offered.lock().unwrap().take();
    state.asking.store(false, Ordering::SeqCst);
}

/// Skip This Version: not offered again by the background checks.
#[tauri::command]
pub fn update_skip(app: AppHandle) {
    let Some(state) = app.try_state::<Updates>() else { return };
    if let Some(update) = state.offered.lock().unwrap().take() {
        let mut saved = load();
        saved.skipped = Some(update.version);
        save(&saved);
    }
    state.asking.store(false, Ordering::SeqCst);
}

/// The offer as a system dialog, when there's no window to show it in. Called with `asking` set.
async fn offer_natively(app: &AppHandle, update: Update) {
    let state = app.state::<Updates>();
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
    state.asking.store(false, Ordering::SeqCst);
}

async fn install(app: &AppHandle, update: Update) {
    let state = app.state::<Updates>();
    if let Some(why) = (state.busy)(app) {
        tell(app, "The update will wait", &format!("{why} {} will offer it again later.", app_name(app))).await;
        return;
    }
    let progress = |phase, downloaded, total| {
        let _ = app.emit_to("main", "update-progress", Progress { phase, downloaded, total });
    };
    progress("downloading", 0, None);
    // A few updates a second are plenty for the bar; the chunks come far faster.
    let (mut got, mut shown) = (0u64, Instant::now());
    let chunk = |n: usize, total: Option<u64>| {
        got += n as u64;
        if shown.elapsed() >= Duration::from_millis(100) {
            shown = Instant::now();
            progress("downloading", got, total);
        }
    };
    match update.download_and_install(chunk, || progress("installing", 0, None)).await {
        Ok(()) => {
            progress("restarting", 0, None);
            crate::restart_for_update(app);
        }
        Err(e) => tell(app, "The update didn't install", &e.to_string()).await,
    }
}

/// Says something about updates: in the window's dialog, or a system one without a window.
async fn tell(app: &AppHandle, title: &str, text: &str) {
    let message = Message { title: title.into(), text: text.into() };
    if window(app).is_some_and(|w| w.emit_to("main", "update-message", message).is_ok()) {
        return;
    }
    let dialog = app.dialog().message(text).title(title).kind(MessageDialogKind::Info);
    let _ = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show()).await;
}
