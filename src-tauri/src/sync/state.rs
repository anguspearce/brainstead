// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the sync has already written, so it writes each thing once (D-20261025-02):
//! which day of each chat became a source, which mail messages were captured, and which meetings'
//! transcripts were fetched. A small JSON file in the app data folder, capped so it can't grow
//! without bound. It records progress only — never message text, and nothing secret.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

const FILE: &str = "sync-state.json";
/// At most this many captured mail ids and transcribed meeting ids are remembered.
const SEEN_CAP: usize = 4000;
/// At most this many written days per chat (old chats drop off). Always more than the page of
/// messages a pass reads: a quiet chat's page can span that many days, and a day dropped while it
/// is still in the page would be written again as a duplicate on every pass.
const DAYS_CAP: usize = super::fetch::CHAT_PAGE + 100;
const _: () = assert!(DAYS_CAP > super::fetch::CHAT_PAGE);

/// Held from [`load`] to [`save`] by whatever changes the state (a sync pass, an on-demand pull), so
/// one can't save its older copy over the other's and have days and threads delivered again.
static CHANGING: Mutex<()> = Mutex::new(());

/// Takes the state for changing: load, change and save while holding what this returns. It waits
/// for whoever holds it now.
pub fn lock() -> MutexGuard<'static, ()> {
    CHANGING.lock().unwrap_or_else(|e| e.into_inner())
}

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
    /// Set once the first sync pass has finished. Mail's first-sync limit goes by
    /// [`State::mail_first_run`]; this is kept for state written before `mail_initialized`.
    pub initialized: bool,
    /// Set once a pass has read the Inbox, so the "load N days on first sync" limit is applied to
    /// mail only that first time, whenever Outlook is switched on (chats apply it per chat, the
    /// first time each is seen).
    #[serde(default)]
    pub mail_initialized: bool,
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
    /// When a chat day's last message came, as of its last write (`"<conv>\n<day>"` → milliseconds
    /// since 1970). A pass can see only the end of a busy day, so the count alone can't say whether
    /// it has grown; the last message can. Absent on older state (then the count decides).
    #[serde(default)]
    pub chat_last: BTreeMap<String, i64>,
    /// Email threads captured, by Outlook ConversationId → the thread source's path.
    #[serde(default)]
    pub thread_paths: BTreeMap<String, String>,
    /// How many messages a thread's source held when last written (ConversationId → count).
    #[serde(default)]
    pub thread_counts: BTreeMap<String, usize>,
    /// When a thread's last message came, as of its last write (ConversationId → milliseconds since
    /// 1970), so a thread that lost one message and gained another still reads as grown.
    #[serde(default)]
    pub thread_last: BTreeMap<String, i64>,
    /// Each thread's ConversationIndex header (hex) → its ConversationId, so a thread carried on
    /// under a new subject can link back to it (D-20261118-06).
    #[serde(default)]
    pub thread_headers: BTreeMap<String, String>,
    /// Email attachments brought into `sources/` ([`attachment_key`] → the file's path), so each is
    /// downloaded once however often its thread is rewritten.
    #[serde(default)]
    pub attachment_paths: BTreeMap<String, String>,
    /// When the mail the user sent was last looked at (RFC 3339), so each pass reads only what's
    /// new since (D-20261120-06). None before the first look.
    #[serde(default)]
    pub sent_checked: Option<String>,
    /// Chats only an on-demand pull has read (conversation ids): no pass has given them their first
    /// sync yet, so the "load N days on first sync" limit is still theirs. A pass that reads one
    /// takes it off. Absent on older state: every chat on file then counts as seen by a pass.
    #[serde(default)]
    pub pulled_chats: std::collections::BTreeSet<String>,
}

/// An attachment's key: its thread, its message's received time and its name. Outlook's message and
/// attachment ids change when a message moves folder, so they aren't used.
pub fn attachment_key(conv_id: &str, received: &str, name: &str) -> String {
    format!("{conv_id}\n{received}\n{name}")
}

fn chat_key(conv: &str, day: &str) -> String {
    format!("{conv}\n{day}")
}

