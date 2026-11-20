// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! One sync pass and the timer that drives it. A pass reads the user's chats, Inbox mail and meeting
//! transcripts and writes what's new into `sources/` through the shared capture path
//! (`nativehost::deliver`), so each lands in the Inbox and is ingested like any capture. Grouping and
//! what-counts-as-new follow D-20261025-02. Everything fails soft: one call failing is reported and
//! the pass carries on, so a changed Microsoft endpoint never breaks capture.

use chrono::{DateTime, Duration, Local, NaiveDate, Timelike, Utc};
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
/// The most Inbox messages a back-fill reads (the whole Inbox, short of a vast one).
const BACKFILL_MAIL: usize = 2000;

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
    /// The sources written or topped up, in the order written.
    pub paths: Vec<String>,
    /// Those of them topped up in place rather than written new.
    #[serde(default)]
    pub topped_up: Vec<String>,
    /// Threads topped up with a reply the user sent (D-20261120-06): ingested again on arrival, so
    /// what the reply settles reaches the initiatives.
    #[serde(default)]
    pub replied: Vec<String>,
    /// Sync is off, or another pass was running: nothing to log.
    #[serde(skip)]
    pub idle: bool,
}

/// Runs one sync pass. Safe to call from the timer or a command; overlapping calls return a
/// `blocked` report rather than running twice.
pub fn run_once(data: &Path) -> Report {
    run_from(data, None)
}

/// A pass, or with `from` a back-fill (D-20261118-23): Teams and Slack conversations from that day
/// on, days already written topped up and days skipped before written now, and every thread in the
/// Outlook Inbox with a message since that day. What it writes is marked a back-fill, so it waits in the Inbox
/// rather than being ingested on arrival.
pub fn run_from(data: &Path, from: Option<NaiveDate>) -> Report {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Report { blocked: Some("A sync is already running.".into()), idle: true, ..Default::default() };
    }
    nativehost::BACKFILL.store(from.is_some(), Ordering::SeqCst);
    let at = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let report = run_inner(data, from);
    if !report.idle {
        super::log::record(data, &report, &at, from.is_some());
    }
    nativehost::BACKFILL.store(false, Ordering::SeqCst);
    RUNNING.store(false, Ordering::SeqCst);
    report
}

