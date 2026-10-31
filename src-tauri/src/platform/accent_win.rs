// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Windows accent colour (Settings › Personalisation › Colours), for Appearance › Accent
//! colour's System (src/accent.ts): `UISettings::GetColorValue(Accent)`, as Settings and Terminal
//! take it. `ColorValuesChanged` also fires for the light and dark switch, and more than once a
//! change, so the page is told only when the accent itself is different. Also whether Windows
//! hides scroll bars (`auto_hide_scrollbars`, src/scrollbars.ts), which `UISettings` gives too.

use std::sync::Mutex;
use tauri::Emitter;
use windows::core::IInspectable;
use windows::Foundation::TypedEventHandler;
use windows::UI::ViewManagement::{UIColorType, UISettings};

/// The accent as `#rrggbb`; `None` when Windows doesn't say (no WinRT, as on Server Core).
pub fn accent() -> Option<String> {
    hex(&UISettings::new().ok()?)
}

/// Whether Windows hides scroll bars until they're used: false when Settings › Accessibility ›
/// Visual effects › Always show scrollbars is on. `None` when Windows doesn't say (before 1809).
pub fn auto_hide_scrollbars() -> Option<bool> {
    UISettings::new().ok()?.AutoHideScrollBars().ok()
}

fn hex(s: &UISettings) -> Option<String> {
    let c = s.GetColorValue(UIColorType::Accent).ok()?;
    Some(format!("#{:02x}{:02x}{:02x}", c.R, c.G, c.B))
}

/// Emits `system-accent-changed` with the new `#rrggbb` to every window when the accent changes.
/// The `UISettings` is kept for the life of the app: released, it stops raising the event.
pub fn watch(app: tauri::AppHandle) {
    let Ok(s) = UISettings::new() else { return };
    let last = Mutex::new(hex(&s));
    let handler = TypedEventHandler::<UISettings, IInspectable>::new(move |sender, _| {
        let now = sender.as_ref().and_then(hex);
        let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
        if now.is_some() && *last != now {
            *last = now.clone();
            let _ = app.emit("system-accent-changed", now);
        }
        Ok(())
    });
    if s.ColorValuesChanged(&handler).is_ok() {
        std::mem::forget(s);
    }
}
