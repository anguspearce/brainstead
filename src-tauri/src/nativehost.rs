// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The capture extensions' native messaging host (`extensions/`, §10 stage 8). Chrome starts this
//! program with the extension's origin as its argument, and each message is a 4-byte length in the
//! machine's byte order then that much JSON, both ways. A capture is written into `sources/` by
//! `brainstead_core::webcapture`, whether or not the app is open, and noted in the app data
//! folder's `captures.json` for Settings and the app's notification. The host's manifest, naming
//! the two extensions Chrome may let in, is registered with each browser when the app starts.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use brainstead_core::webcapture::{self, Capture, MAX_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{platform, settings};

/// The host's name, as the extensions call it.
pub const HOST: &str = "com.wayned.brainstead";
/// The extensions' IDs, fixed by the public keys in their manifests.
pub const OUTLOOK_ID: &str = "fpaoobjjngaheckceaomekehpndbendk";
pub const TEAMS_ID: &str = "ibmlhbfpbgehbnmkfmllkinogjgcagad";
/// How many captures `captures.json` keeps.
const KEEP: usize = 50;

/// `Brainstead chrome-extension://<id>/`: the host Chrome starts. None when started otherwise.
pub fn host_main() -> Option<i32> {
    let origin = std::env::args().nth(1).filter(|a| a.starts_with("chrome-extension://"))?;
    let data = platform::data_dir();
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    while let Some(msg) = read_message(&mut input) {
        let reply = match msg {
            Ok(v) => handle(&data, &origin, v),
            Err(e) => json!({"ok": false, "message": e}),
        };
        if write_message(&mut output, &reply).is_err() {
            break;
        }
    }
    Some(0)
}

fn read_message(r: &mut impl Read) -> Option<Result<Value, String>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).ok()?;
    let n = u32::from_ne_bytes(len) as usize;
    if n > MAX_BYTES {
        // Read past it, so the next message lines up.
        let _ = std::io::copy(&mut r.take(n as u64), &mut std::io::sink());
        return Some(Err(format!("The capture is too big ({} MB at most).", MAX_BYTES / 1024 / 1024)));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).ok()?;
    Some(serde_json::from_slice(&buf).map_err(|e| format!("Couldn't read the capture: {e}")))
}

fn write_message(w: &mut impl Write, v: &Value) -> std::io::Result<()> {
    let b = serde_json::to_vec(v)?;
    w.write_all(&(b.len() as u32).to_ne_bytes())?;
    w.write_all(&b)?;
    w.flush()
}

/// One capture, as `captures.json` keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Captured {
    /// "outlook" or "teams".
    pub extension: String,
    /// "Email thread", "Teams transcript"…
    pub what: String,
    /// The vault path written, or None when it was refused.
    pub path: Option<String>,
    pub error: Option<String>,
    pub at: String,
}

fn extension_of(origin: &str) -> &'static str {
    let id = origin.strip_prefix("chrome-extension://").map(|r| r.trim_end_matches('/'));
    match id {
        Some(TEAMS_ID) => "teams",
        Some(OUTLOOK_ID) => "outlook",
        _ => "other",
    }
}

fn handle(data: &Path, origin: &str, v: Value) -> Value {
    let s = settings::load(&data.join("settings.json"));
    let vault = s.vault_path.as_deref().filter(|p| !p.trim().is_empty()).map(PathBuf::from);
    if v.get("type").and_then(Value::as_str) == Some("ping") {
        return json!({"ok": true, "vault": vault.is_some(), "readOnly": s.read_only, "version": env!("CARGO_PKG_VERSION")});
    }
    let ext = extension_of(origin).to_string();
    // Chrome only starts the helper for the extensions in its manifest, but anything can run the
    // program: an origin that isn't one of them is refused here too.
    if ext == "other" {
        return json!({"ok": false, "message": "Brainstead only takes captures from its own Outlook and Teams extensions."});
    }
    let c: Capture = match serde_json::from_value(v) {
        Ok(c) => c,
        Err(e) => return json!({"ok": false, "message": format!("Brainstead doesn't know this capture: {e}")}),
    };
    let what = c.what().to_string();
    let result = match vault {
        None => Err("Brainstead has no vault chosen yet. Open Brainstead and choose one.".to_string()),
        Some(_) if s.read_only => Err("Brainstead's vault is read-only. Switch off Settings › Vault › Read-only to capture.".to_string()),
        Some(v) => webcapture::write(&v, &c, chrono::Local::now()).map_err(|e| e.to_string()),
    };
    note(
        data,
        Captured {
            extension: ext,
            what,
            path: result.as_ref().ok().cloned(),
            error: result.as_ref().err().cloned(),
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        },
    );
    match result {
        Ok(path) => {
            let filename = path.trim_start_matches("sources/").to_string();
            json!({"ok": true, "filename": filename, "path": path})
        }
        Err(message) => json!({"ok": false, "message": message}),
    }
}

