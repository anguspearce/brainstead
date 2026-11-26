// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What each sync pass did, for Activity › Sync & ingest, the menu-bar window and `sync_activity`
//! (D-20261119-01): the sources it wrote new and topped up, its problems, or why it couldn't run.
//! A pass that did nothing only moves "last synced"; one that hit the same wall as the pass before
//! (signed out, read-only) adds to that entry's count rather than a line every few minutes. Two
//! weeks are kept, in `sync-log.json` in the app's data folder.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::run::Report;

const FILE: &str = "sync-log.json";
/// How long passes are kept.
const DAYS: i64 = 14;
/// And at most this many, whatever the days.
const KEEP: usize = 2000;

/// One pass, or a run of the same blocked or failing pass.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pass {
    /// When it ran (local, `YYYY-MM-DDTHH:MM:SS`); the first of a run of repeats.
    pub at: String,
    /// A back-fill (Settings › Sync › Fetch again): what it wrote waits in the Inbox.
    #[serde(default)]
    pub backfill: bool,
    /// Sources written new (each lands in the Inbox and, with Ingest on arrival, is ingested).
    #[serde(default)]
    pub added: Vec<String>,
    /// Sources that grew in place (a chat day or thread with more messages).
    #[serde(default)]
    pub topped_up: Vec<String>,
    #[serde(default)]
    pub problems: Vec<String>,
    /// Why it couldn't run at all.
    #[serde(default)]
    pub blocked: Option<String>,
    /// How many passes after the first went the same way, and when the last of them ran.
    #[serde(default)]
    pub repeats: u32,
    #[serde(default)]
    pub last: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
    /// When the last pass ran, whatever it did.
    pub last_pass: Option<String>,
    /// Newest first.
    pub passes: Vec<Pass>,
}

pub fn load(data: &Path) -> Log {
    std::fs::read(data.join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Adds a pass that ran at `at` to the log.
pub fn record(data: &Path, r: &Report, at: &str, backfill: bool) {
    let mut log = load(data);
    add(&mut log, r, at, backfill);
    if let Ok(b) = serde_json::to_vec(&log) {
        let _ = brainstead_core::write::write_atomic(&data.join(FILE), &b, false);
    }
}

/// `record`'s change to the log, apart from the file.
fn add(log: &mut Log, r: &Report, at: &str, backfill: bool) {
    log.last_pass = Some(at.to_string());
    let topped: Vec<String> = dedup(r.topped_up.iter());
    let added: Vec<String> = dedup(r.paths.iter().filter(|p| !r.topped_up.contains(p)));
    let wrote = !added.is_empty() || !topped.is_empty();
    if !wrote && r.problems.is_empty() && r.blocked.is_none() {
        return;
    }
    let pass = Pass {
        at: at.into(),
        backfill,
        added,
        topped_up: topped,
        problems: r.problems.clone(),
        blocked: r.blocked.clone(),
        ..Default::default()
    };
    match log.passes.first_mut() {
        // The same wall as last time: counted, not repeated.
        Some(p) if !wrote && p.added.is_empty() && p.topped_up.is_empty() && p.problems == pass.problems && p.blocked == pass.blocked => {
            p.repeats += 1;
            p.last = Some(at.into());
        }
        _ => log.passes.insert(0, pass),
    }
    let since = (chrono::Local::now() - chrono::Duration::days(DAYS)).format("%Y-%m-%dT%H:%M:%S").to_string();
    log.passes.retain(|p| p.last.as_deref().unwrap_or(&p.at) >= since.as_str());
    log.passes.truncate(KEEP);
}

fn dedup<'a>(it: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for p in it {
        if !v.contains(p) {
            v.push(p.clone());
        }
    }
    v
}

/// The sync's passes, for Activity › Sync & ingest and the menu-bar window.
#[tauri::command]
pub fn sync_log() -> Log {
    load(&crate::platform::data_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now(min: i64) -> String {
        (chrono::Local::now() - chrono::Duration::minutes(min)).format("%Y-%m-%dT%H:%M:%S").to_string()
    }

    #[test]
    fn a_pass_is_logged_by_what_it_did_and_the_same_wall_is_counted() {
        let mut log = Log::default();
        // Nothing done: only "last synced" moves.
        add(&mut log, &Report::default(), &now(30), false);
        assert!(log.passes.is_empty() && log.last_pass.is_some());
        // New and topped-up sources, told apart.
        let r = Report {
            paths: vec!["sources/Teams. Chat. Maya - 2026-11-19.md".into(), "sources/Email. Thread. Launch - 2026-11-19.md".into()],
            topped_up: vec!["sources/Email. Thread. Launch - 2026-11-19.md".into()],
            ..Default::default()
        };
        add(&mut log, &r, &now(20), false);
        assert_eq!(log.passes[0].added, ["sources/Teams. Chat. Maya - 2026-11-19.md"]);
        assert_eq!(log.passes[0].topped_up, ["sources/Email. Thread. Launch - 2026-11-19.md"]);
        // Signed out three times running: one entry, counted.
        let out = Report { blocked: Some("Not signed in.".into()), ..Default::default() };
        for m in [15, 10, 5] {
            add(&mut log, &out, &now(m), false);
        }
        assert_eq!(log.passes.len(), 2);
        assert_eq!((log.passes[0].repeats, log.passes[0].last.as_deref()), (2, Some(now(5).as_str())));
        // Something else again starts a new entry.
        add(&mut log, &r, &now(1), false);
        assert_eq!(log.passes.len(), 3);
    }

    #[test]
    fn old_passes_go() {
        let mut log =
            Log { last_pass: None, passes: vec![Pass { at: now(60 * 24 * 15), blocked: Some("x".into()), ..Default::default() }] };
        add(&mut log, &Report { problems: vec!["y".into()], ..Default::default() }, &now(0), false);
        assert_eq!(log.passes.len(), 1);
        assert_eq!(log.passes[0].problems, ["y"]);
    }
}
