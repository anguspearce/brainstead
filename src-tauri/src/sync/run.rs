// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! One sync pass and the timer that drives it. A pass reads the user's chats, Inbox mail and meeting
//! transcripts and writes what's new into `sources/` through the shared capture path
//! (`nativehost::deliver`), so each lands in the Inbox and is ingested like any capture. Grouping and
//! what-counts-as-new follow D-20261025-02. Everything fails soft: one call failing is reported and
//! the pass carries on, so a changed Microsoft endpoint never breaks capture.

use chrono::{Duration, Local, Utc};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use super::auth::SignIn;
use super::fetch::{self, ConvKind, Event};
use super::{auth, state};
use crate::{nativehost, platform, settings};

/// How often the sync runs.
const EVERY_MIN: u64 = 15;
/// A chat's own day (today) is written once it's been quiet this long; earlier days are written as
/// soon as the day is over (D-20261025-02).
const QUIET_HOURS: i64 = 3;
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
    let client = super::client();
    let mut st = state::load(data);

    sync_chats(&client, &sign_in, data, vault, &s, &mut st, &mut r);
    sync_mail(&client, data, vault, &s, &mut st, &mut r);
    sync_transcripts(&client, &sign_in, data, vault, &mut st, &mut r);

    if let Err(e) = state::save(data, &st) {
        r.problems.push(format!("Couldn't save sync progress: {e}"));
    }
    r
}

fn deliver(data: &Path, vault: &Path, c: &brainstead_core::webcapture::Capture, ext: &str, r: &mut Report) -> bool {
    match nativehost::deliver(data, Some(vault), false, ext, c, true) {
        Ok(_) => true,
        Err(e) => {
            r.problems.push(e);
            false
        }
    }
}

fn sync_chats(
    client: &reqwest::blocking::Client,
    sign_in: &SignIn,
    data: &Path,
    vault: &Path,
    s: &settings::Settings,
    st: &mut state::State,
    r: &mut Report,
) {
    let convs = match fetch::conversations(client, sign_in) {
        Ok(c) => c,
        Err(e) => {
            r.problems.push(format!("Couldn't list chats: {e}"));
            return;
        }
    };
    let me = fetch::my_name(sign_in);
    let today = Local::now().format("%Y-%m-%d").to_string();
    let now = Utc::now();
    for conv in convs {
        // 1:1 and group chats always; channels only when opted in; meeting chats are left to the
        // transcript flow; anything else is skipped.
        let take = match conv.kind {
            ConvKind::OneOnOne | ConvKind::Group => true,
            ConvKind::Channel => s.sync_channels.iter().any(|c| c == &conv.id),
            ConvKind::Meeting | ConvKind::Other => false,
        };
        if !take {
            continue;
        }
        let msgs = match fetch::chat_messages(client, sign_in, &conv.id) {
            Ok(m) => m,
            Err(e) => {
                r.problems.push(format!("Couldn't read a chat: {e}"));
                continue;
            }
        };
        for (day, day_msgs) in fetch::by_local_day(&msgs) {
            if st.chat_day_done(&conv.id, &day) {
                continue;
            }
            // A past day is ready once it's over; today only once the chat has gone quiet.
            let ready = if day < today {
                true
            } else if day == today {
                day_msgs.last().map(|m| now - m.time >= Duration::hours(QUIET_HOURS)).unwrap_or(false)
            } else {
                false // a message dated in the future: leave it
            };
            if !ready {
                continue;
            }
            if let Some(cap) = fetch::chat_capture(&conv, &day_msgs, &me, &sign_in.origin) {
                if deliver(data, vault, &cap, "teams", r) {
                    st.mark_chat_day(&conv.id, &day);
                    r.chats += 1;
                }
            }
        }
    }
}

fn sync_mail(client: &reqwest::blocking::Client, data: &Path, vault: &Path, s: &settings::Settings, st: &mut state::State, r: &mut Report) {
    let mail = match fetch::inbox(client, MAIL_TOP) {
        Ok(m) => m,
        Err(e) => {
            r.problems.push(format!("Couldn't read the Inbox: {e}"));
            return;
        }
    };
    for m in mail {
        if m.id.is_empty() || st.mail_done(&m.id) {
            continue;
        }
        if ignored(&m.from_address, &s.sync_ignore_senders) {
            st.mark_mail(&m.id); // don't reconsider it every pass
            continue;
        }
        let cap = fetch::mail_capture(&m);
        if deliver(data, vault, &cap, "outlook", r) {
            st.mark_mail(&m.id);
            r.mail += 1;
        }
    }
}

fn sync_transcripts(
    client: &reqwest::blocking::Client,
    sign_in: &SignIn,
    data: &Path,
    vault: &Path,
    st: &mut state::State,
    r: &mut Report,
) {
    let now = Utc::now();
    let events = match fetch::calendar(client, sign_in, now - Duration::hours(TRANSCRIPT_LOOKBACK_HOURS), now) {
        Ok(e) => e,
        Err(e) => {
            r.problems.push(format!("Couldn't read the calendar: {e}"));
            return;
        }
    };
    for ev in events {
        let Some(id) = ev.thread_id.clone() else { continue };
        if ev.end > now || st.transcript_done(&id) {
            continue; // not finished yet, or already done
        }
        match fetch::transcript_capture(client, &ev) {
            Ok(Some(cap)) => {
                if deliver(data, vault, &cap, "teams", r) {
                    st.mark_transcript(&id);
                    r.transcripts += 1;
                }
            }
            // No transcript yet (not recorded, or still processing): leave it for a later pass, but
            // stop retrying meetings older than the lookback by marking very old ones done.
            Ok(None) => {
                if now - ev.end > Duration::hours(TRANSCRIPT_LOOKBACK_HOURS) {
                    st.mark_transcript(&id);
                }
            }
            Err(e) => r.problems.push(format!("Couldn't fetch a transcript: {e}")),
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

/// Upcoming meetings for the Today screen: the user's Teams meetings from now to the end of the day.
pub fn today_meetings(data: &Path) -> Result<Vec<Event>, String> {
    let s = settings::load(&data.join("settings.json"));
    if !s.sync_enabled {
        return Ok(vec![]);
    }
    let sign_in = auth::load().ok_or("Not signed in to Teams.")?;
    let client = super::client();
    let now = Utc::now();
    let end_of_day = Local::now()
        .date_naive()
        .and_hms_opt(23, 59, 59)
        .and_then(|d| d.and_local_timezone(Local).single())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(now + Duration::hours(12));
    let mut events = fetch::calendar(&client, &sign_in, now - Duration::minutes(15), end_of_day)?;
    events.retain(|e| e.end >= now);
    events.sort_by_key(|e| e.start);
    Ok(events)
}

/// Starts the sync timer: a pass on launch (after a short delay) and every [`EVERY_MIN`] minutes,
/// when sync is on. macOS only; elsewhere there's no sign-in store so a pass just blocks.
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
    fn ignore_matches_address_or_domain() {
        let ignore = vec!["newsletters@acme.example".to_string(), "noreply.example".to_string()];
        assert!(ignored("Newsletters@Acme.Example", &ignore), "exact, case-insensitive");
        assert!(ignored("anything@noreply.example", &ignore), "whole domain");
        assert!(!ignored("maya@acme.example", &ignore));
        assert!(!ignored("", &ignore));
    }
}
