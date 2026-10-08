// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Captures from the Outlook and Teams extensions (`extensions/`), written into `sources/` as the
//! previous app wrote them (its `routes/owa.ts` and `routes/teams.ts`): the same filenames, no
//! frontmatter, a `**Source:**` link and `**Captured:**` time, then one block per message. A free
//! name is picked so nothing is overwritten, and a OneDrive conflict copy beside it refuses. Files
//! dropped onto the Sources screen are copied into `sources/` the same way.

use crate::write::{check_conflict_copies, write_atomic, WriteError};
use chrono::{DateTime, Local, SecondsFormat, Utc};
use regex::Regex;
use serde::Deserialize;
use std::path::Path;
use std::sync::LazyLock;

/// The most threads one combined capture takes (the extension's popup has the same cap).
pub const MAX_BATCH_THREADS: usize = 25;
/// The largest capture accepted, in bytes of JSON.
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub sender: String,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub cc: Option<String>,
    /// Older Outlook builds had one recipients line instead of To and Cc.
    #[serde(default)]
    pub recipients: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Thread {
    pub url: String,
    pub title: String,
    /// `thread`, or `reading-pane` when only the reading pane's text could be read (an invitation,
    /// a card-only mail).
    #[serde(default)]
    pub kind: Option<String>,
    pub messages: Vec<Message>,
    /// A Teams transcript's meeting day (`YYYY-MM-DD`, local), read from the recap page.
    #[serde(default, rename = "meetingDate")]
    pub meeting_date: Option<String>,
    /// Its start (`HH:MM`, local), when the page showed one.
    #[serde(default, rename = "meetingStart")]
    pub meeting_start: Option<String>,
}

impl Thread {
    /// The meeting's day, when the extension sent a real one.
    fn meeting_day(&self) -> Option<&str> {
        self.meeting_date.as_deref().map(str::trim).filter(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok() && d.len() == 10)
    }

    /// The meeting's start, when the extension sent a real one.
    fn meeting_time(&self) -> Option<&str> {
        self.meeting_start.as_deref().map(str::trim).filter(|t| chrono::NaiveTime::parse_from_str(t, "%H:%M").is_ok() && t.len() == 5)
    }
}

/// What an extension sends.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Capture {
    Outlook(Thread),
    OutlookBatch {
        #[serde(default)]
        label: Option<String>,
        threads: Vec<Thread>,
    },
    /// A chat or channel thread (`kind` `chat`), or a meeting transcript (`transcript`).
    Teams(Thread),
}

impl Capture {
    /// Checks what the previous app's schemas checked.
    pub fn check(&self) -> Result<(), String> {
        fn thread(t: &Thread, body_required: bool) -> Result<(), String> {
            if !(t.url.starts_with("https://") || t.url.starts_with("http://")) {
                return Err("The capture has no web address.".into());
            }
            if t.title.trim().is_empty() {
                return Err("The capture has no title.".into());
            }
            if t.messages.is_empty() {
                return Err("The capture has no messages.".into());
            }
            if t.messages.iter().any(|m| m.sender.is_empty() || (body_required && m.body.is_empty())) {
                return Err("A message in the capture has no sender or no text.".into());
            }
            Ok(())
        }
        match self {
            Capture::Outlook(t) => thread(t, false),
            Capture::Teams(t) => thread(t, true),
            Capture::OutlookBatch { threads, .. } => {
                if threads.is_empty() || threads.len() > MAX_BATCH_THREADS {
                    return Err(format!("A combined capture takes 1 to {MAX_BATCH_THREADS} threads."));
                }
                threads.iter().try_for_each(|t| thread(t, false))
            }
        }
    }

    /// What it is, for a notification: "Email thread", "Teams transcript"…
    pub fn what(&self) -> &'static str {
        match self {
            Capture::Outlook(t) if t.kind.as_deref() == Some("reading-pane") => "Email",
            Capture::Outlook(_) => "Email thread",
            Capture::OutlookBatch { .. } => "Email threads",
            Capture::Teams(t) if t.kind.as_deref() == Some("transcript") => "Teams transcript",
            Capture::Teams(_) => "Teams chat",
        }
    }
}

