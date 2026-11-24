// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pictures pasted into Teams chats (D-20261124-02). Teams keeps them in its own picture store, not
//! in the message, so the sync downloads each once with the chat sign-in, writes it to `sources/` as
//! its own source (noted like a capture, so it reaches the Inbox and ingest reads the text in it), and
//! the chat's day source shows it under the message that carried it. A picture that couldn't be
//! brought in is still listed under its message, saying so, and is tried again when the day is next
//! written. A day whose source was written before this, with a picture never tried, is written again
//! once to bring it in.

use chrono::Local;

use super::attachments::named;
use super::fetch::ChatMessage;
use super::state::State;
use crate::nativehost;

/// A picture's key in the state's attachment paths: its chat and its id in Teams' picture store. An
/// empty path there means it was tried and couldn't be brought in.
pub fn picture_key(conv: &str, object: &str) -> String {
    format!("{conv}\npicture\n{object}")
}

/// Downloads a picture from its address: its bytes and content type.
pub type Get<'a> = &'a dyn Fn(&str) -> Result<(Vec<u8>, Option<String>), String>;

/// Whether any of a day's pictures has never been tried.
pub fn untried(st: &State, conv: &str, msgs: &[ChatMessage]) -> bool {
    msgs.iter().flat_map(|m| &m.pictures).any(|p| st.attachment_path(&picture_key(conv, &p.object)).is_none())
}

/// The file extension for a picture: from its content type, else its first bytes, else png.
fn extension(kind: Option<&str>, bytes: &[u8]) -> &'static str {
    let kind = kind.unwrap_or("").to_ascii_lowercase();
    match () {
        _ if kind.contains("jpeg") || kind.contains("jpg") || bytes.starts_with(&[0xFF, 0xD8, 0xFF]) => "jpg",
        _ if kind.contains("gif") || bytes.starts_with(b"GIF8") => "gif",
        _ if kind.contains("webp") || (bytes.len() > 12 && &bytes[8..12] == b"WEBP") => "webp",
        _ => "png",
    }
}

