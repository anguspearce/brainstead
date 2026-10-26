// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! One sync pass and the timer that drives it. A pass reads the user's chats, Inbox mail and meeting
//! transcripts and writes what's new into `sources/` through the shared capture path
//! (`nativehost::deliver`), so each lands in the Inbox and is ingested like any capture. Grouping and
//! what-counts-as-new follow D-20261025-02. Everything fails soft: one call failing is reported and
//! the pass carries on, so a changed Microsoft endpoint never breaks capture.

use chrono::{Duration, Local, Timelike, Utc};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use super::auth::SignIn;
use super::fetch::{self, ConvKind, Event};
use super::{auth, state};
use crate::{nativehost, platform, settings};

/// How often the sync runs.
const EVERY_MIN: u64 = 15;
/// How far back to look for meetings whose transcript might now be ready.
const TRANSCRIPT_LOOKBACK_HOURS: i64 = 48;
/// How many recent Inbox messages to consider each pass.
const MAIL_TOP: usize = 40;

/// A pass only runs one at a time (the timer and a manual "Sync now" could overlap).
static RUNNING: AtomicBool = AtomicBool::new(false);

/// What a pass did, for Settings and for "Sync now".
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub chats: usize,
    pub mail: usize,
    pub transcripts: usize,
    /// Human-readable problems (an endpoint that failed, a refusal); the pass still finished.
    pub problems: Vec<String>,
    /// Set when the pass couldn't start at all (not signed in, read-only, off).
    pub blocked: Option<String>,
}

/// Runs one sync pass. Safe to call from the timer or a command; overlapping calls return a
/// `blocked` report rather than running twice.
pub fn run_once(data: &Path) -> Report {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Report { blocked: Some("A sync is already running.".into()), ..Default::default() };
    }
    let report = run_inner(data);
    RUNNING.store(false, Ordering::SeqCst);
    report
}

fn run_inner(data: &Path) -> Report {
    let mut r = Report::default();
    let s = settings::load(&data.join("settings.json"));
    let vault = s.vault_path.as_deref().filter(|p| !p.trim().is_empty());
    if !s.sync_enabled {
        r.blocked = Some("Teams and Outlook sync is off.".into());
        return r;
    }
    let Some(vault) = vault else {
        r.blocked = Some("No vault chosen.".into());
        return r;
    };
    if s.read_only {
        r.blocked = Some("The vault is read-only.".into());
        return r;
    }
    let Some(sign_in) = auth::load() else {
        r.blocked = Some("Not signed in. Open Teams in your browser so the extension hands the sign-in over.".into());
        return r;
    };

    let vault = Path::new(vault);
    let mut st = state::load(data);
    // The first-sync history limit (Settings): a day/message older than this is skipped so a big
    // mailbox or long chat history can't flood the Inbox. 0 means no limit.
    let p = Pass { client: super::client(), sign_in, data, vault, s: &s, cutoff: cutoff_day(s.sync_initial_days) };

    sync_chats(&p, &mut st, &mut r);
    sync_mail(&p, &mut st, &mut r);
    sync_transcripts(&p, &mut st, &mut r);

    st.initialized = true;
    if let Err(e) = state::save(data, &st) {
        r.problems.push(format!("Couldn't save sync progress: {e}"));
    }
    r
}

/// Everything one sync pass's steps share: the HTTP client, the sign-in, where to write, the
/// settings and the first-sync cutoff.
struct Pass<'a> {
    client: reqwest::blocking::Client,
    sign_in: SignIn,
    data: &'a Path,
    vault: &'a Path,
    s: &'a settings::Settings,
    cutoff: Option<String>,
}

/// The earliest local day the first sync keeps (`YYYY-MM-DD`), from the "load N days on first sync"
/// setting, or None when it's 0 (no limit).
fn cutoff_day(initial_days: u32) -> Option<String> {
    (initial_days > 0).then(|| (Local::now().date_naive() - Duration::days(i64::from(initial_days) - 1)).format("%Y-%m-%d").to_string())
}

