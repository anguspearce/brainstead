// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MCP server's way into the running app (§10 stage 9). The server (a separate process, in Ask
//! or a session outside the app) writes a request into the app data folder's `bridge/requests/`;
//! this takes it up, hands it to the window as an `mcp-request` event, and the window runs it with
//! the same functions its screens call (src/mcpActions.ts) and answers with `mcp_reply`, which is
//! written to `bridge/replies/` for the server to read. `bridge/alive.json` says the app is up,
//! rewritten every two seconds, so the server knows when to open it.

use std::collections::HashMap;
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use brainstead_core::bridge::{self as wire, Reply, Request};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::platform;

/// Replies the window owes, by request id.
#[derive(Default)]
pub struct Waiting {
    senders: Mutex<HashMap<String, mpsc::Sender<Reply>>>,
    /// The window has its listener up.
    ready: Mutex<bool>,
}

/// How long the window may take over one request. Runs answer as soon as they've started.
const WINDOW_TIMEOUT: Duration = Duration::from_secs(120);

/// Starts the heartbeat and the request loop. Not in a screenshot demo.
pub fn start(app: AppHandle) {
    let dir = wire::dir(&platform::data_dir());
    for d in [dir.join(wire::REQUESTS), dir.join(wire::REPLIES)] {
        let _ = std::fs::create_dir_all(d);
    }
    // What an earlier run left behind is never carried out now.
    wire::clear_stale(&dir);
    let alive = dir.join(wire::ALIVE);
    std::thread::spawn(move || loop {
        let _ = wire::write_alive(&alive, std::process::id());
        std::thread::sleep(Duration::from_secs(2));
    });
    std::thread::spawn(move || loop {
        for (path, req) in wire::take_requests(&dir.join(wire::REQUESTS)) {
            let app = app.clone();
            let replies = dir.join(wire::REPLIES);
            // Each on its own thread, so a run that takes a moment doesn't hold up a tick.
            std::thread::spawn(move || {
                let reply = handle(&app, &req);
                let _ = wire::write_reply(&replies, &req.id, &reply);
                let _ = std::fs::remove_file(&path);
            });
        }
        std::thread::sleep(Duration::from_millis(150));
    });
}

/// One copy at a time on the shared app data folder (D-20261118-22). A dev build that starts while
/// the installed app runs (an assistant's Brainstead tools open it when no copy is up) asks it to
/// quit, as its menu's Quit does (unsaved edits asked about first), and waits for it, so the two
/// never sync, run jobs or hold the extensions' helper side by side. The installed app started
/// while a dev build runs steps aside at once. False when this copy should not start.
pub fn one_copy(app: &AppHandle) -> bool {
    let data = platform::data_dir();
    let me = std::process::id();
    let Some((pid, dev)) = wire::other_alive(&data, me) else { return true };
    if !cfg!(debug_assertions) {
        if dev {
            crate::applog!("startup: a dev build (pid {pid}) is running on this data; this copy doesn't start");
            use tauri_plugin_notification::NotificationExt;
            let _ = app
                .notification()
                .builder()
                .title("Brainstead is already running")
                .body("A development build of Brainstead is open, so this copy closed.")
                .show();
            return false;
        }
        return true;
    }
    if dev {
        return true;
    }
    crate::applog!("startup: the installed app (pid {pid}) is running; asking it to quit");
    let req = Request { id: wire::new_id(), action: STEP_ASIDE.into(), args: Value::Null, chat: None, model: None };
    // An installed app from before this rule doesn't know the request: quit it as its Dock menu would.
    if !wire::ask(&data, &req, Duration::from_secs(5)).ok {
        platform::quit_other_copy(pid);
    }
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(60) && wire::other_alive(&data, me).is_some_and(|(p, d)| p == pid && !d) {
        std::thread::sleep(Duration::from_millis(300));
    }
    true
}

/// What a dev build asks the installed app when it starts: quit.
const STEP_ASIDE: &str = "app.step_aside";

fn handle(app: &AppHandle, req: &Request) -> Reply {
    if req.action == STEP_ASIDE {
        crate::applog!("a dev build started: quitting so only one copy runs");
        crate::tray::show_main(app);
        let _ = app.emit_to("main", "quit-requested", "unasked");
        return Reply { ok: true, result: Value::Null, error: None };
    }
    let waiting = app.state::<Waiting>();
    // The window may still be loading when the app was opened for this request.
    let start = Instant::now();
    while !*waiting.ready.lock().unwrap() {
        if start.elapsed() > Duration::from_secs(30) {
            return Reply::err("Brainstead's window didn't start in time. Try again.");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let (tx, rx) = mpsc::channel();
    waiting.senders.lock().unwrap().insert(req.id.clone(), tx);
    if app.emit_to("main", "mcp-request", req).is_err() {
        waiting.senders.lock().unwrap().remove(&req.id);
        return Reply::err("Couldn't reach Brainstead's window.");
    }
    let r = rx.recv_timeout(WINDOW_TIMEOUT).unwrap_or_else(|_| {
        Reply::err("Brainstead took too long to answer. The action may still have happened: check before trying again.")
    });
    waiting.senders.lock().unwrap().remove(&req.id);
    r
}

/// The window's answer to a request.
#[tauri::command]
pub fn mcp_reply(app: AppHandle, id: String, ok: bool, result: Value) {
    let reply = if ok { Reply { ok: true, result, error: None } } else { Reply::err(result.as_str().unwrap_or("It didn't work.")) };
    if let Some(tx) = app.state::<Waiting>().senders.lock().unwrap().remove(&id) {
        let _ = tx.send(reply);
    }
}

/// The window has its listener up.
#[tauri::command]
pub fn mcp_ready(app: AppHandle) {
    *app.state::<Waiting>().ready.lock().unwrap() = true;
}
