// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Teams and Outlook sync: reads the user's own chats, meeting transcripts, Inbox mail and calendar
//! and writes them into `sources/` through the same path the capture extensions use, so the Inbox
//! and "Ingest new sources as they arrive" work unchanged (D-20261025-01, D-20261025-02). The
//! sign-in is handed over by the Teams extension while a Teams tab is open (`auth`); macOS and Windows
//! now. Everything the sync reads is the user's own, and it only ever reads.

pub mod api;
pub mod auth;
pub mod fetch;
pub mod html;
pub mod pull;
pub mod run;
pub mod state;

use reqwest::blocking::Client;
use std::time::Duration;

use crate::platform;

pub use run::watch;

/// An HTTP client for the sync's calls.
pub fn client() -> Client {
    Client::builder().timeout(Duration::from_secs(30)).build().expect("can build an http client")
}

/// Takes the sign-in a Teams-extension handover carried (`auth::from_handover`) and keeps it. Called
/// from the native host when the extension hands over.
pub fn accept_handover(v: &serde_json::Value) -> Result<(), String> {
    let sign_in = auth::from_handover(v).ok_or("The Teams sign-in handover had no live sign-in to use.")?;
    auth::store(&sign_in)
}

/// A meeting for the Today screen (dates as RFC 3339 strings, so no extra chrono features are needed).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingDto {
    pub subject: String,
    pub start: String,
    pub end: String,
    pub organizer: String,
    pub online: bool,
}

/// What Settings shows about the Teams and Outlook sign-in.
#[tauri::command]
pub fn sync_sign_in_state() -> auth::SignInState {
    auth::state()
}

/// The user signs out of the sync (Settings): forgets the sign-in.
#[tauri::command]
pub fn sync_sign_out() -> Result<(), String> {
    auth::forget()
}

/// Runs one sync pass now (Settings › "Sync now").
#[tauri::command]
pub async fn sync_now() -> run::Report {
    tauri::async_runtime::spawn_blocking(|| run::run_once(&platform::data_dir())).await.unwrap_or_default()
}

/// On-demand pull for an assistant: gather the chats, mail and transcripts about `query` over the
/// last `days` days into the vault (reusing what's there) and return the source paths to read.
#[tauri::command]
pub async fn sync_pull(query: Option<String>, days: Option<u32>, kinds: Option<Vec<String>>) -> pull::Outcome {
    tauri::async_runtime::spawn_blocking(move || {
        let q = query.unwrap_or_default();
        let want = kinds.unwrap_or_default();
        let k = if want.is_empty() {
            pull::Kinds::all()
        } else {
            pull::Kinds {
                chats: want.iter().any(|x| x == "chats"),
                mail: want.iter().any(|x| x == "mail"),
                transcripts: want.iter().any(|x| x == "transcripts"),
            }
        };
        pull::pull(&platform::data_dir(), &q, days.unwrap_or(7), k)
    })
    .await
    .unwrap_or_default()
}

/// A channel the user can choose to sync (Settings): its id and name.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub topic: String,
}

/// The channels in the user's conversation list, for the opt-in picker in Settings.
#[tauri::command]
pub async fn sync_list_channels() -> Result<Vec<Channel>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let sign_in = auth::load().ok_or("Not signed in to Teams.")?;
        Ok(fetch::channels(&client(), &sign_in)?.into_iter().map(|c| Channel { id: c.id, topic: c.name }).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Upcoming Teams meetings for the Today screen, read fresh from the calendar. A successful read is
/// also kept in `meetings.json`, so Today can show the last list at once ([`sync_cached_meetings`]).
#[tauri::command]
pub async fn sync_today_meetings() -> Result<Vec<MeetingDto>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let data = platform::data_dir();
        let meetings: Vec<MeetingDto> = run::today_meetings(&data)?
            .into_iter()
            .map(|e| MeetingDto {
                subject: e.subject,
                start: e.start.to_rfc3339(),
                end: e.end.to_rfc3339(),
                organizer: e.organizer,
                online: e.online,
            })
            .collect();
        // Best effort: the cache only saves Today a wait, so failing to write it isn't an error.
        let path = data.join(MEETINGS_CACHE);
        let tmp = path.with_extension("json.tmp");
        if let Ok(json) = serde_json::to_vec(&meetings) {
            if std::fs::write(&tmp, json).is_ok() {
                let _ = brainstead_core::write::rename(&tmp, &path);
            }
        }
        Ok(meetings)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Where the last calendar read for Today is kept, in the app's data folder.
const MEETINGS_CACHE: &str = "meetings.json";

/// The meetings from the last successful calendar read that haven't ended yet — instant, no network.
/// Empty when sync is off or nothing has been read yet.
#[tauri::command]
pub fn sync_cached_meetings() -> Vec<MeetingDto> {
    let data = platform::data_dir();
    if !crate::settings::load(&data.join("settings.json")).sync_enabled {
        return vec![];
    }
    cached_meetings(&data.join(MEETINGS_CACHE), chrono::Utc::now())
}

fn cached_meetings(path: &std::path::Path, now: chrono::DateTime<chrono::Utc>) -> Vec<MeetingDto> {
    let Some(list) = std::fs::read(path).ok().and_then(|b| serde_json::from_slice::<Vec<MeetingDto>>(&b).ok()) else {
        return vec![];
    };
    list.into_iter().filter(|m| chrono::DateTime::parse_from_rfc3339(&m.end).is_ok_and(|end| end >= now)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_meetings_drop_ended_ones_and_tolerate_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MEETINGS_CACHE);
        let now = chrono::Utc::now();
        assert!(cached_meetings(&path, now).is_empty(), "nothing read yet");
        let meeting = |subject: &str, end: chrono::DateTime<chrono::Utc>| MeetingDto {
            subject: subject.into(),
            start: (end - chrono::Duration::minutes(30)).to_rfc3339(),
            end: end.to_rfc3339(),
            organizer: "Maya".into(),
            online: true,
        };
        let list =
            vec![meeting("Standup", now - chrono::Duration::minutes(5)), meeting("Orbit App review", now + chrono::Duration::hours(1))];
        std::fs::write(&path, serde_json::to_vec(&list).unwrap()).unwrap();
        let kept: Vec<String> = cached_meetings(&path, now).into_iter().map(|m| m.subject).collect();
        assert_eq!(kept, vec!["Orbit App review"]);
        std::fs::write(&path, "{ not json").unwrap();
        assert!(cached_meetings(&path, now).is_empty(), "a damaged cache reads as empty");
    }
}
