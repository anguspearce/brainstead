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
    /// Meeting thread ids whose transcript was already captured.
    pub transcripts: Vec<String>,
    /// Set once the first sync pass has finished, so the "load N days on first sync" limit is applied
    /// only that first time for mail (chats apply it per chat, the first time each is seen).
    pub initialized: bool,
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

    /// Marks this chat's day written, keeping the most recent [`DAYS_CAP`].
    pub fn mark_chat_day(&mut self, conv: &str, day: &str) {
        let days = self.chats.entry(conv.to_string()).or_default();
        if !days.iter().any(|d| d == day) {
            days.push(day.to_string());
            days.sort();
            if days.len() > DAYS_CAP {
                let drop = days.len() - DAYS_CAP;
                days.drain(0..drop);
            }
        }
    }

    pub fn mail_done(&self, id: &str) -> bool {
        self.mail.iter().any(|m| m == id)
    }

    pub fn mark_mail(&mut self, id: &str) {
        if !self.mail_done(id) {
            self.mail.push(id.to_string());
            cap(&mut self.mail);
        }
    }

    pub fn transcript_done(&self, meeting: &str) -> bool {
        self.transcripts.iter().any(|m| m == meeting)
    }

    pub fn mark_transcript(&mut self, meeting: &str) {
        if !self.transcript_done(meeting) {
            self.transcripts.push(meeting.to_string());
            cap(&mut self.transcripts);
        }
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
        s.mark_chat_day("c1", "2026-10-24");
        s.mark_chat_day("c1", "2026-10-24"); // again: no duplicate
        assert!(s.chat_day_done("c1", "2026-10-24"));
        assert!(!s.chat_day_done("c1", "2026-10-25"));
        assert_eq!(s.chats["c1"].len(), 1);

        for i in 0..(SEEN_CAP + 50) {
            s.mark_mail(&format!("m{i}"));
        }
        assert_eq!(s.mail.len(), SEEN_CAP);
        assert!(s.mail_done(&format!("m{}", SEEN_CAP + 49)), "newest kept");
        assert!(!s.mail_done("m0"), "oldest dropped");
    }

    #[test]
    fn survives_a_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let mut s = State::default();
        s.mark_transcript("meeting-1");
        save(d.path(), &s).unwrap();
        assert_eq!(load(d.path()), s);
    }
}
