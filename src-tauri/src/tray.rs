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

/// The icon for the state. macOS draws template images in the menu bar's own colour, so the
/// monochrome icons carry the busy and waiting marks. Template images exist only on macOS: elsewhere
/// a monochrome icon is drawn as it is and disappears on a light taskbar, so the colour app icon is
/// used in every state and the state goes in the tooltip (`tooltip`).
fn icon_for(busy: bool, attention: bool) -> &'static [u8] {
    #[cfg(target_os = "macos")]
    {
        if busy {
            include_bytes!("../icons/tray-busy@2x.png")
        } else if attention {
            include_bytes!("../icons/tray-alert@2x.png")
        } else {
            include_bytes!("../icons/tray@2x.png")
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (busy, attention);
        include_bytes!("../icons/32x32.png")
    }
}

/// The icon's tooltip. On macOS the icon itself shows the state; elsewhere the icon is the same in
/// every state, so the tooltip says it.
fn tooltip(busy: bool, attention: bool) -> &'static str {
    if cfg!(target_os = "macos") {
        "Brainstead"
    } else if busy {
        "Brainstead: a run is going"
    } else if attention {
        "Brainstead: something is waiting"
    } else {
        "Brainstead"
    }
}

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
    let icon = tauri::image::Image::from_bytes(icon_for(busy, attention))?;
    TrayIconBuilder::with_id(ID)
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(tooltip(busy, attention))
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
            // The page sizes the window to its contents after it shows: placed above the icon, it
            // is placed again so its bottom edge stays where it was.
            #[cfg(not(target_os = "macos"))]
            tauri::WindowEvent::Resized(size) => {
                if let Some(at) = *PLACED.lock().unwrap() {
                    let (x, y) = place(&at, (size.width as f64, size.height as f64));
                    let _ = t2.set_position(tauri::PhysicalPosition::new(x, y));
                }
            }
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
    #[cfg(target_os = "macos")]
    place_under_icon(app, &w, &rect);
    #[cfg(not(target_os = "macos"))]
    place_in_work_area(app, &w, &rect);
    let _ = w.set_focus();
    let _ = w.emit("tray-opened", ());
}

/// macOS: the menu window under the icon in the menu bar, inside the icon's screen.
#[cfg(target_os = "macos")]
fn place_under_icon(app: &AppHandle, w: &tauri::WebviewWindow, rect: &tauri::Rect) {
    // Placed in points: on macOS they're one space across screens, where physical pixels aren't
    // (each screen's are its points times its own scale, so with a Retina screen beside another,
    // a position in pixels lands on the wrong screen or in the wrong place). The click gives the
    // icon in the pixels of the screen it's on; the screen it's on is the one where, in that
    // screen's points, it falls inside it.
    let monitors = app.available_monitors().unwrap_or_default();
    let icon = rect.position.to_physical::<f64>(1.0);
    let icon_size = rect.size.to_physical::<f64>(1.0);
    // The icon can be reported a few points above its screen's top edge (seen: 5 on a screen
    // placed higher than the built-in one), so it counts as on the screen when it's within an
    // icon's height of its top; across, it must be inside.
    let in_points = |m: &tauri::Monitor| {
        let k = m.scale_factor();
        let (p, sz) = (m.position(), m.size());
        let (left, top) = (p.x as f64 / k, p.y as f64 / k);
        let (x, y, slack) = (icon.x / k, icon.y / k, icon_size.height / k);
        let inside = x >= left && x < left + sz.width as f64 / k && y >= top - slack && y < top + sz.height as f64 / k;
        inside.then_some((k, left, left + sz.width as f64 / k))
    };
    let screen = monitors.iter().find_map(in_points);
    let k = screen.map_or_else(|| w.scale_factor().unwrap_or(2.0), |s| s.0);
    let (x, y, iw, ih) = (icon.x / k, icon.y / k, icon_size.width / k, icon_size.height / k);
    let mut left = x + iw / 2.0 - WIDTH / 2.0;
    if let Some((_, screen_left, screen_right)) = screen {
        left = left.min(screen_right - WIDTH - 8.0).max(screen_left + 8.0);
    }
    let place = tauri::LogicalPosition::new(left, y + ih + 6.0);
    let _ = w.set_position(place);
    let _ = w.show();
    // Once on that screen, place it again, in case macOS moved it while showing it.
    let _ = w.set_position(place);
}

/// Where the menu window was last placed from, in physical pixels, so a resize places it again.
#[cfg(not(target_os = "macos"))]
static PLACED: std::sync::Mutex<Option<Placing>> = std::sync::Mutex::new(None);

