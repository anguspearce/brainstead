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

/// How long the server waits for the app to answer one action: a little over the app's wait for
/// its window to be ready (30 s) plus the time the window may take (120 s), so a freshly opened
/// app's reply isn't lost by a tick. A request older than this has nobody waiting for it.
pub const ACT_TIMEOUT: Duration = Duration::from_secs(160);

/// How long the window may take over an action that waits on a model before it answers
/// ([`waits_on_model`]): cut off at the usual two minutes, its work carried on with nobody to
/// tell, and trying again started a second run.
pub const MODEL_WINDOW_TIMEOUT: Duration = Duration::from_secs(600);

/// [`ACT_TIMEOUT`] for an action that waits on a model: the same margin over the window's wait.
pub const MODEL_ACT_TIMEOUT: Duration = Duration::from_secs(640);

/// The actions (src/mcpActions.ts's names) that ask a model and wait for its answer inside the
/// request: Doc check, Compare, Draft reply and Triage's suggestions. The app's wait for its
/// window and the server's wait for the app both go by this one list, so they can't drift.
pub const MODEL_ACTIONS: [&str; 4] = ["doccheck.run", "compare.run", "reply.draft", "triage.suggest"];

/// The Inbox's listing, which asks a model only when its `suggest` is true.
const INBOX_LIST: &str = "inbox.list";

/// Whether this request waits on a model before it answers, so both sides give it longer.
pub fn waits_on_model(action: &str, args: &serde_json::Value) -> bool {
    MODEL_ACTIONS.contains(&action) || (action == INBOX_LIST && args.get("suggest").and_then(|s| s.as_bool()) == Some(true))
}

/// How long the app waits for its window over this request.
pub fn window_timeout(action: &str, args: &serde_json::Value) -> Duration {
    if waits_on_model(action, args) {
        MODEL_WINDOW_TIMEOUT
    } else {
        WINDOW_TIMEOUT
    }
}

/// How long the server waits for the app over this request.
pub fn act_timeout(action: &str, args: &serde_json::Value) -> Duration {
    if waits_on_model(action, args) {
        MODEL_ACT_TIMEOUT
    } else {
        ACT_TIMEOUT
    }
}

/// How long the window may take over one request. Runs answer as soon as they've started.
pub const WINDOW_TIMEOUT: Duration = Duration::from_secs(120);

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
    /// A dev build (`make dev`), not the installed app (D-20261118-22).
    #[serde(default)]
    dev: bool,
}

pub fn write_alive(path: &Path, pid: u32) -> std::io::Result<()> {
    write_json(path, &Alive { pid, at: now_ms(), dev: cfg!(debug_assertions) })
}

/// Another copy of the app that's up on this app data folder: its process and whether it's a dev
/// build. None when it's this one (`me`) or none has said it's up in the last six seconds.
pub fn other_alive(data: &Path, me: u32) -> Option<(u32, bool)> {
    read_bridge(&dir(data).join(ALIVE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Alive>(&b).ok())
        .filter(|a| a.pid != me && now_ms().saturating_sub(a.at) < 6_000)
        .map(|a| (a.pid, a.dev))
}

/// Whether the app has said it's up in the last six seconds.
pub fn app_alive(data: &Path) -> bool {
    read_bridge(&dir(data).join(ALIVE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Alive>(&b).ok())
        .is_some_and(|a| now_ms().saturating_sub(a.at) < 6_000)
}

/// Whether a bridge file is older than `ACT_TIMEOUT`, so whoever wrote it has stopped waiting:
/// by the time its id starts with (`new_id`), or by when it was written for any other name.
fn stale(path: &Path) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let age = match stem.split('-').next().and_then(|n| n.parse::<u128>().ok()) {
        Some(at) => now_ms().saturating_sub(at),
        None => std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok()).map_or(0, |d| d.as_millis()),
    };
    age > ACT_TIMEOUT.as_millis()
}

