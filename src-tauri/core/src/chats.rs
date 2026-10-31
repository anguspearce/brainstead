// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved chats, as the previous app saves them (its `bff/src/vault/conversations.ts`), so either app
//! lists, opens and resumes the other's: `Chat. <title>.md` at the vault's top level, with the
//! metadata as properties, a readable copy of the turns, and the whole transcript as base64 JSON
//! in an HTML comment, which is what's read back (the prose never is).
//!
//! Brainstead adds `sessionId` for the other CLIs' sessions; `claudeSessionId` holds Claude's
//! only, so the previous app never tries to resume a Codex thread with Claude.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CHAT_TYPE: &str = "Chat";
/// How many unsaved, unpinned chats are kept besides the open tabs; older ones are deleted.
pub const ARCHIVE_RETENTION: usize = 20;
/// The transcript's marker as Brainstead writes it.
const OPEN: &str = "<!-- brainstead:chat-transcript";
/// The marker the previous app wrote, still read so its chats open (joined from two pieces so the
/// previous app's name isn't spelt out here).
const LEGACY_OPEN: &str = concat!("<!-- waynes", "-world:chat-transcript");
const CLOSE: &str = "-->";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    /// user, assistant, tool, system or error.
    pub role: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Vault-relative file name: the handle for reading, renaming and trashing.
    pub filename: String,
    /// Stable across renames.
    pub id: String,
    /// From the file name, which is what the user renames.
    pub title: String,
    pub claude_session_id: Option<String>,
    /// Another CLI's session (Brainstead only).
    pub session_id: Option<String>,
    pub model: Option<String>,
    /// The effort level the chat runs at (Brainstead only): one of its CLI's levels
    /// (`ask::Cli::efforts`), or None for the CLI's own default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub context_tokens: Option<u64>,
    pub context_window: Option<u64>,
    pub compactions: u64,
    /// "pinned" or "archived".
    pub state: String,
    /// "review" for a scheduled review's run (Brainstead only); None for a chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    #[serde(flatten)]
    pub summary: Summary,
    pub transcript: Vec<Item>,
}

