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
    /// The daily and weekly summaries' schedule (Settings › Automations).
    pub summaries: brainstead_core::reviews::ScheduleSettings,
    /// Brainstead runs the daily and weekly summaries (only one app should: the previous app runs them otherwise).
    pub summaries_here: bool,
    /// When the user does the guided weekly review (Settings › Automations).
    pub weekly_review: brainstead_core::reviews::WeeklyReview,
    /// Teams and Outlook sync is on (Settings › Sync; macOS only, D-20261025-01).
    pub sync_enabled: bool,
    /// Channel conversation ids the user chose to sync (off by default; 1:1 and group chats always).
    pub sync_channels: Vec<String>,
    /// Senders or domains whose mail the sync skips ("Don't sync this sender").
    pub sync_ignore_senders: Vec<String>,
    /// 1:1 and group chats the sync skips, by conversation id (a skip suggested from what the user
    /// deletes, `sync::skips`).
    #[serde(default)]
    pub sync_ignore_chats: Vec<String>,
    /// Slack sync is on (D-20261118-01): DMs and group DMs, the channels in `slack_channels`, and
    /// threads that mention the user, while a Slack tab hands its sign-in over.
    #[serde(default)]
    pub sync_slack: bool,
    /// Outlook mail sync, apart from Teams (it needs its own Outlook on the web tab). None follows
    /// `sync_enabled`, as before the two were separate (D-20261118-18).
    #[serde(default)]
    pub sync_outlook: Option<bool>,
    /// Slack channel ids to sync (off unless picked).
    #[serde(default)]
    pub slack_channels: Vec<String>,
    /// How many days of history the first sync loads (a chat the first time it's seen, and the first
    /// mail run), so a big mailbox or long chat history can't flood the Inbox. 0 means no limit.
    pub sync_initial_days: u32,
    /// Bring email attachments into `sources/` with their thread (`sync::attachments`). On by default.
    pub sync_mail_attachments: bool,
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
            sync_ignore_chats: Vec::new(),
            sync_slack: false,
            sync_outlook: None,
            slack_channels: Vec::new(),
            sync_initial_days: 7,
            sync_mail_attachments: true,
            ui: Default::default(),
        }
    }
}

/// Folder names the walker already skips, so excluding them means nothing.
const SYSTEM_DIRS: [&str; 6] = ["wiki", "sources", "Templates", "images", ".trash", ".obsidian"];

impl Settings {
    /// Whether Outlook mail syncs: its own switch, or before there was one, the Teams switch's.
    pub fn mail_on(&self) -> bool {
        self.sync_outlook.unwrap_or(self.sync_enabled)
    }

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

/// The file's bytes; None when there's no file (a first start). Any other error (the file held for
/// a moment by a sync client or a scanner) is tried again a few times before it's given up on.
fn read_file(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let mut tries = 0;
    loop {
        match std::fs::read(path) {
            Ok(b) => return Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) if tries == 3 => return Err(e),
            Err(_) => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

/// The settings in use are the defaults only because the file was there at start and couldn't be
/// read. Nothing is saved over it until it can be (`reread`).
static UNREAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// At start: as [`load_checked`], and a file that's there but can't be read is remembered, so
/// `settings_write` doesn't replace the real settings with the defaults the window was given.
pub fn load_at_start(path: &Path) -> (Settings, Option<String>) {
    let got = load_file(path);
    UNREAD.store(got.is_err(), std::sync::atomic::Ordering::SeqCst);
    got.unwrap_or_else(|why| (Settings::default(), Some(why)))
}

/// Whether the file couldn't be read at start and hasn't been since.
pub fn unread() -> bool {
    UNREAD.load(std::sync::atomic::Ordering::SeqCst)
}

/// The file's settings, now it can be read, to put in use in place of the defaults; while it still
/// can't be, the reason, for the user.
pub fn reread(path: &Path) -> Result<Settings, String> {
    let (s, _) = load_file(path)?;
    UNREAD.store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(s)
}

/// The settings from the file, and what was wrong with it, if anything, for the user. A file that
/// isn't JSON at all is kept beside it (`settings.json.bad`) and the defaults are used; one with a
/// bad value keeps every other setting and puts only that one back to its default. One that can't
/// be read gives the defaults too.
pub fn load_checked(path: &Path) -> (Settings, Option<String>) {
    load_file(path).unwrap_or_else(|why| (Settings::default(), Some(why)))
}

/// [`load_checked`], with a file that's there but can't be read as an error.
fn load_file(path: &Path) -> Result<(Settings, Option<String>), String> {
    let b = match read_file(path) {
        Ok(Some(b)) => b,
        Ok(None) => return Ok((Settings::default(), None)),
        Err(e) => {
            return Err(format!(
                "Brainstead couldn't read its settings file ({e}), so it's using the defaults. Settings aren't saved until it can read the file, so nothing in it is lost."
            ))
        }
    };
    Ok(parsed(path, &b))
}

/// The settings in the file's bytes, a bad file or value handled as [`load_checked`] says.
fn parsed(path: &Path, b: &[u8]) -> (Settings, Option<String>) {
    let keep_copy = || {
        let _ = std::fs::copy(path, path.with_extension("json.bad"));
    };
    let Ok(s) = serde_json::from_slice::<Settings>(b) else {
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
    fn a_file_that_cant_be_read_isnt_saved_over_with_defaults() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("settings.json");
        // A folder in its place: there, but not readable as a file.
        std::fs::create_dir(&p).unwrap();
        let (got, warning) = load_at_start(&p);
        assert_eq!(got, Settings::default());
        assert!(warning.is_some());
        assert!(unread());
        assert!(reread(&p).is_err());
        assert!(unread());
        // Readable again: the real settings come back, and saving is allowed.
        std::fs::remove_dir(&p).unwrap();
        let real = Settings { vault_path: Some("/v".into()), read_only: false, ..Default::default() };
        save(&p, &real).unwrap();
        assert_eq!(reread(&p).unwrap(), real);
        assert!(!unread());
        // No file at all is a first start, not an unread one.
        assert_eq!(load_at_start(&t.path().join("none.json")), (Settings::default(), None));
        assert!(!unread());
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