/// A meeting transcript's key: its thread and the local day it started. A recurring meeting's whole
/// series shares one thread id, so the day tells the sittings apart. The sync pass and the on-demand
/// pull both use it, so each sees what the other has already captured.
pub fn transcript_key(thread: &str, start: DateTime<Utc>) -> String {
    format!("{thread}#{}", start.with_timezone(&Local).date_naive())
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
    /// Whether mail's first-sync history limit still applies: no pass has read the Inbox yet. State
    /// from before `mail_initialized` counts as past it when a pass has run and there are threads
    /// on file, so nobody already syncing mail is limited or flooded again.
    pub fn mail_first_run(&self) -> bool {
        !self.mail_initialized && !(self.initialized && !(self.thread_paths.is_empty() && self.mail.is_empty()))
    }

    /// Whether no sync pass has read this chat yet: it isn't on file, or only an on-demand pull put
    /// it there.
    pub fn chat_first_seen(&self, conv: &str) -> bool {
        !self.chats.contains_key(conv) || self.pulled_chats.contains(conv)
    }

    /// Notes who has just read a chat: a pull (`by_pull`) of one no pass had seen (`first_seen`, as
    /// [`State::chat_first_seen`] said before the read) leaves it still to be seen; a pass ends that.
    pub fn chat_read(&mut self, conv: &str, by_pull: bool, first_seen: bool) {
        if !by_pull {
            self.pulled_chats.remove(conv);
        } else if first_seen && self.chats.contains_key(conv) {
            self.pulled_chats.insert(conv.to_string());
        }
    }

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
                // What was kept about a dropped day goes with it, or those maps grow for ever.
                for old in days.drain(0..drop).collect::<Vec<_>>() {
                    let key = chat_key(conv, &old);
                    self.chat_paths.remove(&key);
                    self.chat_counts.remove(&key);
                    self.chat_last.remove(&key);
                }
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

    /// When this chat day's last message came, as of its last write (None on older state).
    pub fn chat_day_last(&self, conv: &str, day: &str) -> Option<i64> {
        self.chat_last.get(&chat_key(conv, day)).copied()
    }

    /// Records when a chat day's last message came (after writing or topping it up).
    pub fn set_chat_day_last(&mut self, conv: &str, day: &str, last: i64) {
        self.chat_last.insert(chat_key(conv, day), last);
    }

    /// The thread source's path for an email conversation, when captured.
    pub fn thread_path(&self, conv_id: &str) -> Option<&str> {
        self.thread_paths.get(conv_id).map(String::as_str)
    }

    /// How many messages the thread source held when last written (0 if never).
    pub fn thread_count(&self, conv_id: &str) -> usize {
        self.thread_counts.get(conv_id).copied().unwrap_or(0)
    }

    /// When the thread's last message came, as of its last write (None on older state).
    pub fn thread_last(&self, conv_id: &str) -> Option<i64> {
        self.thread_last.get(conv_id).copied()
    }

    /// Records when a thread's last message came (after writing or topping it up).
    pub fn set_thread_last(&mut self, conv_id: &str, last: i64) {
        self.thread_last.insert(conv_id.to_string(), last);
    }

    pub fn attachment_path(&self, key: &str) -> Option<&str> {
        self.attachment_paths.get(key).map(String::as_str)
    }

    pub fn mark_attachment(&mut self, key: &str, path: &str) {
        self.attachment_paths.insert(key.to_string(), path.to_string());
    }

    /// Records a thread source's path and message count.
    pub fn mark_thread(&mut self, conv_id: &str, path: &str, count: usize) {
        self.thread_paths.insert(conv_id.to_string(), path.to_string());
        self.thread_counts.insert(conv_id.to_string(), count);
    }

    /// Records a thread's ConversationIndex header.
    pub fn mark_header(&mut self, conv_id: &str, header: &str) {
        self.thread_headers.insert(header.to_string(), conv_id.to_string());
    }

    /// The name of the source of the other thread whose header is `header`, when it's still known.
    pub fn continued(&self, conv_id: &str, header: &str) -> Option<String> {
        let other = self.thread_headers.get(header).filter(|c| *c != conv_id)?;
        let path = self.thread_path(other)?;
        let name = path.rsplit('/').next().unwrap_or(path);
        Some(name.strip_suffix(".md").unwrap_or(name).to_string())
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
    fn a_quiet_chats_whole_page_of_days_stays_remembered() {
        // One message a day fills the page with as many days: none may drop off, or it is written
        // again as a duplicate on the next pass.
        let mut s = State::default();
        let first = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let days: Vec<String> =
            (0..super::super::fetch::CHAT_PAGE as i64).map(|i| (first + chrono::Duration::days(i)).to_string()).collect();
        for d in &days {
            s.mark_chat_day("c1", d, None);
        }
        assert!(days.iter().all(|d| s.chat_day_done("c1", d)));
        // Older days still drop off in the end.
        for i in 0..DAYS_CAP as i64 {
            s.mark_chat_day("c1", &(first + chrono::Duration::days(1000 + i)).to_string(), None);
        }
        assert_eq!(s.chats["c1"].len(), DAYS_CAP);
        assert!(!s.chat_day_done("c1", &days[0]));
    }

    #[test]
    fn a_dropped_days_path_count_and_last_message_go_with_it() {
        let mut s = State::default();
        let first = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let day = |i: i64| (first + chrono::Duration::days(i)).to_string();
        for i in 0..=DAYS_CAP as i64 {
            s.mark_chat_day("c1", &day(i), Some(&format!("sources/Teams. Chat. Maya - {}.md", day(i))));
            s.set_chat_day_count("c1", &day(i), 3);
            s.set_chat_day_last("c1", &day(i), i);
        }
        // Another chat's days are left alone.
        s.mark_chat_day("c2", &day(0), Some("sources/Teams. Chat. Lena - 2026-01-01.md"));
        s.set_chat_day_count("c2", &day(0), 1);
        assert!(!s.chat_day_done("c1", &day(0)), "one over the cap: the oldest day dropped");
        assert_eq!(s.chat_day_path("c1", &day(0)), None);
        assert!(!s.chat_day_counted("c1", &day(0)));
        assert_eq!(s.chat_day_last("c1", &day(0)), None);
        assert_eq!((s.chat_paths.len(), s.chat_counts.len(), s.chat_last.len()), (DAYS_CAP + 1, DAYS_CAP + 1, DAYS_CAP));
        assert!(s.chat_day_path("c1", &day(1)).is_some() && s.chat_day_path("c2", &day(0)).is_some());
    }

    #[test]
    fn a_transcripts_key_is_its_thread_and_local_day() {
        use chrono::TimeZone;
        let at = |d: u32| Local.with_ymd_and_hms(2026, 10, d, 10, 0, 0).unwrap().with_timezone(&Utc);
        assert_eq!(transcript_key("19:meeting_abc@thread.v2", at(24)), "19:meeting_abc@thread.v2#2026-10-24");
        // A weekly meeting's sittings share the thread and differ by day, so last week's transcript
        // isn't taken for this week's.
        let mut s = State::default();
        s.mark_transcript(&transcript_key("t1", at(17)), Some("sources/Teams. Transcript. Steerco - 2026-10-17.md"));
        assert!(s.transcript_done(&transcript_key("t1", at(17))));
        assert!(!s.transcript_done(&transcript_key("t1", at(24))));
    }

    #[test]
    fn a_chat_stays_first_seen_until_a_pass_reads_it() {
        let mut s = State::default();
        assert!(s.chat_first_seen("c1"));
        // A pull that wrote nothing leaves no mark; one that wrote a day does.
        s.chat_read("c1", true, true);
        assert!(s.pulled_chats.is_empty());
        s.mark_chat_day("c1", "2026-10-24", Some("sources/Teams. Chat. Maya - 2026-10-24.md"));
        s.chat_read("c1", true, true);
        assert!(s.chat_first_seen("c1"), "only a pull has read it");
        s.chat_read("c1", true, true);
        assert!(s.chat_first_seen("c1"), "a second pull changes nothing");
        s.chat_read("c1", false, true);
        assert!(!s.chat_first_seen("c1"), "a pass has read it");
        // A pull of a chat the pass already syncs doesn't make it new again.
        s.chat_read("c1", true, false);
        assert!(!s.chat_first_seen("c1"));
        // State from before the mark was kept: every chat on file counts as seen by a pass.
        let old: State = serde_json::from_str(r#"{"chats":{"c2":["2026-10-20"]}}"#).unwrap();
        assert!(!old.chat_first_seen("c2"));
    }

    #[test]
    fn mail_has_its_own_first_run() {
        // Teams alone has been syncing: mail's first run is still to come when Outlook goes on.
        let teams_only = State { initialized: true, ..Default::default() };
        assert!(State::default().mail_first_run());
        assert!(teams_only.mail_first_run());
        assert!(!State { mail_initialized: true, ..Default::default() }.mail_first_run());
        // State from before the flag, with mail already captured, isn't limited again.
        let old: State =
            serde_json::from_str(r#"{"initialized": true, "threadPaths": {"conv1": "sources/Email. Thread. Roadmap - 2026-10-24.md"}}"#)
                .unwrap();
        assert!(!old.mail_initialized && !old.mail_first_run());
    }

    #[test]
    fn changing_the_state_is_one_at_a_time() {
        let held = lock();
        let (tx, rx) = std::sync::mpsc::channel();
        let other = std::thread::spawn(move || {
            let _held = lock();
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(std::time::Duration::from_millis(200)).is_err(), "waits while it's held");
        drop(held);
        rx.recv_timeout(std::time::Duration::from_secs(5)).expect("gets it once it's free");
        other.join().unwrap();
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