/// A file name component as the previous app cleans it: no `/ \ : * ? " < > |`, spaces collapsed.
pub fn clean(s: &str) -> String {
    let s: String = s.chars().filter(|c| !r#"/\:*?"<>|"#.contains(*c)).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `Chat. <title>.md`, then `Chat. <title> (2).md`… for the first that `taken` says is free.
pub fn filename_for(title: &str, taken: impl Fn(&str) -> bool) -> String {
    let base = match clean(title.strip_suffix(".md").unwrap_or(title)) {
        t if t.is_empty() => "Untitled chat".to_string(),
        t => t,
    };
    for n in 1..1000 {
        let name = if n == 1 { format!("{CHAT_TYPE}. {base}.md") } else { format!("{CHAT_TYPE}. {base} ({n}).md") };
        if !taken(&name) {
            return name;
        }
    }
    format!("{CHAT_TYPE}. {base} {}.md", std::process::id())
}

/// The title a chat's file name gives it.
pub fn title_of(filename: &str) -> String {
    let stem = filename.rsplit('/').next().unwrap_or(filename);
    let stem = stem.strip_suffix(".md").unwrap_or(stem);
    stem.strip_prefix(&format!("{CHAT_TYPE}. ")).unwrap_or(stem).to_string()
}

pub fn is_chat_file(rel: &str) -> bool {
    !rel.contains('/') && rel.starts_with(&format!("{CHAT_TYPE}. ")) && rel.ends_with(".md")
}

/// A YAML value that reads back as the same string in any YAML parser (a JSON string is one):
/// an ISO time left bare would come back as a date.
fn y(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// The readable copy of the turns, then the transcript to read back.
pub fn render_body(c: &Chat) -> String {
    let s = &c.summary;
    let resumable = s.claude_session_id.is_some() || s.session_id.is_some();
    let mut out = vec![
        format!("> Chat · {} · {}", s.model.as_deref().unwrap_or("claude"), if resumable { "resumable" } else { "view-only" }),
        String::new(),
    ];
    for it in &c.transcript {
        let t = it.text.trim();
        if t.is_empty() {
            continue;
        }
        out.push(match it.role.as_str() {
            "user" => format!("**You:** {t}"),
            "assistant" => t.to_string(),
            "tool" => format!("> ⚙ `{t}`"),
            "error" => format!("> ⚠ {t}"),
            _ => format!("> _{t}_"),
        });
        out.push(String::new());
    }
    let body = out.join("\n");
    let json = serde_json::to_string(&c.transcript).unwrap_or_else(|_| "[]".into());
    // Base64, so a message containing `-->` can't end the comment early.
    let payload = base64::engine::general_purpose::STANDARD.encode(json);
    format!("{}\n\n{OPEN}\n{payload}\n{CLOSE}\n", body.trim_end())
}

/// The whole file.
pub fn serialize(c: &Chat) -> String {
    let s = &c.summary;
    let mut fm = vec![
        "---".to_string(),
        "type: chat".into(),
        format!("id: {}", y(&s.id)),
        format!("title: {}", y(&s.title)),
        format!("claudeSessionId: {}", y(s.claude_session_id.as_deref().unwrap_or(""))),
    ];
    if let Some(sid) = &s.session_id {
        fm.push(format!("sessionId: {}", y(sid)));
    }
    if let Some(k) = &s.kind {
        fm.push(format!("kind: {}", y(k)));
    }
    if let Some(e) = &s.effort {
        fm.push(format!("effort: {}", y(e)));
    }
    fm.extend([
        format!("model: {}", y(s.model.as_deref().unwrap_or(""))),
        format!("createdAt: {}", y(&s.created_at)),
        format!("updatedAt: {}", y(&s.updated_at)),
        format!("contextTokens: {}", s.context_tokens.unwrap_or(0)),
        format!("contextWindow: {}", s.context_window.unwrap_or(0)),
        format!("compactions: {}", s.compactions),
        format!("state: {}", if s.state == "pinned" { "pinned" } else { "archived" }),
        "---".into(),
    ]);
    format!("{}\n{}", fm.join("\n"), render_body(c))
}

/// The transcript in a chat file's text; empty when it's missing or unreadable (the chat still lists).
pub fn parse_transcript(text: &str) -> Vec<Item> {
    let Some((start, marker)) = [OPEN, LEGACY_OPEN].into_iter().find_map(|m| text.find(m).map(|i| (i, m))) else { return Vec::new() };
    let from = start + marker.len();
    let Some(end) = text[from..].find(CLOSE) else { return Vec::new() };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(text[from..from + end].trim()) else { return Vec::new() };
    let Ok(Value::Array(items)) = serde_json::from_slice::<Value>(&bytes) else { return Vec::new() };
    const ROLES: [&str; 5] = ["user", "assistant", "tool", "system", "error"];
    items.into_iter().filter_map(|v| serde_json::from_value::<Item>(v).ok()).filter(|i| ROLES.contains(&i.role.as_str())).collect()
}

/// The metadata from a chat file's text; `mtime` (ISO) stands in for missing times.
pub fn parse_summary(filename: &str, text: &str, mtime: &str) -> Summary {
    let fm = crate::frontmatter::split(text);
    let d = &fm.data;
    let st = |k: &str| d[k].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let num = |k: &str| d[k].as_u64().filter(|n| *n > 0);
    Summary {
        filename: filename.to_string(),
        id: st("id").unwrap_or_else(|| filename.to_string()),
        title: title_of(filename),
        claude_session_id: st("claudeSessionId"),
        session_id: st("sessionId"),
        model: st("model"),
        effort: st("effort"),
        created_at: st("createdAt").unwrap_or_else(|| mtime.to_string()),
        updated_at: st("updatedAt").unwrap_or_else(|| mtime.to_string()),
        context_tokens: num("contextTokens"),
        context_window: num("contextWindow"),
        compactions: d["compactions"].as_u64().unwrap_or(0),
        state: if st("state").as_deref() == Some("pinned") { "pinned".into() } else { "archived".into() },
        kind: st("kind"),
    }
}

pub fn parse(filename: &str, text: &str, mtime: &str) -> Chat {
    Chat { summary: parse_summary(filename, text, mtime), transcript: parse_transcript(text) }
}

/// Unpinned chats beyond the retention, oldest first out: the unsaved chats deleted after a write.
pub fn beyond_retention(all: &[Summary]) -> Vec<String> {
    let mut archived: Vec<&Summary> = all.iter().filter(|s| s.state != "pinned").collect();
    archived.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    archived.into_iter().skip(ARCHIVE_RETENTION).map(|s| s.filename.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat() -> Chat {
        Chat {
            summary: Summary {
                filename: "Chat. What's due.md".into(),
                id: "u1".into(),
                title: "What's due".into(),
                claude_session_id: Some("s1".into()),
                model: Some("claude:sonnet".into()),
                created_at: "2026-10-02T08:00:00.000Z".into(),
                updated_at: "2026-10-02T08:05:00.000Z".into(),
                context_tokens: Some(1200),
                context_window: Some(200000),
                state: "archived".into(),
                ..Default::default()
            },
            transcript: vec![
                Item { id: "1".into(), role: "user".into(), text: "What's due? -->".into(), meta: None },
                Item { id: "2".into(), role: "assistant".into(), text: "Two tasks.".into(), meta: Some("claude:sonnet".into()) },
                Item { id: "3".into(), role: "error".into(), text: "Oops".into(), meta: None },
            ],
        }
    }

    #[test]
    fn round_trips_and_reads_like_the_previous_app() {
        let c = chat();
        let text = serialize(&c);
        assert!(text.starts_with("---\ntype: chat\nid: \"u1\"\n"));
        assert!(text.contains("createdAt: \"2026-10-02T08:00:00.000Z\"\n"));
        assert!(text.contains("\n---\n> Chat · claude:sonnet · resumable\n\n**You:** What's due? -->\n\nTwo tasks.\n\n> ⚠ Oops\n\n<!-- brainstead:chat-transcript\n"));
        let back = parse(&c.summary.filename, &text, "x");
        assert_eq!(back, c);
        assert!(!text.contains("effort:"));
        // A chat's own effort is kept beside its model.
        let mut e = chat();
        e.summary.effort = Some("high".into());
        let text = serialize(&e);
        assert!(text.contains(
            "
effort: \"high\"
"
        ));
        assert_eq!(parse(&e.summary.filename, &text, "x"), e);
    }

    #[test]
    fn reads_a_file_from_the_previous_app() {
        // As gray-matter writes it: js-yaml quoting, no sessionId.
        let json = r#"[{"id":"a","role":"user","text":"hi"},{"id":"b","role":"bogus","text":"x"}]"#;
        let b64 = base64::engine::general_purpose::STANDARD.encode(json);
        let text = format!(
            "---\ntype: chat\nid: 9f\ntitle: Old\nclaudeSessionId: ''\nmodel: claude-sonnet-5\ncreatedAt: '2026-09-01T10:00:00.000Z'\nupdatedAt: '2026-09-01T11:00:00.000Z'\ncontextTokens: 0\ncontextWindow: 0\ncompactions: 2\nstate: pinned\n---\n> Chat\n\n{LEGACY_OPEN}\n{b64}\n{CLOSE}\n"
        );
        let c = parse("Chat. Renamed.md", &text, "m");
        assert_eq!(c.summary.title, "Renamed");
        assert_eq!(c.summary.claude_session_id, None);
        assert_eq!(c.summary.context_tokens, None);
        assert_eq!(c.summary.compactions, 2);
        assert_eq!(c.summary.state, "pinned");
        assert_eq!(c.summary.updated_at, "2026-09-01T11:00:00.000Z");
        assert_eq!(c.transcript.len(), 1);
        assert_eq!(parse("Chat. X.md", "no marker", "m").transcript, vec![]);
    }

    #[test]
    fn names_files() {
        assert_eq!(filename_for("What: is due?", |_| false), "Chat. What is due.md");
        assert_eq!(filename_for("  ", |_| false), "Chat. Untitled chat.md");
        assert_eq!(filename_for("A", |n| n == "Chat. A.md"), "Chat. A (2).md");
        assert!(is_chat_file("Chat. A.md") && !is_chat_file("wiki/Chat. A.md") && !is_chat_file("Meeting. A.md"));
    }

    #[test]
    fn keeps_twenty_archived() {
        let all: Vec<Summary> = (0..23)
            .map(|i| Summary {
                filename: format!("Chat. {i}.md"),
                updated_at: format!("2026-10-{:02}", i + 1),
                state: if i == 0 { "pinned".into() } else { "archived".into() },
                ..Default::default()
            })
            .collect();
        assert_eq!(beyond_retention(&all), ["Chat. 2.md", "Chat. 1.md"]);
    }
}
