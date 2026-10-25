// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only calls to Microsoft's web APIs with the user's own sign-in (`auth`). These are the same
//! endpoints the Teams and Outlook web apps call; the sync only ever reads. Chats and the Teams
//! calendar go through chatsvc / the middle tier (a Skype token plus the Spaces bearer); mail and the
//! Outlook calendar through Outlook REST (the Outlook bearer); transcripts through Substrate.

use reqwest::blocking::Client;
use serde_json::Value;

use super::auth::{self, Service};

/// The browser-like headers Microsoft's web APIs expect on a request from the Teams/Outlook origin.
fn web_headers(req: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
    req.header("Origin", "https://teams.microsoft.com")
        .header("Referer", "https://teams.microsoft.com/")
        .header("Accept", "application/json")
}

/// A GET returning JSON, with the bearer token for `svc`. A non-2xx is an error carrying the status.
pub fn get_json(client: &Client, svc: Service, url: &str) -> Result<Value, String> {
    let token = auth::token(client, svc)?;
    let resp = web_headers(client.get(url).bearer_auth(token)).send().map_err(|e| e.to_string())?;
    read(resp, url)
}

/// A GET on chatsvc / the middle tier, which need the Skype token alongside the Spaces bearer.
pub fn get_chatsvc(client: &Client, url: &str) -> Result<Value, String> {
    let spaces = auth::token(client, Service::Spaces)?;
    let skype = skype_token(client, &spaces)?;
    let resp = web_headers(client.get(url).bearer_auth(&spaces).header("Authentication", format!("skypetoken={skype}")))
        .send()
        .map_err(|e| e.to_string())?;
    read(resp, url)
}

/// Reads a response as JSON, turning a non-2xx into an error naming the endpoint and status, so a
/// changed or withdrawn Microsoft endpoint shows up plainly in Settings rather than breaking the sync.
fn read(resp: reqwest::blocking::Response, url: &str) -> Result<Value, String> {
    let status = resp.status();
    let host = url.split('/').nth(2).unwrap_or(url);
    if !status.is_success() {
        let detail = resp.text().unwrap_or_default();
        let code = serde_json::from_str::<Value>(&detail).ok().and_then(|v| v["error"]["code"].as_str().map(str::to_string));
        return Err(match code {
            Some(c) => format!("{host} answered {} ({c})", status.as_u16()),
            None => format!("{host} answered {}", status.as_u16()),
        });
    }
    resp.json().map_err(|e| format!("{host} gave an unreadable answer: {e}"))
}

/// Exchanges the Spaces bearer for a Skype token (authsvc), which chatsvc and the middle tier
/// require. Short-lived, so it's fetched per sync run rather than cached.
fn skype_token(client: &Client, spaces: &str) -> Result<String, String> {
    let resp = client
        .post("https://authsvc.teams.microsoft.com/v1.0/authz")
        .bearer_auth(spaces)
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .map_err(|e| e.to_string())?;
    let body = read(resp, "https://authsvc.teams.microsoft.com/v1.0/authz")?;
    body["tokens"]["skypeToken"].as_str().map(str::to_string).ok_or_else(|| "The Teams auth service returned no Skype token.".into())
}
