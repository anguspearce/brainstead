// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The menu-bar icon and its menu window (§10 stage 10), as a sibling app has them: clicking the
//! icon opens a small window of its own under it (src/Tray.tsx, the page with `?view=tray`),
//! which closes when it loses focus. The icon is plain, busy while a run is going, or has a dot
//! when something waits; the menu window works that out and says so (`tray_set_state`). With
//! "Only in the menu bar when the window is closed" on, closing the window takes Brainstead out of
//! the Dock until the window opens again.

use std::sync::Mutex;

use serde_json::Value;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::{lock, platform, AppState};

const ID: &str = "main";
/// The menu window's width, in points.
const WIDTH: f64 = 360.0;

/// What the icon shows now, so it's only swapped when it changes.
#[derive(Default)]
pub struct TrayState(Mutex<(bool, bool)>);

/// A setting the window keeps (`ui`), as a switch; `default` when it was never set.
fn setting(app: &AppHandle, key: &str, default: bool) -> bool {
    lock(&app.state::<AppState>().settings).ui.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// Shows the icon or takes it away, as Settings › General › Show in the menu bar says.
pub fn apply(app: &AppHandle) {
    let want = setting(app, "menuBar", true);
    match (app.tray_by_id(ID), want) {
        (Some(_), false) => {
            let _ = app.remove_tray_by_id(ID);
            if let Some(t) = app.get_webview_window("tray") {
                let _ = t.hide();
            }
        }
        (None, true) => {
            if let Err(e) = build(app) {
                crate::applog!("tray: {e}");
            }
        }
        _ => {}
    }
}

fn build(app: &AppHandle) -> tauri::Result<()> {
    let (busy, attention) = *app.state::<TrayState>().0.lock().unwrap();
    let look = platform::tray_icon(busy, attention);
    let icon = tauri::image::Image::from_bytes(look.png)?;
    TrayIconBuilder::with_id(ID)
        .icon(icon)
        .icon_as_template(look.template)
        .tooltip(look.tooltip)
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button_state, rect, button, .. } = ev {
                #[cfg(target_os = "macos")]
                {
                    *CLICKED_AT.lock().unwrap() = Some(std::time::Instant::now());
                }
                if button == MouseButton::Left && button_state == MouseButtonState::Up {
                    toggle(tray.app_handle(), rect);
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// When the icon was last clicked.
#[cfg(target_os = "macos")]
static CLICKED_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Whether macOS's "reopen" (which shows the main window) comes from a click on the icon: with no
/// window showing, a click that closes the menu can send one too, and should only close the menu.
#[cfg(target_os = "macos")]
pub fn reopen_from_icon() -> bool {
    CLICKED_AT.lock().unwrap().is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(1))
}

/// When the menu window last closed itself on losing focus.
static BLURRED_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// A click on the icon this soon after the menu closed on losing focus is the click that took the
/// focus: it closes the menu rather than opening it again.
const CLICK_AFTER_BLUR: std::time::Duration = std::time::Duration::from_millis(400);

/// Where the icon was that the menu last opened under. With two screens each menu bar has the
/// icon, and a click on the other screen's takes the focus too: that one opens the menu there.
static OPENED_UNDER: std::sync::Mutex<Option<(f64, f64)>> = std::sync::Mutex::new(None);

/// The icon's place as the click gives it, to tell one screen's icon from another's.
fn icon_place(rect: &tauri::Rect) -> (f64, f64) {
    let p = rect.position.to_physical::<f64>(1.0);
    (p.x, p.y)
}

/// The menu window's own handling: it closes when it loses focus.
pub fn setup_window(app: &AppHandle) {
    if let Some(t) = app.get_webview_window("tray") {
        let t2 = t.clone();
        t.on_window_event(move |ev| match ev {
            tauri::WindowEvent::Focused(false) => {
                if t2.is_visible().unwrap_or(false) {
                    *BLURRED_AT.lock().unwrap() = Some(std::time::Instant::now());
                }
                let _ = t2.hide();
            }
            // The page sizes the window to its contents after it shows.
            tauri::WindowEvent::Resized(size) => platform::tray_window_resized(&t2, *size),
            _ => {}
        });
    }
}

/// Opens the menu window under the icon, or closes it.
fn toggle(app: &AppHandle, rect: tauri::Rect) {
    let Some(w) = app.get_webview_window("tray") else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    // With the main window closed, clicking the icon takes the focus from the menu first, so it has
    // just hidden itself: this click was meant to close it.
    let same_icon = *OPENED_UNDER.lock().unwrap() == Some(icon_place(&rect));
    if BLURRED_AT.lock().unwrap().take().is_some_and(|t| t.elapsed() < CLICK_AFTER_BLUR) && same_icon {
        return;
    }
    *OPENED_UNDER.lock().unwrap() = Some(icon_place(&rect));
    platform::place_tray_window(app, &w, &rect, WIDTH);
    let _ = w.set_focus();
    let _ = w.emit("tray-opened", ());
}

/// The menu window says what the icon should show.
#[tauri::command]
pub fn tray_set_state(app: AppHandle, busy: bool, attention: bool) {
    let st = app.state::<TrayState>();
    let mut now = st.0.lock().unwrap();
    if *now == (busy, attention) {
        return;
    }
    *now = (busy, attention);
    if let Some(t) = app.tray_by_id(ID) {
        let look = platform::tray_icon(busy, attention);
        if let Ok(img) = tauri::image::Image::from_bytes(look.png) {
            let _ = t.set_icon(Some(img));
            let _ = t.set_icon_as_template(look.template);
        }
        let _ = t.set_tooltip(Some(look.tooltip));
    }
}

/// Brings the main window up (back in the Dock first), on `screen` when one is given.
#[tauri::command]
pub fn main_show(app: AppHandle, screen: Option<String>) {
    show_main(&app);
    if let Some(s) = screen {
        let _ = app.emit_to("main", "navigate", s);
    }
    if let Some(t) = app.get_webview_window("tray") {
        let _ = t.hide();
    }
}

/// Quit from the menu window: the main window comes up to ask about unsaved edits, as ⌘Q does.
#[tauri::command]
pub fn tray_quit(app: AppHandle) {
    if let Some(t) = app.get_webview_window("tray") {
        let _ = t.hide();
    }
    show_main(&app);
    let _ = app.emit_to("main", "quit-requested", ());
}

/// A second launch, handed over by the single-instance plugin (Windows and Linux): shows this
/// copy's window. `Brainstead --quit` started from this copy's own program file (the Windows install
/// script) quits it as closing the window does where that quits: unsaved edits are asked about
/// first, but not "Quit Brainstead?". A `--quit` from another copy, such as a dev build, which shares
/// the app's identifier and so its single-instance lock, is ignored.
#[cfg(any(windows, target_os = "linux"))]
pub fn second_launch(app: &AppHandle, args: &[String]) {
    // The log's proof that a hand-over arrived (tools/single_instance_burst.py counts them).
    crate::applog!("second launch handed over: {}", args.iter().skip(1).cloned().collect::<Vec<_>>().join(" "));
    if !args.iter().any(|a| a == "--quit") {
        show_main(app);
        return;
    }
    let real = |p: &std::path::Path| std::fs::canonicalize(p).ok();
    let ours = std::env::current_exe().ok().and_then(|e| real(&e));
    if ours.is_some() && args.first().and_then(|p| real(std::path::Path::new(p))) == ours {
        show_main(app);
        let _ = app.emit_to("main", "quit-requested", "unasked");
    }
}

pub fn show_main(app: &AppHandle) {
    // Back in the Dock before the window shows, or it can come up behind what had focus.
    platform::set_in_dock(app, true);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        platform::activate();
        let _ = w.set_focus();
    }
}

/// Whether closing the main window quits the app rather than hiding it: on Windows when there is
/// no tray icon to bring the window back from ("Show in the menu bar" off), as Windows apps do.
/// macOS hides it, and the Dock icon brings it back.
pub fn close_quits(app: &AppHandle) -> bool {
    cfg!(windows) && !setting(app, "menuBar", true)
}

/// The main window was closed (hidden): out of the Dock when the menu bar is all that's wanted.
pub fn main_closed(app: &AppHandle) {
    if setting(app, "menuBar", true) && setting(app, "menuBarOnly", false) {
        platform::set_in_dock(app, false);
    }
}