fn run_inner(data: &Path, from: Option<NaiveDate>) -> Report {
    let mut r = Report::default();
    let s = settings::load(&data.join("settings.json"));
    let vault = s.vault_path.as_deref().filter(|p| !p.trim().is_empty());
    if !s.sync_enabled && !s.mail_on() && !s.sync_slack {
        r.blocked = Some("Sync is off: switch on Teams, Outlook or Slack in Settings › Sync.".into());
        r.idle = true;
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
    // Slack on its own sign-in (D-20261118-01), whether or not Teams and Outlook are on.
    if s.sync_slack {
        super::slack::sync(data, Path::new(vault), &s, cutoff_day(s.sync_initial_days).as_deref(), from, &mut r);
    }
    if !s.sync_enabled && !s.mail_on() {
        return r;
    }
    let Some(sign_in) = auth::load() else {
        r.blocked = Some(
            "Not signed in. Open Teams (or Outlook on the web, for mail) in your browser so its extension hands the sign-in over.".into(),
        );
        return r;
    };

    let vault = Path::new(vault);
    // Held to the save below: an on-demand pull changes the same state.
    let _changing = state::lock();
    let mut st = state::load(data);
    // The first-sync history limit (Settings): a day/message older than this is skipped so a big
    // mailbox or long chat history can't flood the Inbox. 0 means no limit.
    let p = Pass {
        client: super::client(),
        sign_in,
        data,
        vault,
        s: &s,
        cutoff: cutoff_day(s.sync_initial_days),
        from: from.map(|d| d.format("%Y-%m-%d").to_string()),
    };

    // Teams (chats, transcripts) and Outlook (mail) each on its own switch and its own tab's sign-in.
    if s.sync_enabled {
        sync_chats(&p, &mut st, &mut r);
    }
    if s.mail_on() {
        sync_mail(&p, &mut st, &mut r);
        sync_sent(&p, &mut st, &mut r);
    }
    if s.sync_enabled {
        sync_transcripts(&p, &mut st, &mut r);
    }

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
    /// A back-fill's first day (`YYYY-MM-DD`), see [`run_from`].
    from: Option<String>,
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
        Ok(path) => {
            r.paths.push(path.clone());
            Some(path)
        }
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
        Ok(()) => {
            r.paths.push(path.to_string());
            r.topped_up.push(path.to_string());
            true
        }
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
    // A back-fill starts every chat at its first day; otherwise a chat seen for the first time starts
    // at the first-sync cutoff.
    let cutoff = p.from.as_deref().or(p.cutoff.as_deref());
    for conv in convs {
        // 1:1 and group chats always; channels only when opted in; meeting chats are left to the
        // transcript flow; anything else is skipped.
        let take = match conv.kind {
            ConvKind::OneOnOne | ConvKind::Group => true,
            ConvKind::Channel => p.s.sync_channels.iter().any(|c| c == &conv.id),
            ConvKind::Meeting | ConvKind::Other => false,
        };
        if !take || p.s.sync_ignore_chats.contains(&conv.id) {
            continue;
        }
        // The first time a chat is seen, skip days older than the first-sync cutoff (and mark them
        // done, so they're never pulled later) — that's where the flood of old history would come
        // from. A chat already being synced only gets genuinely new days anyway.
        let first_seen = !st.chats.contains_key(&conv.id);
        // Read back until the oldest day read needs nothing written: a busy day runs past one page.
        let most = if p.from.is_some() { fetch::BACKFILL_CHAT_PAGES } else { fetch::CHAT_PAGES };
        let enough = |msgs: &[fetch::ChatMessage], pages: usize| {
            let Some(first) = msgs.first() else { return true };
            let day = fetch::local_day(first.time);
            let of_day = msgs.iter().take_while(|m| fetch::local_day(m.time) == day);
            let (count, last) = of_day.fold((0, 0), |(n, _), m| (n + 1, m.time.timestamp_millis()));
            let d = Day { conv: &conv.id, day: &day, count, last, partial: false, first_seen };
            read_enough(st, p.from.as_deref(), cutoff, &d, pages)
        };
        let (msgs, more) = match fetch::chat_messages(&p.client, &p.sign_in, &conv.id, most, enough) {
            Ok(m) => m,
            Err(e) => {
                r.problems.push(format!("Couldn't read a chat: {e}"));
                continue;
            }
        };
        // The chat's partner across the whole thread, so a day where only the user wrote still names
        // the other person (a 1:1 with no topic).
        let partner = fetch::chat_display_name(&msgs, &me);
        // With older messages left unread, the oldest day read may be only its end.
        let cut = more.then(|| msgs.first().map(|m| fetch::local_day(m.time))).flatten();
        for (day, day_msgs) in fetch::by_local_day(&msgs) {
            let count = day_msgs.len();
            let when = day_msgs.last().map(|m| m.time);
            let last = when.map_or(0, |t| t.timestamp_millis());
            let d = Day { conv: &conv.id, day: &day, count, last, partial: cut.as_deref() == Some(day.as_str()), first_seen };
            match day_plan(st, p.from.as_deref(), cutoff, &d) {
                DayPlan::Leave => {}
                DayPlan::Adopt => {
                    st.set_chat_day_count(&conv.id, &day, count);
                    st.set_chat_day_last(&conv.id, &day, last);
                }
                // Top up the day's source in place (D-20261025-07). One that's gone (trashed, the
                // Trash emptied, deleted elsewhere) starts a fresh source for the day, so the new
                // messages reach the Inbox rather than being lost (D-20261026-05).
                DayPlan::TopUp => {
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
                                st.set_chat_day_last(&conv.id, &day, last);
                                r.chats += 1;
                            }
                        }
                    }
                }
                // On a chat's first sync, skip (and mark) days older than the cap; a back-fill leaves
                // days before its first day as they are.
                DayPlan::Skip => {
                    if first_seen {
                        st.mark_chat_day(&conv.id, &day, None);
                    }
                }
                DayPlan::Write => {
                    if let Some(cap) = fetch::chat_capture(&conv, &day_msgs, &me, &p.sign_in.origin, &day, &partner) {
                        if let Some(path) = deliver(p.data, p.vault, &cap, "teams", when, r) {
                            st.mark_chat_day(&conv.id, &day, Some(&path));
                            st.set_chat_day_count(&conv.id, &day, count);
                            st.set_chat_day_last(&conv.id, &day, last);
                            r.chats += 1;
                        }
                    }
                }
            }
        }
    }
}

/// One local day of a chat, as a pass read it.
struct Day<'a> {
    conv: &'a str,
    day: &'a str,
    /// How many of its messages were read, and when the last came (milliseconds since 1970).
    count: usize,
    last: i64,
    /// Older messages were left unread and this is the oldest day read, so it may be only its end.
    partial: bool,
    /// The chat has never been synced before.
    first_seen: bool,
}

