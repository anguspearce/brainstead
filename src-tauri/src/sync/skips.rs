// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What to skip, learned from what the user deletes (D-20261118-15). Each synced email the user
//! moves to the Trash counts against its sender, and each synced chat day against its chat; once one
//! has been deleted [`AT`] times, Settings › Teams and Outlook sync suggests skipping it. Nothing is
//! skipped until the user says so (the window adds it to the skip list, then calls `forget`), and
//! **Not this one** stops it being suggested again. Kept in the app data folder, never the vault.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many deletes make a suggestion.
pub const AT: u32 = 3;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Learned {
    /// `sender:<address>` or `chat:<conversation id>` → how often it's been deleted.
    pub counts: BTreeMap<String, Count>,
    /// Suggestions the user turned down.
    pub dismissed: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Count {
    /// `sender` or `chat`.
    pub kind: String,
    /// The address, or the chat's conversation id: what goes in the skip list.
    pub value: String,
    /// What to call it: the address, or the chat's title.
    pub label: String,
    pub deletes: u32,
    /// When it was last deleted, `YYYY-MM-DD`.
    pub last: String,
}

pub fn file(data: &Path) -> PathBuf {
    data.join("skip-suggestions.json")
}

pub fn load(data: &Path) -> Learned {
    std::fs::read(file(data)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(data: &Path, l: &Learned) -> Result<(), String> {
    let b = serde_json::to_vec_pretty(l).map_err(|e| e.to_string())?;
    brainstead_core::write::write_atomic(&file(data), &b, false).map_err(|e| e.to_string())
}

/// What a deleted source counts against: the first `**From:**` of a synced email, or the chat a
/// synced chat day came from (by the sync's record of where it wrote it). None for anything else.
pub fn subject_of(rel: &str, text: &str, chat_of: impl Fn(&str) -> Option<String>) -> Option<(String, String, String)> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    if name.starts_with("Email. Thread. ") {
        let from = text.lines().find_map(|l| l.trim().strip_prefix("**From:**"))?.trim().to_ascii_lowercase();
        return (!from.is_empty() && from.contains('@')).then(|| ("sender".into(), from.clone(), from));
    }
    if name.starts_with("Teams. Chat. ") {
        let id = chat_of(rel)?;
        let title = text.lines().find_map(|l| l.strip_prefix("# ")).map(str::trim).unwrap_or(&id).to_string();
        return Some(("chat".into(), id, title));
    }
    None
}

/// Counts a delete of `rel` (its text read before it went), when it's a synced email or chat.
pub fn learn(data: &Path, rel: &str, text: &str, today: &str, chat_of: impl Fn(&str) -> Option<String>) -> Result<(), String> {
    let Some((kind, value, label)) = subject_of(rel, text, chat_of) else { return Ok(()) };
    let mut l = load(data);
    let c = l.counts.entry(format!("{kind}:{value}")).or_default();
    c.kind = kind;
    c.value = value;
    c.label = label;
    c.deletes += 1;
    c.last = today.to_string();
    save(data, &l)
}

/// A suggestion to skip a sender or a chat.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub key: String,
    pub kind: String,
    pub value: String,
    pub label: String,
    pub deletes: u32,
    pub last: String,
}

/// What's deleted often enough, not turned down and not skipped already, most deleted first.
pub fn suggestions(l: &Learned, skipped_senders: &[String], skipped_chats: &[String]) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = l
        .counts
        .iter()
        .filter(|(k, c)| c.deletes >= AT && !l.dismissed.contains(k))
        .filter(|(_, c)| match c.kind.as_str() {
            "sender" => !super::run::ignored(&c.value, skipped_senders),
            "chat" => !skipped_chats.contains(&c.value),
            _ => false,
        })
        .map(|(k, c)| Suggestion {
            key: k.clone(),
            kind: c.kind.clone(),
            value: c.value.clone(),
            label: c.label.clone(),
            deletes: c.deletes,
            last: c.last.clone(),
        })
        .collect();
    out.sort_by(|a, b| b.deletes.cmp(&a.deletes).then_with(|| a.label.cmp(&b.label)));
    out
}

/// Turned down: not suggested again, however often it's deleted.
pub fn dismiss(data: &Path, key: &str) -> Result<(), String> {
    let mut l = load(data);
    if !l.dismissed.iter().any(|k| k == key) {
        l.dismissed.push(key.to_string());
    }
    save(data, &l)
}

/// Accepted (it's on a skip list now) or taken off one: its count starts again.
pub fn forget(data: &Path, key: &str) -> Result<(), String> {
    let mut l = load(data);
    l.counts.remove(key);
    l.dismissed.retain(|k| k != key);
    save(data, &l)
}

