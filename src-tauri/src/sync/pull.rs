// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! On-demand pull (the `pull_teams_outlook` MCP tool): an assistant asks for the chats, mail and
//! meeting transcripts about some topic over the last few days, and this gathers them into the vault
//! and hands back the source paths to read. It reuses what the sync already pulled — an item already
//! in the vault is vended, not fetched again — and pulls only what's missing (D-20261025-04). Chats
//! and mail threads go through the sync pass's own steps (`run::sync_chat`, `run::sync_thread`), so a
//! chat is read back page by page to the first day asked for and a source that has gained messages
//! is topped up. A chat on Chats to skip and mail from a sender on Mail to skip are left out here as
//! they are there, and the result says so. Reads only, like the rest of the sync; macOS and Windows (wherever there is a
//! secret store).

use chrono::{Duration, Local, Utc};
use serde::Serialize;

use super::fetch;
use super::run::{self, Pass};
use super::{auth, state};
use crate::{nativehost, settings};

/// One source the pull gathered.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// "email", "chat" or "transcript".
    pub kind: String,
    pub title: String,
    /// Local day (`YYYY-MM-DD`).
    pub date: String,
    /// The vault path to read, or empty when it was pulled before this feature stored paths (then the
    /// assistant finds it with search).
    pub path: String,
    /// It was already in the vault (vended), not fetched now.
    pub already_had: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub items: Vec<Item>,
    /// What went wrong.
    pub problems: Vec<String>,
    /// What wasn't read because it's on a skip list (Settings › Sync): said, but not a problem.
    pub left_out: Vec<String>,
    pub blocked: Option<String>,
}

/// Which kinds to pull.
#[derive(Clone, Copy)]
pub struct Kinds {
    pub chats: bool,
    pub mail: bool,
    pub transcripts: bool,
}

impl Kinds {
    pub fn all() -> Kinds {
        Kinds { chats: true, mail: true, transcripts: true }
    }
}

/// Lowercased words of the query; empty means "no filter".
fn terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(|w| w.to_ascii_lowercase()).collect()
}

/// Whether `text` contains every query term (case-insensitive). No terms → always matches.
fn matches(text: &str, terms: &[String]) -> bool {
    if terms.is_empty() {
        return true;
    }
    let hay = text.to_ascii_lowercase();
    terms.iter().all(|t| hay.contains(t))
}

/// Gathers the chats, mail and transcripts about `query` over the last `days` days into the vault,
/// reusing what's already there. `query` empty means everything in the window.
pub fn pull(data: &std::path::Path, query: &str, days: u32, kinds: Kinds) -> Outcome {
    let mut out = Outcome::default();
    let s = settings::load(&data.join("settings.json"));
    let Some(vault) = s.vault_path.as_deref().filter(|p| !p.trim().is_empty()) else {
        out.blocked = Some("No vault chosen.".into());
        return out;
    };
    if s.read_only {
        out.blocked = Some("The vault is read-only, so nothing new can be pulled in.".into());
        return out;
    }
    let Some(sign_in) = auth::load() else {
        out.blocked = Some("Not signed in. Open Teams in your browser so the extension hands the sign-in over.".into());
        return out;
    };
    let vault = std::path::Path::new(vault);
    // What the sync pass's own steps take, so a chat or a thread is read and written here as there.
    // No first-sync cutoff: the days asked for decide.
    let p = Pass { client: super::client(), sign_in, data, vault, s: &s, cutoff: None, from: None, regather: true };
    // Waits for a sync pass that's running, and keeps one from starting, so neither saves over the
    // other's progress.
    let _changing = state::lock();
    let mut st = state::load(data);
    let terms = terms(query);
    let now = Utc::now();
    let since = now - Duration::days(i64::from(days.max(1)));
    let cutoff_day = since.with_timezone(&Local).format("%Y-%m-%d").to_string();

    if kinds.chats {
        pull_chats(&p, &mut st, &terms, &cutoff_day, &mut out);
    }
    if kinds.mail {
        pull_mail(&p, query, &terms, since, &mut st, &mut out);
    }
    if kinds.transcripts {
        pull_transcripts(&p.client, &p.sign_in, vault, data, &terms, since, now, &mut st, &mut out);
    }

    if let Err(e) = state::save(data, &st) {
        out.problems.push(format!("Couldn't save sync progress: {e}"));
    }
    out
}