static KEBAB: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)+$").unwrap());
static UNREAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\(\d+\)\s*").unwrap());
static OUTLOOK_TAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\s*[-–—|]\s*Outlook(\s+on\s+the\s+web)?$").unwrap());
static TEAMS_TAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\s*[-–—|]\s*Microsoft\s+Teams$").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// The most bytes a cleaned title takes, so the whole name (prefix, date, a `-2`) stays under the
/// 255 bytes a file name may have.
const TITLE_BYTES: usize = 180;

/// A title made safe for a filename (`cleanTitleCore`): no path or reserved characters, none that
/// break a wikilink to it (`# ^ [ ] |`), no control characters, one space at a time, no dots at the
/// ends, at most 80 characters and 180 bytes, and `kebab-case` words given capitals.
pub fn clean_title(s: &str) -> String {
    let s: String = s.chars().filter(|c| !r#"/\:*?"<>|#^[]"#.contains(*c) && !c.is_control()).collect();
    let s = SPACES.replace_all(&s, " ");
    let mut s: String = s.trim_matches('.').trim().chars().take(80).collect();
    if s.len() > TITLE_BYTES {
        let mut cut = TITLE_BYTES;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s = s.trim_end_matches('.').trim_end().to_string();
    }
    let s = if s.is_empty() { "untitled".to_string() } else { s };
    if !KEBAB.is_match(&s) {
        return s;
    }
    s.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn outlook_title(t: &str) -> String {
    clean_title(&OUTLOOK_TAIL.replace(&UNREAD.replace(t, ""), ""))
}

fn teams_title(t: &str) -> String {
    clean_title(&TEAMS_TAIL.replace(t, ""))
}

fn message_lines(m: &Message, level: usize, out: &mut Vec<String>) {
    let hashes = "#".repeat(level);
    out.push(match &m.date {
        Some(d) if !d.is_empty() => format!("{hashes} {} — {d}", m.sender),
        _ => format!("{hashes} {}", m.sender),
    });
    out.push(String::new());
    if let Some(to) = m.to.as_ref().or(m.recipients.as_ref()).filter(|t| !t.is_empty()) {
        out.extend([format!("**To:** {to}"), String::new()]);
    }
    if let Some(cc) = m.cc.as_ref().filter(|c| !c.is_empty()) {
        out.extend([format!("**Cc:** {cc}"), String::new()]);
    }
    out.extend([m.body.trim().to_string(), String::new(), "---".into(), String::new()]);
}

fn reading_pane_note(kind: Option<&str>) -> Option<String> {
    (kind == Some("reading-pane")).then(|| {
        "**Capture:** reading-pane text only, not a threaded conversation (meeting invitation, card-only mail, or a DOM the thread extractor didn't recognise).".to_string()
    })
}

/// The file's name (before a free one is picked) and its text.
pub fn render(c: &Capture, now: DateTime<Utc>, today: &str) -> (String, String) {
    let captured = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut l: Vec<String> = Vec::new();
    let name = match c {
        Capture::Outlook(t) => {
            let title = outlook_title(&t.title);
            l.extend([
                format!("# {title}"),
                String::new(),
                format!("**Source:** [Open in Outlook]({})", t.url),
                format!("**Captured:** {captured}"),
            ]);
            l.extend(reading_pane_note(t.kind.as_deref()));
            l.extend([String::new(), "---".into(), String::new()]);
            t.messages.iter().for_each(|m| message_lines(m, 2, &mut l));
            let prefix = if t.kind.as_deref() == Some("reading-pane") { "Email. Item." } else { "Email. Thread." };
            format!("{prefix} {title} - {today}.md")
        }
        Capture::OutlookBatch { label, threads } => {
            let label = match label.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(s) => outlook_title(s),
                None => {
                    let first = outlook_title(&threads[0].title);
                    outlook_title(&if threads.len() > 1 { format!("{first} +{} more", threads.len() - 1) } else { first })
                }
            };
            l.extend([format!("# {label}"), String::new(), format!("**Captured:** {captured}"), format!("**Threads:** {}", threads.len())]);
            l.extend([String::new(), "---".into(), String::new()]);
            for t in threads {
                l.extend([format!("## {}", t.title), String::new(), format!("**Source:** [Open in Outlook]({})", t.url)]);
                l.extend(reading_pane_note(t.kind.as_deref()));
                l.push(String::new());
                t.messages.iter().for_each(|m| message_lines(m, 3, &mut l));
            }
            format!("Email. Threads. {label} - {today}.md")
        }
        Capture::Teams(t) => {
            let title = teams_title(&t.title);
            let transcript = t.kind.as_deref() == Some("transcript");
            let link = if transcript { "Open meeting recap" } else { "Open in Teams" };
            // A transcript is named with the day of the meeting when the extension found it, not
            // the day it was captured; the meeting note screen reads `**Meeting:**`.
            let meeting = t.meeting_day().filter(|_| transcript);
            l.extend([format!("# {title}"), String::new(), format!("**Source:** [{link}]({})", t.url)]);
            if let Some(day) = meeting {
                l.push(match t.meeting_time() {
                    Some(start) => format!("**Meeting:** {day} {start}"),
                    None => format!("**Meeting:** {day}"),
                });
            }
            l.push(format!("**Captured:** {captured}"));
            l.extend([String::new(), "---".into(), String::new()]);
            for m in &t.messages {
                l.push(match &m.date {
                    Some(d) if !d.is_empty() => format!("## {} — {d}", m.sender),
                    _ => format!("## {}", m.sender),
                });
                l.extend([String::new(), m.body.trim().to_string(), String::new(), "---".into(), String::new()]);
            }
            let prefix = if transcript { "Teams. Transcript." } else { "Teams. Chat." };
            format!("{prefix} {title} - {}.md", meeting.unwrap_or(today))
        }
    };
    (name, l.join("\n"))
}

/// A free name in `dir` (as [`free_name`]), claimed by creating it empty, so two captures written at
/// the same moment (two host processes) can't both pick it. Its conflict copies are checked too; on
/// refusal the claim is let go.
fn claim_name(dir: &Path, name: &str) -> Result<String, WriteError> {
    for _ in 0..1000 {
        let name = free_name(dir, name);
        let abs = dir.join(&name);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&abs) {
            Ok(_) => {
                if let Err(e) = check_conflict_copies(&abs) {
                    let _ = std::fs::remove_file(&abs);
                    return Err(e);
                }
                return Ok(name);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(WriteError::Io(e.to_string())),
        }
    }
    Err(WriteError::Io(format!("Couldn't find a free name for {name}.")))
}

/// Writes `bytes` over the file `claim_name` made, or lets the claim go if that fails.
fn fill_claim(abs: &Path, bytes: &[u8]) -> Result<(), WriteError> {
    write_atomic(abs, bytes, false).inspect_err(|_| {
        let _ = std::fs::remove_file(abs);
    })
}

/// `name`, or the first `name-2`, `name-3`… not taken in `dir`.
pub fn free_name(dir: &Path, name: &str) -> String {
    let (base, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let mut candidate = name.to_string();
    let mut n = 2;
    while dir.join(&candidate).exists() {
        candidate = format!("{base}-{n}{ext}");
        n += 1;
    }
    candidate
}

/// Writes the capture into the vault's `sources/`; its vault path.
pub fn write(vault: &Path, c: &Capture, now: DateTime<Local>) -> Result<String, WriteError> {
    c.check().map_err(WriteError::Invalid)?;
    let dir = vault.join("sources");
    std::fs::create_dir_all(&dir).map_err(|e| WriteError::Io(e.to_string()))?;
    let (name, text) = render(c, now.with_timezone(&Utc), &now.format("%Y-%m-%d").to_string());
    let name = claim_name(&dir, &name)?;
    fill_claim(&dir.join(&name), text.as_bytes())?;
    Ok(format!("sources/{name}"))
}

/// The largest file that can be dropped onto Sources.
pub const MAX_IMPORT_BYTES: u64 = 200 * 1024 * 1024;

/// A file from outside the vault (dropped onto the Sources screen) copied into `sources/` under its
/// own name, or a free one; its vault path. Folders, files already in the vault and files over
/// [`MAX_IMPORT_BYTES`] are refused.
pub fn import_file(vault: &Path, from: &Path) -> Result<String, WriteError> {
    let name = from
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .ok_or_else(|| WriteError::Invalid("That isn't a file.".into()))?;
    let meta = std::fs::metadata(from).map_err(|e| WriteError::NotFound(format!("Couldn't read “{name}”: {e}")))?;
    if !meta.is_file() {
        return Err(WriteError::Invalid(format!("“{name}” is a folder: drop the files in it instead.")));
    }
    if meta.len() > MAX_IMPORT_BYTES {
        return Err(WriteError::Invalid(format!("“{name}” is over {} MB.", MAX_IMPORT_BYTES / 1024 / 1024)));
    }
    let (canon_from, canon_vault) = (crate::paths::real_path(from), crate::paths::real_path(vault));
    if canon_from.starts_with(&canon_vault) {
        return Err(WriteError::Exists(format!("“{name}” is already in the vault.")));
    }
    let dir = vault.join("sources");
    std::fs::create_dir_all(&dir).map_err(|e| WriteError::Io(e.to_string()))?;
    let bytes = std::fs::read(from).map_err(|e| WriteError::Io(format!("Couldn't read “{name}”: {e}")))?;
    let name = claim_name(&dir, &name)?;
    fill_claim(&dir.join(&name), &bytes)?;
    Ok(format!("sources/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn cap(v: serde_json::Value) -> Capture {
        serde_json::from_value(v).unwrap()
    }
    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 3, 8, 40, 0).unwrap()
    }

    #[test]
    fn titles_as_the_previous_app_cleans_them() {
        assert_eq!(outlook_title("(3) Re: Budget / Q3 - Outlook"), "Re Budget Q3");
        assert_eq!(outlook_title("Roadmap — Outlook on the web"), "Roadmap");
        assert_eq!(teams_title("Chat with Lena | Microsoft Teams"), "Chat with Lena");
        assert_eq!(clean_title("orbit-app-steerco"), "Orbit App Steerco");
        assert_eq!(clean_title("...  "), "untitled");
        assert_eq!(clean_title(&"x".repeat(100)).len(), 80);
        assert_eq!(clean_title("Re: [Orbit] #42 ^up | done"), "Re Orbit 42 up done");
        let wide = clean_title(&"日本".repeat(60));
        assert!(wide.len() <= TITLE_BYTES && wide.chars().count() <= 80, "{}", wide.len());
    }

    #[test]
    fn one_thread() {
        let c = cap(json!({"type": "outlook", "url": "https://outlook.office.com/mail/id/1", "title": "Re: Launch date - Outlook",
            "kind": "thread", "titleSource": "quoted-chain",
            "messages": [{"sender": "Maya", "to": "Lena", "cc": "Theo", "date": "Mon 28 Sep 09:12", "body": "  Moving to 14 November.\n"},
                         {"sender": "Lena", "recipients": "Maya", "body": "OK"}]}));
        let (name, text) = render(&c, at(), "2026-10-03");
        assert_eq!(name, "Email. Thread. Re Launch date - 2026-10-03.md");
        assert_eq!(
            text,
            "# Re Launch date\n\n**Source:** [Open in Outlook](https://outlook.office.com/mail/id/1)\n**Captured:** 2026-10-03T08:40:00.000Z\n\n---\n\n\
             ## Maya — Mon 28 Sep 09:12\n\n**To:** Lena\n\n**Cc:** Theo\n\nMoving to 14 November.\n\n---\n\n\
             ## Lena\n\n**To:** Maya\n\nOK\n\n---\n"
        );
    }

    #[test]
    fn reading_pane_and_batch() {
        let pane = cap(json!({"type": "outlook", "url": "https://outlook.office.com/x", "title": "Invite", "kind": "reading-pane",
            "messages": [{"sender": "(reading pane)", "body": "Steerco, Tuesday"}]}));
        let (name, text) = render(&pane, at(), "2026-10-03");
        assert_eq!(name, "Email. Item. Invite - 2026-10-03.md");
        assert!(text.contains("\n**Capture:** reading-pane text only"));

        let t = json!({"url": "https://outlook.office.com/a", "title": "First", "messages": [{"sender": "Maya", "body": "Hi"}]});
        let batch = cap(json!({"type": "outlook-batch", "threads": [t, t, t]}));
        let (name, text) = render(&batch, at(), "2026-10-03");
        assert_eq!(name, "Email. Threads. First +2 more - 2026-10-03.md");
        assert!(text.starts_with("# First +2 more\n\n**Captured:** 2026-10-03T08:40:00.000Z\n**Threads:** 3\n\n---\n\n## First\n\n**Source:** [Open in Outlook](https://outlook.office.com/a)\n\n### Maya\n\nHi\n\n---\n"));
        let labelled = cap(json!({"type": "outlook-batch", "label": " Northwind renewal ", "threads": [t]}));
        assert_eq!(render(&labelled, at(), "2026-10-03").0, "Email. Threads. Northwind renewal - 2026-10-03.md");
        let many = cap(json!({"type": "outlook-batch", "threads": vec![t; 26]}));
        assert!(many.check().is_err());
    }

    #[test]
    fn teams_chat_and_transcript() {
        let chat = cap(json!({"type": "teams", "url": "https://teams.microsoft.com/v2/", "title": "Chat with Lena - Microsoft Teams",
            "messages": [{"sender": "Lena", "date": "2026-10-02T09:40:00Z", "body": "Ready?"}], "passes": 3, "atTop": true}));
        let (name, text) = render(&chat, at(), "2026-10-03");
        assert_eq!(name, "Teams. Chat. Chat with Lena - 2026-10-03.md");
        assert!(text.contains("**Source:** [Open in Teams](https://teams.microsoft.com/v2/)\n"));
        assert!(text.ends_with("## Lena — 2026-10-02T09:40:00Z\n\nReady?\n\n---\n"));
        let tr = cap(json!({"type": "teams", "kind": "transcript", "url": "https://teams.microsoft.com/v2/", "title": "Orbit App Steerco",
            "messages": [{"sender": "Maya", "date": "0:14", "body": "Let's start."}]}));
        let (name, text) = render(&tr, at(), "2026-10-03");
        assert_eq!(name, "Teams. Transcript. Orbit App Steerco - 2026-10-03.md");
        assert!(text.contains("[Open meeting recap]"));
        let empty =
            cap(json!({"type": "teams", "url": "https://teams.microsoft.com/", "title": "T", "messages": [{"sender": "A", "body": ""}]}));
        assert!(empty.check().is_err());
        let nourl = cap(json!({"type": "teams", "url": "about:blank", "title": "T", "messages": [{"sender": "A", "body": "x"}]}));
        assert!(nourl.check().is_err());
    }

    #[test]
    fn a_transcript_is_named_with_the_meeting_day() {
        let tr = |extra: serde_json::Value| {
            let mut v = json!({"type": "teams", "kind": "transcript", "url": "https://teams.microsoft.com/v2/", "title": "Orbit App Steerco",
                "messages": [{"sender": "Maya", "date": "0:14", "body": "Let's start."}]});
            v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            cap(v)
        };
        let (name, text) = render(&tr(json!({"meetingDate": "2026-10-02", "meetingStart": "10:00"})), at(), "2026-10-03");
        assert_eq!(name, "Teams. Transcript. Orbit App Steerco - 2026-10-02.md");
        assert!(text.contains("**Source:** [Open meeting recap](https://teams.microsoft.com/v2/)\n**Meeting:** 2026-10-02 10:00\n**Captured:** 2026-10-03T08:40:00.000Z\n"));
        let (_, text) = render(&tr(json!({"meetingDate": "2026-10-02"})), at(), "2026-10-03");
        assert!(text.contains("\n**Meeting:** 2026-10-02\n**Captured:**"));
        // Nothing usable: the capture day, and no line.
        for bad in [json!({}), json!({"meetingDate": "2 Oct"}), json!({"meetingDate": "2026-02-31"}), json!({"meetingDate": null})] {
            let (name, text) = render(&tr(bad), at(), "2026-10-03");
            assert_eq!(name, "Teams. Transcript. Orbit App Steerco - 2026-10-03.md");
            assert!(!text.contains("**Meeting:**"));
        }
        let (_, text) = render(&tr(json!({"meetingDate": "2026-10-02", "meetingStart": "late"})), at(), "2026-10-03");
        assert!(text.contains("\n**Meeting:** 2026-10-02\n"));
        // A chat keeps the capture day.
        let chat = cap(json!({"type": "teams", "url": "https://teams.microsoft.com/v2/", "title": "Standup", "meetingDate": "2026-10-02",
            "messages": [{"sender": "Lena", "body": "Ready?"}]}));
        let (name, text) = render(&chat, at(), "2026-10-03");
        assert_eq!(name, "Teams. Chat. Standup - 2026-10-03.md");
        assert!(!text.contains("**Meeting:**"));
    }

    #[test]
    fn imports_a_dropped_file() {
        let vault = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let f = out.path().join("Board pack.pdf");
        std::fs::write(&f, b"%PDF").unwrap();
        assert_eq!(import_file(vault.path(), &f).unwrap(), "sources/Board pack.pdf");
        assert_eq!(import_file(vault.path(), &f).unwrap(), "sources/Board pack-2.pdf");
        assert_eq!(std::fs::read(vault.path().join("sources/Board pack.pdf")).unwrap(), b"%PDF");
        assert!(matches!(import_file(vault.path(), out.path()), Err(WriteError::Invalid(_))));
        assert!(matches!(import_file(vault.path(), &vault.path().join("sources/Board pack.pdf")), Err(WriteError::Exists(_))));
    }

    #[test]
    fn writes_with_a_free_name_and_refuses_beside_a_conflict_copy() {
        let dir = tempfile::tempdir().unwrap();
        let now = Local.with_ymd_and_hms(2026, 10, 3, 10, 0, 0).unwrap();
        let c = cap(
            json!({"type": "teams", "url": "https://teams.microsoft.com/", "title": "Standup", "messages": [{"sender": "A", "body": "x"}]}),
        );
        assert_eq!(write(dir.path(), &c, now).unwrap(), "sources/Teams. Chat. Standup - 2026-10-03.md");
        assert_eq!(write(dir.path(), &c, now).unwrap(), "sources/Teams. Chat. Standup - 2026-10-03-2.md");
        let other = cap(
            json!({"type": "teams", "url": "https://teams.microsoft.com/", "title": "Retro", "messages": [{"sender": "A", "body": "x"}]}),
        );
        std::fs::write(dir.path().join("sources/Teams. Chat. Retro - 2026-10-03 2.md"), "").unwrap();
        assert!(matches!(write(dir.path(), &other, now), Err(WriteError::Conflict(_))));
        // The refused capture's claim on the name is let go.
        assert!(!dir.path().join("sources/Teams. Chat. Retro - 2026-10-03.md").exists());
    }

    #[test]
    fn captures_at_the_same_moment_get_their_own_names() {
        let dir = tempfile::tempdir().unwrap();
        let now = Local.with_ymd_and_hms(2026, 10, 3, 10, 0, 0).unwrap();
        let c = cap(
            json!({"type": "teams", "url": "https://teams.microsoft.com/", "title": "Standup", "messages": [{"sender": "A", "body": "x"}]}),
        );
        let names: Vec<String> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..8).map(|_| s.spawn(|| write(dir.path(), &c, now).unwrap())).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let unique: std::collections::BTreeSet<_> = names.iter().collect();
        assert_eq!(unique.len(), 8, "{names:?}");
        assert!(names.iter().all(|n| std::fs::read_to_string(dir.path().join(n)).unwrap().contains("# Standup")));
    }

    /// The previous app's own routes, run on invented payloads (tests/fixtures/webcapture,
    /// recorded 2026-10-03): the same file names, in order (a second capture of a thread gets
    /// `-2`), and the same text, apart from the capture time and date.
    #[test]
    fn matches_the_previous_apps_recorded_output() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/webcapture/recorded.json");
        let cases: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
        let vault = tempfile::tempdir().unwrap();
        let now = Local.with_ymd_and_hms(2026, 10, 3, 9, 30, 0).unwrap();
        let captured = now.with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::Millis, true);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let c: Capture = serde_json::from_value(case["payload"].clone()).unwrap();
            let rel = write(vault.path(), &c, now).unwrap();
            assert_eq!(rel, format!("sources/{}", case["file"].as_str().unwrap().replace("{DATE}", "2026-10-03")), "{name}: file name");
            let body = std::fs::read_to_string(vault.path().join(&rel)).unwrap();
            assert_eq!(body, case["body"].as_str().unwrap().replace("{CAPTURED}", &captured), "{name}: text");
        }
    }
}
