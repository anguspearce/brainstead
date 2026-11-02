// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ollama's server on this computer (or the one `OLLAMA_HOST` names): whether it answers, its
//! version, the models it has pulled and what each can do. Read for Settings › AI assistants'
//! Found on this computer and for a run's effort; the models themselves run through Codex
//! (brainstead_core::ask::Cli::Ollama), which reaches the same server. The replies are read by
//! brainstead_core::ask::ollama, which is where the tests are.

use std::time::Duration;

use brainstead_core::ask::ollama::{self, Show, Tag};

/// The server Brainstead and Codex use: `OLLAMA_HOST` as Ollama reads it, else localhost:11434.
pub fn host() -> String {
    ollama::host(std::env::var("OLLAMA_HOST").ok().as_deref())
}

/// A local server answers at once or not at all; a remote one named in `OLLAMA_HOST` gets a moment.
const TIMEOUT: Duration = Duration::from_secs(2);

fn client() -> Option<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder().timeout(TIMEOUT).build().ok()
}

fn get(host: &str, path: &str) -> Option<serde_json::Value> {
    client()?.get(format!("{host}{path}")).send().ok()?.error_for_status().ok()?.json().ok()
}

/// What the server at `host` says: its version and the models it has pulled, in its order. None
/// when nothing answers there (Ollama not running, or not installed).
pub fn server(host: &str) -> Option<(Option<String>, Vec<Tag>)> {
    let tags = get(host, "/api/tags")?;
    let version = get(host, "/api/version").as_ref().and_then(ollama::parse_version);
    Some((version, ollama::parse_tags(&tags)))
}

/// What `model` can do (`POST /api/show`): tools, and its thinking levels by name. None when the
/// server doesn't answer or doesn't have the model.
pub fn show(host: &str, model: &str) -> Option<Show> {
    let reply = client()?
        .post(format!("{host}/api/show"))
        .json(&serde_json::json!({"model": model}))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json::<serde_json::Value>()
        .ok()?;
    Some(ollama::parse_show(&reply))
}