/// Whether a recorded source is still a file in the vault. The pull vends it only when it's really
/// there, so a source the user deleted is fetched again. An item synced before paths were stored has
/// no path to check, so it's treated as gone and re-materialised (which can duplicate a pre-path item
/// that is in fact still present; everything synced since stores its path, so that shrinks away).
fn still_there(vault: &std::path::Path, path: Option<&str>) -> bool {
    path.is_some_and(|p| vault.join(p).exists())
}

fn deliver(
    data: &std::path::Path,
    vault: &std::path::Path,
    c: &brainstead_core::webcapture::Capture,
    ext: &str,
    when: Option<chrono::DateTime<Utc>>,
    out: &mut Outcome,
) -> Option<String> {
    match nativehost::deliver(data, Some(vault), false, ext, c, true, when.map(|t| t.to_rfc3339())) {
        Ok(path) => Some(path),
        Err(e) => {
            out.problems.push(e);
            None
        }
    }
}

/// Whether a pull takes this day of a chat: one of the days asked for, about the topic (the chat's
/// name, or any message that day).
fn covered(topic: &str, day: &str, msgs: &[fetch::ChatMessage], cutoff_day: &str, terms: &[String]) -> bool {
    if day < cutoff_day {
        return false;
    }
    let text = format!("{topic}\n{}", msgs.iter().map(|m| m.body.as_str()).collect::<Vec<_>>().join("\n"));
    matches(&text, terms)
}

/// Each chat is read and written by the sync pass's own step (`run::sync_chat`): back, page by page,
/// to the first day asked for, so a day is written whole, and one already written is topped up when
/// it has grown.
fn pull_chats(p: &Pass, st: &mut state::State, terms: &[String], cutoff_day: &str, out: &mut Outcome) {
    let convs = match fetch::conversations(&p.client, &p.sign_in) {
        Ok(c) => c,
        Err(e) => {
            out.problems.push(format!("Couldn't list chats: {e}"));
            return;
        }
    };
    let me = fetch::my_name(&p.sign_in);
    let mut skipped = 0;
    for conv in convs {
        if !run::chat_taken(&conv, p.s) {
            continue;
        }
        // On Chats to skip: not read, as in a sync pass, so whether it's about the topic isn't known.
        if run::chat_skipped(&conv.id, p.s) {
            skipped += 1;
            continue;
        }
        let covers = |day: &str, msgs: &[fetch::ChatMessage]| covered(&conv.topic, day, msgs, cutoff_day, terms);
        // Read as far back as a back-fill would go, should the days asked for hold that much.
        let days = run::Days { from: Some(cutoff_day), cutoff: Some(cutoff_day), most: fetch::BACKFILL_CHAT_PAGES, covers: &covers };
        let mut r = run::Report::default();
        let read = run::sync_chat(p, &conv, &me, &days, st, &mut r);
        out.problems.append(&mut r.problems);
        // What to call this chat in the result: its topic, else the partner, else a generic name.
        let label = if !conv.topic.is_empty() {
            conv.topic.clone()
        } else if !read.partner.is_empty() {
            read.partner.clone()
        } else {
            "Teams chat".to_string()
        };
        for d in read.days {
            // A source that couldn't be written again isn't handed back.
            if d.fresh || still_there(p.vault, Some(&d.path)) {
                out.items.push(Item { kind: "chat".into(), title: label.clone(), date: d.day, path: d.path, already_had: !d.fresh });
            }
        }
    }
    out.left_out.extend(chats_left_out(skipped));
}