/// Writes a capture and returns its vault path, or None (recording the problem) on refusal. `when` is
/// the item's own time, so the Inbox dates it by when it happened rather than when it was captured.
fn deliver(
    data: &Path,
    vault: &Path,
    c: &brainstead_core::webcapture::Capture,
    ext: &str,
    when: Option<chrono::DateTime<Utc>>,
    r: &mut Report,
) -> Option<String> {
    match nativehost::deliver(data, Some(vault), false, ext, c, true, when.map(|t| t.to_rfc3339())) {
        Ok(path) => Some(path),
        Err(e) => {
            r.problems.push(e);
            None
        }
    }
}

/// Tops up an existing source in place (no capture event, so no immediate re-ingest): the file grows
/// and becomes "changed since cited", which the daily check re-ingests. Returns whether it was written.
fn rewrite(vault: &Path, path: &str, c: &brainstead_core::webcapture::Capture, r: &mut Report) -> bool {
    match brainstead_core::webcapture::rewrite(vault, path, c, Local::now()) {
        Ok(()) => true,
        Err(e) => {
            r.problems.push(e.to_string());
            false
        }
    }
}

fn sync_chats(p: &Pass, st: &mut state::State, r: &mut Report) {
    let convs = match fetch::conversations(&p.client, &p.sign_in) {
        Ok(c) => c,
        Err(e) => {
            r.problems.push(format!("Couldn't list chats: {e}"));
            return;
        }
    };
    let me = fetch::my_name(&p.sign_in);
    let cutoff = p.cutoff.as_deref();
    for conv in convs {
        // 1:1 and group chats always; channels only when opted in; meeting chats are left to the
        // transcript flow; anything else is skipped.
        let take = match conv.kind {
            ConvKind::OneOnOne | ConvKind::Group => true,
            ConvKind::Channel => p.s.sync_channels.iter().any(|c| c == &conv.id),
            ConvKind::Meeting | ConvKind::Other => false,
        };
        if !take {
            continue;
        }
        let msgs = match fetch::chat_messages(&p.client, &p.sign_in, &conv.id) {
            Ok(m) => m,
            Err(e) => {
                r.problems.push(format!("Couldn't read a chat: {e}"));
                continue;
            }
        };
        // The chat's partner across the whole thread, so a day where only the user wrote still names
        // the other person (a 1:1 with no topic).
        let partner = fetch::chat_display_name(&msgs, &me);
        // The first time a chat is seen, skip days older than the first-sync cutoff (and mark them
        // done, so they're never pulled later) — that's where the flood of old history would come
        // from. A chat already being synced only gets genuinely new days anyway.
        let first_seen = !st.chats.contains_key(&conv.id);
        for (day, day_msgs) in fetch::by_local_day(&msgs) {
            let count = day_msgs.len();
            let when = day_msgs.last().map(|m| m.time);
            if st.chat_day_done(&conv.id, &day) {
                // Already written: top it up in place if it has gained messages (D-20261025-07). Days
                // written before top-up tracking adopt their current count once, without a rewrite.
                if !st.chat_day_counted(&conv.id, &day) {
                    st.set_chat_day_count(&conv.id, &day, count);
                } else if count > st.chat_day_count(&conv.id, &day) {
                    // Top up the day's source in place. One that's gone (trashed, the Trash emptied,
                    // deleted elsewhere) starts a fresh source for the day, so the new messages reach
                    // the Inbox rather than being lost (D-20261026-05).
                    if let Some(path) = st.chat_day_path(&conv.id, &day).map(str::to_string) {
                        if let Some(cap) = fetch::chat_capture(&conv, &day_msgs, &me, &p.sign_in.origin, &day, &partner) {
                            let written = if p.vault.join(&path).is_file() {
                                rewrite(p.vault, &path, &cap, r).then_some(path)
                            } else {
                                deliver(p.data, p.vault, &cap, "teams", when, r)
                            };
                            if let Some(path) = written {
                                st.mark_chat_day(&conv.id, &day, Some(&path));
                                st.set_chat_day_count(&conv.id, &day, count);
                                r.chats += 1;
                            }
                        }
                    }
                }
                continue;
            }
            // First time for this day: on a chat's first sync, skip (and mark) days older than the cap.
            if first_seen {
                if let Some(cutoff) = cutoff {
                    if day.as_str() < cutoff {
                        st.mark_chat_day(&conv.id, &day, None);
                        continue;
                    }
                }
            }
            if let Some(cap) = fetch::chat_capture(&conv, &day_msgs, &me, &p.sign_in.origin, &day, &partner) {
                if let Some(path) = deliver(p.data, p.vault, &cap, "teams", when, r) {
                    st.mark_chat_day(&conv.id, &day, Some(&path));
                    st.set_chat_day_count(&conv.id, &day, count);
                    r.chats += 1;
                }
            }
        }
    }
}