/// What the menu window's place is worked out from, all in physical pixels: on Windows one
/// coordinate space spans every monitor, each monitor at its own scale.
#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placing {
    /// The icon's left, top, width and height.
    icon: (f64, f64, f64, f64),
    /// The icon's monitor's work area (the screen less the taskbar): left, top, width, height.
    work: (f64, f64, f64, f64),
    /// The middle of the icon's monitor, top to bottom.
    middle: f64,
    /// The monitor's scale, for the gap to the icon and the margin inside the work area.
    scale: f64,
}

/// The menu window's top-left for a window of `size`: above the icon when the icon is in the lower
/// half of its monitor (a taskbar at the bottom), below it otherwise (a taskbar at the top),
/// centred on the icon across, and kept 8 points inside the work area.
#[cfg(not(target_os = "macos"))]
fn place(at: &Placing, size: (f64, f64)) -> (f64, f64) {
    let (ix, iy, iw, ih) = at.icon;
    let (wx, wy, ww, wh) = at.work;
    let (w, h) = size;
    let margin = 8.0 * at.scale;
    let gap = 6.0 * at.scale;
    let clamp = |v: f64, lo: f64, hi: f64| v.min(hi).max(lo);
    let x = clamp(ix + iw / 2.0 - w / 2.0, wx + margin, wx + ww - margin - w);
    let above = iy + ih / 2.0 >= at.middle;
    let y = if above { iy - gap - h } else { iy + ih + gap };
    (x, clamp(y, wy + margin, wy + wh - margin - h))
}

/// Windows and Linux: the menu window next to the icon, inside the work area of the icon's
/// monitor (`place`).
#[cfg(not(target_os = "macos"))]
fn place_in_work_area(app: &AppHandle, w: &tauri::WebviewWindow, rect: &tauri::Rect) {
    let p = rect.position.to_physical::<f64>(1.0);
    let sz = rect.size.to_physical::<f64>(1.0);
    let (cx, cy) = (p.x + sz.width / 2.0, p.y + sz.height / 2.0);
    let monitors = app.available_monitors().unwrap_or_default();
    let on = |m: &&tauri::Monitor| {
        let (mp, ms) = (m.position(), m.size());
        cx >= mp.x as f64 && cx < mp.x as f64 + ms.width as f64 && cy >= mp.y as f64 && cy < mp.y as f64 + ms.height as f64
    };
    let Some(m) = monitors.iter().find(on).or(monitors.first()) else {
        let _ = w.show();
        return;
    };
    let wa = m.work_area();
    let at = Placing {
        icon: (p.x, p.y, sz.width, sz.height),
        work: (wa.position.x as f64, wa.position.y as f64, wa.size.width as f64, wa.size.height as f64),
        middle: m.position().y as f64 + m.size().height as f64 / 2.0,
        scale: m.scale_factor(),
    };
    *PLACED.lock().unwrap() = Some(at);
    let size = w.outer_size().map(|s| (s.width as f64, s.height as f64)).unwrap_or((WIDTH * at.scale, 480.0 * at.scale));
    let (x, y) = place(&at, size);
    let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    let _ = w.show();
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
        if let Ok(img) = tauri::image::Image::from_bytes(icon_for(busy, attention)) {
            let _ = t.set_icon(Some(img));
            let _ = t.set_icon_as_template(cfg!(target_os = "macos"));
        }
        let _ = t.set_tooltip(Some(tooltip(busy, attention)));
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

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    /// A 1920x1080 monitor at 1.5x with a 72-pixel taskbar, at the bottom or the top.
    fn at(taskbar_top: bool, icon_x: f64) -> Placing {
        let (icon_y, work_y) = if taskbar_top { (20.0, 72.0) } else { (1040.0, 0.0) };
        Placing { icon: (icon_x, icon_y, 36.0, 36.0), work: (0.0, work_y, 1920.0, 1008.0), middle: 540.0, scale: 1.5 }
    }

    #[test]
    fn the_menu_window_sits_above_a_bottom_taskbar_and_below_a_top_one() {
        let size = (540.0, 600.0);
        // Bottom taskbar: above the icon, its bottom inside the work area, centred on the icon.
        let (x, y) = place(&at(false, 1000.0), size);
        assert_eq!(x, 1000.0 + 18.0 - 270.0);
        assert_eq!(y, 1008.0 - 12.0 - 600.0);
        // Top taskbar: below it.
        let (_, y) = place(&at(true, 1000.0), size);
        assert_eq!(y, 72.0 + 12.0);
        // An icon at the right edge: kept 8 points inside.
        let (x, _) = place(&at(false, 1900.0), size);
        assert_eq!(x, 1920.0 - 12.0 - 540.0);
    }
}
