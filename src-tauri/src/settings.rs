// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! settings.json in the app's data folder. The window holds the settings and sends the whole set
//! back (debounced) when one changes; this side checks them and writes the file atomically.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// The vault folder; None until one is chosen.
    pub vault_path: Option<String>,
    /// Folder names skipped at any depth.
    pub excluded: Vec<String>,
    /// Never write to the vault. On until stage 3 (capture and tasks) is built and trusted.
    pub read_only: bool,
    /// "system", "light" or "dark".
    pub theme: String,
    /// The user chose to go on without Full Disk Access.
    pub skipped_full_disk_access: bool,
    /// The global quick-capture shortcut, as "Control+Alt+Space".
    pub capture_shortcut: String,
    /// The daily and weekly summaries' schedule (Settings › Jobs & schedule).
    pub summaries: brainstead_core::reviews::ScheduleSettings,
    /// Brainstead runs the daily and weekly summaries (only one app should: the previous app runs them otherwise).
    pub summaries_here: bool,
    /// When the user does the guided weekly review (Settings › Jobs & schedule).
    pub weekly_review: brainstead_core::reviews::WeeklyReview,
    /// Teams and Outlook sync is on (Settings › Teams and Outlook sync; macOS only, D-20261025-01).
    pub sync_enabled: bool,
    /// Channel conversation ids the user chose to sync (off by default; 1:1 and group chats always).
    pub sync_channels: Vec<String>,
    /// Senders or domains whose mail the sync skips ("Don't sync this sender").
    pub sync_ignore_senders: Vec<String>,
    /// How many days of history the first sync loads (a chat the first time it's seen, and the first
    /// mail run), so a big mailbox or long chat history can't flood the Inbox. 0 means no limit.
    pub sync_initial_days: u32,
    /// Anything the window keeps that this side doesn't need to know about (sidebar state and the like).
    #[serde(flatten)]
    pub ui: serde_json::Map<String, serde_json::Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            vault_path: None,
            excluded: Vec::new(),
            read_only: true,
            theme: "system".into(),
            skipped_full_disk_access: false,
            capture_shortcut: crate::capture::DEFAULT_SHORTCUT.into(),
            summaries: Default::default(),
            summaries_here: false,
            weekly_review: Default::default(),
            sync_enabled: false,
            sync_channels: Vec::new(),
            sync_ignore_senders: Vec::new(),
            sync_initial_days: 7,
            ui: Default::default(),
        }
    }
}

/// Folder names the walker already skips, so excluding them means nothing.
const SYSTEM_DIRS: [&str; 6] = ["wiki", "sources", "Templates", "images", ".trash", ".obsidian"];

impl Settings {
    /// Cleans what the window sent: trimmed, unique, valid folder names; a known theme.
    pub fn normalised(mut self) -> Result<Settings, String> {
        // The Keychain switch, gone 2026-10-03 (nothing was ever kept there).
        self.ui.remove("hasSecrets");
        let mut ex: Vec<String> = Vec::new();
        for n in self.excluded.iter().map(|s| s.trim()) {
            if n.is_empty() {
                continue;
            }
            if n.contains(['/', '\\']) || n == "." || n == ".." {
                return Err(format!("“{n}” isn't a folder name. Exclude folders by name, without a path."));
            }
            if SYSTEM_DIRS.contains(&n) {
                return Err(format!("{n}/ is a system folder; Brainstead already treats it specially."));
            }
            if !ex.iter().any(|x| x == n) {
                ex.push(n.to_string());
            }
        }
        self.excluded = ex;
        if !["system", "light", "dark"].contains(&self.theme.as_str()) {
            self.theme = "system".into();
        }
        self.vault_path = self.vault_path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        self.summaries.validate()?;
        self.weekly_review.validate()?;
        Ok(self)
    }

    /// As `normalised`, but what it would refuse is dropped (an excluded name) or put back to its
    /// default (a summary or weekly review time), so the rest survive a bad value.
    pub fn lenient(mut self) -> Settings {
        self.excluded.retain(|n| Settings { excluded: vec![n.clone()], ..Default::default() }.normalised().is_ok());
        if self.summaries.validate().is_err() {
            self.summaries = Default::default();
        }
        if self.weekly_review.validate().is_err() {
            self.weekly_review = Default::default();
        }
        self.normalised().unwrap_or_default()
    }
}

pub fn load(path: &Path) -> Settings {
    load_checked(path).0
}

/// The settings from the file, and what was wrong with it, if anything, for the user. A file that
/// isn't JSON at all is kept beside it (`settings.json.bad`) and the defaults are used; one with a
/// bad value keeps every other setting and puts only that one back to its default.
pub fn load_checked(path: &Path) -> (Settings, Option<String>) {
    let Ok(b) = std::fs::read(path) else { return (Settings::default(), None) };
    let keep_copy = || {
        let _ = std::fs::copy(path, path.with_extension("json.bad"));
    };
    let Ok(s) = serde_json::from_slice::<Settings>(&b) else {
        keep_copy();
        return (
            Settings::default(),
            Some(
                "Brainstead couldn't read its settings, so it started with the defaults. The old file is kept as settings.json.bad.".into(),
            ),
        );
    };
    match s.clone().normalised() {
        Ok(n) => (n, None),
        Err(why) => {
            keep_copy();
            (s.lenient(), Some(format!("One of Brainstead's settings wasn't valid and was put back to its default ({why}). The old file is kept as settings.json.bad.")))
        }
    }
}

/// Written to a temporary file and renamed over the old one, so a crash never leaves half a file.
pub fn save(path: &Path, s: &Settings) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    brainstead_core::write::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_round_trip() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("settings.json");
        let s = load(&p);
        assert!(s.read_only);
        assert_eq!(s.theme, "system");
        let mut s2 = s.clone();
        s2.vault_path = Some("/v".into());
        s2.ui.insert("sidebarSections".into(), serde_json::json!({"library": true}));
        save(&p, &s2).unwrap();
        assert_eq!(load(&p), s2);
    }

    #[test]
    fn checks_excluded_names() {
        let s = Settings { excluded: vec![" old ".into(), "old".into(), "".into()], theme: "neon".into(), ..Default::default() };
        let n = s.normalised().unwrap();
        assert_eq!(n.excluded, ["old"]);
        assert_eq!(n.theme, "system");
        assert!(Settings { excluded: vec!["a/b".into()], ..Default::default() }.normalised().is_err());
        assert!(Settings { excluded: vec!["wiki".into()], ..Default::default() }.normalised().is_err());
    }

    #[test]
    fn a_bad_value_keeps_the_rest() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("settings.json");
        let mut s =
            Settings { vault_path: Some("/v".into()), read_only: false, excluded: vec!["old".into(), "a/b".into()], ..Default::default() };
        s.summaries.daily_time = "25:99".into();
        std::fs::write(&p, serde_json::to_vec(&s).unwrap()).unwrap();
        let (got, warning) = load_checked(&p);
        assert!(warning.is_some());
        assert_eq!((got.vault_path.as_deref(), got.read_only), (Some("/v"), false));
        assert_eq!(got.excluded, ["old"]);
        assert_eq!(got.summaries.daily_time, "06:30");
        assert!(t.path().join("settings.json.bad").exists());
        assert_eq!(load_checked(&t.path().join("none.json")).1, None);
    }

    #[test]
    fn bad_file_is_kept() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("settings.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert_eq!(load(&p), Settings::default());
        assert!(t.path().join("settings.json.bad").exists());
    }
}