/// What a pass does with one day of a chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DayPlan {
    Leave,
    /// Written before its count or last message was kept: they're noted now, without a rewrite.
    Adopt,
    /// Written, and it has gained messages: its source is rewritten in place.
    TopUp,
    /// Older than the first-sync cutoff, or than a back-fill's first day: not written.
    Skip,
    /// Not written yet: it becomes a source.
    Write,
}

/// `from` is a back-fill's first day; `cutoff` is that, or else the first-sync cutoff.
fn day_plan(st: &state::State, from: Option<&str>, cutoff: Option<&str>, d: &Day) -> DayPlan {
    // A back-fill writes a day the first sync skipped (marked done, but no file).
    let skipped = st.chat_day_path(d.conv, d.day).is_none();
    let refill = from.is_some_and(|f| d.day >= f) && skipped;
    if st.chat_day_done(d.conv, d.day) && !refill {
        // Never written (skipped, or from before paths were kept): there's no source to top up.
        if skipped {
            return DayPlan::Leave;
        }
        // By its last message where that was kept: the count of a day cut short says nothing.
        let grown = match st.chat_day_last(d.conv, d.day) {
            Some(was) => d.last > was,
            None => st.chat_day_counted(d.conv, d.day) && d.count > st.chat_day_count(d.conv, d.day),
        };
        return match (grown, d.partial) {
            // Only the day's end was read: rewriting from it would drop the messages before.
            (_, true) => DayPlan::Leave,
            (true, false) => DayPlan::TopUp,
            (false, false) if st.chat_day_last(d.conv, d.day).is_none() => DayPlan::Adopt,
            (false, false) => DayPlan::Leave,
        };
    }
    if (d.first_seen || from.is_some()) && cutoff.is_some_and(|c| d.day < c) {
        return DayPlan::Skip;
    }
    DayPlan::Write
}

/// Whether a chat has been read far enough back: `d` is the oldest day read so far (taken as whole)
/// and `pages` how many pages that took.
fn read_enough(st: &state::State, from: Option<&str>, cutoff: Option<&str>, d: &Day, pages: usize) -> bool {
    // A back-fill reads back to its first day.
    if let Some(f) = from {
        return d.day < f;
    }
    match day_plan(st, from, cutoff, d) {
        DayPlan::Leave | DayPlan::Skip => true,
        DayPlan::TopUp => false,
        // A chat seen for the first time with no first-sync limit takes its newest page, as before.
        DayPlan::Write => d.first_seen && cutoff.is_none(),
        // Written before last messages were kept, and the first page holds as many as were written:
        // the day may have filled a page and lost its start to a rewrite. One more page settles it.
        DayPlan::Adopt => !(pages == 1 && st.chat_day_counted(d.conv, d.day) && d.count >= st.chat_day_count(d.conv, d.day)),
    }
}

