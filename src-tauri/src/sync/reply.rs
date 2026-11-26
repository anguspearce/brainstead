// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Draft reply's Put in Outlook and Put in Teams (D-20261120-05): the reply, unsent, where the user
//! sends it from. Never a send.
//!
//! - An email thread: a reply-all draft on the thread's latest message, made with Outlook REST and
//!   the mail sign-in, with the reply above the quoted history. Its web link opens it in Outlook.
//! - A Teams chat: the reply is left in the app data folder's `reply-drafts/` under a one-time key,
//!   and the chat's link is opened with that key on it. The Teams extension sees the link open,
//!   collects the reply through the native host ([`take_teams`]) and types it into the chat's
//!   message box, where it waits for the user to press Send.

use std::path::{Path, PathBuf};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::auth::Service;
use super::{api, fetch, state};

/// Where a reply went: `outlook` (a draft, `url` opens it) or `teams` (`url` opens the chat, and the
/// extension fills its box).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Placed {
    pub to: String,
    pub url: String,
}

/// The thread's kind from its path, as Draft reply's picker tells them apart.
fn kind_of(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.starts_with("Email. ") {
        Some("outlook")
    } else if name.starts_with("Teams. Chat. ") {
        Some("teams")
    } else {
        None
    }
}

/// The links on a capture's `**Source:**` lines, in order (a forked thread has one per branch).
pub fn source_links(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|l| l.trim().strip_prefix("**Source:** ["))
        .filter_map(|rest| rest.split_once("](").map(|(_, u)| u))
        .filter_map(|u| u.split_once(')').map(|(u, _)| u.trim().to_string()))
        .filter(|u| u.starts_with("https://"))
        .collect()
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(n) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(n);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The Outlook REST id of the message an Outlook link opens: its `ItemID=` (the sync's links) or its
/// `/id/` segment (a capture from the Outlook tab). Links carry the EWS form of the id; REST takes the
/// same id with `/` and `+` as `-` and `_`.
pub fn message_id(url: &str) -> Option<String> {
    let raw = if let Some((_, q)) = url.split_once("ItemID=") {
        q.split('&').next()?.to_string()
    } else {
        let (_, rest) = url.split_once("/id/")?;
        rest.split(['?', '#', '/']).next()?.to_string()
    };
    let id = decode(&raw).replace('/', "-").replace('+', "_");
    (!id.is_empty()).then_some(id)
}

/// The Teams conversation id a synced chat's link opens (`…/_#/conversations/<id>`).
pub fn conversation_id(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("/conversations/")?;
    let id = decode(rest.split(['?', '&', '/']).next()?);
    id.contains(':').then_some(id)
}

/// The reply as HTML paragraphs, for a draft's body and the Teams box.
pub fn reply_html(text: &str) -> String {
    let esc = |p: &str| p.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    text.trim().split("\n\n").map(str::trim).filter(|p| !p.is_empty()).map(|p| format!("<p>{}</p>", esc(p).replace('\n', "<br>"))).collect()
}

/// `body` (a reply draft's HTML, the quoted history in it) with the reply put first, inside `<body>`.
pub fn with_reply(body: &str, reply: &str) -> String {
    let lower = body.to_ascii_lowercase();
    match lower.find("<body").and_then(|at| lower[at..].find('>').map(|end| at + end + 1)) {
        Some(at) => format!("{}{reply}{}", &body[..at], &body[at..]),
        None => format!("{reply}{body}"),
    }
}

/// The latest message of an Outlook conversation that isn't a draft (the thread's own, in any folder).
fn latest_in(client: &Client, conv_id: &str) -> Result<Option<String>, String> {
    let filter = format!("ConversationId eq '{}'", conv_id.replace('\'', "''"));
    let url = format!(
        "https://outlook.office.com/api/v2.0/me/messages?$filter={}&$top=100&$select=Id,ReceivedDateTime,IsDraft",
        fetch::urlencode(&filter)
    );
    let body = api::get_json(client, Service::Outlook, &url)?;
    let mut msgs: Vec<(&str, &str)> = body["value"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["IsDraft"].as_bool() != Some(true))
        .filter_map(|m| Some((m["ReceivedDateTime"].as_str()?, m["Id"].as_str()?)))
        .collect();
    msgs.sort();
    Ok(msgs.last().map(|(_, id)| id.to_string()))
}

/// Makes the reply-all draft on `message` with `text` above the history; its web link.
fn outlook_draft(client: &Client, message: &str, text: &str) -> Result<String, String> {
    let base = "https://outlook.office.com/api/v2.0/me/messages";
    let draft = api::write_json(
        client,
        Service::Outlook,
        reqwest::Method::POST,
        &format!("{base}/{}/createreplyall", fetch::urlencode(message)),
        &json!({}),
    )
    .map_err(|e| format!("Outlook didn't make the draft: {e}"))?;
    let id = draft["Id"].as_str().ok_or("Outlook made no draft.")?;
    let history = draft["Body"]["Content"].as_str().unwrap_or("");
    let html = draft["Body"]["ContentType"].as_str().is_none_or(|t| t.eq_ignore_ascii_case("html"));
    let content = if html { with_reply(history, &reply_html(text)) } else { format!("{}\n\n{history}", text.trim()) };
    let kind = if html { "HTML" } else { "Text" };
    let saved = api::write_json(
        client,
        Service::Outlook,
        reqwest::Method::PATCH,
        &format!("{base}/{}", fetch::urlencode(id)),
        &json!({"Body": {"ContentType": kind, "Content": content}}),
    )
    .map_err(|e| format!("Outlook made the draft but didn't take the reply: {e}. It's in Drafts, empty"))?;
    Ok(saved["WebLink"].as_str().or(draft["WebLink"].as_str()).unwrap_or("https://outlook.office.com/mail/drafts").to_string())
}

/// One reply waiting for the Teams extension.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Waiting {
    pub conversation: String,
    pub text: String,
    pub html: String,
    /// Seconds since 1970 it was left; the extension can collect it for [`KEEP_SECS`].
    pub at: i64,
}

