// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Windows accent colour (Settings › Personalisation › Colours), for Appearance › Accent
//! colour's System (src/accent.ts). It is the fourth entry of the colour set Windows keeps in the
//! registry (`Explorer\Accent\AccentPalette`), which is what `UISettings::GetColorValue(Accent)`
//! reports. The registry is read, with `UISettings` as the fallback, and watched with
//! `RegNotifyChangeKeyValue` as the taskbar's theme is (platform/tray.rs), rather than through
//! `UISettings` and its `ColorValuesChanged`: the copy of the palette `UISettings` holds in a
//! running process is taken when the key's first value changes, which can be before the palette
//! itself is written, and is kept until the key changes again, and the event can fire before the
//! palette is written, or not at all for a change. Read through it, a change showed as the
//! previous colour, or not at all. The main window gaining focus reads the colour again too, for
//! a change that reached the registry while the watch was not armed. The page is told only when
//! the colour differs from what it was last told. Also whether Windows hides scroll bars
//! (`auto_hide_scrollbars`, src/scrollbars.ts), which `UISettings` gives too.

use std::sync::Mutex;
use tauri::Emitter;
use windows::UI::ViewManagement::{UIColorType, UISettings};

/// The registry key of the colour set, under HKEY_CURRENT_USER, and the palette's accent entry.
const ACCENT_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent";
const PALETTE_ACCENT: usize = 3 * 4;

/// How long after a write to the key the colour is read once more: Windows writes the key's
/// values one by one on a change, and the palette may come after the one that woke the watch.
const SETTLE_MS: u32 = 500;

/// The accent as `#rrggbb`: the palette's entry in the registry, else what `UISettings` holds;
/// `None` when Windows doesn't say (no colour set and no WinRT, as on Server Core).
pub fn accent() -> Option<String> {
    palette_accent().or_else(|| hex(&UISettings::new().ok()?))
}

/// The accent entry of the registry's palette (`rr gg bb 00` per entry), read fresh each time.
fn palette_accent() -> Option<String> {
    let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey(ACCENT_KEY).ok()?;
    let value = key.get_raw_value("AccentPalette").ok()?;
    let c = value.bytes.get(PALETTE_ACCENT..PALETTE_ACCENT + 3)?;
    Some(format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
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

/// The accent the windows were last told (`None` before `watch` ran).
static TOLD: Mutex<Option<String>> = Mutex::new(None);

/// Reads the accent now and emits `system-accent-changed` with the new `#rrggbb` to every window
/// when it differs from the one they were last told.
fn refresh(app: &tauri::AppHandle) {
    let Some(now) = accent() else { return };
    let mut told = TOLD.lock().unwrap_or_else(|e| e.into_inner());
    if told.as_deref() == Some(now.as_str()) {
        return;
    }
    *told = Some(now.clone());
    let _ = app.emit("system-accent-changed", now);
}

/// Watches the colour set's key on a thread of its own and tells every window when the accent
/// changed. Why the watch stops, if it does, goes in the log.
pub fn watch(app: tauri::AppHandle) {
    *TOLD.lock().unwrap_or_else(|e| e.into_inner()) = accent();
    std::thread::spawn(move || watch_key(&app));
}

/// The main window gained focus: a change made while the watch wasn't armed is picked up now.
pub fn focused(app: &tauri::AppHandle) {
    refresh(app);
}

/// Waits on `RegNotifyChangeKeyValue` for the key's values, reading the accent after each wake-up
/// and once more `SETTLE_MS` later. The notification is armed before each read, so a write that
/// lands between the read and the wait still wakes it.
fn watch_key(app: &tauri::AppHandle) {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
    let wide: Vec<u16> = ACCENT_KEY.encode_utf16().chain([0]).collect();
    let mut key: HKEY = std::ptr::null_mut();
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, wide.as_ptr(), 0, KEY_NOTIFY, &mut key) };
    if rc != 0 {
        crate::applog!(
            "accent: HKCU\\{ACCENT_KEY} can't be opened to watch (error {rc}); the accent follows Windows only when the window gains focus"
        );
        return;
    }
    // Auto-reset, so one wait consumes one wake-up.
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
    let error = 'watch: {
        if event.is_null() {
            break 'watch format!("no event to watch it with ({})", std::io::Error::last_os_error());
        }
        let mut armed = false;
        let mut wait = INFINITE;
        loop {
            if !armed {
                let rc = unsafe { RegNotifyChangeKeyValue(key, 0, REG_NOTIFY_CHANGE_LAST_SET, event, 1) };
                if rc != 0 {
                    break 'watch format!("it can't be watched (error {rc})");
                }
                armed = true;
            }
            refresh(app);
            match unsafe { WaitForSingleObject(event, wait) } {
                WAIT_OBJECT_0 => {
                    armed = false;
                    wait = SETTLE_MS;
                }
                WAIT_TIMEOUT => wait = INFINITE,
                _ => break 'watch format!("the wait failed ({})", std::io::Error::last_os_error()),
            }
        }
    };
    crate::applog!("accent: HKCU\\{ACCENT_KEY} is no longer watched: {error}; the accent follows Windows only when the window gains focus");
    if !event.is_null() {
        unsafe { CloseHandle(event) };
    }
    unsafe { RegCloseKey(key) };
}
