// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! the daily check (stage 7b; Settings › Jobs & schedule): contradictions for the pages that
//! changed, `index.md`'s catalogue brought up to date, and the wiki pages not in the page shape
//! counted. Off until switched on; at the time
//! set, or on waking when the time passed while asleep or closed (the reviews' "most recent
//! occurrence wins" rule). No cap: it only looks at what changed. A notification says what it did.

use brainstead_core::catalogue;
use brainstead_core::proposals::{Kind, Origin};
use brainstead_core::reviews::schedule;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::vault::VaultService;
use crate::AppState;

const DEFAULT_TIME: &str = "09:00";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    /// Local `YYYY-MM-DDTHH:MM:SS`.
    pub last_run: Option<String>,
    pub running: bool,
    pub summary: String,
    /// The step going now, while running.
    #[serde(default)]
    pub doing: String,
    /// Changed sources ingested again, by path, with the version that was: one isn't ingested
    /// again until it changes once more, whatever came of the run.
    #[serde(default)]
    pub refreshed: std::collections::BTreeMap<String, String>,
}

/// The changed sources still to ingest again: those not tried at the version they are now
/// (`version`, None when unreadable). `tried` keeps only the sources still changed.
fn to_refresh(
    changed: &[String],
    tried: &mut std::collections::BTreeMap<String, String>,
    version: impl Fn(&str) -> Option<String>,
) -> Vec<String> {
    tried.retain(|p, _| changed.contains(p));
    let mut out = Vec::new();
    for p in changed {
        let Some(v) = version(p) else { continue };
        if tried.get(p) != Some(&v) {
            tried.insert(p.clone(), v);
            out.push(p.clone());
        }
    }
    out
}

/// Stop was pressed: the steps left are skipped.
static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn stopped() -> bool {
    STOP.load(std::sync::atomic::Ordering::SeqCst)
}

#[tauri::command]
pub fn daily_check_stop(app: AppHandle) {
    STOP.store(true, std::sync::atomic::Ordering::SeqCst);
    crate::contradict::stop(&app);
}

/// How far a run has got, 0 to 1, from its step (and the contradiction check's batches).
pub fn progress(doing: &str, contra: &str) -> f32 {
    let batch = |s: &str| -> Option<(f32, f32)> {
        let n: Vec<f32> = s.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect();
        (n.len() >= 2 && n[1] > 0.0).then(|| (n[0], n[1]))
    };
    match doing {
        "contradictions" => {
            if let Some((i, n)) = contra.starts_with("Reading").then(|| batch(contra)).flatten() {
                0.02 + 0.73 * (i - 1.0) / n
            } else if contra.starts_with("Finding") {
                0.75
            } else if let Some((i, n)) = contra.starts_with("Judging").then(|| batch(contra)).flatten() {
                0.77 + 0.1 * (i - 1.0) / n
            } else {
                0.02
            }
        }
        "refresh" => 0.9,
        "index" => 0.97,
        "shape" => 0.99,
        _ => 0.0,
    }
}

fn path() -> std::path::PathBuf {
    crate::platform::data_dir().join("daily-check.json")
}

