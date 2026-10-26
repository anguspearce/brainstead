// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Email attachments the sync brings into `sources/` with their thread (Settings › Teams and Outlook
//! sync › Bring in email attachments, on by default). A file ingest can read (PDF, Word, PowerPoint,
//! Excel, text) up to [`MAX_BYTES`] is downloaded once, written as its own source and noted like a
//! capture, so it reaches the Inbox and is ingested as it arrives; the thread's source links to it
//! under the message that carried it. Inline pictures (signature logos) are left out, and anything
//! else is listed by name with why it wasn't brought in.

use chrono::Local;
use reqwest::blocking::Client;

use super::fetch::{self, AttachmentKind, Mail, MailAttachment};
use super::state::{self, State};
use crate::nativehost;
use brainstead_core::webcapture::Attachment;

/// The largest attachment brought in. Checked against a sample mailbox: one attachment in a few
/// hundred was over 25 MB, none over 30.
pub const MAX_BYTES: u64 = 30 * 1024 * 1024;

/// Where attachments are written and noted.
pub struct Target<'a> {
    pub data: &'a std::path::Path,
    pub vault: &'a std::path::Path,
}

/// An HTTP client with time for a large download.
pub fn client() -> Client {
    Client::builder().timeout(std::time::Duration::from_secs(300)).build().expect("can build an http client")
}

/// Why a file attachment isn't brought in, or None when it is.
pub fn skip_reason(a: &MailAttachment) -> Option<&'static str> {
    if a.kind == AttachmentKind::Item {
        return Some("an attached email or invitation, not brought in");
    }
    let readable = brainstead_core::extract::ingestable(&a.name) && brainstead_core::extract::kind_of(&a.name) != Some("image");
    if !readable {
        return Some("not brought in: not a kind of file ingest reads");
    }
    if a.size > MAX_BYTES {
        return Some("not brought in: over 30 MB");
    }
    None
}

/// The source file's name: `Email. Attachment. <name> - <day>.<ext>`, the name cleaned of
/// characters that break links or file names and kept short.
pub fn source_name(name: &str, day: &str) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !e.is_empty() && e.len() <= 8 && !e.contains(' ') => (s, Some(e.to_ascii_lowercase())),
        _ => (name, None),
    };
    let cleaned: String = stem.chars().map(|c| if c.is_control() || "[]|#^\\/:*?\"<>".contains(c) { ' ' } else { c }).collect::<String>();
    let mut stem = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if stem.chars().count() > 80 {
        stem = stem.chars().take(80).collect::<String>().trim_end().to_string();
    }
    if stem.is_empty() {
        stem = "attachment".into();
    }
    match ext {
        Some(e) => format!("Email. Attachment. {stem} - {day}.{e}"),
        None => format!("Email. Attachment. {stem} - {day}"),
    }
}

/// Each message's attachments, by position, bringing in the ones not brought in before. Messages
/// without attachments, and inline pictures, give no entries. A failed download is listed by name
/// and recorded in `problems`; it's tried again the next time the thread is written.
pub fn for_thread(t: &Target, msgs: &[Mail], st: &mut State, problems: &mut Vec<String>) -> Vec<Vec<Attachment>> {
    let list_client = super::client();
    let mut download_client: Option<Client> = None;
    msgs.iter()
        .map(|m| {
            if !m.has_attachments || m.id.is_empty() {
                return Vec::new();
            }
            let found = match fetch::attachments(&list_client, &m.id) {
                Ok(a) => a,
                Err(e) => {
                    problems.push(format!("Couldn't list an email's attachments: {e}"));
                    return Vec::new();
                }
            };
            let received = m.received.to_rfc3339();
            found
                .into_iter()
                .filter(|a| !a.inline)
                .map(|a| {
                    let mut out = Attachment { name: a.name.clone(), ..Default::default() };
                    if a.kind == AttachmentKind::Reference {
                        out.url = a.url.clone();
                        out.note = Some("a shared link, not brought in".into());
                        return out;
                    }
                    if let Some(why) = skip_reason(&a) {
                        out.note = Some(why.into());
                        return out;
                    }
                    let key = state::attachment_key(&m.conversation_id, &received, &a.name);
                    if let Some(path) = st.attachment_path(&key) {
                        // Brought in before. Not fetched again if it has since been moved to the Trash.
                        if t.vault.join(path).is_file() {
                            out.path = Some(path.to_string());
                        } else {
                            out.note = Some("removed from Sources".into());
                        }
                        return out;
                    }
                    let client = download_client.get_or_insert_with(client);
                    let day = m.received.with_timezone(&Local).format("%Y-%m-%d").to_string();
                    let written = fetch::attachment_bytes(client, &m.id, &a.id).and_then(|bytes| {
                        nativehost::deliver_file(
                            t.data,
                            t.vault,
                            "outlook",
                            "Email attachment",
                            &source_name(&a.name, &day),
                            &bytes,
                            Some(received.clone()),
                        )
                    });
                    match written {
                        Ok(path) => {
                            st.mark_attachment(&key, &path);
                            out.path = Some(path);
                        }
                        Err(e) => {
                            problems.push(format!("Couldn't bring in the attachment “{}”: {e}", a.name));
                            out.note = Some("couldn't be brought in; tried again when the thread next changes".into());
                        }
                    }
                    out
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn att(name: &str, size: u64, kind: AttachmentKind) -> MailAttachment {
        MailAttachment { id: "a".into(), name: name.into(), size, inline: false, kind, url: None }
    }

    #[test]
    fn brings_in_readable_files_up_to_the_limit() {
        assert_eq!(skip_reason(&att("Proposal.pdf", 2_000_000, AttachmentKind::File)), None);
        assert_eq!(skip_reason(&att("Plan.DOCX", MAX_BYTES, AttachmentKind::File)), None, "at the limit, any case");
        assert_eq!(skip_reason(&att("notes.txt", 10, AttachmentKind::File)), None);
        assert!(skip_reason(&att("Deck.pptx", MAX_BYTES + 1, AttachmentKind::File)).unwrap().contains("30 MB"));
        assert!(skip_reason(&att("photo.png", 10, AttachmentKind::File)).is_some(), "pictures aren't brought in");
        assert!(skip_reason(&att("invite.ics", 10, AttachmentKind::File)).is_some());
        assert!(skip_reason(&att("old.doc", 10, AttachmentKind::File)).is_some(), "ingest doesn't read .doc");
        assert!(skip_reason(&att("Fwd.pdf", 10, AttachmentKind::Item)).is_some(), "an attached email");
    }

    #[test]
    fn names_the_source_after_the_attachment_and_its_day() {
        assert_eq!(source_name("Orbit App proposal.PDF", "2026-10-27"), "Email. Attachment. Orbit App proposal - 2026-10-27.pdf");
        assert_eq!(source_name("Q3 [draft] | v2: final?.docx", "2026-10-27"), "Email. Attachment. Q3 draft v2 final - 2026-10-27.docx");
        assert_eq!(source_name(".pdf", "2026-10-27"), "Email. Attachment. attachment - 2026-10-27.pdf");
        let long = source_name(&format!("{}.xlsx", "a".repeat(200)), "2026-10-27");
        assert_eq!(long, format!("Email. Attachment. {} - 2026-10-27.xlsx", "a".repeat(80)));
    }
}