fn sync_mail(p: &Pass, st: &mut state::State, r: &mut Report) {
    let mail = match if p.from.is_some() { fetch::inbox_all(&p.client, BACKFILL_MAIL) } else { fetch::inbox(&p.client, MAIL_TOP) } {
        Ok(m) => m,
        Err(e) => {
            r.problems.push(format!("Couldn't read the Inbox: {e}"));
            return;
        }
    };
    // A back-fill reads the whole Inbox, but takes only the threads with a message since its first
    // day: a thread left in the Inbox for months stays out.
    let first_run = st.mail_first_run() && p.from.is_none();
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
        if let Some(from) = p.from.as_deref() {
            if m.received.with_timezone(&Local).format("%Y-%m-%d").to_string().as_str() < from {
                continue;
            }
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
    // Only here, once the Inbox was read: a pass with mail off, or one that couldn't read it,
    // leaves mail's first run still to come.
    st.mail_initialized = true;
}

/// How far back the first look at the user's sent mail goes.
const SENT_FIRST_DAYS: i64 = 3;
/// At most this many sent-window messages are looked at in a pass.
const SENT_MOST: usize = 500;

/// The user's own replies (D-20261120-06): a thread only comes up in [`sync_mail`] when the Inbox has
/// a new message for it, so a reply sent and then archived with the thread would never reach its
/// source. Each pass reads the mail sent since the last, in every folder, and tops up each thread
/// Brainstead already has a source for; one it tops up is ingested again on arrival
/// ([`Report::replied`]). Mail the user starts afresh is left out.
fn sync_sent(p: &Pass, st: &mut state::State, r: &mut Report) {
    if p.from.is_some() {
        return;
    }
    let now = Utc::now();
    // A little overlap, for mail that took a moment to show as sent.
    let since = st
        .sent_checked
        .as_deref()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&Utc) - Duration::minutes(30))
        .unwrap_or(now - Duration::days(SENT_FIRST_DAYS));
    let convs = match fetch::sent_since(&p.client, since, &fetch::my_address(&p.sign_in), SENT_MOST) {
        Ok(c) => c,
        Err(e) => {
            r.problems.push(format!("Couldn't read the mail you sent: {e}"));
            return;
        }
    };
    for conv in convs {
        let Some(path) = st.thread_path(&conv).map(str::to_string) else { continue };
        // Topped up already this pass (a new Inbox message came too): the reply is in it.
        if r.topped_up.contains(&path) {
            if !r.replied.contains(&path) {
                r.replied.push(path);
            }
            continue;
        }
        // Only a thread topped up in place: one written afresh (its source had gone) arrives as a
        // capture, ingested on arrival as any is.
        let before = r.topped_up.len();
        sync_thread(p, &conv, st, r);
        if r.topped_up.len() > before {
            if let Some(path) = r.topped_up.last().cloned() {
                if !r.replied.contains(&path) {
                    r.replied.push(path);
                }
            }
        }
    }
    st.sent_checked = Some(now.to_rfc3339());
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
    let last = when.map_or(0, |t| t.timestamp_millis());
    // The thread's attachments are gathered only when its source is about to be written, so an
    // unchanged thread costs no attachment calls.
    let capture = |st: &mut state::State, r: &mut Report| {
        let attachments = if p.s.sync_mail_attachments {
            {
                let since = p
                    .from
                    .as_deref()
                    .and_then(|f| NaiveDate::parse_from_str(f, "%Y-%m-%d").ok())
                    .and_then(|d| d.and_hms_opt(0, 0, 0))
                    .and_then(|t| t.and_local_timezone(Local).earliest())
                    .map(|t| t.with_timezone(&Utc));
                super::attachments::for_thread(
                    &super::attachments::Target { data: p.data, vault: p.vault, since },
                    &msgs,
                    st,
                    &mut r.problems,
                )
            }
        } else {
            Vec::new()
        };
        let continues = fetch::replies_into(&msgs).and_then(|h| st.continued(conversation_id, &h));
        fetch::thread_capture(&msgs, &attachments, continues)
    };
    if let Some(h) = fetch::thread_header(&msgs) {
        st.mark_header(conversation_id, &h);
    }
    match st.thread_path(conversation_id).map(str::to_string) {
        None => {
            if let Some(cap) = capture(st, r) {
                if let Some(path) = deliver(p.data, p.vault, &cap, "outlook", when, r) {
                    st.mark_thread(conversation_id, &path, count);
                    st.set_thread_last(conversation_id, last);
                    r.mail += 1;
                }
            }
        }
        // Top up the thread in place. One that's gone (trashed, the Trash emptied, deleted elsewhere)
        // starts a fresh source with the whole thread, so the new mail reaches the Inbox rather than
        // being lost (D-20261026-05).
        Some(path) if thread_grown(st, conversation_id, count, last) => {
            if let Some(cap) = capture(st, r) {
                let written = if p.vault.join(&path).is_file() {
                    rewrite(p.vault, &path, &cap, r).then_some(path)
                } else {
                    deliver(p.data, p.vault, &cap, "outlook", when, r)
                };
                if let Some(path) = written {
                    st.mark_thread(conversation_id, &path, count);
                    st.set_thread_last(conversation_id, last);
                    r.mail += 1;
                }
            }
        }
        // Written before last messages were kept: noted now, without a rewrite.
        Some(_) if st.thread_last(conversation_id).is_none() => st.set_thread_last(conversation_id, last),
        _ => {}
    }
}

