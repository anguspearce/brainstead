// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading the user's own chats, Inbox mail and calendar, and turning them into capture values the
//! vault is written from. The HTTP lives in `api`; here is what to ask for and how to shape the
//! answer. Parsing is split from fetching so it can be tested against saved JSON without the network.

use chrono::{DateTime, Local, Utc};
use reqwest::blocking::Client;
use serde_json::Value;

use super::api;
use super::auth::{self, Service, SignIn};
use super::html;
use brainstead_core::webcapture::{Attachment, Capture, Message, Thread};

/// A chat message, once read from chatsvc.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub id: String,
    pub sender: String,
    pub body: String,
    pub time: DateTime<Utc>,
    /// Files shared with the message. They live in OneDrive or SharePoint, so only their links come.
    pub files: Vec<SharedFile>,
}

/// A file shared in a Teams chat: its name and where it lives.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedFile {
    pub name: String,
    pub url: String,
}

/// The files a chat message shares (its `files` property, sent as JSON text or as an array).
fn shared_files(m: &Value) -> Vec<SharedFile> {
    let files = match &m["properties"]["files"] {
        Value::String(t) => serde_json::from_str(t).unwrap_or(Value::Null),
        v => v.clone(),
    };
    files
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| {
                    let url = f["objectUrl"].as_str().filter(|u| u.starts_with("https://"))?;
                    let name = f["fileName"].as_str().or(f["title"].as_str()).map(str::trim).filter(|n| !n.is_empty())?;
                    Some(SharedFile { name: name.to_string(), url: url.to_string() })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A conversation in the user's list.
#[derive(Debug, Clone, PartialEq)]
pub struct Conv {
    pub id: String,
    pub topic: String,
    pub kind: ConvKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvKind {
    OneOnOne,
    Group,
    Channel,
    Meeting,
    Other,
}

impl ConvKind {
    pub fn of(id: &str) -> ConvKind {
        if id.contains("meeting_") {
            ConvKind::Meeting
        } else if id.contains("@thread.tacv2") {
            ConvKind::Channel
        } else if id.ends_with("@unq.gbl.spaces") {
            ConvKind::OneOnOne
        } else if id.contains("@thread.v2") {
            ConvKind::Group
        } else {
            ConvKind::Other
        }
    }
}

/// A calendar event, for Today and for finding meetings to fetch transcripts of.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub subject: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub organizer: String,
    pub online: bool,
    /// The meeting's chat/thread id, when it's a Teams meeting (for the transcript lookup).
    pub thread_id: Option<String>,
}

// ── Chats ──────────────────────────────────────────────────────────────────────────────

/// The user's conversations, newest activity first.
pub fn conversations(client: &Client, s: &SignIn) -> Result<Vec<Conv>, String> {
    let url = format!("{}/v1/users/ME/conversations?view=msnp24Equivalent&pageSize=200", s.chat_service.trim_end_matches('/'));
    let body = api::get_chatsvc(client, &url)?;
    Ok(parse_conversations(&body))
}

pub fn parse_conversations(body: &Value) -> Vec<Conv> {
    body["conversations"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let id = c["id"].as_str()?.to_string();
                    let topic = c["threadProperties"]["topic"].as_str().unwrap_or("").to_string();
                    Some(Conv { kind: ConvKind::of(&id), id, topic })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A conversation's recent messages, oldest first. One page (enough for a day of a normal chat).
pub fn chat_messages(client: &Client, s: &SignIn, conv_id: &str) -> Result<Vec<ChatMessage>, String> {
    let url = format!(
        "{}/v1/users/ME/conversations/{}/messages?view=msnp24Equivalent|supportsMessageProperties&pageSize=200",
        s.chat_service.trim_end_matches('/'),
        urlencode(conv_id)
    );
    let body = api::get_chatsvc(client, &url)?;
    Ok(parse_messages(&body))
}

pub fn parse_messages(body: &Value) -> Vec<ChatMessage> {
    let mut msgs: Vec<ChatMessage> = body["messages"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    // Only real chat messages: skip control/system events (member added, call ended…).
                    let mt = m["messagetype"].as_str().unwrap_or("");
                    if mt != "RichText/Html" && mt != "Text" {
                        return None;
                    }
                    let content = m["content"].as_str().unwrap_or("");
                    let body = if mt == "RichText/Html" { html::to_text(content) } else { content.trim().to_string() };
                    // A message that only shares a file has no text but is kept, for the file's link.
                    let files = shared_files(m);
                    if body.is_empty() && files.is_empty() {
                        return None;
                    }
                    let time = m["originalarrivaltime"].as_str().or_else(|| m["composetime"].as_str())?;
                    let time = DateTime::parse_from_rfc3339(time).ok()?.with_timezone(&Utc);
                    Some(ChatMessage {
                        id: m["id"].as_str().or(m["originalarrivaltime"].as_str()).unwrap_or("").to_string(),
                        sender: m["imdisplayname"].as_str().filter(|s| !s.is_empty()).unwrap_or("Unknown").to_string(),
                        body,
                        time,
                        files,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    msgs.sort_by_key(|m| m.time);
    msgs
}

/// The distinct people other than the user who have written in a conversation, sorted — the chat's
/// display name for a chat with no topic (for a 1:1, the other person). Empty when only the user has.
pub fn chat_display_name(msgs: &[ChatMessage], me: &str) -> String {
    let mut names: Vec<&str> = msgs.iter().map(|m| m.sender.as_str()).filter(|n| *n != me && *n != "Unknown").collect();
    names.sort_unstable();
    names.dedup();
    names.join(", ")
}

/// Builds a Teams chat capture for one local `day` of a conversation. The title is the chat's topic,
/// else the people who wrote that day, else `fallback` (the chat's partner across the whole thread, so
/// a 1:1 day where only the user wrote still names the other person), else "Teams chat". The file is
/// named by `day`, not the capture day, via `meeting_date`.
pub fn chat_capture(conv: &Conv, day_msgs: &[ChatMessage], me: &str, url_base: &str, day: &str, fallback: &str) -> Option<Capture> {
    if day_msgs.is_empty() {
        return None;
    }
    let title = if !conv.topic.is_empty() {
        conv.topic.clone()
    } else {
        let today_names = chat_display_name(day_msgs, me);
        if !today_names.is_empty() {
            today_names
        } else if !fallback.is_empty() {
            fallback.to_string()
        } else {
            "Teams chat".to_string()
        }
    };
    let messages = day_msgs
        .iter()
        .map(|m| Message {
            sender: m.sender.clone(),
            from_address: None,
            to: None,
            cc: None,
            recipients: None,
            date: Some(m.time.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()),
            body: m.body.clone(),
            attachments: m
                .files
                .iter()
                .map(|f| Attachment { name: f.name.clone(), url: Some(f.url.clone()), ..Default::default() })
                .collect(),
        })
        .collect();
    Some(Capture::Teams(Thread {
        url: format!("{url_base}/_#/conversations/{}", urlencode(&conv.id)),
        title,
        kind: Some("chat".into()),
        messages,
        // Carries the conversation day so the file is named by it (webcapture::render); a chat shows no
        // **Meeting:** line (that's transcript-only), so this only affects the filename.
        meeting_date: Some(day.to_string()),
        meeting_start: None,
    }))
}

// ── Channels ───────────────────────────────────────────────────────────────────────────

/// A channel the user can choose to sync: its conversation id and a "Team / Channel" name.
#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

/// The user's teams and their channels, from the chat-service aggregator (the conversations list
/// doesn't carry channel names). Named "Team / Channel" so the picker is readable.
pub fn channels(client: &Client, s: &SignIn) -> Result<Vec<Channel>, String> {
    // Prefer the aggregator address from the sign-in; for a sign-in from before it was stored, derive
    // it from the chat-service address (same host and region, `/api/chatsvc/` → `/api/csa/`).
    let base = if !s.csa_service.trim().is_empty() {
        s.csa_service.trim_end_matches('/').to_string()
    } else {
        s.chat_service.trim_end_matches('/').replacen("/api/chatsvc/", "/api/csa/", 1)
    };
    if base.is_empty() || !base.contains("/api/csa/") {
        return Err("No channel service address in the sign-in; open Teams in the browser again.".into());
    }
    let url = format!("{base}/api/v3/teams/users/me?isPrefetch=false&enableMembershipSummary=true");
    Ok(parse_channels(&api::get_csa(client, &url)?))
}

pub fn parse_channels(body: &Value) -> Vec<Channel> {
    let mut out = Vec::new();
    for team in body["teams"].as_array().into_iter().flatten() {
        let team_name = team["displayName"].as_str().unwrap_or("").trim();
        for ch in team["channels"].as_array().into_iter().flatten() {
            let (Some(id), Some(name)) = (ch["id"].as_str(), ch["displayName"].as_str()) else { continue };
            let name = name.trim();
            let label = if team_name.is_empty() { name.to_string() } else { format!("{team_name} / {name}") };
            out.push(Channel { id: id.to_string(), name: label });
        }
    }
    out.sort_by_key(|c| c.name.to_lowercase());
    out
}

// ── Mail ───────────────────────────────────────────────────────────────────────────────

/// A mail message, once read from Outlook REST.
#[derive(Debug, Clone, PartialEq)]
pub struct Mail {
    pub id: String,
    /// The Outlook ConversationId: all messages of one thread share it.
    pub conversation_id: String,
    pub subject: String,
    pub from: String,
    pub from_address: String,
    pub to: String,
    pub received: DateTime<Utc>,
    pub body: String,
    pub web_link: String,
    /// Outlook says it carries attachments (inline images in a signature count).
    pub has_attachments: bool,
}

const MAIL_SELECT: &str = "Subject,From,ToRecipients,Body,ReceivedDateTime,WebLink,ConversationId,HasAttachments";

/// Recent Inbox messages, newest first.
pub fn inbox(client: &Client, top: usize) -> Result<Vec<Mail>, String> {
    let url = format!(
        "https://outlook.office.com/api/v2.0/me/mailfolders/inbox/messages?$top={top}&$orderby=ReceivedDateTime%20desc&$select={MAIL_SELECT}"
    );
    let body = api::get_json(client, Service::Outlook, &url)?;
    Ok(parse_mail(&body))
}

/// Every message of one conversation across all folders (so a thread includes the user's own sent
/// replies), oldest first.
pub fn conversation(client: &Client, conversation_id: &str, top: usize) -> Result<Vec<Mail>, String> {
    // Filter by ConversationId only — combining it with $orderby makes Outlook reject the query as an
    // "InefficientFilter", so we sort by received time ourselves below.
    let filter = format!("ConversationId eq '{}'", conversation_id.replace('\'', "''"));
    let url = format!("https://outlook.office.com/api/v2.0/me/messages?$filter={}&$top={top}&$select={MAIL_SELECT}", urlencode(&filter));
    let mut msgs = parse_mail(&api::get_json(client, Service::Outlook, &url)?);
    msgs.sort_by_key(|m| m.received);
    Ok(msgs)
}

/// Inbox and folders searched for `query` (Outlook REST `$search`, by relevance), newest handling
/// left to the caller. Used by the on-demand pull.
pub fn search_mail(client: &Client, query: &str, top: usize) -> Result<Vec<Mail>, String> {
    let url = format!(
        "https://outlook.office.com/api/v2.0/me/messages?$search={}&$top={top}&$select={MAIL_SELECT}",
        urlencode(&format!("\"{}\"", query.replace('"', "")))
    );
    Ok(parse_mail(&api::get_json(client, Service::Outlook, &url)?))
}

pub fn parse_mail(body: &Value) -> Vec<Mail> {
    body["value"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    let received = m["ReceivedDateTime"].as_str()?;
                    let received = DateTime::parse_from_rfc3339(received).ok()?.with_timezone(&Utc);
                    let from = &m["From"]["EmailAddress"];
                    let to = m["ToRecipients"]
                        .as_array()
                        .map(|r| r.iter().filter_map(|x| x["EmailAddress"]["Name"].as_str()).collect::<Vec<_>>().join(", "))
                        .unwrap_or_default();
                    let content = m["Body"]["Content"].as_str().unwrap_or("");
                    let is_html = m["Body"]["ContentType"].as_str().map(|t| t.eq_ignore_ascii_case("html")).unwrap_or(true);
                    Some(Mail {
                        id: m["Id"].as_str().unwrap_or("").to_string(),
                        conversation_id: m["ConversationId"].as_str().unwrap_or("").to_string(),
                        subject: m["Subject"].as_str().unwrap_or("(no subject)").to_string(),
                        from: from["Name"].as_str().unwrap_or("Unknown").to_string(),
                        from_address: from["Address"].as_str().unwrap_or("").to_string(),
                        to,
                        received,
                        body: if is_html { html::to_text(content) } else { content.trim().to_string() },
                        web_link: m["WebLink"].as_str().unwrap_or("").to_string(),
                        has_attachments: m["HasAttachments"].as_bool().unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// What kind of attachment Outlook says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    /// A file carried in the message.
    File,
    /// An attached email or calendar item.
    Item,
    /// A link to a file in OneDrive or SharePoint.
    Reference,
}

/// An attachment's details, without its content.
#[derive(Debug, Clone, PartialEq)]
pub struct MailAttachment {
    pub id: String,
    pub name: String,
    pub size: u64,
    /// Shown in the body (a signature logo, a pasted picture) rather than attached.
    pub inline: bool,
    pub kind: AttachmentKind,
    /// Where a reference attachment's file lives.
    pub url: Option<String>,
}

/// A message's attachments, without their content.
pub fn attachments(client: &Client, message_id: &str) -> Result<Vec<MailAttachment>, String> {
    let url = format!("https://outlook.office.com/api/v2.0/me/messages/{}/attachments?$select=Name,Size,IsInline", urlencode(message_id));
    Ok(parse_attachments(&api::get_json(client, Service::Outlook, &url)?))
}

pub fn parse_attachments(body: &Value) -> Vec<MailAttachment> {
    body["value"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| {
                    let kind = match x["@odata.type"].as_str().unwrap_or("").rsplit('.').next() {
                        Some("ItemAttachment") => AttachmentKind::Item,
                        Some("ReferenceAttachment") => AttachmentKind::Reference,
                        _ => AttachmentKind::File,
                    };
                    MailAttachment {
                        id: x["Id"].as_str().unwrap_or("").to_string(),
                        name: x["Name"].as_str().filter(|n| !n.trim().is_empty()).unwrap_or("attachment").trim().to_string(),
                        size: x["Size"].as_u64().unwrap_or(0),
                        inline: x["IsInline"].as_bool().unwrap_or(false),
                        kind,
                        url: x["SourceUrl"].as_str().filter(|u| u.starts_with("https://")).map(str::to_string),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A file attachment's content. `client` should allow for a large download.
pub fn attachment_bytes(client: &Client, message_id: &str, attachment_id: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let url = format!("https://outlook.office.com/api/v2.0/me/messages/{}/attachments/{}", urlencode(message_id), urlencode(attachment_id));
    let body = api::get_json(client, Service::Outlook, &url)?;
    let content = body["ContentBytes"].as_str().ok_or("Outlook sent the attachment without its content.")?;
    base64::engine::general_purpose::STANDARD.decode(content).map_err(|e| format!("Couldn't read the attachment's content: {e}"))
}

/// An Outlook capture for a whole email thread: one message block per mail, oldest first. A one-mail
/// thread renders the same as a single email. `msgs` must be non-empty.
/// `attachments` holds each message's files, by position (shorter or empty when there are none).
pub fn thread_capture(msgs: &[Mail], attachments: &[Vec<Attachment>]) -> Option<Capture> {
    let first = msgs.first()?;
    // Title from the most recent message's subject (replies keep the subject, possibly "Re:").
    let title = msgs.last().map(|m| m.subject.clone()).unwrap_or_else(|| first.subject.clone());
    let link = msgs
        .last()
        .and_then(|m| (!m.web_link.is_empty()).then(|| m.web_link.clone()))
        .unwrap_or_else(|| "https://outlook.office.com/mail/".into());
    let messages = msgs
        .iter()
        .enumerate()
        .map(|(i, m)| Message {
            sender: m.from.clone(),
            from_address: Some(m.from_address.clone()).filter(|a| !a.is_empty()),
            to: Some(m.to.clone()).filter(|t| !t.is_empty()),
            cc: None,
            recipients: None,
            date: Some(m.received.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()),
            body: m.body.clone(),
            attachments: attachments.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    Some(Capture::Outlook(Thread { url: link, title, kind: None, messages, meeting_date: None, meeting_start: None }))
}

// ── Calendar ───────────────────────────────────────────────────────────────────────────

/// The user's meetings between `start` and `end` (Teams middle-tier calendar).
pub fn calendar(client: &Client, s: &SignIn, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Event>, String> {
    let mt = s.middle_tier.trim_end_matches('/');
    let fmt = |t: DateTime<Utc>| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let url = format!(
        "{mt}/v2.1/me/calendars/calendarView?startDate={}&endDate={}&$top=100&$skip=0&$orderby=startTime%20asc&$filter=isCancelled%20eq%20false&$select=subject,startTime,endTime,isOnlineMeeting,skypeTeamsMeetingUrl,skypeTeamsData,organizerName",
        urlencode(&fmt(start)),
        urlencode(&fmt(end)),
    );
    let body = api::get_chatsvc(client, &url)?;
    Ok(parse_calendar(&body))
}

pub fn parse_calendar(body: &Value) -> Vec<Event> {
    body["value"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let start = DateTime::parse_from_rfc3339(e["startTime"].as_str()?).ok()?.with_timezone(&Utc);
                    let end = e["endTime"]
                        .as_str()
                        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                        .map(|t| t.with_timezone(&Utc))
                        .unwrap_or(start);
                    Some(Event {
                        subject: e["subject"].as_str().unwrap_or("(no subject)").to_string(),
                        start,
                        end,
                        organizer: e["organizerName"].as_str().unwrap_or("").to_string(),
                        online: e["isOnlineMeeting"].as_bool().unwrap_or(false),
                        thread_id: meeting_thread_id(e),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The meeting's thread id, from the Teams meeting data on a calendar event. Teams stores it in
/// `skypeTeamsData` (a JSON string with a `cid`) or in the join URL.
fn meeting_thread_id(e: &Value) -> Option<String> {
    if let Some(data) = e["skypeTeamsData"].as_str() {
        if let Ok(v) = serde_json::from_str::<Value>(data) {
            if let Some(cid) = v["cid"].as_str().filter(|s| !s.is_empty()) {
                return Some(cid.to_string());
            }
        }
    }
    // Fall back to the thread id embedded in the join URL (…/l/meetup-join/19%3ameeting_…%40thread.v2/…).
    let url = e["skypeTeamsMeetingUrl"].as_str()?;
    let dec = url.replace("%3a", ":").replace("%3A", ":").replace("%40", "@");
    let start = dec.find("19:meeting_")?;
    let tail = &dec[start..];
    let end = tail.find("@thread.v2")? + "@thread.v2".len();
    Some(tail[..end].to_string())
}

// ── Transcripts ────────────────────────────────────────────────────────────────────────

/// The messages people typed in a meeting's chat, as webcapture messages, with a divider ahead of
/// them — for folding into the meeting's transcript source (D-20261025-02). Empty on any failure or
/// when the meeting had no chat, so it never blocks the transcript.
fn meeting_chat(client: &Client, s: &SignIn, ev: &Event) -> Vec<Message> {
    let Some(thread_id) = &ev.thread_id else { return Vec::new() };
    let url = format!(
        "{}/v1/users/ME/conversations/{}/messages?view=msnp24Equivalent|supportsMessageProperties&pageSize=200",
        s.chat_service.trim_end_matches('/'),
        urlencode(thread_id)
    );
    // A recurring meeting's chat thread carries every session's messages, so keep only this meeting's
    // day (local) — the chat that belongs with this transcript.
    let day = ev.start.with_timezone(&Local).date_naive();
    let msgs: Vec<ChatMessage> = api::get_chatsvc(client, &url)
        .map(|b| parse_messages(&b))
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.time.with_timezone(&Local).date_naive() == day)
        .collect();
    if msgs.is_empty() {
        return Vec::new();
    }
    let mut out =
        vec![divider(brainstead_core::webcapture::MEETING_CHAT_HEADING, "Messages typed in the meeting chat during the meeting:")];
    out.extend(msgs.iter().map(|m| Message {
        sender: m.sender.clone(),
        from_address: None,
        to: None,
        cc: None,
        recipients: None,
        date: Some(m.time.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()),
        body: m.body.clone(),
        attachments: Vec::new(),
    }));
    out
}

/// The transcript for a meeting thread, as a Teams transcript capture, when one exists. Best-effort:
/// returns Ok(None) when the meeting had no transcript (not recorded, or still processing). The
/// meeting's chat, when any, is folded in after the transcript.
pub fn transcript_capture(client: &Client, s: &SignIn, ev: &Event) -> Result<Option<Capture>, String> {
    let Some(thread_id) = &ev.thread_id else { return Ok(None) };
    let filter = format!("ItemProperties/Default/MeetingThreadId eq '{thread_id}'");
    let select = "ItemId,ItemProperties/Default/TranscriptJson,ItemProperties/Default/RecordingStartDateTime,Visualization";
    let url = format!(
        "https://substrate.office.com/api/beta/me/WorkingSetFiles/?$filter={}&$orderby={}&$select={}",
        urlencode(&filter),
        urlencode("FileCreatedTime desc"),
        urlencode(select),
    );
    // WorkingSetFiles needs the Substrate token and this Prefer header.
    let token = auth::token(Service::Substrate)?;
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .header("Accept", "application/json")
        .header("Prefer", r#"substrate.flexibleschema,outlook.data-source="Substrate",exchange.behavior="SubstrateFiles""#)
        .send()
        .map_err(|e| format!("Couldn't reach Substrate for a transcript: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("substrate.office.com answered {}", resp.status().as_u16()));
    }
    let body: Value = resp.json().map_err(|e| format!("Substrate gave an unreadable answer: {e}"))?;
    let Some(cap) = parse_transcript(&body, ev)? else { return Ok(None) };
    let Capture::Teams(mut thread) = cap else { return Ok(Some(cap)) };
    // Fold the meeting chat in after the transcript, when there is one. Both parts are then labelled
    // with a divider so a reader (or the meeting-note model) can tell the spoken transcript from the
    // typed chat; a transcript with no chat is left unlabelled, as before.
    let chat = meeting_chat(client, s, ev);
    if !chat.is_empty() {
        thread.messages.insert(0, divider("Meeting transcript", "What was said in the meeting (times are offsets from the start):"));
        thread.messages.extend(chat);
    }
    Ok(Some(Capture::Teams(thread)))
}

/// A heading-only message used to label a section within a capture.
fn divider(heading: &str, note: &str) -> Message {
    Message {
        sender: heading.to_string(),
        from_address: None,
        to: None,
        cc: None,
        recipients: None,
        date: None,
        body: note.to_string(),
        attachments: Vec::new(),
    }
}

/// A transcript item's own recording start, as a UTC time, when it has a readable one.
fn recording_time(item: &Value) -> Option<DateTime<Utc>> {
    item["ItemProperties"]["Default"]["RecordingStartDateTime"]
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}

/// The transcript for this meeting occurrence, or Ok(None) when none matches (so the sitting waits for
/// its own). Err carries a flag when a transcript is picked whose own recording day disagrees with the
/// day we'd file it under — a cross-check so a mis-dated transcript is surfaced, not silently written.
pub fn parse_transcript(body: &Value, ev: &Event) -> Result<Option<Capture>, String> {
    // A recurring meeting's whole series shares one MeetingThreadId, so Substrate returns a transcript
    // for every occurrence. Pick the one recorded during *this* occurrence — matched by its recording
    // start falling in the meeting's window — not just the newest on the series. Otherwise an earlier
    // occurrence's transcript is taken for a meeting whose own transcript isn't ready yet and stamped
    // with this occurrence's date (the Oct bi-weekly landing an earlier sitting under today's date).
    let lo = ev.start - chrono::Duration::hours(1);
    let hi = ev.end + chrono::Duration::hours(6);
    let Some(item) =
        body["value"].as_array().into_iter().flatten().find(|i| {
            i["ItemProperties"]["Default"]["TranscriptJson"].is_string() && recording_time(i).is_some_and(|t| t >= lo && t <= hi)
        })
    else {
        return Ok(None);
    };
    // Cross-check: the chosen transcript's own recording day must agree with the occurrence we file it
    // under. It does by construction of the window above; this guards a future change to the matching,
    // or a transcript that isn't this sitting's, from mis-dating a note silently.
    let want = ev.start.with_timezone(&Local).date_naive();
    if let Some(got) = recording_time(item).map(|t| t.with_timezone(&Local).date_naive()) {
        if got != want {
            return Err(format!(
                "A transcript for “{}” was recorded {got} but the meeting is {want}; skipped so it isn't filed under the wrong date.",
                ev.subject
            ));
        }
    }
    let raw = item["ItemProperties"]["Default"]["TranscriptJson"].as_str().ok_or("A transcript had no text.")?;
    let data: Value = serde_json::from_str(raw).map_err(|e| format!("A transcript was unreadable: {e}"))?;
    let Some(entries) = data["entries"].as_array().filter(|e| !e.is_empty()) else {
        return Ok(None);
    };
    let messages = entries
        .iter()
        .filter_map(|e| {
            let text = e["text"].as_str()?.trim();
            if text.is_empty() {
                return None;
            }
            Some(Message {
                sender: e["speakerDisplayName"].as_str().filter(|s| !s.is_empty()).unwrap_or("Unknown").to_string(),
                from_address: None,
                to: None,
                cc: None,
                recipients: None,
                date: e["startOffset"].as_str().map(|s| s.trim_end_matches("0000").to_string()),
                body: text.to_string(),
                attachments: Vec::new(),
            })
        })
        .collect::<Vec<_>>();
    if messages.is_empty() {
        return Ok(None);
    }
    let start_local = ev.start.with_timezone(&Local);
    Ok(Some(Capture::Teams(Thread {
        url: "https://teams.microsoft.com/".to_string(),
        title: ev.subject.clone(),
        kind: Some("transcript".into()),
        messages,
        meeting_date: Some(start_local.format("%Y-%m-%d").to_string()),
        meeting_start: Some(start_local.format("%H:%M").to_string()),
    })))
}

/// Percent-encodes a path/query segment (conversation ids and OData filters carry `:`, `@`, spaces).
pub fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The signed-in user's own display name, from a cached token's claims, for titling 1:1 chats.
pub fn my_name(s: &SignIn) -> String {
    s.tokens.values().find_map(|t| auth::claims(&t.token)["name"].as_str().map(str::to_string)).unwrap_or_default()
}

/// Groups messages by their local calendar day (`YYYY-MM-DD`).
pub fn by_local_day(msgs: &[ChatMessage]) -> std::collections::BTreeMap<String, Vec<ChatMessage>> {
    let mut map: std::collections::BTreeMap<String, Vec<ChatMessage>> = std::collections::BTreeMap::new();
    for m in msgs {
        let day = m.time.with_timezone(&Local).format("%Y-%m-%d").to_string();
        map.entry(day).or_default().push(m.clone());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifies_conversations_by_id() {
        assert_eq!(ConvKind::of("19:abc@unq.gbl.spaces"), ConvKind::OneOnOne);
        assert_eq!(ConvKind::of("19:abc@thread.v2"), ConvKind::Group);
        assert_eq!(ConvKind::of("19:abc@thread.tacv2"), ConvKind::Channel);
        assert_eq!(ConvKind::of("19:meeting_abc@thread.v2"), ConvKind::Meeting);
        assert_eq!(ConvKind::of("48:notifications"), ConvKind::Other);
    }

    #[test]
    fn parses_attachments_and_lists_them_under_their_message() {
        let body = json!({"value": [
            {"@odata.type": "#Microsoft.OutlookServices.FileAttachment", "Id": "f1", "Name": "Orbit App proposal.pdf", "Size": 2048, "IsInline": false},
            {"@odata.type": "#Microsoft.OutlookServices.FileAttachment", "Id": "f2", "Name": "logo.png", "Size": 900, "IsInline": true},
            {"@odata.type": "#Microsoft.OutlookServices.ItemAttachment", "Id": "i1", "Name": "Fwd: Budget", "Size": 5000, "IsInline": false},
            {"@odata.type": "#Microsoft.OutlookServices.ReferenceAttachment", "Id": "r1", "Name": "Plan.docx", "SourceUrl": "https://acme-my.sharepoint.com/personal/maya/Documents/Plan.docx"}
        ]});
        let a = parse_attachments(&body);
        assert_eq!(a.len(), 4);
        assert_eq!((a[0].kind, a[0].size, a[0].inline), (AttachmentKind::File, 2048, false));
        assert!(a[1].inline);
        assert_eq!(a[2].kind, AttachmentKind::Item);
        assert_eq!(a[3].kind, AttachmentKind::Reference);
        assert_eq!(a[3].url.as_deref(), Some("https://acme-my.sharepoint.com/personal/maya/Documents/Plan.docx"));

        let mail = parse_mail(&json!({"value": [{
            "Id": "AAA", "ConversationId": "conv1", "Subject": "Proposal", "ReceivedDateTime": "2026-10-24T06:30:00Z",
            "From": {"EmailAddress": {"Name": "Lena", "Address": "lena@acme.example"}},
            "Body": {"ContentType": "Text", "Content": "Attached."}, "HasAttachments": true
        }]}));
        assert!(mail[0].has_attachments);
        let files = vec![vec![Attachment {
            name: "Orbit App proposal.pdf".into(),
            path: Some("sources/Email. Attachment. Orbit App proposal - 2026-10-24.pdf".into()),
            ..Default::default()
        }]];
        match thread_capture(&mail, &files).unwrap() {
            Capture::Outlook(th) => assert_eq!(th.messages[0].attachments, files[0]),
            _ => panic!("expected an Outlook capture"),
        }
    }

    #[test]
    fn keeps_a_chat_message_that_only_shares_a_file() {
        let files = r#"[{"fileName": "Orbit App plan.pdf", "objectUrl": "https://acme-my.sharepoint.com/personal/maya/Documents/Microsoft Teams Chat Files/Orbit App plan.pdf"}]"#;
        let body = json!({"messages": [
            {"id": "1", "messagetype": "RichText/Html", "content": "", "imdisplayname": "Maya", "originalarrivaltime": "2026-10-24T09:00:00.000Z", "properties": {"files": files}},
            {"id": "2", "messagetype": "RichText/Html", "content": "", "imdisplayname": "Maya", "originalarrivaltime": "2026-10-24T09:01:00.000Z"}
        ]});
        let msgs = parse_messages(&body);
        assert_eq!(msgs.len(), 1, "an empty message without a file is still dropped");
        assert_eq!(
            msgs[0].files,
            [SharedFile {
                name: "Orbit App plan.pdf".into(),
                url: "https://acme-my.sharepoint.com/personal/maya/Documents/Microsoft Teams Chat Files/Orbit App plan.pdf".into()
            }]
        );
        let conv = Conv { id: "19:x@unq.gbl.spaces".into(), topic: "".into(), kind: ConvKind::OneOnOne };
        match chat_capture(&conv, &msgs, "Me", "https://teams.cloud.microsoft", "2026-10-24", "Maya").unwrap() {
            Capture::Teams(th) => {
                assert_eq!(th.messages[0].attachments[0].url.as_deref(), Some(msgs[0].files[0].url.as_str()));
                assert_eq!(th.messages[0].attachments[0].path, None, "linked, not brought in");
            }
            _ => panic!("expected a Teams capture"),
        }
    }

    #[test]
    fn parses_chat_messages_and_skips_system_events() {
        let body = json!({"messages": [
            {"id": "2", "messagetype": "RichText/Html", "content": "<p>Hi &amp; bye</p>", "imdisplayname": "Maya", "originalarrivaltime": "2026-10-24T09:00:00.000Z"},
            {"id": "3", "messagetype": "Text", "content": "plain", "imdisplayname": "Lena", "originalarrivaltime": "2026-10-24T08:00:00.000Z"},
            {"id": "4", "messagetype": "ThreadActivity/AddMember", "content": "x", "originalarrivaltime": "2026-10-24T07:00:00.000Z"},
            {"id": "5", "messagetype": "RichText/Html", "content": "   ", "imdisplayname": "Z", "originalarrivaltime": "2026-10-24T10:00:00.000Z"},
        ]});
        let msgs = parse_messages(&body);
        assert_eq!(msgs.len(), 2, "system event and empty message dropped");
        assert_eq!((msgs[0].sender.as_str(), msgs[0].body.as_str()), ("Lena", "plain"), "oldest first");
        assert_eq!(msgs[1].body, "Hi & bye");
    }

    #[test]
    fn titles_a_one_on_one_by_the_other_person_and_names_the_file_by_the_day() {
        let conv = Conv { id: "19:x@unq.gbl.spaces".into(), topic: String::new(), kind: ConvKind::OneOnOne };
        let t = "2026-10-24T09:00:00Z".parse().unwrap();
        let msgs = vec![
            ChatMessage { id: "1".into(), sender: "Maya".into(), body: "hi".into(), time: t, files: vec![] },
            ChatMessage { id: "2".into(), sender: "Me Myself".into(), body: "hello".into(), time: t, files: vec![] },
        ];
        let cap = chat_capture(&conv, &msgs, "Me Myself", "https://teams.microsoft.com", "2026-10-24", "Maya").unwrap();
        match &cap {
            Capture::Teams(th) => {
                assert_eq!(th.title, "Maya");
                assert_eq!(th.messages.len(), 2);
                assert_eq!(th.kind.as_deref(), Some("chat"));
                assert_eq!(th.meeting_date.as_deref(), Some("2026-10-24"));
            }
            _ => panic!("expected a Teams capture"),
        }
        // The file is named by the conversation day, not the capture day; no Meeting line for a chat.
        let (name, text) = brainstead_core::webcapture::render(&cap, "2026-10-25T00:00:00Z".parse().unwrap(), "2026-10-25");
        assert_eq!(name, "Teams. Chat. Maya - 2026-10-24.md");
        assert!(!text.contains("**Meeting:**"), "a chat has no Meeting line");
    }

    #[test]
    fn a_one_on_one_day_where_only_i_wrote_still_names_the_partner() {
        let conv = Conv { id: "19:x@unq.gbl.spaces".into(), topic: String::new(), kind: ConvKind::OneOnOne };
        let t = "2026-10-14T16:00:00Z".parse().unwrap();
        let msgs = vec![ChatMessage { id: "1".into(), sender: "Me Myself".into(), body: "any update?".into(), time: t, files: vec![] }];
        let cap = chat_capture(&conv, &msgs, "Me Myself", "https://teams.microsoft.com", "2026-10-14", "Marnus Freeman").unwrap();
        match cap {
            Capture::Teams(th) => assert_eq!(th.title, "Marnus Freeman", "falls back to the partner"),
            _ => panic!("expected a Teams capture"),
        }
        assert_eq!(chat_display_name(&msgs, "Me Myself"), "", "no other writers that day");
    }

    #[test]
    fn names_channels_as_team_slash_channel() {
        let body = json!({"teams": [
            {"displayName": "Orbit App", "channels": [
                {"id": "19:aaa@thread.tacv2", "displayName": "General"},
                {"id": "19:bbb@thread.tacv2", "displayName": "Design"}]},
            {"displayName": "", "channels": [{"id": "19:ccc@thread.tacv2", "displayName": "Lonely"}]},
            {"displayName": "No channels"},
        ]});
        let chans = parse_channels(&body);
        let names: Vec<&str> = chans.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Lonely", "Orbit App / Design", "Orbit App / General"], "sorted, team-prefixed");
        assert_eq!(chans[1].id, "19:bbb@thread.tacv2");
    }

    #[test]
    fn parses_mail_and_builds_a_thread_capture() {
        let body = json!({"value": [
            {
                "Id": "AAA", "ConversationId": "conv1", "Subject": "Roadmap", "ReceivedDateTime": "2026-10-24T06:30:00Z",
                "From": {"EmailAddress": {"Name": "Lena", "Address": "lena@acme.example"}},
                "ToRecipients": [{"EmailAddress": {"Name": "Me", "Address": "me@acme.example"}}],
                "Body": {"ContentType": "HTML", "Content": "<div>Point one</div><div>Point two</div>"},
                "WebLink": "https://outlook.office.com/mail/id/AAA"
            },
            {
                "Id": "BBB", "ConversationId": "conv1", "Subject": "Re: Roadmap", "ReceivedDateTime": "2026-10-24T07:00:00Z",
                "From": {"EmailAddress": {"Name": "Me", "Address": "me@acme.example"}},
                "ToRecipients": [{"EmailAddress": {"Name": "Lena", "Address": "lena@acme.example"}}],
                "Body": {"ContentType": "Text", "Content": "Thanks."},
                "WebLink": "https://outlook.office.com/mail/id/BBB"
            }
        ]});
        let mail = parse_mail(&body);
        assert_eq!(mail.len(), 2);
        assert_eq!(mail[0].conversation_id, "conv1");
        assert_eq!(mail[0].from_address, "lena@acme.example");
        assert_eq!(mail[0].body, "Point one\nPoint two");
        assert!(!mail[0].has_attachments);
        match thread_capture(&mail, &[]).unwrap() {
            Capture::Outlook(th) => {
                assert_eq!(th.title, "Re: Roadmap", "titled by the latest message");
                assert_eq!(th.messages.len(), 2, "the whole thread, incl. the reply");
                assert_eq!(th.messages[0].sender, "Lena");
                assert_eq!(th.messages[1].sender, "Me");
            }
            _ => panic!("expected an Outlook capture"),
        }
    }

    #[test]
    fn reads_the_meeting_thread_id_from_the_join_url() {
        let e = json!({"skypeTeamsMeetingUrl": "https://teams.microsoft.com/l/meetup-join/19%3ameeting_ABC123%40thread.v2/0?context=x"});
        assert_eq!(meeting_thread_id(&e).as_deref(), Some("19:meeting_ABC123@thread.v2"));
        let e2 = json!({"skypeTeamsData": "{\"cid\":\"19:meeting_XYZ@thread.v2\"}"});
        assert_eq!(meeting_thread_id(&e2).as_deref(), Some("19:meeting_XYZ@thread.v2"));
        assert_eq!(meeting_thread_id(&json!({})), None);
    }

    #[test]
    fn builds_a_transcript_capture() {
        let ev = Event {
            subject: "Steerco".into(),
            start: "2026-10-24T13:00:00Z".parse().unwrap(),
            end: "2026-10-24T14:00:00Z".parse().unwrap(),
            organizer: "Lena".into(),
            online: true,
            thread_id: Some("19:meeting_ABC@thread.v2".into()),
        };
        let tj = json!({"entries": [
            {"speakerDisplayName": "Maya", "text": "Welcome.", "startOffset": "00:00:01.0000000"},
            {"speakerDisplayName": "", "text": "  ", "startOffset": "00:00:02.0000000"},
        ]})
        .to_string();
        let item = |rec: &str| json!({"ItemProperties": {"Default": {"TranscriptJson": tj, "RecordingStartDateTime": rec}}});
        // Newest first, as Substrate returns them: an earlier sitting of the same recurring series, then
        // this occurrence's. The earlier one must not be taken for this meeting.
        let body = json!({"value": [item("2026-11-07T13:01:00Z"), item("2026-10-24T13:02:00Z")]});
        match parse_transcript(&body, &ev).unwrap().unwrap() {
            Capture::Teams(th) => {
                assert_eq!(th.kind.as_deref(), Some("transcript"));
                assert_eq!(th.title, "Steerco");
                assert_eq!(th.messages.len(), 1, "empty line dropped");
                assert_eq!(th.messages[0].sender, "Maya");
                assert_eq!(th.meeting_date.as_deref(), Some("2026-10-24"), "dated by this occurrence");
            }
            _ => panic!("expected a Teams transcript capture"),
        }
        // No transcript present → None.
        assert!(parse_transcript(&json!({"value": []}), &ev).unwrap().is_none());
        // Only an off-occurrence transcript (this sitting's isn't ready yet) → None, not the wrong one.
        assert!(parse_transcript(&json!({"value": [item("2026-10-10T13:00:00Z")]}), &ev).unwrap().is_none());
        // Cross-check: a transcript that is in the window but recorded on a different day (here a
        // multi-day event) is flagged, not filed under the wrong date.
        let multiday = Event { end: "2026-10-27T12:00:00Z".parse().unwrap(), ..ev.clone() };
        let err = parse_transcript(&json!({"value": [item("2026-10-26T12:00:00Z")]}), &multiday).unwrap_err();
        assert!(err.contains("2026-10-26") && err.contains("2026-10-24"), "{err}");
    }
}
