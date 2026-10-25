// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Teams and Outlook sync: reads the user's own chats, meeting transcripts, Inbox mail and calendar
//! and writes them into `sources/` through the same path the capture extensions use, so the Inbox
//! and "Ingest new sources as they arrive" work unchanged (D-20261025-01, D-20261025-02). The
//! sign-in is handed over by the Teams extension while a Teams tab is open (`auth`); macOS only for
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
    let sign_in = auth::from_handover(v).ok_or("The Teams sign-in handover had no refresh token.")?;
    auth::store(&sign_in)
}

/// A meeting for the Today screen (dates as RFC 3339 strings, so no extra chrono features are needed).
#[derive(Debug, Clone, serde::Serialize)]
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
        let convs = fetch::conversations(&client(), &sign_in)?;
        Ok(convs
            .into_iter()
            .filter(|c| c.kind == fetch::ConvKind::Channel)
            .map(|c| Channel { topic: if c.topic.is_empty() { "(unnamed channel)".into() } else { c.topic }, id: c.id })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Upcoming Teams meetings for the Today screen.
#[tauri::command]
pub async fn sync_today_meetings() -> Result<Vec<MeetingDto>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        run::today_meetings(&platform::data_dir()).map(|events| {
            events
                .into_iter()
                .map(|e| MeetingDto {
                    subject: e.subject,
                    start: e.start.to_rfc3339(),
                    end: e.end.to_rfc3339(),
                    organizer: e.organizer,
                    online: e.online,
                })
                .collect()
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