/// Whether a thread has gained mail since its source was written: more messages than then, or a
/// later last one (a thread that lost a message and gained another counts the same).
fn thread_grown(st: &state::State, conversation_id: &str, count: usize, last: i64) -> bool {
    count > st.thread_count(conversation_id) || st.thread_last(conversation_id).is_some_and(|was| last > was)
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
        let id = state::transcript_key(&thread, ev.start);
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
pub fn watch(app: tauri::AppHandle) {
    let data = platform::data_dir();
    std::thread::Builder::new()
        .name("teams-sync".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(20));
            let mut told = String::new();
            loop {
                let report = run_once(&data);
                tell_replied(&app, &report);
                // A sign-in that has just gone stale or been turned down needs the user: told once,
                // and again only after it has come back and gone again.
                let s = settings::load(&data.join("settings.json"));
                let now = sign_in_trouble(&auth::state(), s.sync_enabled, s.mail_on());
                if !now.is_empty() && now != told {
                    crate::notify::show(&app, crate::notify::Need::You, "Sync needs you", &now);
                }
                told = now;
                std::thread::sleep(std::time::Duration::from_secs(EVERY_MIN * 60));
            }
        })
        .expect("can start a thread");
}

/// Tells the window which threads a pass topped up with the user's own reply (`sync-replied`), for
/// it to ingest them again when new sources are ingested as they arrive (D-20261120-06).
pub fn tell_replied(app: &tauri::AppHandle, r: &Report) {
    use tauri::Emitter;
    if !r.replied.is_empty() {
        let _ = app.emit("sync-replied", &r.replied);
    }
}

