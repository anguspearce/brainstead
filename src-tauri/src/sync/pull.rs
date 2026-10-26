// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! On-demand pull (the `pull_teams_outlook` MCP tool): an assistant asks for the chats, mail and
//! meeting transcripts about some topic over the last few days, and this gathers them into the vault
//! and hands back the source paths to read. It reuses what the sync already pulled — an item already
//! in the vault is vended, not fetched again — and pulls only what's missing (D-20261025-04). Reads
//! only, like the rest of the sync; macOS and Windows (wherever there is a secret store).

use chrono::{Duration, Local, Utc};
use serde::Serialize;

use super::fetch::{self, ConvKind};
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
    pub problems: Vec<String>,
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
    let client = super::client();
    let mut st = state::load(data);
    let terms = terms(query);
    let now = Utc::now();
    let since = now - Duration::days(i64::from(days.max(1)));
    let cutoff_day = since.with_timezone(&Local).format("%Y-%m-%d").to_string();

    if kinds.chats {
        pull_chats(&client, &sign_in, vault, data, &s, &mut st, &terms, &cutoff_day, &mut out);
    }
    if kinds.mail {
        pull_mail(&client, vault, data, query, &terms, since, s.sync_mail_attachments, &mut st, &mut out);
    }
    if kinds.transcripts {
        pull_transcripts(&client, &sign_in, vault, data, &terms, since, now, &mut st, &mut out);
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

#[allow(clippy::too_many_arguments)]
fn pull_chats(
    client: &reqwest::blocking::Client,
    sign_in: &auth::SignIn,
    vault: &std::path::Path,
    data: &std::path::Path,
    s: &settings::Settings,
    st: &mut state::State,
    terms: &[String],
    cutoff_day: &str,
    out: &mut Outcome,
) {
    let convs = match fetch::conversations(client, sign_in) {
        Ok(c) => c,
        Err(e) => {
            out.problems.push(format!("Couldn't list chats: {e}"));
            return;
        }
    };
    let me = fetch::my_name(sign_in);
    for conv in convs {
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
                out.problems.push(format!("Couldn't read a chat: {e}"));
                continue;
            }
        };
        let partner = fetch::chat_display_name(&msgs, &me);
        // What to call this chat in the result: its topic, else the partner, else a generic name.
        let label = if !conv.topic.is_empty() {
            conv.topic.clone()
        } else if !partner.is_empty() {
            partner.clone()
        } else {
            "Teams chat".to_string()
        };
        for (day, day_msgs) in fetch::by_local_day(&msgs) {
            if day.as_str() < cutoff_day {
                continue;
            }
            // Does this day of this chat match the topic? (the chat's name, or any message in it)
            let text = format!("{}\n{}", conv.topic, day_msgs.iter().map(|m| m.body.as_str()).collect::<Vec<_>>().join("\n"));
            if !matches(&text, terms) {
                continue;
            }
            if st.chat_day_done(&conv.id, &day) && still_there(vault, st.chat_day_path(&conv.id, &day)) {
                out.items.push(Item {
                    kind: "chat".into(),
                    title: label.clone(),
                    date: day.clone(),
                    path: st.chat_day_path(&conv.id, &day).unwrap_or("").to_string(),
                    already_had: true,
                });
                continue;
            }
            if let Some(cap) = fetch::chat_capture(&conv, &day_msgs, &me, &sign_in.origin, &day, &partner) {
                let when = day_msgs.last().map(|m| m.time);
                if let Some(path) = deliver(data, vault, &cap, "teams", when, out) {
                    st.mark_chat_day(&conv.id, &day, Some(&path));
                    out.items.push(Item { kind: "chat".into(), title: label.clone(), date: day, path, already_had: false });
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn pull_mail(
    client: &reqwest::blocking::Client,
    vault: &std::path::Path,
    data: &std::path::Path,
    query: &str,
    terms: &[String],
    since: chrono::DateTime<Utc>,
    attachments: bool,
    st: &mut state::State,
    out: &mut Outcome,
) {
    // With a topic, search the mailbox for it; without, take the recent Inbox. Mail is grouped one
    // source per thread, so matching messages flag their conversation, then each thread is gathered.
    let mail = if query.trim().is_empty() { fetch::inbox(client, 60) } else { fetch::search_mail(client, query, 60) };
    let mail = match mail {
        Ok(m) => m,
        Err(e) => {
            out.problems.push(format!("Couldn't search mail: {e}"));
            return;
        }
    };
    let mut seen = std::collections::HashSet::new();
    for m in mail {
        if m.received < since || m.conversation_id.is_empty() || !seen.insert(m.conversation_id.clone()) {
            continue;
        }
        // $search already matched the thread; for an inbox listing without a query, matches() passes.
        if query.trim().is_empty() && !matches(&format!("{} {}", m.subject, m.from), terms) {
            continue;
        }
        // Already captured and still present → vend it; otherwise gather the whole conversation.
        if let Some(path) = st.thread_path(&m.conversation_id).filter(|p| still_there(vault, Some(p))).map(str::to_string) {
            let date = m.received.with_timezone(&Local).format("%Y-%m-%d").to_string();
            out.items.push(Item { kind: "email".into(), title: m.subject, date, path, already_had: true });
            continue;
        }
        let thread = match fetch::conversation(client, &m.conversation_id, 100) {
            Ok(t) => t,
            Err(e) => {
                out.problems.push(format!("Couldn't read a mail thread: {e}"));
                continue;
            }
        };
        let files = if attachments {
            super::attachments::for_thread(&super::attachments::Target { data, vault }, &thread, st, &mut out.problems)
        } else {
            Vec::new()
        };
        let Some(cap) = fetch::thread_capture(&thread, &files) else { continue };
        let when = thread.last().map(|x| x.received);
        let title = thread.last().map(|x| x.subject.clone()).unwrap_or(m.subject);
        let date = when.unwrap_or(m.received).with_timezone(&Local).format("%Y-%m-%d").to_string();
        if let Some(path) = deliver(data, vault, &cap, "outlook", when, out) {
            st.mark_thread(&m.conversation_id, &path, thread.len());
            out.items.push(Item { kind: "email".into(), title, date, path, already_had: false });
        }
    }
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
        let Some(id) = ev.thread_id.clone() else { continue };
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
}
