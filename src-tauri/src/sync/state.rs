// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the Teams and Outlook sync has already written, so it writes each thing once (D-20261025-02):
//! which day of each chat became a source, which mail messages were captured, and which meetings'
//! transcripts were fetched. A small JSON file in the app data folder, capped so it can't grow
//! without bound. It records progress only — never message text, and nothing secret.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "sync-state.json";
/// At most this many captured mail ids and transcribed meeting ids are remembered.
const SEEN_CAP: usize = 4000;
/// At most this many written days per chat (old chats drop off).
const DAYS_CAP: usize = 180;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    /// Conversation id → the local days (`YYYY-MM-DD`) already written as a source.
    pub chats: BTreeMap<String, Vec<String>>,
    /// Ids of mail messages already captured, newest last.
    pub mail: Vec<String>,
    /// Per-occurrence keys (thread id + the occurrence's local day) whose transcript was already
    /// captured; a recurring meeting's series shares one thread id, so the day disambiguates sittings.
    pub transcripts: Vec<String>,
    /// Set once the first sync pass has finished, so the "load N days on first sync" limit is applied
    /// only that first time for mail (chats apply it per chat, the first time each is seen).
    pub initialized: bool,
    /// The vault path written for a chat day (`"<conv>\n<day>"` → `sources/…`), for the on-demand
    /// pull to vend what's already there. Absent on older state (then the pull finds the file by name).
    #[serde(default)]
    pub chat_paths: BTreeMap<String, String>,
    /// The vault path written for a mail message (id → `sources/…`).
    #[serde(default)]
    pub mail_paths: BTreeMap<String, String>,
    /// The vault path written for a meeting's transcript (per-occurrence key → `sources/…`).
    #[serde(default)]
    pub transcript_paths: BTreeMap<String, String>,
    /// How many messages a chat day's source held when last written (`"<conv>\n<day>"` → count), so a
    /// day that gains messages is topped up in place rather than frozen.
    #[serde(default)]
    pub chat_counts: BTreeMap<String, usize>,
    /// Email threads captured, by Outlook ConversationId → the thread source's path.
    #[serde(default)]
    pub thread_paths: BTreeMap<String, String>,
    /// How many messages a thread's source held when last written (ConversationId → count).
    #[serde(default)]
    pub thread_counts: BTreeMap<String, usize>,
}

fn chat_key(conv: &str, day: &str) -> String {
    format!("{conv}\n{day}")
}

pub fn path(data: &Path) -> PathBuf {
    data.join(FILE)
}