fn load() -> State {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(s: &State) {
    if let Ok(j) = serde_json::to_vec_pretty(s) {
        let _ = brainstead_core::write::write_atomic(&path(), &j, false);
    }
}

fn now() -> NaiveDateTime {
    chrono::Local::now().naive_local()
}

fn settings(app: &AppHandle) -> (bool, String) {
    let s = app.state::<AppState>().settings.lock().unwrap().clone();
    let on = s.ui.get("dailyCheckEnabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let t =
        s.ui.get("dailyCheckTime")
            .and_then(|v| v.as_str())
            .filter(|t| schedule::parse_time(t).is_some())
            .unwrap_or(DEFAULT_TIME)
            .to_string();
    (on, t)
}

/// What Settings shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    last_run: Option<String>,
    next: Option<String>,
    running: bool,
    summary: String,
    /// While running: what it's doing, in words, and how far it has got (0 to 1).
    doing: String,
    progress: f32,
}

#[tauri::command]
pub fn daily_check_status(app: AppHandle) -> Status {
    let st = load();
    let (on, t) = settings(&app);
    Status {
        last_run: st.last_run,
        next: on.then(|| schedule::next_occurrence(now(), &t, None).format("%Y-%m-%dT%H:%M:%S").to_string()),
        running: st.running,
        summary: st.summary,
        doing: match st.doing.as_str() {
            "contradictions" => {
                let c = crate::contradict::doing();
                if c.is_empty() {
                    "Checking for contradictions".into()
                } else {
                    c
                }
            }
            "refresh" => "Ingesting changed sources again".into(),
            "index" => "Updating index.md".into(),
            "shape" => "Checking the page shape".into(),
            _ => String::new(),
        },
        progress: if st.running { progress(&st.doing, &crate::contradict::doing()) } else { 0.0 },
    }
}

#[tauri::command]
pub fn daily_check_run_now(app: AppHandle, unattended: Option<bool>) {
    // An assistant nobody is watching started it: its changes are held when a check fails.
    let trigger = unattended.unwrap_or(false).then_some("scheduled");
    std::thread::spawn(move || run(&app, trigger));
}

/// Whether the daily check is going.
pub fn going() -> bool {
    load().running
}

pub fn start_scheduler(app: &AppHandle) {
    // A run the app quit during isn't going any more.
    let st = load();
    if st.running {
        save(&State { running: false, doing: String::new(), summary: "Stopped when Brainstead closed".into(), ..st });
    }
    crate::contradict::clear_stale();
    let app = app.clone();
    std::thread::spawn(move || loop {
        let (on, t) = settings(&app);
        if on && app.state::<VaultService>().root().is_some() {
            let due = schedule::last_occurrence(now(), &t, None);
            let st = load();
            let last = st.last_run.as_deref().and_then(|l| NaiveDateTime::parse_from_str(l, "%Y-%m-%dT%H:%M:%S").ok());
            // Switched on today: it first runs at the next time, not at once.
            match last {
                None => {
                    save(&State { last_run: Some(now().format("%Y-%m-%dT%H:%M:%S").to_string()), ..st });
                }
                Some(l) if l < due && !st.running => run(&app, Some("scheduled")),
                _ => {}
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(60));
    });
}

/// The daily check: `trigger` is `scheduled` when the scheduler started it, None for Run now.
/// Its changes, and those of the ingests and contradiction check it starts, are one group in
/// Changes.
fn run(app: &AppHandle, trigger: Option<&str>) {
    let group = format!("daily-check-{}", now().format("%Y%m%d%H%M%S"));
    let mut st = load();
    if st.running {
        return;
    }
    st.running = true;
    st.doing = "contradictions".into();
    save(&st);
    STOP.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = app.emit("daily-check-changed", ());
    let mut did: Vec<String> = Vec::new();
    let step = |name: &str| {
        let mut s = load();
        s.doing = name.into();
        save(&s);
        let _ = app.emit("daily-check-changed", ());
    };

    // Contradictions for what changed.
    match crate::contradict::run_now(app, trigger, Some(&group)) {
        Some(l) => match l.error {
            Some(e) => did.push(format!("contradictions failed: {e}")),
            None => {
                let mut line = format!("{} pages read, {} new clashes judged, {} fixes proposed", l.extracted, l.judged, l.proposed);
                if l.failed > 0 {
                    line.push_str(&format!(", {} not made ({})", l.failed, l.failures.join(" ")));
                }
                did.push(line)
            }
        },
        None => did.push("a contradiction check was already running".into()),
    }

    // Sources changed since the pages citing them: ingested again, when switched on (Settings ›
    // Jobs & schedule), so the pages catch up. The runs go on in the background, applying or
    // queueing their changes like any ingest. A source is tried once per version: a run that
    // changed nothing isn't repeated every day.
    let refresh = app.state::<AppState>().settings.lock().unwrap().ui.get("refreshStale").and_then(|v| v.as_bool()).unwrap_or(false);
    if refresh && !stopped() {
        step("refresh");
        let changed: Vec<String> = crate::knowledge::run_lint(app)
            .map(|r| r.checks.into_iter().filter(|c| c.id == "changed-sources").flat_map(|c| c.items).filter_map(|i| i.page).collect())
            .unwrap_or_default();
        let root = app.state::<VaultService>().root();
        let mut tried = load().refreshed;
        let todo = to_refresh(&changed, &mut tried, |p| {
            let abs = root.as_ref()?.join(p);
            std::fs::read(abs).ok().map(|b| brainstead_core::write::version(&b))
        });
        if changed.is_empty() {
            did.push("no changed sources to ingest again".into());
        } else if todo.is_empty() {
            did.push(format!("{} ingested again already, unchanged since", plural(changed.len(), "changed source")));
        } else {
            match crate::ingest::ingest_start_for(app, todo.clone(), trigger, Some(&group)) {
                Ok(_) => {
                    did.push(format!("{} ingested again", plural(todo.len(), "changed source")));
                    st.refreshed = tried;
                }
                Err(e) => did.push(format!(
                    "changed sources not ingested: {}",
                    serde_json::to_value(&e).ok().and_then(|v| v["message"].as_str().map(String::from)).unwrap_or_default()
                )),
            }
        }
    }

    // index.md's catalogue.
    if stopped() {
        did.push("stopped".into());
    } else {
        step("index");
        did.push(index_md(app, trigger, &group));
    }

    // Pages edited out of the page shape elsewhere: reported, for Knowledge health's Reshape pages.
    if !stopped() {
        if let Some(root) = app.state::<VaultService>().root() {
            step("shape");
            let results: Vec<_> = brainstead_core::pageshape::survey(&root).into_iter().map(|(r, _)| r).collect();
            did.push(brainstead_core::pageshape::drift_line(&results));
        }
    }

    st.running = false;
    st.doing.clear();
    st.last_run = Some(now().format("%Y-%m-%dT%H:%M:%S").to_string());
    st.summary = did.join("; ");
    save(&st);
    let _ = app.emit("daily-check-changed", ());
    let _ = tauri_plugin_notification::NotificationExt::notification(app).builder().title("Daily check").body(&st.summary).show();
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// Brings index.md's catalogue up to date, or proposes adding it.
fn index_md(app: &AppHandle, trigger: Option<&str>, group: &str) -> String {
    let Some(root) = app.state::<VaultService>().root() else { return "no vault".into() };
    let abs = root.join("index.md");
    let before = std::fs::read_to_string(&abs).unwrap_or_default();
    let cat = catalogue::render(&root);
    if before.contains(catalogue::START) {
        let Some(after) = catalogue::update(&before, &cat) else { return "index.md already current".into() };
        if crate::edits::writable(&app.state::<AppState>()).is_err() {
            return "index.md not updated: Brainstead is read-only".into();
        }
        let res = if abs.exists() {
            brainstead_core::write::save_file(&abs, &after, &brainstead_core::write::version(before.as_bytes()))
        } else {
            brainstead_core::write::create_file(&abs, &after)
        };
        return match res {
            Ok(_) => {
                let entry = brainstead_core::reviews::log_entry("index", "index.md", Some("Catalogue brought up to date"), now());
                let _ = crate::edits::add_log(&root, &entry);
                crate::edits::written(app, &[abs, root.join("log.md")]);
                "index.md updated".into()
            }
            Err(e) => format!("index.md not updated: {e}"),
        };
    }
    // First time: the markers and the catalogue added, as an agent change (revertable). Held once:
    // while it waits in Changes, the next day's run doesn't hold another.
    let held = brainstead_core::changes::Store::new(&crate::platform::data_dir()).list();
    if already_held(&held, "index.md", CATALOGUE_TITLE) {
        return "index.md's catalogue is still held in Changes".into();
    }
    let exists = abs.exists();
    let after = catalogue::with_markers(&before, &cat);
    let origin = Origin {
        kind: "lint".into(),
        label: Some("Wiki catalogue".into()),
        run: Some(group.into()),
        trigger: trigger.map(String::from),
        ..Default::default()
    };
    let s = crate::changes::Submit::new(
        "index.md",
        if exists { Kind::Edit } else { Kind::New },
        CATALOGUE_TITLE,
        "Brainstead keeps it current between its markers from now on; the rest of the file stays yours.",
        origin,
        brainstead_core::changes::from_texts(exists.then_some(before.as_str()), &after),
    );
    match crate::changes::submit(app, s) {
        Ok(o) if o.applied => "index.md's catalogue added".into(),
        Ok(_) => "index.md's catalogue is held in Changes".into(),
        Err(_) => "index.md unchanged".into(),
    }
}

const CATALOGUE_TITLE: &str = "Add the wiki catalogue to index.md";

/// A change to `page` with this title is already waiting in Changes.
fn already_held(changes: &[brainstead_core::changes::Change], page: &str, title: &str) -> bool {
    changes.iter().any(|c| c.status == brainstead_core::changes::Status::Held && c.page == page && c.title == title)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_changed_source_is_tried_once_per_version() {
        let mut tried = std::collections::BTreeMap::new();
        let changed = vec!["sources/a.pdf".to_string(), "sources/b.md".to_string(), "sources/gone.md".to_string()];
        let v = |p: &str| (p != "sources/gone.md").then(|| format!("v1-{p}"));
        assert_eq!(super::to_refresh(&changed, &mut tried, v), ["sources/a.pdf", "sources/b.md"]);
        // The next day, nothing has changed: nothing to do.
        assert!(super::to_refresh(&changed, &mut tried, v).is_empty());
        // b changed again; a is no longer stale and is forgotten.
        let v2 = |p: &str| (p == "sources/b.md").then(|| "v2".to_string());
        assert_eq!(super::to_refresh(&changed[1..], &mut tried, v2), ["sources/b.md"]);
        assert!(!tried.contains_key("sources/a.pdf"));
    }

    #[test]
    fn the_catalogue_is_held_once() {
        use brainstead_core::changes::{Change, Instruction, Status};
        use brainstead_core::proposals::{Kind, Origin};
        let mk = |status| {
            let mut c = Change::new(
                "index.md",
                Kind::Edit,
                super::CATALOGUE_TITLE,
                "",
                Origin::default(),
                None,
                Instruction::Page { content: "x".into() },
            );
            c.status = status;
            c
        };
        assert!(!super::already_held(&[], "index.md", super::CATALOGUE_TITLE));
        assert!(super::already_held(&[mk(Status::Held)], "index.md", super::CATALOGUE_TITLE));
        // Turned down or reverted: the next day's run may hold it again.
        assert!(!super::already_held(&[mk(Status::Rejected), mk(Status::Reverted)], "index.md", super::CATALOGUE_TITLE));
        assert!(!super::already_held(&[mk(Status::Held)], "wiki/index.md", super::CATALOGUE_TITLE));
    }

    #[test]
    fn progress_follows_the_steps_and_batches() {
        assert_eq!(super::progress("contradictions", "Reading pages: batch 1 of 17"), 0.02);
        let half = super::progress("contradictions", "Reading pages: batch 9 of 17");
        assert!(half > 0.35 && half < 0.4, "{half}");
        assert_eq!(super::progress("contradictions", "Finding clashes"), 0.75);
        assert!(super::progress("contradictions", "Judging: batch 2 of 4") > 0.77);
        assert_eq!(super::progress("refresh", ""), 0.9);
        assert_eq!(super::progress("index", ""), 0.97);
        assert_eq!(super::progress("", ""), 0.0);
    }
}