fn sync_mail(p: &Pass, st: &mut state::State, r: &mut Report) {
    let mail = match fetch::inbox(&p.client, MAIL_TOP) {
        Ok(m) => m,
        Err(e) => {
            r.problems.push(format!("Couldn't read the Inbox: {e}"));
            return;
        }
    };
    let first_run = !st.initialized;
    let cutoff = p.cutoff.as_deref();
    // The conversations with a fresh Inbox message this pass (deduped, newest first already). Mail is
    // grouped one source per thread (D-20261025-07): an Inbox message flags its thread for syncing.
    let mut seen = std::collections::HashSet::new();
    for m in mail {
        if m.conversation_id.is_empty() || !seen.insert(m.conversation_id.clone()) {
            continue;
        }
        // Skip (for good this pass) a thread whose latest inbound message is from an ignored sender,
        // or, on the first run, older than the cap.
        if ignored(&m.from_address, &p.s.sync_ignore_senders) {
            continue;
        }
        if first_run {
            if let Some(cutoff) = cutoff {
                if m.received.with_timezone(&Local).format("%Y-%m-%d").to_string().as_str() < cutoff {
                    continue;
                }
            }
        }
        sync_thread(p, &m.conversation_id, st, r);
    }
}

/// Writes or tops up one email thread's source. Fetches the whole conversation (incl. the user's own
/// replies), writes a new thread source the first time, and rewrites it in place when it has grown.
fn sync_thread(p: &Pass, conversation_id: &str, st: &mut state::State, r: &mut Report) {
    let msgs = match fetch::conversation(&p.client, conversation_id, 100) {
        Ok(m) => m,
        Err(e) => {
            r.problems.push(format!("Couldn't read a mail thread: {e}"));
            return;
        }
    };
    let count = msgs.len();
    let when = msgs.last().map(|m| m.received);
    // The thread's attachments are gathered only when its source is about to be written, so an
    // unchanged thread costs no attachment calls.
    let capture = |st: &mut state::State, r: &mut Report| {
        let attachments = if p.s.sync_mail_attachments {
            super::attachments::for_thread(&super::attachments::Target { data: p.data, vault: p.vault }, &msgs, st, &mut r.problems)
        } else {
            Vec::new()
        };
        fetch::thread_capture(&msgs, &attachments)
    };
    match st.thread_path(conversation_id).map(str::to_string) {
        None => {
            if let Some(cap) = capture(st, r) {
                if let Some(path) = deliver(p.data, p.vault, &cap, "outlook", when, r) {
                    st.mark_thread(conversation_id, &path, count);
                    r.mail += 1;
                }
            }
        }
        // Top up the thread in place. One that's gone (trashed, the Trash emptied, deleted elsewhere)
        // starts a fresh source with the whole thread, so the new mail reaches the Inbox rather than
        // being lost (D-20261026-05).
        Some(path) if count > st.thread_count(conversation_id) => {
            if let Some(cap) = capture(st, r) {
                let written = if p.vault.join(&path).is_file() {
                    rewrite(p.vault, &path, &cap, r).then_some(path)
                } else {
                    deliver(p.data, p.vault, &cap, "outlook", when, r)
                };
                if let Some(path) = written {
                    st.mark_thread(conversation_id, &path, count);
                    r.mail += 1;
                }
            }
        }
        _ => {}
    }
}

