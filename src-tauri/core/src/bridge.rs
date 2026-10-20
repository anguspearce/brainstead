// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The files the MCP server and the running app pass between them (§10 stage 9): a request per
//! action in `bridge/requests/`, the app's reply in `bridge/replies/`, and `bridge/alive.json`,
//! which the app rewrites every two seconds while it runs. The app side is src-tauri/src/bridge.rs.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const REQUESTS: &str = "requests";
pub const REPLIES: &str = "replies";
pub const ALIVE: &str = "alive.json";

/// The bridge folder in the app data folder.
pub fn dir(data: &Path) -> PathBuf {
    data.join("bridge")
}

/// One action for the app to do.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub id: String,
    pub action: String,
    #[serde(default)]
    pub args: Value,
    /// Who asked: the Ask chat, or none for a session outside the app.
    #[serde(default)]
    pub chat: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reply {
    pub ok: bool,
    #[serde(default)]
    pub result: Value,
    #[serde(default)]
    pub error: Option<String>,
}

impl Reply {
    pub fn err(msg: &str) -> Reply {
        Reply { ok: false, result: Value::Null, error: Some(msg.to_string()) }
    }
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

fn write_json(path: &Path, v: &impl Serialize) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(v).unwrap_or_default())?;
    crate::write::rename(&tmp, path)
}

/// A bridge file's bytes. On Windows the file can't be opened for the moment the other side's
/// rename replaces it, so a failure other than "not there" is tried twice more before it counts.
fn read_bridge(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut tries = 0;
    loop {
        match std::fs::read(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound && tries < 2 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(25));
            }
            r => return r,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Alive {
    pid: u32,
    at: u128,
}

pub fn write_alive(path: &Path, pid: u32) -> std::io::Result<()> {
    write_json(path, &Alive { pid, at: now_ms() })
}

/// Whether the app has said it's up in the last six seconds.
pub fn app_alive(data: &Path) -> bool {
    read_bridge(&dir(data).join(ALIVE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Alive>(&b).ok())
        .is_some_and(|a| now_ms().saturating_sub(a.at) < 6_000)
}

/// Requests waiting, each claimed by renaming it so it's taken up once.
pub fn take_requests(dir: &Path) -> Vec<(PathBuf, Request)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let taken = p.with_extension("taken");
        if std::fs::rename(&p, &taken).is_err() {
            continue;
        }
        let bytes = match read_bridge(&taken) {
            Ok(b) => b,
            // Still held open (Windows): put it back for the next look rather than lose it.
            Err(_) => {
                let _ = std::fs::rename(&taken, &p);
                continue;
            }
        };
        match serde_json::from_slice::<Request>(&bytes) {
            Ok(r) => out.push((taken, r)),
            Err(_) => {
                let _ = std::fs::remove_file(&taken);
            }
        }
    }
    out
}

pub fn write_reply(dir: &Path, id: &str, r: &Reply) -> std::io::Result<()> {
    write_json(&dir.join(format!("{id}.json")), r)
}

/// A fresh request id.
pub fn new_id() -> String {
    format!("{}-{}", now_ms(), std::process::id())
}

/// Sends a request and waits for the app's reply, up to `timeout`.
pub fn ask(data: &Path, req: &Request, timeout: Duration) -> Reply {
    let d = dir(data);
    let _ = std::fs::create_dir_all(d.join(REQUESTS));
    let _ = std::fs::create_dir_all(d.join(REPLIES));
    if let Err(e) = write_json(&d.join(REQUESTS).join(format!("{}.json", req.id)), req) {
        return Reply::err(&format!("Couldn't send the request to Brainstead: {e}"));
    }
    let reply = d.join(REPLIES).join(format!("{}.json", req.id));
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(r) = read_bridge(&reply).ok().and_then(|b| serde_json::from_slice::<Reply>(&b).ok()) {
            let _ = std::fs::remove_file(&reply);
            return r;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Reply::err("Brainstead didn't answer in time. Is it open? The action may still happen: check before trying again.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_request_goes_round() {
        let t = tempfile::tempdir().unwrap();
        let data = t.path().to_path_buf();
        assert!(!app_alive(&data));
        std::fs::create_dir_all(dir(&data)).unwrap();
        write_alive(&dir(&data).join(ALIVE), 1).unwrap();
        assert!(app_alive(&data));
        // The app's side: take the request up and answer it.
        let d2 = data.clone();
        let app = std::thread::spawn(move || loop {
            let got = take_requests(&dir(&d2).join(REQUESTS));
            if let Some((p, r)) = got.into_iter().next() {
                assert_eq!(r.action, "task.edit");
                write_reply(&dir(&d2).join(REPLIES), &r.id, &Reply { ok: true, result: json!({"done": r.args["x"]}), error: None })
                    .unwrap();
                std::fs::remove_file(p).unwrap();
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        });
        let req = Request { id: new_id(), action: "task.edit".into(), args: json!({"x": 3}), chat: None, model: None };
        let r = ask(&data, &req, Duration::from_secs(5));
        app.join().unwrap();
        assert_eq!(r, Reply { ok: true, result: json!({"done": 3}), error: None });
        let r = ask(&data, &Request { id: new_id(), ..req }, Duration::from_millis(300));
        assert!(!r.ok && r.error.unwrap().contains("didn't answer"));
    }
}