/// What the result says of the chats on Chats to skip, so less coming back isn't taken for nothing
/// being there.
fn chats_left_out(skipped: usize) -> Option<String> {
    match skipped {
        0 => None,
        1 => Some("Left out, not read: 1 chat on Chats to skip (Settings › Sync)".into()),
        n => Some(format!("Left out, not read: {n} chats on Chats to skip (Settings › Sync)")),
    }
}

/// The same for mail in the days asked for whose sender is on Mail to skip.
fn mail_left_out(senders: &std::collections::BTreeSet<String>) -> Option<String> {
    let senders: Vec<&str> = senders.iter().map(String::as_str).collect();
    (!senders.is_empty()).then(|| format!("Left out, not read: mail from {}, on Mail to skip (Settings › Sync)", senders.join(", ")))
}

/// Whether the mailbox lists a message later than the last one in the thread's source (None: the
/// source is from before last messages were kept, and is handed back as it is).
fn newer_than_source(last: Option<i64>, received: chrono::DateTime<Utc>) -> bool {
    last.is_some_and(|was| received.timestamp_millis() > was)
}

fn pull_mail(p: &Pass, query: &str, terms: &[String], since: chrono::DateTime<Utc>, st: &mut state::State, out: &mut Outcome) {
    // With a topic, search the mailbox for it; without, take the recent Inbox. Mail is grouped one
    // source per thread, so matching messages flag their conversation, then each thread is gathered.
    let mail = if query.trim().is_empty() { fetch::inbox(&p.client, 60) } else { fetch::search_mail(&p.client, query, 60) };
    let mail = match mail {
        Ok(m) => m,
        Err(e) => {
            out.problems.push(format!("Couldn't search mail: {e}"));
            return;
        }
    };
    let mut seen = std::collections::HashSet::new();
    let mut skipped = std::collections::BTreeSet::new();
    for m in mail {
        if m.received < since || m.conversation_id.is_empty() || !seen.insert(m.conversation_id.clone()) {
            continue;
        }
        // $search already matched the thread; for an inbox listing without a query, matches() passes.
        if query.trim().is_empty() && !matches(&format!("{} {}", m.subject, m.from), terms) {
            continue;
        }
        // On Mail to skip: the thread is left out by its latest message, as in a sync pass.
        if run::ignored(&m.from_address, &p.s.sync_ignore_senders) {
            skipped.insert(m.from_address.trim().to_ascii_lowercase());
            continue;
        }
        // Already captured, still present and holding this message → vend it, without a fetch.
        let had = st.thread_path(&m.conversation_id).filter(|path| still_there(p.vault, Some(path))).map(str::to_string);
        if had.is_none() || newer_than_source(st.thread_last(&m.conversation_id), m.received) {
            // Otherwise the sync pass's own step (`run::sync_thread`): the whole conversation, page
            // by page, written new or topped up in place.
            let mut r = run::Report::default();
            run::sync_thread(p, &m.conversation_id, st, &mut r);
            out.problems.append(&mut r.problems);
        }
        let Some(path) = st.thread_path(&m.conversation_id).filter(|path| still_there(p.vault, Some(path))).map(str::to_string) else {
            continue;
        };
        let already_had = had.as_deref() == Some(path.as_str());
        // A thread just read is dated by its last message.
        let last = st.thread_last(&m.conversation_id).filter(|_| !already_had).and_then(chrono::DateTime::from_timestamp_millis);
        let date = last.unwrap_or(m.received).with_timezone(&Local).format("%Y-%m-%d").to_string();
        out.items.push(Item { kind: "email".into(), title: m.subject, date, path, already_had });
    }
    out.left_out.extend(mail_left_out(&skipped));
}