/// What's wrong with the Teams and Outlook sign-in, for the services switched on, in words; empty
/// when nothing is (or the sync isn't signed in at all, which Settings › Sync says).
fn sign_in_trouble(st: &auth::SignInState, teams: bool, mail: bool) -> String {
    let on = |svc: &&String| if svc.as_str() == "mail" { mail } else { teams };
    let rejected: Vec<&str> = st.rejected.iter().filter(on).map(String::as_str).collect();
    let stale: Vec<&str> = st.stale.iter().filter(on).filter(|s| !st.rejected.contains(s)).map(String::as_str).collect();
    let mut out = Vec::new();
    if !rejected.is_empty() {
        out.push(format!("Signed out: {}. Sign in again in that Chrome tab.", rejected.join(", ")));
    }
    if !stale.is_empty() {
        out.push(format!("Gone stale: {}. Keep Teams (and Outlook on the web, for mail) open in Chrome.", stale.join(", ")));
    }
    if !st.signed_in || out.is_empty() {
        return String::new();
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_trouble_names_what_is_switched_on() {
        let st = auth::SignInState {
            signed_in: true,
            handed_over: None,
            stale: vec!["mail".into(), "transcripts".into()],
            rejected: vec![],
            extension_outdated: None,
            extension_shipped: String::new(),
            teams: true,
            outlook: false,
        };
        assert_eq!(
            sign_in_trouble(&st, true, false),
            "Gone stale: transcripts. Keep Teams (and Outlook on the web, for mail) open in Chrome."
        );
        assert_eq!(sign_in_trouble(&st, false, false), "");
        let out = auth::SignInState { rejected: vec!["chats".into()], stale: vec!["chats".into()], ..st.clone() };
        assert_eq!(sign_in_trouble(&out, true, false), "Signed out: chats. Sign in again in that Chrome tab.");
        assert_eq!(sign_in_trouble(&auth::SignInState { signed_in: false, ..out }, true, false), "");
    }

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

    /// A state with one chat ("c1") whose `day` was written holding `count` messages, the last at
    /// `last` (None: written before last messages were kept).
    fn written(day: &str, count: usize, last: Option<i64>) -> state::State {
        let mut st = state::State::default();
        st.mark_chat_day("c1", day, Some("sources/Teams. Chat. Maya - 2026-10-24.md"));
        st.set_chat_day_count("c1", day, count);
        if let Some(l) = last {
            st.set_chat_day_last("c1", day, l);
        }
        st
    }

    fn day(day: &str, count: usize, last: i64, partial: bool) -> Day<'_> {
        Day { conv: "c1", day, count, last, partial, first_seen: false }
    }

    #[test]
    fn a_day_cut_short_by_the_page_is_never_rewritten() {
        // 195 messages were written; the day now has 230 and the page holds its last 200.
        let st = written("2026-10-24", 195, Some(1000));
        let cut = day("2026-10-24", 200, 2000, true);
        assert_eq!(day_plan(&st, None, None, &cut), DayPlan::Leave, "rewriting would drop the first 30");
        // Read whole (the page before it was fetched), it is topped up.
        assert_eq!(day_plan(&st, None, None, &day("2026-10-24", 230, 2000, false)), DayPlan::TopUp);
        // So the pass reads on while the oldest day read has grown, and stops when it hasn't.
        assert!(!read_enough(&st, None, None, &day("2026-10-24", 200, 2000, false), 1));
        assert!(read_enough(&st, None, None, &day("2026-10-24", 200, 1000, false), 1));
    }

    #[test]
    fn a_day_grows_by_its_last_message_not_its_count() {
        // 230 written; 20 more came, and the page holds only the last 200: fewer than were written.
        let st = written("2026-10-24", 230, Some(1000));
        assert!(!read_enough(&st, None, None, &day("2026-10-24", 200, 2000, false), 1), "a later last message: grown");
        assert_eq!(day_plan(&st, None, None, &day("2026-10-24", 250, 2000, false)), DayPlan::TopUp);
        assert_eq!(day_plan(&st, None, None, &day("2026-10-24", 230, 1000, false)), DayPlan::Leave);
    }

    #[test]
    fn a_day_written_before_last_messages_were_kept_goes_by_its_count_then_adopts() {
        let st = written("2026-10-24", 5, None);
        assert_eq!(day_plan(&st, None, None, &day("2026-10-24", 6, 2000, false)), DayPlan::TopUp);
        assert_eq!(day_plan(&st, None, None, &day("2026-10-24", 5, 2000, false)), DayPlan::Adopt);
        assert_eq!(day_plan(&st, None, None, &day("2026-10-24", 5, 2000, true)), DayPlan::Leave, "cut short: not adopted");
        assert!(read_enough(&st, None, None, &day("2026-10-24", 3, 2000, false), 1), "fewer than written: only its end is in the page");
        // A day that filled the page and stuck there: the first page holds as many as were written,
        // so one more page is read to see the whole day, and no further on its account.
        let stuck = written("2026-10-24", 200, None);
        assert!(!read_enough(&stuck, None, None, &day("2026-10-24", 200, 2000, false), 1));
        assert!(read_enough(&stuck, None, None, &day("2026-10-24", 200, 2000, false), 2));
    }

    #[test]
    fn new_days_are_read_back_to_the_cutoff_or_the_back_fills_first_day() {
        let st = written("2026-10-20", 5, Some(1000));
        // A day not written yet: read on, in case it began before this page.
        assert!(!read_enough(&st, None, None, &day("2026-10-24", 50, 2000, false), 1));
        assert_eq!(
            day_plan(&st, None, None, &day("2026-10-24", 50, 2000, true)),
            DayPlan::Write,
            "the cap on pages: what was read is kept"
        );
        // A chat seen for the first time: back to the first-sync cutoff, or one page with no limit.
        let new = |d| Day { conv: "c2", first_seen: true, ..day(d, 50, 2000, false) };
        assert!(!read_enough(&st, None, Some("2026-10-22"), &new("2026-10-24"), 1));
        assert!(read_enough(&st, None, Some("2026-10-22"), &new("2026-10-21"), 1));
        assert_eq!(day_plan(&st, None, Some("2026-10-22"), &new("2026-10-21")), DayPlan::Skip);
        assert!(read_enough(&st, None, None, &new("2026-10-24"), 1));
        // A back-fill reads back past its first day, whatever is already written on the way.
        let from = Some("2026-10-18");
        assert!(!read_enough(&st, from, from, &day("2026-10-20", 5, 1000, false), 1));
        assert!(read_enough(&st, from, from, &day("2026-10-17", 5, 1000, false), 3));
        assert_eq!(day_plan(&st, from, from, &day("2026-10-17", 5, 1000, false)), DayPlan::Skip);
        // A day the first sync skipped (done, no file) is written by a back-fill and otherwise left.
        let mut skipped = state::State::default();
        skipped.mark_chat_day("c1", "2026-10-19", None);
        assert_eq!(day_plan(&skipped, from, from, &day("2026-10-19", 5, 1000, false)), DayPlan::Write);
        assert_eq!(day_plan(&skipped, None, None, &day("2026-10-19", 5, 1000, false)), DayPlan::Leave);
    }

    #[test]
    fn a_thread_grows_by_count_or_by_a_later_last_message() {
        let mut st = state::State::default();
        st.mark_thread("conv1", "sources/Email. Thread. Roadmap - 2026-10-24.md", 100);
        // From before last messages were kept (and stuck at the 100 one page held): the count decides.
        assert!(thread_grown(&st, "conv1", 130, 2000));
        assert!(!thread_grown(&st, "conv1", 100, 2000));
        st.set_thread_last("conv1", 1000);
        assert!(thread_grown(&st, "conv1", 100, 2000), "one deleted, one new: the same count, a later message");
        assert!(!thread_grown(&st, "conv1", 99, 1000));
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