/// The captures noted so far, newest first.
pub fn recent(data: &Path) -> Vec<Captured> {
    read_list(&data.join("captures.json")).unwrap_or_default()
}

/// The captures waiting in the Inbox: every one written and not yet clarified, however many came
/// after it (`captures.json` keeps only the last few).
const INBOX: &str = "captures-inbox.json";

fn read_list(f: &Path) -> Option<Vec<Captured>> {
    std::fs::read(f).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

fn write_list(f: &Path, all: &[Captured]) {
    if let Ok(b) = serde_json::to_vec_pretty(all) {
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = brainstead_core::write::write_atomic(f, &b, false);
    }
}

/// Runs `f` holding `captures.lock` in the app data folder: Chrome starts one host process per
/// extension, and the app changes the Inbox list too, so each read-change-write is one at a time.
fn locked<T>(data: &Path, f: impl FnOnce() -> T) -> T {
    let _ = std::fs::create_dir_all(data);
    let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(data.join("captures.lock"));
    // Unlocked when the file closes, at the end of this function.
    let _held = lock.as_ref().ok().filter(|l| l.lock().is_ok());
    f()
}

fn note(data: &Path, c: Captured) {
    locked(data, || {
        if c.path.is_some() {
            // Seeded (the first time) before this capture is in captures.json.
            let f = data.join(INBOX);
            let mut inbox = read_list(&f).unwrap_or_else(|| seed_inbox(data));
            inbox.insert(0, c.clone());
            write_list(&f, &inbox);
        }
        let mut all = recent(data);
        all.insert(0, c);
        all.truncate(KEEP);
        write_list(&data.join("captures.json"), &all);
    })
}

/// Before the Inbox had its own list: the captures `captures.json` still has.
fn seed_inbox(data: &Path) -> Vec<Captured> {
    recent(data).into_iter().filter(|c| c.path.is_some()).collect()
}

/// The captures in the Inbox, newest first, less those `gone` says are clarified or deleted
/// (which are dropped from the list for good).
pub fn inbox(data: &Path, gone: impl Fn(&Captured) -> bool) -> Vec<Captured> {
    locked(data, || {
        let f = data.join(INBOX);
        let all = read_list(&f);
        let had = all.is_some();
        let all = all.unwrap_or_else(|| seed_inbox(data));
        let n = all.len();
        let keep: Vec<Captured> = all.into_iter().filter(|c| !gone(c)).collect();
        if keep.len() != n || !had {
            write_list(&f, &keep);
        }
        keep
    })
}

/// In the app: looks at `captures.json` every couple of seconds and, for each new capture,
/// re-indexes the file and tells the window (`capture`, then `vault-changed`). The folder watcher
/// would see the file too, but a synced folder can be slow to report it.
pub fn watch(app: tauri::AppHandle) {
    let data = platform::data_dir();
    let file = data.join("captures.json");
    let stamp = |f: &Path| std::fs::metadata(f).and_then(|m| m.modified()).ok();
    let mut last = stamp(&file);
    let mut seen = recent(&data).first().map(|c| c.at.clone()).unwrap_or_default();
    std::thread::Builder::new()
        .name("captures".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let now = stamp(&file);
            if now == last {
                continue;
            }
            last = now;
            let all = recent(&data);
            let fresh: Vec<Captured> = all.iter().take_while(|c| c.at > seen).cloned().collect();
            if let Some(c) = all.first() {
                seen = c.at.clone();
            }
            for c in fresh.into_iter().rev() {
                if let (Some(rel), Some(root)) = (&c.path, app.state::<crate::vault::VaultService>().root()) {
                    crate::edits::written(&app, &[root.join(rel)]);
                }
                use tauri::{Emitter, Manager};
                let _ = app.emit("capture", &c);
                // A notification too when Brainstead isn't the window in front.
                let focused = app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
                if !focused {
                    use tauri_plugin_notification::NotificationExt;
                    let (title, body) = match (&c.path, &c.error) {
                        (Some(p), _) => {
                            (format!("{} captured", c.what), p.trim_start_matches("sources/").trim_end_matches(".md").to_string())
                        }
                        (None, e) => (format!("{} not captured", c.what), e.clone().unwrap_or_default().chars().take(200).collect()),
                    };
                    let _ = app.notification().builder().title(title).body(body).show();
                }
            }
        })
        .expect("can start a thread");
}

