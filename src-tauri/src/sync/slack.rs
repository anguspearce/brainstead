// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Slack sync (D-20261118-01, D-20261118-07). The Slack extension hands over the open Slack tab's
//! sign-in (each workspace's client token, and the session cookie); it's kept in the login Keychain
//! (`platform::secret_*`), never a file. Each pass reads the user's own DMs and group DMs, the
//! channels they picked (Settings › Sync › Slack), and threads that mention them,
//! with Slack's own Web API, read-only, and writes one source per conversation per day
//! (`Slack. Chat. <name> - <day>.md`), each thread's replies under its first message, through the
//! shared capture path. A day that gains messages (a thread revived later) is rewritten in place.
//! Everything fails soft: a call that fails is reported and the pass carries on.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use brainstead_core::webcapture::{Attachment, Capture, Message, Thread};
use chrono::{DateTime, Local, TimeZone, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The Keychain account the sign-in is kept under.
const ACCOUNT: &str = "slack-sign-in";
/// How many days back a pass looks: new days, and days whose threads gained replies.
const LOOK_BACK_DAYS: i64 = 7;
/// The most conversations a pass reads, most recently active first, so Slack's rate limits hold.
const MOST_CONVERSATIONS: usize = 40;
/// The most messages a pass reads from one conversation's history (a page is 200).
const MOST_MESSAGES: usize = 1000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    /// `https://acme.slack.com/`
    pub url: String,
    /// The web client's token (`xoxc-…`).
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignIn {
    pub workspaces: Vec<Workspace>,
    /// The session cookie `d` (`xoxd-…`) the token works with.
    pub cookie: String,
    /// When it was handed over, epoch seconds.
    pub handed_over: i64,
}

static SLACK_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^https://[a-z0-9-]+(\.enterprise)?\.slack\.com/$").unwrap());

/// The sign-in a hand-over carries, checked: workspaces with a client token at a slack.com address,
/// and the session cookie.
pub fn from_handover(v: &Value, now: i64) -> Result<SignIn, String> {
    let cookie =
        v["cookie"].as_str().map(str::trim).filter(|c| c.starts_with("xoxd-")).ok_or("The Slack hand-over had no session cookie.")?;
    let workspaces: Vec<Workspace> = v["workspaces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| serde_json::from_value::<Workspace>(w.clone()).ok())
        .map(|mut w| {
            if !w.url.ends_with('/') {
                w.url.push('/');
            }
            w
        })
        .filter(|w| w.token.starts_with("xoxc-") && SLACK_URL.is_match(&w.url) && !w.id.is_empty())
        .collect();
    if workspaces.is_empty() {
        return Err("The Slack hand-over had no workspace sign-in.".into());
    }
    Ok(SignIn { workspaces, cookie: cookie.to_string(), handed_over: now })
}

/// Keeps a hand-over's sign-in (the native host calls this).
pub fn accept_handover(v: &Value) -> Result<(), String> {
    let s = from_handover(v, Utc::now().timestamp())?;
    crate::platform::secret_set(ACCOUNT, &serde_json::to_string(&s).map_err(|e| e.to_string())?)
}

pub fn load() -> Option<SignIn> {
    crate::platform::secret_get(ACCOUNT).and_then(|j| serde_json::from_str(&j).ok())
}

/// Forgets the sign-in (Settings' Sign out).
#[tauri::command]
pub fn slack_sign_out() -> Result<(), String> {
    crate::platform::secret_delete(ACCOUNT)
}

/// What Settings shows about the Slack sign-in. No secret.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInState {
    pub signed_in: bool,
    pub workspaces: Vec<String>,
    /// RFC 3339.
    pub handed_over: Option<String>,
}

#[tauri::command]
pub fn slack_sign_in_state() -> SignInState {
    match load() {
        Some(s) => SignInState {
            signed_in: true,
            workspaces: s.workspaces.iter().map(|w| if w.name.is_empty() { w.id.clone() } else { w.name.clone() }).collect(),
            handed_over: Utc.timestamp_opt(s.handed_over, 0).single().map(|t| t.to_rfc3339()),
        },
        None => SignInState { signed_in: false, workspaces: vec![], handed_over: None },
    }
}

// ── Slack's API ────────────────────────────────────────────────────────────────────────────────

/// One API call, as the web client makes it: the token in the form, the session cookie sent along.
fn call(
    client: &reqwest::blocking::Client,
    ws: &Workspace,
    cookie: &str,
    method: &str,
    params: &[(&str, String)],
) -> Result<Value, String> {
    let mut form: Vec<(&str, String)> = vec![("token", ws.token.clone())];
    form.extend(params.iter().cloned());
    // Rate limited: wait as long as Slack says (at most half a minute), up to three times, so a
    // back-fill's many calls get through; then it carries on next pass.
    let mut tries = 0;
    let resp = loop {
        let resp = client
            .post(format!("{}api/{method}", ws.url))
            .header("Cookie", format!("d={cookie}"))
            .form(&form)
            .send()
            .map_err(|e| format!("Slack {method}: {e}"))?;
        if resp.status().as_u16() != 429 {
            break resp;
        }
        tries += 1;
        if tries > 3 {
            return Err(format!("Slack {method}: rate limited, it carries on next time"));
        }
        let wait = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(5);
        std::thread::sleep(std::time::Duration::from_secs(wait.clamp(1, 30)));
    };
    let v: Value = resp.json().map_err(|e| format!("Slack {method}: {e}"))?;
    if v["ok"].as_bool() == Some(true) {
        Ok(v)
    } else {
        let why = v["error"].as_str().unwrap_or("refused");
        Err(match why {
            "invalid_auth" | "not_authed" | "token_revoked" => {
                "Slack's sign-in has lapsed: open Slack in the browser so the extension hands it over again.".into()
            }
            w => format!("Slack {method}: {w}"),
        })
    }
}

/// A conversation, as the pass needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Conv {
    pub id: String,
    /// `im`, `mpim` or `channel`.
    pub kind: String,
    /// The channel's name, or for a DM the other person's user id (named later).
    pub name: String,
    pub user: Option<String>,
    /// When it last changed, epoch seconds (0 when Slack didn't say).
    pub updated: i64,
}