fn sync_transcripts(p: &Pass, st: &mut state::State, r: &mut Report) {
    let now = Utc::now();
    let events = match fetch::calendar(&p.client, &p.sign_in, now - Duration::hours(TRANSCRIPT_LOOKBACK_HOURS), now) {
        Ok(e) => e,
        Err(e) => {
            r.problems.push(format!("Couldn't read the calendar: {e}"));
            return;
        }
    };
    for ev in events {
        let Some(thread) = ev.thread_id.clone() else { continue };
        // A recurring meeting's whole series shares one thread id; key the state per occurrence (thread
        // id + the occurrence's local day) so each sitting is captured once and none shadows another.
        let id = format!("{thread}#{}", ev.start.with_timezone(&Local).date_naive());
        if ev.end > now || st.transcript_done(&id) {
            continue; // not finished yet, or already done
        }
        match fetch::transcript_capture(&p.client, &p.sign_in, &ev) {
            Ok(Some(cap)) => {
                if let Some(path) = deliver(p.data, p.vault, &cap, "teams", Some(ev.start), r) {
                    st.mark_transcript(&id, Some(&path));
                    r.transcripts += 1;
                }
            }
            // No transcript yet (not recorded, or still processing): leave it for a later pass, but
            // stop retrying meetings older than the lookback by marking very old ones done.
            Ok(None) => {
                if now - ev.end > Duration::hours(TRANSCRIPT_LOOKBACK_HOURS) {
                    st.mark_transcript(&id, None);
                }
            }
            Err(e) => r.problems.push(e),
        }
    }
}

/// Whether `address` matches an ignore entry: an exact address, or a bare domain entry.
pub fn ignored(address: &str, ignore: &[String]) -> bool {
    let addr = address.trim().to_ascii_lowercase();
    if addr.is_empty() {
        return false;
    }
    let domain = addr.rsplit('@').next().unwrap_or("");
    ignore.iter().map(|e| e.trim().to_ascii_lowercase()).any(|e| {
        if e.is_empty() {
            false
        } else if e.contains('@') {
            e == addr
        } else {
            e == domain
        }
    })
}

/// Upcoming meetings for the Today screen: the user's Teams meetings from now to the end of today —
/// and, from midday on, through the end of tomorrow as well, so the afternoon shows what's coming up
/// the next morning.
pub fn today_meetings(data: &Path) -> Result<Vec<Event>, String> {
    let s = settings::load(&data.join("settings.json"));
    if !s.sync_enabled {
        return Ok(vec![]);
    }
    let sign_in = auth::load().ok_or("Not signed in to Teams.")?;
    let client = super::client();
    let now = Utc::now();
    let last_day = Local::now().date_naive() + if Local::now().hour() >= 12 { Duration::days(1) } else { Duration::days(0) };
    let end = last_day
        .and_hms_opt(23, 59, 59)
        .and_then(|d| d.and_local_timezone(Local).single())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(now + Duration::hours(12));
    let mut events = fetch::calendar(&client, &sign_in, now - Duration::minutes(15), end)?;
    events.retain(|e| e.end >= now);
    events.sort_by_key(|e| e.start);
    Ok(events)
}

/// Starts the sync timer: a pass on launch (after a short delay) and every [`EVERY_MIN`] minutes,
/// when sync is on. A pass just blocks where there is no sign-in store (Linux).
pub fn watch(_app: tauri::AppHandle) {
    let data = platform::data_dir();
    std::thread::Builder::new()
        .name("teams-sync".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(20));
            loop {
                let _ = run_once(&data);
                std::thread::sleep(std::time::Duration::from_secs(EVERY_MIN * 60));
            }
        })
        .expect("can start a thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoff_day_counts_inclusive_days_or_none() {
        assert_eq!(cutoff_day(0), None, "0 means no limit");
        // N days includes today, so the cutoff is N-1 days back.
        let one = cutoff_day(1).unwrap();
        assert_eq!(one, Local::now().format("%Y-%m-%d").to_string(), "1 day is today only");
        let week = cutoff_day(7).unwrap();
        assert_eq!(week, (Local::now().date_naive() - Duration::days(6)).format("%Y-%m-%d").to_string());
        assert!(week < one, "a wider window reaches further back");
    }

    #[test]
    fn ignore_matches_address_or_domain() {
        let ignore = vec!["newsletters@acme.example".to_string(), "noreply.example".to_string()];
        assert!(ignored("Newsletters@Acme.Example", &ignore), "exact, case-insensitive");
        assert!(ignored("anything@noreply.example", &ignore), "whole domain");
        assert!(!ignored("maya@acme.example", &ignore));
        assert!(!ignored("", &ignore));
    }
}