/// The host manifest for this program.
fn manifest(exe: &Path) -> Value {
    json!({
        "name": HOST,
        "description": "Brainstead: captures from the Outlook and Teams extensions",
        "path": exe,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{OUTLOOK_ID}/"), format!("chrome-extension://{TEAMS_ID}/")],
    })
}

/// A browser and whether it knows the host.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Browser {
    pub name: String,
    /// The host manifest is there.
    pub registered: bool,
    /// It names this copy of Brainstead (not another build).
    pub current: bool,
}

/// Registers the host with each installed browser (`platform::register_native_host`: a manifest
/// in each browser's folder, or the registry on Windows), when it isn't already this. A dev build
/// only reports: it leaves the installed app's registration alone.
pub fn register() -> Vec<Browser> {
    let Ok(exe) = std::env::current_exe() else { return vec![] };
    let may_write = !cfg!(debug_assertions);
    platform::register_native_host(HOST, &manifest(&exe), may_write)
        .into_iter()
        .map(|h| Browser { name: h.browser.to_string(), registered: h.registered, current: h.current })
        .collect()
}

/// The extensions' folders as shipped: in the app's resources, or the repo's `extensions/` in a
/// dev build. A dev build's resources are a copy made when its Rust was last built, so an edit to
/// an extension since wouldn't reach Chrome: it reads the repo's folder first.
fn shipped(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    let repo = || Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../extensions")).filter(|d| d.join("outlook-capture").is_dir());
    let res = || app.path().resource_dir().ok().map(|d| d.join("extensions")).filter(|d| d.join("outlook-capture").is_dir());
    if cfg!(debug_assertions) {
        repo().or_else(res)
    } else {
        res().or_else(repo)
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name();
        let n = name.to_string_lossy();
        if n.starts_with('.') || name == "test-harness" || n.ends_with(".test.js") {
            continue;
        }
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to.join(&name))?;
        } else {
            std::fs::copy(e.path(), to.join(&name))?;
        }
    }
    Ok(())
}

/// What Settings › Capture extensions shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    /// Where to load the extensions from (a copy in the app data folder, kept up to date).
    pub folder: Option<String>,
    pub browsers: Vec<Browser>,
    /// The last captures, newest first.
    pub recent: Vec<Captured>,
    /// Each extension's version in `folder` (its manifest's), by folder name: what Load unpacked
    /// loads, to check against the version Chrome shows on the extension's card.
    pub versions: std::collections::BTreeMap<String, String>,
}

#[tauri::command]
pub async fn capture_status(app: tauri::AppHandle) -> Result<CaptureStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let data = platform::data_dir();
        // A stable folder for Load unpacked, refreshed from this build's copy.
        let folder = shipped(&app).and_then(|from| {
            let to = data.join("extensions");
            ["outlook-capture", "teams-capture"]
                .iter()
                .try_for_each(|d| copy_dir(&from.join(d), &to.join(d)))
                .ok()
                .map(|_| to.to_string_lossy().into_owned())
        });
        let mut recent = recent(&data);
        recent.truncate(10);
        let versions = ["outlook-capture", "teams-capture"]
            .iter()
            .filter_map(|d| {
                let b = std::fs::read(data.join("extensions").join(d).join("manifest.json")).ok()?;
                let v: Value = serde_json::from_slice(&b).ok()?;
                Some((d.to_string(), v["version"].as_str()?.to_string()))
            })
            .collect();
        CaptureStatus { folder, browsers: register(), recent, versions }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Shows an extension's folder (as `capture_status` copied it) in Finder, for Load unpacked.