/// Brings in the day's pictures not brought in before, filling each one's `path`, or its `note` when
/// it isn't there. `get` downloads a picture: its bytes and content type.
#[allow(clippy::too_many_arguments)]
pub fn for_day(
    data: &std::path::Path,
    vault: &std::path::Path,
    conv: &str,
    title: &str,
    msgs: &mut [ChatMessage],
    st: &mut State,
    problems: &mut Vec<String>,
    get: Get,
) {
    for m in msgs.iter_mut() {
        let day = m.time.with_timezone(&Local).format("%Y-%m-%d").to_string();
        let when = m.time.to_rfc3339();
        for p in m.pictures.iter_mut() {
            let key = picture_key(conv, &p.object);
            if let Some(path) = st.attachment_path(&key).filter(|p| !p.is_empty()) {
                // Brought in before. Not fetched again if it has since been moved to the Trash.
                if vault.join(path).is_file() {
                    p.path = Some(path.to_string());
                } else {
                    p.note = Some("removed from Sources".into());
                }
                continue;
            }
            let written = get(&p.url).and_then(|(bytes, kind)| {
                let name = named("Teams. Picture.", &format!("{title}.{}", extension(kind.as_deref(), &bytes)), &day);
                nativehost::deliver_file(data, vault, "teams", "Teams chat picture", &name, &bytes, Some(when.clone()))
            });
            match written {
                Ok(path) => {
                    st.mark_attachment(&key, &path);
                    p.path = Some(path);
                    p.note = None;
                }
                Err(e) => {
                    st.mark_attachment(&key, "");
                    problems.push(format!("Couldn't bring in a picture from a Teams chat: {e}"));
                    p.note = Some("a picture Brainstead couldn't bring in; tried again when the chat next changes".into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::fetch::{self, ChatPicture};
    use chrono::{TimeZone, Utc};

    fn msg(objects: &[&str]) -> ChatMessage {
        ChatMessage {
            id: "1".into(),
            sender: "Maya".into(),
            body: String::new(),
            time: Utc.with_ymd_and_hms(2026, 11, 23, 9, 0, 0).unwrap(),
            files: vec![],
            pictures: objects
                .iter()
                .map(|o| ChatPicture {
                    object: o.to_string(),
                    url: format!("https://eu-api.asm.skype.com/v1/objects/{o}/views/imgo"),
                    ..Default::default()
                })
                .collect(),
        }
    }

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0, 0];

    #[test]
    fn brings_a_picture_in_once_and_shows_it_under_its_message() {
        let d = tempfile::tempdir().unwrap();
        let vault = d.path().join("vault");
        let mut st = State::default();
        let mut problems = Vec::new();
        let mut msgs = vec![msg(&["0-weu-d1-abc"])];
        assert!(untried(&st, "19:c", &msgs));
        let ok = |_: &str| Ok((PNG.to_vec(), Some("image/png".to_string())));
        for_day(d.path(), &vault, "19:c", "Steerco", &mut msgs, &mut st, &mut problems, &ok);
        let path = msgs[0].pictures[0].path.clone().unwrap();
        assert!(path.starts_with("sources/Teams. Picture. Steerco - "), "{path}");
        assert!(path.ends_with(".png"));
        assert_eq!(std::fs::read(vault.join(&path)).unwrap(), PNG);
        assert!(problems.is_empty());
        assert!(!untried(&st, "19:c", &msgs));

        // Read again: taken from the state, not downloaded again.
        let mut again = vec![msg(&["0-weu-d1-abc"])];
        let never = |_: &str| -> Result<(Vec<u8>, Option<String>), String> { panic!("downloaded twice") };
        for_day(d.path(), &vault, "19:c", "Steerco", &mut again, &mut st, &mut problems, &never);
        assert_eq!(again[0].pictures[0].path.as_deref(), Some(path.as_str()));

        // The chat's source shows it under its message.
        let conv = fetch::Conv { id: "19:c".into(), topic: String::new(), kind: fetch::ConvKind::OneOnOne };
        let cap = fetch::chat_capture(&conv, &again, "Me", "https://teams.microsoft.com", "2026-11-23", "Maya").unwrap();
        let (_, text) = brainstead_core::webcapture::render(&cap, Utc::now(), "2026-11-24");
        assert!(text.contains(&format!("![[{path}|Picture]]")), "{text}");

        // Moved to the Trash: not fetched again, and said so.
        std::fs::remove_file(vault.join(&path)).unwrap();
        let mut gone = vec![msg(&["0-weu-d1-abc"])];
        for_day(d.path(), &vault, "19:c", "Steerco", &mut gone, &mut st, &mut problems, &never);
        assert_eq!(gone[0].pictures[0].note.as_deref(), Some("removed from Sources"));
    }

    #[test]
    fn a_picture_that_cant_come_is_listed_and_not_retried_every_pass() {
        let d = tempfile::tempdir().unwrap();
        let vault = d.path().join("vault");
        let mut st = State::default();
        let mut problems = Vec::new();
        let mut msgs = vec![msg(&["0-weu-d1-bad"])];
        let fail = |_: &str| -> Result<(Vec<u8>, Option<String>), String> { Err("eu-api.asm.skype.com answered 403".into()) };
        for_day(d.path(), &vault, "19:c", "Steerco", &mut msgs, &mut st, &mut problems, &fail);
        assert!(msgs[0].pictures[0].path.is_none());
        assert!(msgs[0].pictures[0].note.as_deref().unwrap().contains("couldn't bring in"));
        assert_eq!(problems.len(), 1);
        assert!(!untried(&st, "19:c", &msgs), "a failed picture doesn't make every pass rewrite the day");
        // When the day is written again, it's tried again.
        let ok = |_: &str| Ok((vec![0xFF, 0xD8, 0xFF, 0xE0], None));
        for_day(d.path(), &vault, "19:c", "Steerco", &mut msgs, &mut st, &mut problems, &ok);
        assert!(msgs[0].pictures[0].path.as_deref().unwrap().ends_with(".jpg"));
    }
}