pub fn parse_convs(v: &Value) -> Vec<Conv> {
    v["channels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let id = c["id"].as_str()?.to_string();
            let kind = if c["is_im"].as_bool() == Some(true) {
                "im"
            } else if c["is_mpim"].as_bool() == Some(true) {
                "mpim"
            } else {
                "channel"
            };
            if c["is_archived"].as_bool() == Some(true) {
                return None;
            }
            // `updated` is in milliseconds where Slack gives it.
            let updated = c["updated"].as_i64().map(|u| if u > 10_000_000_000 { u / 1000 } else { u }).unwrap_or(0);
            Some(Conv {
                id,
                kind: kind.into(),
                name: c["name"].as_str().unwrap_or("").to_string(),
                user: c["user"].as_str().map(str::to_string),
                updated,
            })
        })
        .collect()
}

/// A message, as the pass needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Msg {
    pub ts: String,
    pub user: String,
    pub text: String,
    /// The thread it starts or belongs to.
    pub thread_ts: Option<String>,
    pub reply_count: u64,
    pub files: Vec<String>,
}

pub fn parse_msgs(v: &Value) -> Vec<Msg> {
    v["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["subtype"].as_str().is_none_or(|s| matches!(s, "thread_broadcast" | "file_share" | "bot_message")))
        .filter_map(|m| {
            Some(Msg {
                ts: m["ts"].as_str()?.to_string(),
                user: m["user"].as_str().or(m["username"].as_str()).or(m["bot_id"].as_str()).unwrap_or("").to_string(),
                text: m["text"].as_str().unwrap_or("").to_string(),
                thread_ts: m["thread_ts"].as_str().map(str::to_string),
                reply_count: m["reply_count"].as_u64().unwrap_or(0),
                files: m["files"].as_array().into_iter().flatten().filter_map(|f| f["name"].as_str().map(str::to_string)).collect(),
            })
        })
        .collect()
}

/// A Slack timestamp (`1731920400.000200`) as a time.
pub fn time_of(ts: &str) -> Option<DateTime<Utc>> {
    let secs: i64 = ts.split('.').next()?.parse().ok()?;
    Utc.timestamp_opt(secs, 0).single()
}

static MENTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<@([A-Z0-9]+)(?:\|([^>]*))?>").unwrap());
static CHANNEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<#([A-Z0-9]+)(?:\|([^>]*))?>").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<(https?://[^|>]+)(?:\|([^>]*))?>").unwrap());
static SPECIAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<!(here|channel|everyone)(?:\|[^>]*)?>").unwrap());

/// Slack's markup as plain markdown: mentions and channels by name, links as links.
pub fn plain(text: &str, name: &dyn Fn(&str) -> String) -> String {
    let t =
        MENTION.replace_all(text, |c: &regex::Captures| format!("@{}", c.get(2).map_or_else(|| name(&c[1]), |m| m.as_str().to_string())));
    let t = CHANNEL.replace_all(&t, |c: &regex::Captures| format!("#{}", c.get(2).map_or(&c[1], |m| m.as_str())));
    let t = LINK.replace_all(&t, |c: &regex::Captures| match c.get(2) {
        Some(label) => format!("[{}]({})", label.as_str(), &c[1]),
        None => c[1].to_string(),
    });
    let t = SPECIAL.replace_all(&t, "@$1");
    t.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// A conversation's day as a capture: its top-level messages in order, each thread's replies
/// under its first message. `day`'s messages are those whose own time falls on it (local).
pub fn day_capture(
    title: &str,
    url: &str,
    day: &str,
    tops: &[Msg],
    replies: &HashMap<String, Vec<Msg>>,
    name: &dyn Fn(&str) -> String,
) -> Option<Capture> {
    let stamp = |ts: &str| time_of(ts).map(|t| t.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string());
    let msg = |m: &Msg| Message {
        sender: if m.user.is_empty() { "Unknown".into() } else { name(&m.user) },
        from_address: None,
        to: None,
        cc: None,
        recipients: None,
        date: stamp(&m.ts),
        body: plain(&m.text, name),
        attachments: m.files.iter().map(|f| Attachment { name: f.clone(), note: Some("in Slack".into()), ..Default::default() }).collect(),
        branch: None,
        replies: Vec::new(),
    };
    let messages: Vec<Message> = tops
        .iter()
        .map(|m| {
            let mut out = msg(m);
            if let Some(rs) = replies.get(&m.ts) {
                out.replies = rs.iter().filter(|r| r.ts != m.ts).map(msg).collect();
            }
            out
        })
        .collect();
    if messages.is_empty() {
        return None;
    }
    Some(Capture::Slack(Thread {
        url: url.to_string(),
        title: title.to_string(),
        kind: Some("chat".into()),
        messages,
        meeting_date: Some(day.to_string()),
        meeting_start: None,
        continues: None,
    }))
}

/// How many messages a day's capture holds, replies included: a day that grows is rewritten.
pub fn count(c: &Capture) -> usize {
    match c {
        Capture::Slack(t) => t.messages.iter().map(|m| 1 + m.replies.len()).sum(),
        _ => 0,
    }
}

/// Top-level messages by their local day, oldest first.
pub fn by_day(msgs: &[Msg]) -> BTreeMap<String, Vec<Msg>> {
    let mut out: BTreeMap<String, Vec<Msg>> = BTreeMap::new();
    let mut tops: Vec<&Msg> = msgs.iter().filter(|m| m.thread_ts.as_deref().is_none_or(|t| t == m.ts)).collect();
    tops.sort_by(|a, b| a.ts.partial_cmp(&b.ts).unwrap_or(std::cmp::Ordering::Equal));
    for m in tops {
        if let Some(t) = time_of(&m.ts) {
            out.entry(t.with_timezone(&Local).format("%Y-%m-%d").to_string()).or_default().push(m.clone());
        }
    }
    out
}

// ── Progress ───────────────────────────────────────────────────────────────────────────────────

/// What's been written, in the app data folder: each conversation's day's source and how many
/// messages it held.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Progress {
    /// `"<conv>\n<day>"` → the source's path.
    pub paths: BTreeMap<String, String>,
    pub counts: BTreeMap<String, usize>,
    /// Conversations seen before (the first-sync limit applies only the first time).
    pub seen: Vec<String>,
    /// Each conversation's `updated` when it was last read, and when that was (epoch seconds): a
    /// pass reads what changed first, then what's waited longest, so every conversation gets its
    /// turn however many there are.
    pub updated_read: BTreeMap<String, i64>,
    pub read_at: BTreeMap<String, i64>,
}

/// Which conversations a pass reads, at most `limit` (D-20261118-19): those changed in the window
/// since they were last read (most recent first), then those Slack gives no recent time for, then
/// the unchanged ones, each of the last two longest-unread first. Slack's `updated` only orders
/// them: a DM's isn't when it last had a message, so none is left out for an old one.
pub fn pick(convs: &[Conv], prog: &Progress, since_for: impl Fn(&str) -> i64, limit: usize) -> Vec<Conv> {
    let mut in_window: Vec<&Conv> = convs.iter().collect();
    let read_at = |c: &Conv| prog.read_at.get(&c.id).copied().unwrap_or(0);
    let recent = |c: &Conv| c.updated > 0 && c.updated >= since_for(&c.id);
    let changed = |c: &Conv| recent(c) && prog.updated_read.get(&c.id).is_none_or(|u| *u < c.updated);
    in_window.sort_by_key(|c| {
        let rank = if changed(c) {
            0
        } else if !recent(c) {
            1
        } else {
            2
        };
        // Changed: newest first; the rest: longest unread first.
        let order = if rank == 0 { -c.updated } else { read_at(c) };
        (rank, order)
    });
    in_window.into_iter().take(limit).cloned().collect()
}

fn progress_file(data: &Path) -> PathBuf {
    data.join("slack-sync.json")
}

pub fn progress(data: &Path) -> Progress {
    std::fs::read(progress_file(data)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_progress(data: &Path, p: &Progress) -> Result<(), String> {
    let b = serde_json::to_vec_pretty(p).map_err(|e| e.to_string())?;
    brainstead_core::write::write_atomic(&progress_file(data), &b, false).map_err(|e| e.to_string())
}

/// The conversation a Slack day's source came from (for skip suggestions).
pub fn conv_of(data: &Path, rel: &str) -> Option<String> {
    progress(data).paths.iter().find(|(_, p)| p.as_str() == rel).and_then(|(k, _)| k.split('\n').next()).map(str::to_string)
}

// ── A pass ─────────────────────────────────────────────────────────────────────────────────────

/// One Slack pass over every handed-over workspace. Adds to `r`.
/// With `from`, a back-fill (D-20261118-23): every conversation from that day on.
pub fn sync(
    data: &Path,
    vault: &Path,
    s: &crate::settings::Settings,
    cutoff: Option<&str>,
    from: Option<chrono::NaiveDate>,
    r: &mut super::run::Report,
) {
    let Some(sign_in) = load() else {
        r.problems.push("Slack isn't signed in: open Slack in the browser with the Slack extension loaded.".into());
        return;
    };
    let client = super::client();
    let mut prog = progress(data);
    for ws in &sign_in.workspaces {
        if let Err(e) = workspace(&client, ws, &sign_in.cookie, data, vault, s, (cutoff, from), &mut prog, r) {
            r.problems.push(format!("{}: {e}", if ws.name.is_empty() { &ws.id } else { &ws.name }));
        }
    }
    if let Err(e) = save_progress(data, &prog) {
        r.problems.push(format!("Couldn't save Slack sync progress: {e}"));
    }
}

#[allow(clippy::too_many_arguments)]
fn workspace(
    client: &reqwest::blocking::Client,
    ws: &Workspace,
    cookie: &str,
    data: &Path,
    vault: &Path,
    s: &crate::settings::Settings,
    (cutoff, from): (Option<&str>, Option<chrono::NaiveDate>),
    prog: &mut Progress,
    r: &mut super::run::Report,
) -> Result<(), String> {
    let me = call(client, ws, cookie, "auth.test", &[])?["user_id"].as_str().unwrap_or("").to_string();
    let local_midnight =
        |d: chrono::NaiveDate| d.and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest()).map(|t| t.with_timezone(&Utc));
    // A back-fill looks back to its first day, for every conversation.
    let back_to = from.and_then(local_midnight);
    let since = back_to.unwrap_or_else(|| Utc::now() - chrono::Duration::days(LOOK_BACK_DAYS));
    // A conversation seen for the first time starts as far back as History on first sync says
    // (everything for 0), never less than the usual week.
    let first_since = match cutoff.and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
        Some(d) => {
            d.and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest()).map_or(since, |t| t.with_timezone(&Utc)).min(since)
        }
        None => DateTime::<Utc>::UNIX_EPOCH,
    };
    let first_since = back_to.unwrap_or(first_since);
    // Those seen before this pass, so marking one seen below doesn't change its start mid-pass.
    let seen_before: Vec<String> = prog.seen.clone();
    let since_for = |id: &str| if seen_before.iter().any(|s| s == id) { since } else { first_since };

    // People's names, asked once each.
    let names: std::cell::RefCell<HashMap<String, String>> = Default::default();
    let name_of = |id: &str| -> String {
        if let Some(n) = names.borrow().get(id) {
            return n.clone();
        }
        let n = call(client, ws, cookie, "users.info", &[("user", id.to_string())])
            .ok()
            .and_then(|v| {
                let p = &v["user"]["profile"];
                [p["display_name"].as_str(), p["real_name"].as_str(), v["user"]["name"].as_str()]
                    .into_iter()
                    .flatten()
                    .find(|x| !x.trim().is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| id.to_string());
        names.borrow_mut().insert(id.to_string(), n.clone());
        n
    };

    // The conversations: DMs and group DMs, and the channels picked; most recently active first.
    let mut convs: Vec<Conv> = Vec::new();
    let mut cursor = String::new();
    loop {
        let mut params = vec![
            ("types", "im,mpim,public_channel,private_channel".to_string()),
            ("limit", "200".to_string()),
            ("exclude_archived", "true".to_string()),
        ];
        if !cursor.is_empty() {
            params.push(("cursor", cursor.clone()));
        }
        let v = call(client, ws, cookie, "users.conversations", &params)?;
        convs.extend(parse_convs(&v));
        cursor = v["response_metadata"]["next_cursor"].as_str().unwrap_or("").to_string();
        if cursor.is_empty() || convs.len() > 1000 {
            break;
        }
    }
    let wanted = |c: &Conv| (c.kind != "channel" || s.slack_channels.contains(&c.id)) && !s.sync_ignore_chats.contains(&c.id);
    let wanted_convs: Vec<Conv> = convs.iter().filter(|c| wanted(c)).cloned().collect();
    let mut chosen: Vec<Conv> =
        pick(&wanted_convs, prog, |id| since_for(id).timestamp(), if from.is_some() { usize::MAX } else { MOST_CONVERSATIONS });

    // Threads that mention the user in channels not picked: those threads alone.
    let mut only: HashMap<String, Vec<String>> = HashMap::new();
    if !me.is_empty() {
        let q = format!("<@{me}> after:{}", (since - chrono::Duration::days(1)).format("%Y-%m-%d"));
        if let Ok(v) = call(client, ws, cookie, "search.messages", &[("query", q), ("count", "40".into()), ("sort", "timestamp".into())]) {
            for m in v["messages"]["matches"].as_array().into_iter().flatten() {
                let Some(ch) = m["channel"]["id"].as_str() else { continue };
                if s.sync_ignore_chats.iter().any(|x| x == ch) {
                    continue;
                }
                // One of the user's own conversations (a DM, a group DM, a picked channel): read it
                // whole, this pass, not just the thread that names them.
                if let Some(c) = wanted_convs.iter().find(|c| c.id == ch) {
                    if !chosen.iter().any(|x| x.id == ch) {
                        chosen.push(c.clone());
                    }
                    continue;
                }
                // A channel they didn't pick: only the threads that name them, every one of them.
                if chosen.iter().any(|c| c.id == ch) && !only.contains_key(ch) {
                    continue;
                }
                // The thread's first message: from the permalink's thread_ts, else the message itself.
                let ts = m["permalink"]
                    .as_str()
                    .and_then(|p| p.split("thread_ts=").nth(1))
                    .map(|t| t.split('&').next().unwrap_or(t).to_string());
                let Some(ts) = ts.or_else(|| m["ts"].as_str().map(str::to_string)) else { continue };
                only.entry(ch.to_string()).or_default().push(ts);
                if !chosen.iter().any(|c| c.id == ch) {
                    if let Some(c) = convs.iter().find(|c| c.id == ch) {
                        chosen.push(c.clone());
                    } else {
                        chosen.push(Conv {
                            id: ch.into(),
                            kind: "channel".into(),
                            name: m["channel"]["name"].as_str().unwrap_or(ch).to_string(),
                            user: None,
                            updated: 0,
                        });
                    }
                }
            }
        }
    }

    for c in &chosen {
        let first = !prog.seen.contains(&c.id);
        let oldest = format!("{}.000000", since_for(&c.id).timestamp().max(0));
        let oldest_day = since_for(&c.id).with_timezone(&Local).format("%Y-%m-%d").to_string();
        // Page by page, up to MOST_MESSAGES, so a busy week isn't cut at Slack's page size.
        let mut msgs: Vec<Msg> = Vec::new();
        let mut cursor = String::new();
        let mut more = false;
        let mut failed = None;
        loop {
            let mut params = vec![("channel", c.id.clone()), ("oldest", oldest.clone()), ("limit", "200".to_string())];
            if !cursor.is_empty() {
                params.push(("cursor", cursor.clone()));
            }
            match call(client, ws, cookie, "conversations.history", &params) {
                Ok(v) => {
                    msgs.extend(parse_msgs(&v));
                    more = v["has_more"].as_bool() == Some(true);
                    cursor = v["response_metadata"]["next_cursor"].as_str().unwrap_or("").to_string();
                    if !more || cursor.is_empty() || msgs.len() >= MOST_MESSAGES {
                        break;
                    }
                }
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        // A later page that failed (a rate limit): what was read is still written, but the
        // conversation isn't marked read, so the next pass reads it again for the rest.
        let partial = failed.is_some();
        if let Some(e) = failed {
            r.problems.push(e);
            if msgs.is_empty() {
                continue;
            }
        }
        if !partial {
            prog.updated_read.insert(c.id.clone(), c.updated);
            prog.read_at.insert(c.id.clone(), Utc::now().timestamp());
        }
        // Counts only, never what was said: enough to see where messages go missing.
        crate::applog!(
            "slack: {} {} from {}: {} message(s){}{}",
            c.kind,
            &c.id[..c.id.len().min(4)],
            &oldest_day,
            msgs.len(),
            if more { ", more not read" } else { "" },
            if only.contains_key(&c.id) { ", mentions only" } else { "" }
        );
        let tops: Vec<Msg> = match only.get(&c.id) {
            Some(threads) => msgs.into_iter().filter(|m| threads.contains(&m.ts)).collect(),
            None => msgs,
        };
        let title = match c.kind.as_str() {
            "im" => c.user.as_deref().map(name_of).unwrap_or_else(|| c.name.clone()),
            "mpim" => c.name.trim_start_matches("mpdm-").trim_end_matches("-1").replace("--", ", "),
            _ => format!("#{}", c.name),
        };
        let url = format!("{}archives/{}", ws.url, c.id);
        for (day, day_tops) in by_day(&tops) {
            if first && from.is_none() && cutoff.is_some_and(|cut| day.as_str() < cut) {
                continue;
            }
            let mut replies: HashMap<String, Vec<Msg>> = HashMap::new();
            for m in day_tops.iter().filter(|m| m.reply_count > 0) {
                match call(
                    client,
                    ws,
                    cookie,
                    "conversations.replies",
                    &[("channel", c.id.clone()), ("ts", m.ts.clone()), ("limit", "200".into())],
                ) {
                    Ok(v) => {
                        replies.insert(m.ts.clone(), parse_msgs(&v));
                    }
                    Err(e) => r.problems.push(e),
                }
            }
            let Some(cap) = day_capture(&title, &url, &day, &day_tops, &replies, &name_of) else { continue };
            let key = format!("{}\n{day}", c.id);
            let n = count(&cap);
            let when = day_tops.last().and_then(|m| time_of(&m.ts));
            match prog.paths.get(&key).cloned() {
                // Written, and nothing new since.
                Some(_) if prog.counts.get(&key).copied().unwrap_or(0) >= n => {}
                // Grown (a thread revived): rewritten in place. One that's gone starts afresh.
                Some(path) if vault.join(&path).is_file() => match brainstead_core::webcapture::rewrite(vault, &path, &cap, Local::now()) {
                    Ok(()) => {
                        prog.counts.insert(key, n);
                        r.chats += 1;
                        r.topped_up.push(path.clone());
                        r.paths.push(path);
                    }
                    Err(e) => r.problems.push(e.to_string()),
                },
                _ => match crate::nativehost::deliver(data, Some(vault), false, "slack", &cap, true, when.map(|t| t.to_rfc3339())) {
                    Ok(path) => {
                        r.paths.push(path.clone());
                        prog.paths.insert(key.clone(), path);
                        prog.counts.insert(key, n);
                        r.chats += 1;
                    }
                    Err(e) => r.problems.push(e),
                },
            }
        }
        if first && !partial {
            prog.seen.push(c.id.clone());
        }
    }
    Ok(())
}

/// The channels the user is in, for Settings to pick from: (id, name, workspace).
#[derive(Debug, Clone, Serialize)]
pub struct ChannelRow {
    id: String,
    name: String,
    workspace: String,
}

#[tauri::command]
pub async fn slack_channels() -> Result<Vec<ChannelRow>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let s = load().ok_or("Slack isn't signed in.")?;
        let client = super::client();
        let mut out = Vec::new();
        for ws in &s.workspaces {
            let v = call(
                &client,
                ws,
                &s.cookie,
                "users.conversations",
                &[("types", "public_channel,private_channel".into()), ("limit", "500".into()), ("exclude_archived", "true".into())],
            )?;
            for c in parse_convs(&v).into_iter().filter(|c| c.kind == "channel") {
                out.push(ChannelRow {
                    id: c.id,
                    name: format!("#{}", c.name),
                    workspace: if ws.name.is_empty() { ws.id.clone() } else { ws.name.clone() },
                });
            }
        }
        out.sort_by(|a, b| a.workspace.cmp(&b.workspace).then_with(|| a.name.cmp(&b.name)));
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_handover_keeps_only_slack_workspaces_with_a_client_token() {
        let v = json!({"cookie": "xoxd-abc", "workspaces": [
            {"id": "T1", "name": "Acme", "url": "https://acme.slack.com", "token": "xoxc-1"},
            {"id": "T2", "name": "Bad", "url": "https://evil.example/", "token": "xoxc-2"},
            {"id": "T3", "name": "Bot", "url": "https://acme.slack.com/", "token": "xoxb-3"}
        ]});
        let s = from_handover(&v, 100).unwrap();
        assert_eq!(s.workspaces.iter().map(|w| (w.id.as_str(), w.url.as_str())).collect::<Vec<_>>(), [("T1", "https://acme.slack.com/")]);
        assert!(from_handover(&json!({"cookie": "nope", "workspaces": v["workspaces"]}), 1).unwrap_err().contains("cookie"));
        assert!(from_handover(&json!({"cookie": "xoxd-a", "workspaces": []}), 1).unwrap_err().contains("workspace"));
    }

    #[test]
    fn slack_markup_reads_as_plain_markdown() {
        let name = |id: &str| if id == "U1" { "Lena".to_string() } else { id.to_string() };
        assert_eq!(
            plain("<@U1> can you check <https://x.example/a|the doc> in <#C9|launch>? <!here> &amp; thanks <https://y.example>", &name),
            "@Lena can you check [the doc](https://x.example/a) in #launch? @here & thanks https://y.example"
        );
    }

    #[test]
    fn a_day_holds_its_messages_with_each_thread_under_its_first() {
        // Two top-level messages on one day, the first with a thread whose reply came two days later.
        let day1 = Local.with_ymd_and_hms(2026, 11, 16, 9, 0, 0).unwrap().timestamp();
        let ts = |s: i64| format!("{s}.000100");
        let history = json!({"ok": true, "messages": [
            {"ts": ts(day1 + 600), "user": "U2", "text": "Lunch?"},
            {"ts": ts(day1), "user": "U1", "text": "Can <@U2> send the pricing?", "thread_ts": ts(day1), "reply_count": 1},
            {"ts": ts(day1 + 60), "user": "U2", "text": "a reply shown in channel", "subtype": "channel_join"}
        ]});
        let tops = parse_msgs(&history);
        let days = by_day(&tops);
        assert_eq!(days.keys().collect::<Vec<_>>(), ["2026-11-16"]);
        let first = &days["2026-11-16"];
        assert_eq!(first.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["Can <@U2> send the pricing?", "Lunch?"]);
        let replies = parse_msgs(&json!({"messages": [
            {"ts": ts(day1), "user": "U1", "text": "Can <@U2> send the pricing?", "thread_ts": ts(day1), "reply_count": 1},
            {"ts": ts(day1 + 2 * 86400), "user": "U2", "text": "Sent!", "thread_ts": ts(day1), "files": [{"name": "Pricing.pdf"}]}
        ]}));
        let mut threads = HashMap::new();
        threads.insert(ts(day1), replies);
        let name = |id: &str| match id {
            "U1" => "Sam".to_string(),
            "U2" => "Lena".to_string(),
            x => x.to_string(),
        };
        let cap = day_capture("Lena", "https://acme.slack.com/archives/D1", "2026-11-16", first, &threads, &name).unwrap();
        assert_eq!(count(&cap), 3);
        let (file, text) = brainstead_core::webcapture::render(&cap, Utc::now(), "2026-11-18");
        assert_eq!(file, "Slack. Chat. Lena - 2026-11-16.md");
        assert!(text.contains("**Source:** [Open in Slack](https://acme.slack.com/archives/D1)"), "{text}");
        let sam = text.find("## Sam — 2026-11-16 09:00\n\nCan @Lena send the pricing?").unwrap();
        let reply = text.find("### ↳ Lena — 2026-11-18 09:00\n\nSent!").unwrap();
        let lunch = text.find("## Lena — 2026-11-16 09:10\n\nLunch?").unwrap();
        assert!(sam < reply && reply < lunch, "{text}");
        assert!(text.contains("Pricing.pdf"), "{text}");
    }

    #[test]
    fn every_conversation_gets_its_turn_however_many_there_are() {
        let conv = |id: &str, updated: i64| Conv { id: id.into(), kind: "im".into(), name: String::new(), user: None, updated };
        // 50 conversations active this week, nothing read yet: the 40 most recent.
        let convs: Vec<Conv> = (0..50).map(|i| conv(&format!("D{i:02}"), 1_000 + i)).collect();
        let mut prog = Progress::default();
        let first = pick(&convs, &prog, |_| 0, 40);
        assert_eq!((first.len(), first[0].id.as_str(), first[39].id.as_str()), (40, "D49", "D10"));
        // Read them; the next pass takes the 10 left, then the longest-unread of the rest.
        for (i, c) in first.iter().enumerate() {
            prog.updated_read.insert(c.id.clone(), c.updated);
            prog.read_at.insert(c.id.clone(), 5_000 + i as i64);
        }
        let next = pick(&convs, &prog, |_| 0, 40);
        assert_eq!(
            next[..10].iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            (0..10).rev().map(|i| format!("D{i:02}")).collect::<Vec<_>>()
        );
        assert_eq!(next[10].id, "D49", "then the one read longest ago");
        // A conversation that changes goes first; one with no time comes before the unchanged.
        let mut convs = convs;
        convs[5].updated = 9_000;
        convs.push(conv("G1", 0));
        let again = pick(&convs, &prog, |_| 0, 40);
        assert_eq!((again[0].id.as_str(), again[1].id.as_str()), ("D05", "D09"));
        assert!(again.iter().position(|c| c.id == "G1").unwrap() < again.iter().position(|c| c.id == "D49").unwrap());
        // A DM whose `updated` is old (it isn't when it last had a message) is still read, after
        // the ones that changed.
        let old_dm = pick(&[conv("D99", 10), conv("D98", 500)], &Progress::default(), |_| 100, 40);
        assert_eq!(old_dm.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["D98", "D99"]);
    }

    #[test]
    fn conversations_and_when_they_changed() {
        let v = json!({"channels": [
            {"id": "D1", "is_im": true, "user": "U2", "updated": 1_731_920_400_000_i64},
            {"id": "G1", "is_mpim": true, "name": "mpdm-sam--lena--maya-1"},
            {"id": "C1", "name": "launch", "updated": 1_731_920_400},
            {"id": "C2", "name": "old", "is_archived": true}
        ]});
        let c = parse_convs(&v);
        assert_eq!(
            c.iter().map(|x| (x.id.as_str(), x.kind.as_str(), x.updated)).collect::<Vec<_>>(),
            [("D1", "im", 1_731_920_400), ("G1", "mpim", 0), ("C1", "channel", 1_731_920_400)]
        );
    }
}