/// Clears what an earlier run left in the bridge folder (`dir`): requests and replies nobody is
/// waiting for any more, and half-written files. A claimed request (`.taken`) always goes: the
/// copy of the app that claimed it has gone, and it must not be carried out again. The app calls
/// this once as it starts, before it looks for requests.
pub fn clear_stale(dir: &Path) {
    for sub in [REQUESTS, REPLIES] {
        let Ok(rd) = std::fs::read_dir(dir.join(sub)) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("taken") || stale(&p) {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}

/// Requests waiting, each claimed by renaming it so it's taken up once. One nobody is waiting for
/// any more (the app wasn't open, or was busy) is dropped rather than carried out late.
pub fn take_requests(dir: &Path) -> Vec<(PathBuf, Request)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        if stale(&p) {
            let _ = std::fs::remove_file(&p);
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
    let sent = d.join(REQUESTS).join(format!("{}.json", req.id));
    if let Err(e) = write_json(&sent, req) {
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
    // Take the request back, so it isn't carried out whenever the app next looks. If it was still
    // there, the app never took it up.
    if std::fs::remove_file(&sent).is_ok() {
        return Reply::err("Brainstead didn't answer in time. Is it open? Nothing was done.");
    }
    Reply::err("Brainstead didn't answer in time. Is it open? The action may still happen: check before trying again.")
}

#[cfg(test)]
mod tests {

    #[test]
    fn another_copy_is_seen_by_its_heartbeat() {
        let t = tempfile::tempdir().unwrap();
        let d = dir(t.path());
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(other_alive(t.path(), 7), None, "none up");
        write_alive(&d.join(ALIVE), 42).unwrap();
        assert_eq!(other_alive(t.path(), 42), None, "that's this copy");
        assert_eq!(other_alive(t.path(), 7), Some((42, cfg!(debug_assertions))));
        // A heartbeat from before the dev flag reads as the installed app's.
        std::fs::write(d.join(ALIVE), format!("{{\"pid\":5,\"at\":{}}}", now_ms())).unwrap();
        assert_eq!(other_alive(t.path(), 7), Some((5, false)));
    }

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

    fn request(id: &str) -> Request {
        Request { id: id.into(), action: "task.edit".into(), args: json!({}), chat: None, model: None }
    }

    #[test]
    fn an_unanswered_request_is_taken_back() {
        let t = tempfile::tempdir().unwrap();
        let requests = dir(t.path()).join(REQUESTS);
        // Nobody took it up: it's removed, and the error says nothing happened.
        let req = request(&new_id());
        let r = ask(t.path(), &req, Duration::from_millis(200));
        assert!(r.error.unwrap().contains("Nothing was done"));
        assert!(!requests.join(format!("{}.json", req.id)).exists());
        assert!(take_requests(&requests).is_empty());
        // The app took it up and never answered: it may still happen.
        let req = request(&format!("{}-late", now_ms()));
        let d = requests.clone();
        let app = std::thread::spawn(move || loop {
            if !take_requests(&d).is_empty() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        });
        let r = ask(t.path(), &req, Duration::from_millis(400));
        app.join().unwrap();
        assert!(r.error.unwrap().contains("may still happen"));
    }

    #[test]
    fn an_old_request_is_dropped_not_carried_out() {
        let t = tempfile::tempdir().unwrap();
        let d = dir(t.path());
        let (requests, replies) = (d.join(REQUESTS), d.join(REPLIES));
        std::fs::create_dir_all(&requests).unwrap();
        std::fs::create_dir_all(&replies).unwrap();
        let long_ago = now_ms() - ACT_TIMEOUT.as_millis() - 5_000;
        let aged = |p: &Path| {
            let then = SystemTime::now() - ACT_TIMEOUT - Duration::from_secs(5);
            std::fs::File::options().write(true).open(p).unwrap().set_modified(then).unwrap();
        };
        let old = format!("{long_ago}-7");
        let fresh = new_id();
        for id in [old.as_str(), fresh.as_str(), "named", "named-new"] {
            write_json(&requests.join(format!("{id}.json")), &request(id)).unwrap();
        }
        // An id with no time in it is judged by when the file was written.
        aged(&requests.join("named.json"));
        let mut got: Vec<String> = take_requests(&requests).into_iter().map(|(_, r)| r.id).collect();
        got.sort();
        let mut want = vec![fresh.clone(), "named-new".to_string()];
        want.sort();
        assert_eq!(got, want);
        assert!(!requests.join(format!("{old}.json")).exists());
        assert!(!requests.join("named.json").exists());

        // Starting up: claimed requests, old replies and old half-written files go; fresh ones stay.
        write_reply(&replies, &old, &Reply::err("late")).unwrap();
        write_reply(&replies, &fresh, &Reply::err("soon")).unwrap();
        std::fs::write(replies.join("half.tmp"), "{").unwrap();
        aged(&replies.join("half.tmp"));
        write_json(&requests.join("waiting.json"), &request("waiting")).unwrap();
        clear_stale(&d);
        let names = |p: &Path| {
            let mut v: Vec<String> =
                std::fs::read_dir(p).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            v.sort();
            v
        };
        assert_eq!(names(&requests), ["waiting.json"]);
        assert_eq!(names(&replies), [format!("{fresh}.json")]);
    }

    /// An action that waits on a model gets the longer wait on both sides, the server's always a
    /// little over the window's; the rest keep the short one. Every name listed is an action the
    /// window has (src/mcpActions.ts).
    #[test]
    fn model_actions_get_longer_on_both_sides() {
        let none = json!({});
        for a in MODEL_ACTIONS {
            assert_eq!((window_timeout(a, &none), act_timeout(a, &none)), (MODEL_WINDOW_TIMEOUT, MODEL_ACT_TIMEOUT), "{a}");
        }
        assert_eq!(window_timeout(INBOX_LIST, &json!({"suggest": true})), MODEL_WINDOW_TIMEOUT);
        assert_eq!(act_timeout(INBOX_LIST, &json!({"suggest": true})), MODEL_ACT_TIMEOUT);
        for (a, args) in [(INBOX_LIST, none.clone()), (INBOX_LIST, json!({"suggest": false})), ("tasks.list", json!({"suggest": true}))] {
            assert_eq!((window_timeout(a, &args), act_timeout(a, &args)), (WINDOW_TIMEOUT, ACT_TIMEOUT), "{a} {args}");
        }
        // The server outlasts the app's wait for its window to be ready (30 s) and for its answer.
        assert!(ACT_TIMEOUT > WINDOW_TIMEOUT + Duration::from_secs(30));
        assert!(MODEL_ACT_TIMEOUT > MODEL_WINDOW_TIMEOUT + Duration::from_secs(30));
        let window = include_str!("../../../src/mcpActions.ts");
        for a in MODEL_ACTIONS.iter().chain([&INBOX_LIST]) {
            assert!(window.contains(&format!("\n  \"{a}\": ")), "{a} isn't one of the window's actions");
        }
    }
}
