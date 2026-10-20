// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Quick capture: a global shortcut (⌃⌥Space on macOS, Ctrl+Shift+Space on Windows and Linux,
//! unless changed in Settings) shows a small window
//! over whatever app is in front. Saving or Esc hides it and, when Brainstead wasn't in front
//! before, gives the front back to the app that was.

use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// The shortcut until Settings changes it. Not Ctrl+Alt off macOS: many keyboard layouts there
/// type AltGr as Ctrl+Alt, so a Ctrl+Alt shortcut takes a character the user types.
#[cfg(target_os = "macos")]
pub const DEFAULT_SHORTCUT: &str = "Control+Alt+Space";
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_SHORTCUT: &str = "Control+Shift+Space";

#[derive(Default)]
pub struct CaptureState {
    /// The shortcut registered now.
    current: Mutex<Option<Shortcut>>,
    /// Why the chosen shortcut couldn't be registered, for Settings.
    error: Mutex<Option<String>>,
    /// Whether Brainstead was in front when capture opened.
    was_active: Mutex<bool>,
}

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, sc, ev| {
            if ev.state() == ShortcutState::Pressed && crate::lock(&app.state::<CaptureState>().current).as_ref() == Some(sc) {
                show(app);
            }
        })
        .build()
}

/// Registers `spec` ("Control+Alt+Space") in place of the current shortcut. On failure the old
/// one is kept, and the reason is logged and kept for Settings (`capture_shortcut_status`).
pub fn register(app: &AppHandle, spec: &str) {
    let st = app.state::<CaptureState>();
    let gs = app.global_shortcut();
    let spec = if spec.trim().is_empty() { DEFAULT_SHORTCUT } else { spec.trim() };
    let new: Shortcut = match spec.parse() {
        Ok(s) => s,
        Err(e) => {
            let why = format!("“{spec}” isn't a shortcut: {e}");
            crate::applog!("capture shortcut: {why}");
            *crate::lock(&st.error) = Some(why);
            return;
        }
    };
    let mut cur = crate::lock(&st.current);
    if cur.as_ref() == Some(&new) {
        return;
    }
    // The new one first: if it's refused, the old one is still there and still works.
    match gs.register(new) {
        Ok(()) => {
            if let Some(old) = cur.replace(new) {
                let _ = gs.unregister(old);
            }
            *crate::lock(&st.error) = None;
        }
        Err(e) => {
            let why = format!("The system wouldn't give Brainstead {spec}; another app may use it. ({e})");
            crate::applog!("capture shortcut: {why}");
            *crate::lock(&st.error) = Some(why);
        }
    }
}

pub fn show(app: &AppHandle) {
    let Some(w) = app.get_webview_window("capture") else { return };
    let active = app.get_webview_window("main").and_then(|m| m.is_focused().ok()).unwrap_or(false);
    *crate::lock(&app.state::<CaptureState>().was_active) = active;
    let _ = w.center();
    let _ = w.show();
    let _ = w.set_focus();
    let _ = tauri::Emitter::emit_to(app, "capture", "capture-open", ());
}

/// Opens the quick capture window from inside the app (Today's Capture button).
#[tauri::command]
pub fn capture_show(app: AppHandle) {
    show(&app);
}

/// Hides the capture window, and the app too when it wasn't in front before.
#[tauri::command]
pub fn capture_close(app: AppHandle) {
    if let Some(w) = app.get_webview_window("capture") {
        let _ = w.hide();
    }
    if !*crate::lock(&app.state::<CaptureState>().was_active) {
        crate::platform::give_back_front(&app);
    }
}

/// The shortcut in use and, if the chosen one was refused, why.
#[tauri::command]
pub fn capture_shortcut_status(app: AppHandle) -> (Option<String>, Option<String>) {
    let st = app.state::<CaptureState>();
    let cur = crate::lock(&st.current).map(|s| s.into_string());
    let err = crate::lock(&st.error).clone();
    (cur, err)
}

#[cfg(test)]
mod tests {
    /// The default the window shows (src/platform.ts `captureDefault`) is the one registered:
    /// tests/fixtures/keylabels.json holds it per system, for both sides to check against.
    #[test]
    fn the_default_shortcut_is_the_one_the_window_shows() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/keylabels.json")).expect("keylabels.json");
        let system = if cfg!(target_os = "macos") { "macos" } else { "windows" };
        assert_eq!(fixture["captureDefault"][system].as_str(), Some(super::DEFAULT_SHORTCUT));
    }
}