/// How long a reply waits for the Teams extension to collect it.
pub const KEEP_SECS: i64 = 10 * 60;

fn waiting_dir(data: &Path) -> PathBuf {
    data.join("reply-drafts")
}

/// The one-time key a waiting reply is kept under: letters and digits only, so it's safe as a file
/// name and in a link.
fn fresh_key() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{n:x}{:x}", std::process::id())
}

fn valid_key(k: &str) -> bool {
    !k.is_empty() && k.len() <= 64 && k.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Leaves `text` for the Teams extension and gives the link that opens the chat with its key.
pub fn leave_for_teams(data: &Path, url: &str, conversation: &str, text: &str, now: i64) -> Result<String, String> {
    let dir = waiting_dir(data);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Old ones nobody collected go.
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let old =
            std::fs::read(e.path()).ok().and_then(|b| serde_json::from_slice::<Waiting>(&b).ok()).is_none_or(|w| now - w.at > KEEP_SECS);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let key = fresh_key();
    let w = Waiting { conversation: conversation.to_string(), text: text.trim().to_string(), html: reply_html(text), at: now };
    std::fs::write(dir.join(format!("{key}.json")), serde_json::to_vec(&w).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(teams_link(url, &key))
}

/// The chat's link with the key on it, before the `#` route so Teams' routing keeps the chat.
pub fn teams_link(url: &str, key: &str) -> String {
    let (before, route) = url.split_once('#').map(|(a, b)| (a, Some(b))).unwrap_or((url, None));
    let sep = if before.contains('?') { '&' } else { '?' };
    match route {
        Some(r) => format!("{before}{sep}brainstead-reply={key}#{r}"),
        None => format!("{before}{sep}brainstead-reply={key}"),
    }
}

/// The native host's `reply-draft` (from the Teams extension only): the reply left under `key`,
/// taken so it's typed in once.
pub fn take_teams(data: &Path, key: &str, now: i64) -> Value {
    if !valid_key(key) {
        return json!({"ok": false, "message": "That isn't a reply Brainstead left."});
    }
    let file = waiting_dir(data).join(format!("{key}.json"));
    let w = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Waiting>(&b).ok());
    let _ = std::fs::remove_file(&file);
    match w {
        Some(w) if now - w.at <= KEEP_SECS => json!({"ok": true, "conversation": w.conversation, "text": w.text, "html": w.html}),
        Some(_) => json!({"ok": false, "message": "That reply waited too long: put it in Teams again from Draft reply."}),
        None => json!({"ok": false, "message": "Brainstead has no reply waiting for this link (it was typed in already)."}),
    }
}

/// Puts `text` where the thread at `path` (in `vault`) is answered from, unsent.
pub fn place(data: &Path, vault: &Path, path: &str, text: &str, now: i64) -> Result<Placed, String> {
    if text.trim().is_empty() {
        return Err("There's no reply to put there.".into());
    }
    if !path.starts_with("sources/") || path.split('/').any(|p| p == "..") {
        return Err("Only a captured thread in sources/ can be answered there.".into());
    }
    let kind = kind_of(path).ok_or("This isn't an email or a Teams chat: copy the reply instead.")?;
    let content = std::fs::read_to_string(vault.join(path)).map_err(|_| format!("{path} isn't there any more."))?;
    let links = source_links(&content);
    if kind == "teams" {
        let url = links.last().ok_or("This chat has no link to Teams: copy the reply instead.")?;
        let conv = conversation_id(url)
            .ok_or("Brainstead can't tell which Teams chat this was (it was captured by hand): copy the reply instead.")?;
        let link = leave_for_teams(data, url, &conv, text, now)?;
        return Ok(Placed { to: "teams".into(), url: link });
    }
    let client = super::client();
    // A synced thread: its conversation, from what the sync wrote, so the reply goes on its latest message.
    let st = state::load(data);
    let conv = st.thread_paths.iter().find(|(_, p)| p.as_str() == path).map(|(c, _)| c.clone());
    let message = match conv {
        Some(c) => latest_in(&client, &c)?,
        None => None,
    }
    .or_else(|| links.iter().rev().find_map(|u| message_id(u)))
    .ok_or("Brainstead can't tell which Outlook message this was: copy the reply instead.")?;
    let url = outlook_draft(&client, &message, text)?;
    Ok(Placed { to: "outlook".into(), url })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_links_and_ids() {
        let c = "# Re launch\n\n**Source:** [Open in Outlook](https://outlook.office365.com/owa/?ItemID=AAMk%2Ba%2Fb%3D&exvsurl=1)\n\nbody";
        let l = source_links(c);
        assert_eq!(l, vec!["https://outlook.office365.com/owa/?ItemID=AAMk%2Ba%2Fb%3D&exvsurl=1"]);
        assert_eq!(message_id(&l[0]).as_deref(), Some("AAMk_a-b="));
        assert_eq!(message_id("https://outlook.office.com/mail/inbox/id/AAQk%2Fx%3D").as_deref(), Some("AAQk-x="));
        assert_eq!(message_id("https://outlook.office.com/mail/"), None);
        let t = "https://teams.microsoft.com/_#/conversations/19%3Aabc%40thread.v2";
        assert_eq!(conversation_id(t).as_deref(), Some("19:abc@thread.v2"));
        assert_eq!(conversation_id("https://teams.microsoft.com/v2/"), None);
        assert_eq!(teams_link(t, "k1"), "https://teams.microsoft.com/_?brainstead-reply=k1#/conversations/19%3Aabc%40thread.v2");
    }

    #[test]
    fn puts_the_reply_above_the_history() {
        assert_eq!(reply_html("Hi Maya,\n\nYes <b> & done.\nLena"), "<p>Hi Maya,</p><p>Yes &lt;b&gt; &amp; done.<br>Lena</p>");
        assert_eq!(
            with_reply("<html><BODY dir=\"ltr\"><div>old</div></body></html>", "<p>new</p>"),
            "<html><BODY dir=\"ltr\"><p>new</p><div>old</div></body></html>"
        );
        assert_eq!(with_reply("<div>old</div>", "<p>new</p>"), "<p>new</p><div>old</div>");
    }

    #[test]
    fn a_teams_reply_is_collected_once() {
        let d = tempfile::tempdir().unwrap();
        let link =
            leave_for_teams(d.path(), "https://teams.microsoft.com/_#/conversations/19%3Aa", "19:a", "Thanks\n\nLena", 1000).unwrap();
        let key = link.split("brainstead-reply=").nth(1).unwrap().split('#').next().unwrap().to_string();
        let got = take_teams(d.path(), &key, 1010);
        assert_eq!(got["ok"], true);
        assert_eq!(got["conversation"], "19:a");
        assert_eq!(got["html"], "<p>Thanks</p><p>Lena</p>");
        assert_eq!(take_teams(d.path(), &key, 1011)["ok"], false);
        assert_eq!(take_teams(d.path(), "../x", 1011)["ok"], false);
        // One left too long isn't typed in.
        let link = leave_for_teams(d.path(), "https://teams.microsoft.com/_#/conversations/19%3Aa", "19:a", "Late", 2000).unwrap();
        let key = link.split("brainstead-reply=").nth(1).unwrap().split('#').next().unwrap().to_string();
        assert_eq!(take_teams(d.path(), &key, 2000 + KEEP_SECS + 1)["ok"], false);
    }

    #[test]
    fn refuses_what_it_cant_answer() {
        let d = tempfile::tempdir().unwrap();
        let v = d.path().join("vault");
        std::fs::create_dir_all(v.join("sources")).unwrap();
        std::fs::write(
            v.join("sources/Teams. Chat. Lena - 2026-11-20.md"),
            "# Lena\n\n**Source:** [Open in Teams](https://teams.microsoft.com/v2/)\n",
        )
        .unwrap();
        assert!(place(d.path(), &v, "sources/Teams. Chat. Lena - 2026-11-20.md", "Hi", 0).unwrap_err().contains("captured by hand"));
        assert!(place(d.path(), &v, "sources/Notes.md", "Hi", 0).is_err());
        assert!(place(d.path(), &v, "sources/../x.md", "Hi", 0).is_err());
        assert!(place(d.path(), &v, "sources/Email. Thread. X.md", " ", 0).is_err());
    }
}