/// A chat's title, from what was learned of it: for the skipped chats' list.
pub fn chat_names(l: &Learned) -> BTreeMap<String, String> {
    l.counts.values().filter(|c| c.kind == "chat").map(|c| (c.value.clone(), c.label.clone())).collect()
}

/// The conversation a synced chat day's source came from, by where the sync wrote it.
pub fn chat_of(data: &Path, rel: &str) -> Option<String> {
    let st = super::state::load(data);
    st.chat_paths.iter().find(|(_, p)| p.as_str() == rel).and_then(|(k, _)| k.split('\n').next()).map(str::to_string)
}

/// A source the user is moving to the Trash: counted, if it's a synced email or chat. Never stops
/// the move.
pub fn on_delete(rel: &str, text: &str) {
    let data = crate::platform::data_dir();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if let Err(e) = learn(&data, rel, text, &today, |r| chat_of(&data, r)) {
        crate::applog!("sync: couldn't count a delete for skip suggestions: {e}");
    }
}

/// The suggestions, and the titles of chats learned, for Settings › Teams and Outlook sync.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    suggestions: Vec<Suggestion>,
    chat_names: BTreeMap<String, String>,
}

#[tauri::command]
pub fn skip_suggestions(st: tauri::State<crate::AppState>) -> Listed {
    let s = st.settings.lock().unwrap().clone();
    let l = load(&crate::platform::data_dir());
    Listed { suggestions: suggestions(&l, &s.sync_ignore_senders, &s.sync_ignore_chats), chat_names: chat_names(&l) }
}

/// **Not this one**: never suggested again.
#[tauri::command]
pub fn skip_suggestion_dismiss(key: String) -> Result<(), String> {
    dismiss(&crate::platform::data_dir(), &key)
}

/// After **Skip** put it on a skip list (the window does), or it came off one: counted afresh.
#[tauri::command]
pub fn skip_suggestion_forget(key: String) -> Result<(), String> {
    forget(&crate::platform::data_dir(), &key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIL: &str = "# Weekly offers\n\n**Source:** [Open in Outlook](https://outlook.office.com/mail/)\n\n---\n\n## Deals Team — 2026-11-17 09:00\n\n**From:** Offers@Shop.Example\n\n**To:** Sam\n\nBuy now.\n";

    #[test]
    fn deletes_count_against_a_sender_or_chat_and_suggest_a_skip_at_three() {
        let t = tempfile::tempdir().unwrap();
        let data = t.path();
        let chat = |rel: &str| (rel == "sources/teams/Teams. Chat. Lunch club - 2026-11-17.md").then(|| "19:abc".to_string());
        for _ in 0..2 {
            learn(data, "sources/emails/Email. Thread. Weekly offers - 2026-11-17.md", MAIL, "2026-11-17", chat).unwrap();
        }
        assert!(suggestions(&load(data), &[], &[]).is_empty(), "two isn't enough");
        learn(data, "sources/emails/Email. Thread. Weekly offers - 2026-11-18.md", MAIL, "2026-11-18", chat).unwrap();
        let s = suggestions(&load(data), &[], &[]);
        assert_eq!(s.len(), 1);
        assert_eq!(
            (s[0].key.as_str(), s[0].value.as_str(), s[0].deletes, s[0].last.as_str()),
            ("sender:offers@shop.example", "offers@shop.example", 3, "2026-11-18")
        );
        // Skipped already (by the address or its whole domain): not suggested.
        assert!(suggestions(&load(data), &["shop.example".into()], &[]).is_empty());
        // A chat by its conversation id, called by its title.
        for _ in 0..3 {
            learn(data, "sources/teams/Teams. Chat. Lunch club - 2026-11-17.md", "# Lunch club\n\nhi", "2026-11-18", chat).unwrap();
        }
        let s = suggestions(&load(data), &[], &[]);
        assert_eq!(
            s.iter().map(|x| (x.kind.as_str(), x.label.as_str())).collect::<Vec<_>>(),
            [("chat", "Lunch club"), ("sender", "offers@shop.example")]
        );
        assert_eq!(chat_names(&load(data)).get("19:abc").map(String::as_str), Some("Lunch club"));
        // Turned down: gone for good; accepted: its count starts again.
        dismiss(data, "chat:19:abc").unwrap();
        forget(data, "sender:offers@shop.example").unwrap();
        assert!(suggestions(&load(data), &[], &[]).is_empty());
        // Anything else isn't counted.
        learn(data, "sources/Report.pdf", "", "2026-11-18", chat).unwrap();
        learn(data, "Me. Scratchpad.md", "**From:** x@y.z", "2026-11-18", chat).unwrap();
        assert_eq!(load(data).counts.len(), 1);
    }
}