#[allow(clippy::too_many_arguments)]
fn pull_transcripts(
    client: &reqwest::blocking::Client,
    sign_in: &auth::SignIn,
    vault: &std::path::Path,
    data: &std::path::Path,
    terms: &[String],
    since: chrono::DateTime<Utc>,
    now: chrono::DateTime<Utc>,
    st: &mut state::State,
    out: &mut Outcome,
) {
    let events = match fetch::calendar(client, sign_in, since, now) {
        Ok(e) => e,
        Err(e) => {
            out.problems.push(format!("Couldn't read the calendar: {e}"));
            return;
        }
    };
    for ev in events {
        let Some(thread) = ev.thread_id.as_deref() else { continue };
        // Keyed as the sync pass keys it, per sitting.
        let id = state::transcript_key(thread, ev.start);
        if ev.end > now || !matches(&format!("{} {}", ev.subject, ev.organizer), terms) {
            continue;
        }
        let date = ev.start.with_timezone(&Local).format("%Y-%m-%d").to_string();
        if st.transcript_done(&id) && still_there(vault, st.transcript_path(&id)) {
            let p = st.transcript_path(&id).unwrap_or("");
            out.items.push(Item { kind: "transcript".into(), title: ev.subject.clone(), date, path: p.to_string(), already_had: true });
            continue;
        }
        match fetch::transcript_capture(client, sign_in, &ev) {
            Ok(Some(cap)) => {
                if let Some(path) = deliver(data, vault, &cap, "teams", Some(ev.start), out) {
                    st.mark_transcript(&id, Some(&path));
                    out.items.push(Item { kind: "transcript".into(), title: ev.subject, date, path, already_had: false });
                }
            }
            Ok(None) => {} // no transcript for this meeting
            Err(e) => out.problems.push(format!("Couldn't fetch a transcript: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_requires_every_term() {
        let t = terms("Fit Program");
        assert!(matches("notes about the FIT program rollout", &t));
        assert!(!matches("the program is on track", &t), "missing 'fit'");
        assert!(matches("anything", &terms("")), "no terms matches all");
    }

    #[test]
    fn a_day_is_taken_when_asked_for_and_about_the_topic() {
        let said = |body: &str| {
            vec![fetch::ChatMessage { id: "1".into(), sender: "Maya".into(), body: body.into(), time: Utc::now(), files: vec![] }]
        };
        let t = terms("orbit");
        assert!(covered("", "2026-10-24", &said("The Orbit App launch moved"), "2026-10-22", &t));
        assert!(covered("Orbit App", "2026-10-22", &said("See you at ten"), "2026-10-22", &t), "the chat's name counts");
        assert!(!covered("", "2026-10-24", &said("See you at ten"), "2026-10-22", &t));
        assert!(!covered("Orbit App", "2026-10-21", &said("Orbit"), "2026-10-22", &t), "before the days asked for");
        assert!(covered("", "2026-10-24", &said("anything"), "2026-10-22", &terms("")));
    }

    #[test]
    fn what_the_skip_lists_left_out_is_said() {
        assert_eq!(chats_left_out(0), None);
        assert_eq!(chats_left_out(1).unwrap(), "Left out, not read: 1 chat on Chats to skip (Settings › Sync)");
        assert_eq!(chats_left_out(3).unwrap(), "Left out, not read: 3 chats on Chats to skip (Settings › Sync)");
        let mut senders = std::collections::BTreeSet::new();
        assert_eq!(mail_left_out(&senders), None);
        senders.insert("offers@shop.example".to_string());
        senders.insert("news@acme.example".to_string());
        assert_eq!(
            mail_left_out(&senders).unwrap(),
            "Left out, not read: mail from news@acme.example, offers@shop.example, on Mail to skip (Settings › Sync)"
        );
    }

    #[test]
    fn a_thread_is_read_again_only_for_a_message_its_source_lacks() {
        let at = |ms| chrono::DateTime::from_timestamp_millis(ms).unwrap();
        assert!(newer_than_source(Some(1000), at(2000)));
        assert!(!newer_than_source(Some(1000), at(1000)));
        assert!(!newer_than_source(Some(1000), at(500)), "an earlier message of the thread");
        assert!(!newer_than_source(None, at(2000)), "from before last messages were kept");
    }
}