#[tauri::command]
pub fn capture_reveal(which: String) -> Result<(), String> {
    if !["outlook-capture", "teams-capture"].contains(&which.as_str()) {
        return Err("No such extension.".into());
    }
    platform::reveal(&platform::data_dir().join("extensions").join(which))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framed(v: &Value) -> Vec<u8> {
        let mut b = vec![];
        write_message(&mut b, v).unwrap();
        b
    }

    #[test]
    fn messages_are_length_prefixed_json() {
        let v = json!({"type": "ping"});
        let b = framed(&v);
        assert_eq!(&b[..4], &(15u32).to_ne_bytes());
        let mut r = &b[..];
        assert_eq!(read_message(&mut r).unwrap().unwrap(), v);
        assert!(read_message(&mut r).is_none());
        let mut big = (MAX_BYTES as u32 + 1).to_ne_bytes().to_vec();
        big.extend(std::iter::repeat_n(b' ', MAX_BYTES + 1));
        big.extend(framed(&v));
        let mut r = &big[..];
        assert!(read_message(&mut r).unwrap().is_err());
        assert_eq!(read_message(&mut r).unwrap().unwrap(), v);
    }

    #[test]
    fn writes_a_capture_and_notes_it() {
        let data = tempfile::tempdir().unwrap();
        let vault = tempfile::tempdir().unwrap();
        let origin = format!("chrome-extension://{TEAMS_ID}/");
        let chat = json!({"type": "teams", "url": "https://teams.microsoft.com/", "title": "Standup", "messages": [{"sender": "Maya", "body": "Done"}]});
        // No vault yet, then read-only, then written.
        assert_eq!(handle(data.path(), &origin, chat.clone())["ok"], false);
        let mut s = settings::Settings { vault_path: Some(vault.path().to_string_lossy().into()), ..Default::default() };
        std::fs::write(data.path().join("settings.json"), serde_json::to_vec(&s).unwrap()).unwrap();
        let r = handle(data.path(), &origin, chat.clone());
        assert!(r["message"].as_str().unwrap().contains("read-only"));
        s.read_only = false;
        std::fs::write(data.path().join("settings.json"), serde_json::to_vec(&s).unwrap()).unwrap();
        let r = handle(data.path(), &origin, chat);
        assert_eq!(r["ok"], true, "{r}");
        let path = r["path"].as_str().unwrap();
        assert!(vault.path().join(path).exists());
        let seen = recent(data.path());
        assert_eq!(seen.len(), 3);
        assert_eq!((seen[0].extension.as_str(), seen[0].what.as_str(), seen[0].path.as_deref()), ("teams", "Teams chat", Some(path)));
        assert!(seen[1].error.is_some());
        let ping = handle(data.path(), &origin, json!({"type": "ping"}));
        assert_eq!((ping["ok"].clone(), ping["vault"].clone()), (json!(true), json!(true)));
        assert_eq!(handle(data.path(), &origin, json!({"type": "nope"}))["ok"], false);
        // Another extension is refused and nothing is noted or written.
        let other = handle(
            data.path(),
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/",
            json!({"type": "teams", "url": "https://teams.microsoft.com/", "title": "X", "messages": [{"sender": "A", "body": "B"}]}),
        );
        assert_eq!(other["ok"], false);
        assert_eq!(recent(data.path()).len(), 3);
        assert_eq!(extension_of(&format!("chrome-extension://{TEAMS_ID}x/")), "other");
        assert_eq!(extension_of(&format!("chrome-extension://{OUTLOOK_ID}/")), "outlook");
    }

    #[test]
    fn the_inbox_keeps_every_capture_until_it_is_clarified() {
        let data = tempfile::tempdir().unwrap();
        let cap = |i: usize| Captured {
            extension: "teams".into(),
            what: "Teams chat".into(),
            path: Some(format!("sources/c{i}.md")),
            error: None,
            at: format!("2026-10-03T10:{:02}:{:02}.000Z", i / 60, i % 60),
        };
        // An older install: captures.json only. Its captures seed the Inbox list.
        write_list(&data.path().join("captures.json"), &[cap(0)]);
        for i in 1..=KEEP + 10 {
            note(data.path(), cap(i));
        }
        note(data.path(), Captured { path: None, error: Some("read-only".into()), ..cap(999) });
        assert_eq!(recent(data.path()).len(), KEEP);
        let all = inbox(data.path(), |_| false);
        assert_eq!(all.len(), KEEP + 11);
        assert_eq!(all.last().unwrap().path.as_deref(), Some("sources/c0.md"));
        let left = inbox(data.path(), |c| c.path.as_deref() == Some("sources/c0.md"));
        assert_eq!(left.len(), KEEP + 10);
        assert_eq!(inbox(data.path(), |_| false).len(), KEEP + 10);
    }

    #[test]
    fn the_manifest_allows_only_the_two_extensions() {
        let m = manifest(Path::new("/Applications/Brainstead.app/Contents/MacOS/Brainstead"));
        assert_eq!(m["name"], HOST);
        assert_eq!(m["allowed_origins"].as_array().unwrap().len(), 2);
    }
}
