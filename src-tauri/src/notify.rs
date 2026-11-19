// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Brainstead's notifications, through Settings › General › Notifications: All, Only when it needs
//! me (the default) or Off. A synced chat or thread, an ingest that went fine and a summary written
//! are only news; a failure, a refused capture, a capture the user clicked and a sync sign-in gone
//! stale need the user (D-20261119-01).

use tauri::{AppHandle, Manager};

/// Whether a notification is only news or needs the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Something happened: a synced item in, an ingest done, a summary written.
    News,
    /// Something needs the user: a failure, a refusal, a sign-in to renew, a capture they asked for.
    You,
}

/// The setting's values, as the settings file keeps them ("needs_me", the default, is any other).
pub const ALL: &str = "all";
pub const OFF: &str = "off";

/// Whether a notification of this kind is shown under `mode` (the setting; unset or unknown is the
/// default, Only when it needs me).
pub fn shown(mode: Option<&str>, need: Need) -> bool {
    match mode {
        Some(ALL) => true,
        Some(OFF) => false,
        _ => need == Need::You,
    }
}

/// Shows the notification when the setting lets it through.
pub fn show(app: &AppHandle, need: Need, title: &str, body: &str) {
    let mode = app
        .try_state::<crate::AppState>()
        .and_then(|s| s.settings.lock().ok().and_then(|s| s.ui.get("notifications").and_then(|v| v.as_str()).map(str::to_string)));
    if !shown(mode.as_deref(), need) {
        return;
    }
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_lets_through_what_it_says() {
        for mode in [None, Some("needs_me"), Some("something else")] {
            assert!(shown(mode, Need::You), "{mode:?}");
            assert!(!shown(mode, Need::News), "{mode:?}");
        }
        assert!(shown(Some(ALL), Need::News) && shown(Some(ALL), Need::You));
        assert!(!shown(Some(OFF), Need::News) && !shown(Some(OFF), Need::You));
    }
}
