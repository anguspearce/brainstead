// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The menu-bar icon (the tray icon on Windows) and where its menu window opens: under the icon in
//! macOS's menu bar, beside it in the taskbar's notification area elsewhere (`crate::tray` uses
//! these).

use tauri::AppHandle;

/// What the icon shows: its image, whether that is a template image (drawn in the menu bar's own
/// colour) and its tooltip.
pub struct TrayLook {
    pub png: &'static [u8],
    pub template: bool,
    pub tooltip: &'static str,
}

/// What the icon shows in this state: a run going (`busy`), something waiting (`attention`).
pub fn tray_icon(busy: bool, attention: bool) -> TrayLook {
    TrayLook { png: icon_for(busy, attention), template: cfg!(target_os = "macos"), tooltip: tooltip(busy, attention) }
}

/// The icon for the state. macOS draws template images in the menu bar's own colour, so the
/// monochrome icons carry the busy and waiting marks. Template images exist only on macOS: elsewhere
/// a monochrome icon is drawn as it is and disappears on a light taskbar, so the colour app icon is
/// used in every state and the state goes in the tooltip (`tooltip`).
fn icon_for(busy: bool, attention: bool) -> &'static [u8] {
    #[cfg(target_os = "macos")]
    {
        if busy {
            include_bytes!("../../icons/tray-busy@2x.png")
        } else if attention {
            include_bytes!("../../icons/tray-alert@2x.png")
        } else {
            include_bytes!("../../icons/tray@2x.png")
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (busy, attention);
        include_bytes!("../../icons/32x32.png")
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

/// Places the menu window, `width` points wide, by the icon the click gave, and shows it.
pub fn place_tray_window(app: &AppHandle, w: &tauri::WebviewWindow, rect: &tauri::Rect, width: f64) {
    #[cfg(target_os = "macos")]
    place_under_icon(app, w, rect, width);
    #[cfg(not(target_os = "macos"))]
    place_in_work_area(app, w, rect, width);
}

/// The menu window was resized. The page sizes it to its contents after it shows; placed above the
/// icon (Windows and Linux), it is placed again so its bottom edge stays where it was.
pub fn tray_window_resized(w: &tauri::WebviewWindow, size: tauri::PhysicalSize<u32>) {
    #[cfg(not(target_os = "macos"))]
    if let Some(at) = *PLACED.lock().unwrap() {
        let (x, y) = place(&at, (size.width as f64, size.height as f64));
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    }
    #[cfg(target_os = "macos")]
    let _ = (w, size);
}

/// macOS: the menu window under the icon in the menu bar, inside the icon's screen.
#[cfg(target_os = "macos")]
fn place_under_icon(app: &AppHandle, w: &tauri::WebviewWindow, rect: &tauri::Rect, width: f64) {
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
    let mut left = x + iw / 2.0 - width / 2.0;
    if let Some((_, screen_left, screen_right)) = screen {
        left = left.min(screen_right - width - 8.0).max(screen_left + 8.0);
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
fn place_in_work_area(app: &AppHandle, w: &tauri::WebviewWindow, rect: &tauri::Rect, width: f64) {
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
    let size = w.outer_size().map(|s| (s.width as f64, s.height as f64)).unwrap_or((width * at.scale, 480.0 * at.scale));
    let (x, y) = place(&at, size);
    let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    let _ = w.show();
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