pub fn load(data: &Path) -> State {
    std::fs::read(path(data)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(data: &Path, s: &State) -> Result<(), String> {
    let b = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    brainstead_core::write::write_atomic(&path(data), &b, false).map_err(|e| e.to_string())
}

impl State {
    /// Whether this chat's day has already been written.
    pub fn chat_day_done(&self, conv: &str, day: &str) -> bool {
        self.chats.get(conv).is_some_and(|days| days.iter().any(|d| d == day))
    }

    /// Marks this chat's day written, keeping the most recent [`DAYS_CAP`]. `path` is the source file
    /// it became (None when the day was skipped by the first-sync cap and no file was written).
    pub fn mark_chat_day(&mut self, conv: &str, day: &str, path: Option<&str>) {
        let days = self.chats.entry(conv.to_string()).or_default();
        if !days.iter().any(|d| d == day) {
            days.push(day.to_string());
            days.sort();
            if days.len() > DAYS_CAP {
                let drop = days.len() - DAYS_CAP;
                days.drain(0..drop);
            }
        }
        if let Some(p) = path {
            self.chat_paths.insert(chat_key(conv, day), p.to_string());
        }
    }

    /// The source file a chat day became, when it's known.
    pub fn chat_day_path(&self, conv: &str, day: &str) -> Option<&str> {
        self.chat_paths.get(&chat_key(conv, day)).map(String::as_str)
    }

    /// How many messages this chat day's source held when last written (0 if never).
    pub fn chat_day_count(&self, conv: &str, day: &str) -> usize {
        self.chat_counts.get(&chat_key(conv, day)).copied().unwrap_or(0)
    }

    /// Whether a count has ever been recorded for this chat day (false for days written before
    /// top-up tracking existed, so they aren't needlessly rewritten once on upgrade).
    pub fn chat_day_counted(&self, conv: &str, day: &str) -> bool {
        self.chat_counts.contains_key(&chat_key(conv, day))
    }

    /// Records the message count for a chat day's source (after writing or topping it up).
    pub fn set_chat_day_count(&mut self, conv: &str, day: &str, count: usize) {
        self.chat_counts.insert(chat_key(conv, day), count);
    }

    /// The thread source's path for an email conversation, when captured.
    pub fn thread_path(&self, conv_id: &str) -> Option<&str> {
        self.thread_paths.get(conv_id).map(String::as_str)
    }

    /// How many messages the thread source held when last written (0 if never).
    pub fn thread_count(&self, conv_id: &str) -> usize {
        self.thread_counts.get(conv_id).copied().unwrap_or(0)
    }

    /// Records a thread source's path and message count.
    pub fn mark_thread(&mut self, conv_id: &str, path: &str, count: usize) {
        self.thread_paths.insert(conv_id.to_string(), path.to_string());
        self.thread_counts.insert(conv_id.to_string(), count);
    }

    pub fn transcript_done(&self, meeting: &str) -> bool {
        self.transcripts.iter().any(|m| m == meeting)
    }

    pub fn mark_transcript(&mut self, meeting: &str, path: Option<&str>) {
        if !self.transcript_done(meeting) {
            self.transcripts.push(meeting.to_string());
            cap(&mut self.transcripts);
        }
        if let Some(p) = path {
            self.transcript_paths.insert(meeting.to_string(), p.to_string());
        }
    }

    pub fn transcript_path(&self, meeting: &str) -> Option<&str> {
        self.transcript_paths.get(meeting).map(String::as_str)
    }
}

fn cap(v: &mut Vec<String>) {
    if v.len() > SEEN_CAP {
        let drop = v.len() - SEEN_CAP;
        v.drain(0..drop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_and_caps() {
        let mut s = State::default();
        s.mark_chat_day("c1", "2026-10-24", Some("sources/Teams. Chat. X - 2026-10-24.md"));
        s.mark_chat_day("c1", "2026-10-24", None); // again: no duplicate, keeps the path
        assert!(s.chat_day_done("c1", "2026-10-24"));
        assert!(!s.chat_day_done("c1", "2026-10-25"));
        assert_eq!(s.chats["c1"].len(), 1);
        assert_eq!(s.chat_day_path("c1", "2026-10-24"), Some("sources/Teams. Chat. X - 2026-10-24.md"));
        assert!(!s.chat_day_counted("c1", "2026-10-24"), "no count recorded yet");
        s.set_chat_day_count("c1", "2026-10-24", 5);
        assert!(s.chat_day_counted("c1", "2026-10-24"));
        assert_eq!(s.chat_day_count("c1", "2026-10-24"), 5);

        // Threads: recorded with their path and message count, re-recorded as they grow.
        s.mark_thread("conv1", "sources/Email. Thread. Roadmap - 2026-10-24.md", 2);
        assert_eq!(s.thread_path("conv1"), Some("sources/Email. Thread. Roadmap - 2026-10-24.md"));
        assert_eq!(s.thread_count("conv1"), 2);
        s.mark_thread("conv1", "sources/Email. Thread. Roadmap - 2026-10-24.md", 3);
        assert_eq!(s.thread_count("conv1"), 3);

        for i in 0..(SEEN_CAP + 50) {
            s.mark_transcript(&format!("m{i}"), None);
        }
        assert_eq!(s.transcripts.len(), SEEN_CAP);
        assert!(s.transcript_done(&format!("m{}", SEEN_CAP + 49)), "newest kept");
        assert!(!s.transcript_done("m0"), "oldest dropped");
    }

    #[test]
    fn survives_a_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let mut s = State::default();
        s.mark_transcript("meeting-1", Some("sources/Teams. Transcript. Steerco - 2026-10-24.md"));
        save(d.path(), &s).unwrap();
        assert_eq!(load(d.path()), s);
    }
}
