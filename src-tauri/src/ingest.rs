// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stage 7b's commands: Fix name, and (below) the staged ingest, contradictions and meeting notes
//! from transcripts. Writes are refused while read-only, run off the main thread, re-index what
//! they wrote and can be undone with ⌘Z.

use std::path::PathBuf;

use brainstead_core::fixname::{self, Request, Row};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::edits::{writable, written, EditError, FileUndo, Res, UndoEntry, UndoStack};
use crate::vault::VaultService;
use crate::AppState;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| EditError::from(e.to_string()))?
}

fn root(app: &AppHandle) -> Res<PathBuf> {
    Ok(app.state::<VaultService>().root().ok_or("No vault is open.".to_string())?)
}

/// What Fix name would do: the right name's page, when it has one, and every file the wrong
/// name is in with its action.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    right_page: Option<String>,
    rows: Vec<Row>,
}

#[tauri::command]
pub async fn fixname_plan(app: AppHandle, mut req: Request) -> Res<Plan> {
    blocking(move || {
        let root = root(&app)?;
        if req.right_page.is_none() {
            let r = app.state::<VaultService>().with_index(|ix, _| brainstead_core::names::resolve(ix, req.right.trim()))?;
            req.right_page = r.exact.map(|(p, _)| p).filter(|p| p.starts_with("wiki/"));
        }
        let rows = fixname::audit(&root, &req);
        Ok(Plan { right_page: req.right_page, rows })
    })
    .await
}

/// Applies the plan, as one undo. `rows` are the ones left ticked.
#[tauri::command]
pub async fn fixname_apply(app: AppHandle, req: Request, rows: Vec<Row>) -> Res<String> {
    blocking(move || {
        writable(&app.state::<AppState>())?;
        let root = root(&app)?;
        let subs = brainstead_core::names::path(&crate::platform::data_dir());
        let changes = fixname::apply(&root, &subs, &req, &rows)?;
        if changes.is_empty() {
            return Err(brainstead_core::write::WriteError::Invalid("Nothing to change.".into()).into());
        }
        let n = changes.iter().filter(|c| c.path != subs).count();
        let label = format!("Fixed “{}” → “{}” in {n} file{}", req.wrong, req.right, if n == 1 { "" } else { "s" });
        let mut files: Vec<FileUndo> = changes
            .iter()
            .map(|c| FileUndo {
                // The corrections file is outside the vault: undo knows it by name.
                path: if c.path == subs {
                    brainstead_core::names::UNDO_NAME.to_string()
                } else {
                    brainstead_core::vault::rel_of(&root, &c.path).unwrap_or_default()
                },
                before: c.before.clone(),
                version: c.version.clone(),
            })
            .collect();
        let entry = brainstead_core::reviews::log_entry("fix-name", &req.right, Some(&label), chrono::Local::now().naive_local());
        match crate::edits::add_log(&root, &entry) {
            Ok(u) => files.push(u),
            Err(e) => crate::applog!("fix-name log: {e}"),
        }
        app.state::<UndoStack>().push(UndoEntry::files(label.clone(), files));
        let mut paths: Vec<_> = changes.iter().map(|c| c.path.clone()).collect();
        paths.push(root.join("log.md"));
        written(&app, &paths);
        Ok(label)
    })
    .await
}

// ── The staged ingest ─────────────────────────────────────────────────────────

use brainstead_core::claims::Kept;
use brainstead_core::ingest::{self as core_ingest, Dropped, Source, SourceText};
use brainstead_core::proposals::{self, Origin};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tauri::Emitter;

/// One step of a run, as the Sources screen shows it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub name: String,
    /// waiting, running, done, skipped or failed.
    pub status: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub ms: u64,
}

/// An ingest run, kept as `runs/<id>.json` in the app data folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    /// ingest, or meeting (a note from a transcript).
    #[serde(default = "ingest_kind")]
    pub kind: String,
    /// For a meeting note: its type, name and date.
    #[serde(default)]
    pub note: Option<NoteSpec>,
    pub source: String,
    pub started: String,
    #[serde(default)]
    pub finished: Option<String>,
    pub model: String,
    /// queued, running, done, failed or stopped.
    pub status: String,
    pub steps: Vec<Step>,
    /// Its changes' ids (in Changes), applied or held.
    #[serde(default)]
    pub proposals: Vec<String>,
    /// `scheduled` when the daily check (or an unattended session) started it; None when the
    /// user did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    /// The run it's part of in Changes (the daily check's); its own id when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default)]
    pub dropped: Vec<Dropped>,
    #[serde(default)]
    pub pages: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
    /// The `log.md` ingest line has been added (with the first page accepted).
    #[serde(default)]
    pub logged: bool,
    /// How many page changes the run is making, saved before the first is made: the `log.md`
    /// line is written as that one applies, before the run on disk lists any.
    #[serde(default)]
    pub planned: usize,
}

fn ingest_kind() -> String {
    "ingest".into()
}

/// The note a transcript becomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSpec {
    #[serde(rename = "type")]
    pub note_type: String,
    pub name: String,
    pub date: String,
}

impl NoteSpec {
    fn filename(&self) -> String {
        format!("{}. {} - {}.md", self.note_type.trim(), self.name.trim(), self.date.trim())
    }
}

const MEETING_STEPS: [&str; 4] = ["Read the transcript", "Correct names", "Draft the note", "Make the note"];

const STEPS: [&str; 6] =
    ["Read the source", "Correct names", "Find the pages it mentions", "Draft the changes", "Check the quotes", "Make the changes"];

fn runs_dir() -> PathBuf {
    crate::platform::data_dir().join("runs")
}

fn save_run(r: &Run) {
    let _ = std::fs::create_dir_all(runs_dir());
    if let Ok(j) = serde_json::to_vec_pretty(r) {
        let _ = brainstead_core::write::write_atomic(&runs_dir().join(format!("{}.json", r.id)), &j, false);
    }
}

pub fn load_run(id: &str) -> Option<Run> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    std::fs::read(runs_dir().join(format!("{id}.json"))).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

/// The `log.md` line for a run's first accepted page; None after that (one line per ingest).
pub fn take_log(id: &str) -> Option<String> {
    let mut r = load_run(id)?;
    if r.logged {
        return None;
    }
    r.logged = true;
    save_run(&r);
    let link = brainstead_core::lint::name_of(&r.source).trim_end_matches(".md").to_string();
    let n = r.proposals.len().max(r.planned);
    let summary = format!("{n} page change{}", if n == 1 { "" } else { "s" });
    Some(brainstead_core::reviews::log_entry("ingest", &link, Some(&summary), chrono::Local::now().naive_local()))
}

/// Runs asked to stop.
#[derive(Default)]
pub struct Stops(Mutex<HashSet<String>>);

fn emit(app: &AppHandle, r: &Run) {
    save_run(r);
    let _ = app.emit("ingest-changed", r);
}

/// A source's provenance pane: what the index knows of it, the pages citing it with their passages,
/// and its latest ingest run.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    #[serde(flatten)]
    facts: brainstead_core::lists::SourceFacts,
    last_run: Option<Run>,
}

#[tauri::command]
pub async fn source_provenance(app: AppHandle, path: String) -> Res<Option<Provenance>> {
    blocking(move || {
        let svc = app.state::<crate::vault::VaultService>();
        let Some(mut facts) = svc.with_index(|ix, _| ix.source_facts(&path))? else { return Ok(None) };
        if let Some(root) = svc.root() {
            brainstead_core::lists::fill_passages(&root, &mut facts.citers);
        }
        let last_run = all_runs().into_iter().find(|r| r.kind == "ingest" && r.source == path);
        Ok(Some(Provenance { facts, last_run }))
    })
    .await
}

/// Every run kept in `runs/`, newest first.
fn all_runs() -> Vec<Run> {
    let mut v: Vec<Run> = std::fs::read_dir(runs_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice(&b).ok())
        .collect();
    v.sort_by(|a: &Run, b: &Run| b.started.cmp(&a.started).then_with(|| b.id.cmp(&a.id)));
    v
}

/// How many runs `runs/` keeps.
const KEEP_RUNS: usize = 100;

/// Every run, newest first: the last 100, the older ones deleted (never one still going).
#[tauri::command]
pub async fn ingest_runs() -> Res<Vec<Run>> {
    blocking(|| {
        let mut v = all_runs();
        for old in v.iter().skip(KEEP_RUNS).filter(|r| r.status != "queued" && r.status != "running") {
            let _ = std::fs::remove_file(runs_dir().join(format!("{}.json", old.id)));
        }
        v.truncate(KEEP_RUNS);
        Ok(v)
    })
    .await
}

#[tauri::command]
pub fn ingest_stop(app: AppHandle, id: String) {
    app.state::<Stops>().0.lock().unwrap().insert(id.clone());
    app.state::<std::sync::Arc<crate::ask::Running>>().cancel(&id);
}

/// The runs waiting, and whether a worker is taking them: every ingest and meeting note goes
/// through this one queue, whoever asked (a screen, an assistant, the daily check), so two
/// never change the same page at once (D-20261006-05).
static QUEUE: Mutex<(Vec<Run>, bool)> = Mutex::new((Vec::new(), false));

/// Whether an ingest or meeting note run is going or waiting.
pub fn going() -> bool {
    let q = crate::lock(&QUEUE);
    q.1 || !q.0.is_empty()
}

/// The day a run's source is from, for the queue's order: from its name (a meeting note's own
/// date), else the file's last change.
fn run_day(app: &AppHandle, r: &Run) -> String {
    if let Some(n) = &r.note {
        return n.date.clone();
    }
    brainstead_core::ingest::source_day(&r.source)
        .or_else(|| {
            let root = root(app).ok()?;
            let t = std::fs::metadata(root.join(&r.source)).ok()?.modified().ok()?;
            Some(chrono::DateTime::<chrono::Local>::from(t).date_naive().to_string())
        })
        .unwrap_or_default()
}

/// Queues these runs and announces them. What waits is taken oldest source first (a newer source
/// then has the last word on a page), in the order asked among sources of the same day.
fn enqueue(app: &AppHandle, runs: Vec<Run>) {
    let mut q = crate::lock(&QUEUE);
    for r in runs {
        emit(app, &r);
        q.0.push(r);
    }
    let days: HashMap<String, String> = q.0.iter().map(|r| (r.id.clone(), run_day(app, r))).collect();
    q.0.sort_by(|a, b| days[&a.id].cmp(&days[&b.id]));
    if q.1 {
        return;
    }
    q.1 = true;
    let app = app.clone();
    std::thread::spawn(move || loop {
        let next = {
            let mut q = crate::lock(&QUEUE);
            if q.0.is_empty() {
                q.1 = false;
                return;
            }
            q.0.remove(0)
        };
        let mut r = next;
        // Stopped while it waited: never started.
        if app.state::<Stops>().0.lock().unwrap().contains(&r.id) {
            r.status = "stopped".into();
            r.finished = Some(proposals::now_local());
            r.error = Some("Stopped before it started.".into());
            for s in r.steps.iter_mut() {
                s.status = "skipped".into();
            }
            emit(&app, &r);
            continue;
        }
        run(&app, &mut r);
    });
}

/// Runs left "queued" or "running" by an app that closed (or a request that failed half way):
/// marked stopped at start, as nothing will take them.
pub fn settle_stale() {
    for mut r in all_runs().into_iter().filter(|r| r.status == "queued" || r.status == "running") {
        r.status = "stopped".into();
        r.finished = Some(proposals::now_local());
        r.error = Some("Brainstead closed before it finished.".into());
        for s in r.steps.iter_mut().filter(|s| s.status == "waiting" || s.status == "running") {
            s.status = "skipped".into();
        }
        save_run(&r);
    }
}

/// Ingests these sources (or notes) one after another, in the background, oldest first, after
/// any already waiting. Returns the runs' ids.
/// `unattended`: an assistant session nobody is watching asked, so its changes are treated as a
/// scheduled run's.
#[tauri::command]
pub fn ingest_start(app: AppHandle, paths: Vec<String>, unattended: Option<bool>) -> Res<Vec<String>> {
    ingest_start_for(&app, paths, unattended.unwrap_or(false).then_some("scheduled"), None)
}

/// [`ingest_start`] for a job: its trigger, and the group its changes go in.
pub fn ingest_start_for(app: &AppHandle, paths: Vec<String>, trigger: Option<&str>, group: Option<&str>) -> Res<Vec<String>> {
    let app = app.clone();
    let root = root(&app)?;
    let model = crate::ask::job_model(&app, "ingest");
    let mut runs = Vec::new();
    for p in paths {
        brainstead_core::trash::safe_rel(&p).map_err(EditError::from)?;
        if !root.join(&p).is_file() {
            return Err(brainstead_core::write::WriteError::NotFound(format!("{p} isn't in the vault.")).into());
        }
        let r = Run {
            id: proposals::new_id(),
            kind: ingest_kind(),
            note: None,
            source: p,
            started: proposals::now_local(),
            finished: None,
            model: model.clone(),
            status: "queued".into(),
            steps: STEPS.iter().map(|n| Step { name: n.to_string(), status: "waiting".into(), detail: String::new(), ms: 0 }).collect(),
            proposals: vec![],
            trigger: trigger.map(String::from),
            group: group.map(String::from),
            dropped: vec![],
            pages: vec![],
            error: None,
            logged: false,
            planned: 0,
        };
        runs.push(r);
    }
    // Every path is checked before any run is announced: a bad one leaves nothing half queued.
    let ids = runs.iter().map(|r| r.id.clone()).collect();
    enqueue(&app, runs);
    Ok(ids)
}

/// The pages a text may touch: every entity, concept and summary page, with its aliases.
fn wiki_pages(root: &std::path::Path) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for sub in ["entities", "concepts", "summaries"] {
        let Ok(rd) = std::fs::read_dir(root.join("wiki").join(sub)) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".md") {
                continue;
            }
            let text = std::fs::read_to_string(e.path()).unwrap_or_default();
            let fm = brainstead_core::frontmatter::split(&text);
            out.push((format!("wiki/{sub}/{name}"), brainstead_core::frontmatter::list(&fm.data, &["aliases", "alias"], false)));
        }
    }
    out.sort();
    out
}

fn run(app: &AppHandle, r: &mut Run) {
    let stopped = |r: &Run| app.state::<Stops>().0.lock().unwrap().contains(&r.id);
    r.status = "running".into();
    emit(app, r);
    let result = if r.kind == "meeting" { meeting_steps(app, r, &stopped) } else { steps(app, r, &stopped) };
    let now = proposals::now_local();
    r.finished = Some(now);
    match result {
        Ok(()) => r.status = "done".into(),
        Err(e) if stopped(r) => {
            r.status = "stopped".into();
            r.error = Some(e);
        }
        Err(e) => {
            r.status = "failed".into();
            r.error = Some(e);
        }
    }
    for s in r.steps.iter_mut().filter(|s| s.status == "waiting" || s.status == "running") {
        s.status = "skipped".into();
    }
    emit(app, r);
    let link = brainstead_core::lint::name_of(&r.source).to_string();
    let _ = tauri_plugin_notification::NotificationExt::notification(app)
        .builder()
        .title("Ingest")
        .body(match r.status.as_str() {
            "done" => format!("{link}: {} change{} to review", r.proposals.len(), if r.proposals.len() == 1 { "" } else { "s" }),
            "stopped" => format!("{link}: stopped"),
            _ => format!("{link}: {}", r.error.clone().unwrap_or_default()),
        })
        .show();
}

/// Marks step `i` running, runs `f`, records how it went.
fn step<T>(app: &AppHandle, r: &mut Run, i: usize, f: impl FnOnce(&mut Run) -> Result<(T, String), String>) -> Result<T, String> {
    r.steps[i].status = "running".into();
    emit(app, r);
    let t0 = std::time::Instant::now();
    let out = f(r);
    r.steps[i].ms = t0.elapsed().as_millis() as u64;
    match out {
        Ok((v, detail)) => {
            r.steps[i].status = "done".into();
            r.steps[i].detail = detail;
            emit(app, r);
            Ok(v)
        }
        Err(e) => {
            r.steps[i].status = "failed".into();
            r.steps[i].detail = e.clone();
            emit(app, r);
            Err(e)
        }
    }
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

fn steps(app: &AppHandle, r: &mut Run, stopped: &dyn Fn(&Run) -> bool) -> Result<(), String> {
    let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
    let abs = root.join(&r.source);
    let sees_images = brainstead_core::ask::Cli::route(&r.model).sees_images();
    // An image source, as the model is shown it (when its assistant sees images).
    let mut picture: Option<PathBuf> = None;
    let origin = |r: &Run, label: &str| Origin {
        kind: "ingest".into(),
        chat: Some(r.id.clone()),
        label: Some(label.into()),
        run: Some(r.group.clone().unwrap_or_else(|| r.id.clone())),
        trigger: r.trigger.clone(),
    };

    // 1. The text.
    let mut source = step(app, r, 0, |r| {
        if brainstead_core::vault::is_note_ext(&r.source) || r.source.ends_with(".txt") {
            let t = std::fs::read_to_string(&abs).map_err(|e| format!("Couldn't read it: {e}"))?;
            let n = t.chars().count();
            Ok((Source { rel: r.source.clone(), text: SourceText::Text(t) }, format!("{n} characters")))
        } else {
            let bytes = std::fs::read(&abs).map_err(|e| format!("Couldn't read it: {e}"))?;
            let sha = brainstead_core::write::version(&bytes);
            let x = crate::vault::text_cache().get(&abs, &sha);
            if x.kind == "image" {
                return image_source(r, &abs, &sha, x, sees_images, &mut picture);
            }
            if x.parts.is_empty() {
                return Err("Brainstead couldn't read any text from it (a scanned PDF, or a format it doesn't read).".into());
            }
            let d = format!(
                "{} · {}",
                plural(x.parts.len(), if x.kind == "pdf" { "page" } else { "part" }),
                plural(x.text().chars().count(), "character")
            );
            Ok((Source { rel: r.source.clone(), text: SourceText::Parts(x) }, d))
        }
    })?;
    if stopped(r) {
        return Err("Stopped.".into());
    }

    // 2. Names: corrections for a markdown source are made (an agent change); the model reads the
    // corrected text. When the correction is held, the text as it is on disk is kept: a change
    // whose quotes are only in the corrected text is flagged.
    let mut on_disk = match &source.text {
        SourceText::Text(t) => Some(t.clone()),
        SourceText::Parts(_) | SourceText::Image(_) => None,
    };
    step(app, r, 1, |r| {
        let SourceText::Text(t) = &source.text else { return Ok(((), "Not a text file: left as it is".into())) };
        let subs = brainstead_core::names::load(&brainstead_core::names::path(&crate::platform::data_dir()));
        if subs.is_empty() {
            return Ok(((), "No name corrections saved yet".into()));
        }
        let (fixed, changes) = brainstead_core::names::apply(t, &subs, brainstead_core::lint::name_of(&r.source), |_| false);
        let asks = brainstead_core::names::questions(t, &subs, brainstead_core::lint::name_of(&r.source)).len();
        if changes.is_empty() {
            let d =
                if asks > 0 { format!("None certain; {} to check by hand", plural(asks, "ambiguous one")) } else { "None needed".into() };
            return Ok(((), d));
        }
        let what = changes.iter().map(|c| format!("{} → {}", c.wrong, c.right)).collect::<Vec<_>>().join(", ");
        let mut s = crate::changes::Submit::new(
            &r.source,
            proposals::Kind::Edit,
            &format!("Correct names: {what}"),
            "From the vault's corrections list, before ingesting.",
            origin(r, "Name corrections"),
            brainstead_core::changes::from_texts(Some(t), &fixed),
        );
        s.model = None;
        let o = crate::changes::submit(app, s).map_err(|e| e.message().to_string())?;
        r.proposals.push(o.id);
        if o.applied {
            on_disk = None;
        }
        source.text = SourceText::Text(fixed);
        let done = if o.applied { "made" } else { "held in Changes" };
        Ok(((), format!("{} {done} ({what})", plural(changes.len(), "correction"))))
    })?;

    // 3. The pages it mentions.
    let body = match &source.text {
        SourceText::Text(t) => t.clone(),
        SourceText::Parts(x) | SourceText::Image(x) => x.text(),
    };
    let pages = step(app, r, 2, |r| {
        let m = brainstead_core::names::mentions(&body, &wiki_pages(&root));
        let pages: Vec<(String, String)> =
            m.iter().filter_map(|m| std::fs::read_to_string(root.join(&m.page)).ok().map(|t| (m.page.clone(), t))).collect();
        r.pages = m.iter().map(|m| m.page.clone()).collect();
        let names: Vec<String> =
            m.iter().take(6).map(|m| brainstead_core::lint::stem(brainstead_core::lint::name_of(&m.page)).to_string()).collect();
        let more = if m.len() > 6 { format!(" and {} more", m.len() - 6) } else { String::new() };
        Ok((pages, if m.is_empty() { "None yet".into() } else { format!("{}: {}{more}", plural(m.len(), "page"), names.join(", ")) }))
    })?;
    if stopped(r) {
        return Err("Stopped.".into());
    }

    // 4. The model.
    let model = r.model.clone();
    let answer = step(app, r, 3, |r| {
        let today = chrono::Local::now().format("%A %-d %B %Y").to_string();
        let prompt = core_ingest::prompt(&source, &pages, &brainstead_core::catalogue::render(&root), &today);
        let system = brainstead_core::ask::system_prompt(&crate::ask::today(), false);
        let images: Vec<PathBuf> = picture.iter().cloned().collect();
        let out = crate::ask::run_with_images(app, &r.id, &prompt, &model, None, &system, None, &images, &mut |_| {});
        if let Some(p) = &picture {
            let _ = std::fs::remove_file(p);
        }
        if let Some(e) = out.error {
            return Err(e);
        }
        let a = core_ingest::parse_answer(&out.text)?;
        let n = a.pages.len() + usize::from(a.summary.is_some());
        Ok((a, format!("{} drafted", plural(n, "page change"))))
    })?;
    if stopped(r) {
        return Err("Stopped.".into());
    }

    // 5. The checks.
    let today = chrono::Local::now().date_naive().to_string();
    let planned = step(app, r, 4, |r| {
        let read = |rel: &str| brainstead_core::trash::safe_rel(rel).ok().and_then(|_| std::fs::read_to_string(root.join(rel)).ok());
        let resolve = |name: &str| {
            app.state::<VaultService>()
                .with_index(|ix, _| brainstead_core::names::resolve(ix, name))
                .ok()
                .and_then(|x| x.exact.map(|(p, _)| p))
                .filter(|p| p.starts_with("wiki/"))
        };
        let (planned, dropped) = core_ingest::plan(&source, &answer, read, resolve, &today, core_ingest::page_budget(pages.len()));
        let d = format!("{} kept, {} dropped", planned.len(), dropped.len());
        r.dropped = dropped;
        Ok((planned, d))
    })?;

    // 6. Made, or held: by the rule for agent changes (src/changes.rs). Started by the user, they're
    // applied and flagged where a check failed; from the daily check, a change that fails a
    // check is held in Changes.
    let on_disk = on_disk.filter(|t| !matches!(&source.text, SourceText::Text(now) if now == t)).map(|t| proposals::normalise_quote(&t));
    step(app, r, 5, |r| {
        let label = format!("Ingest of {}", brainstead_core::lint::name_of(&r.source));
        let (mut applied, mut held, mut failed) = (0, 0, 0);
        r.planned = planned.len();
        save_run(r);
        for p in planned {
            let instruction = match &p.before {
                Some(b) => brainstead_core::changes::from_texts(Some(b), &p.after),
                None => brainstead_core::changes::Instruction::Page { content: p.after.clone() },
            };
            let mut s =
                crate::changes::Submit::new(&p.page, p.kind, &p.title, &format!("From {}.", r.source), origin(r, &label), instruction);
            s.flags = flags(&p.warnings, on_disk.as_deref(), &p.quotes);
            s.quotes = p.quotes;
            s.model = Some(model.clone());
            if !p.claims.is_empty() {
                let kept = p.claims.iter().map(|c| Kept::from_claim(c, &r.source, p.entry.as_deref(), &today)).collect();
                s.claims = Some(brainstead_core::claims::Update { source: r.source.clone(), claims: kept, ..Default::default() });
            }
            match crate::changes::submit(app, s) {
                Ok(o) => {
                    r.proposals.push(o.id);
                    if o.applied {
                        applied += 1;
                    } else {
                        held += 1;
                    }
                }
                Err(e) => {
                    crate::applog!("ingest {}: {} not changed: {}", r.id, p.page, e.message());
                    failed += 1;
                }
            }
        }
        let mut d = vec![];
        if applied > 0 {
            d.push(format!("{} made", plural(applied, "page change")));
        }
        if held > 0 {
            d.push(format!("{} held in Changes", plural(held, "change")));
        }
        if failed > 0 {
            d.push(format!("{} couldn't be made", plural(failed, "change")));
        }
        Ok(((), if d.is_empty() { "Nothing to change".into() } else { d.join(", ") }))
    })?;
    Ok(())
}

/// What an ingest's change is flagged for: its own warnings (a quote not found, text read from an
/// image) and, when the source's names were corrected but that correction is held, a quote that
/// is only in the corrected text (`on_disk` is the source as it is on disk, normalised).
fn flags(warnings: &[String], on_disk: Option<&str>, quotes: &[proposals::Quote]) -> Vec<String> {
    let mut out: Vec<String> = warnings
        .iter()
        .map(|w| if w == core_ingest::IMAGE_WARNING { "It rests on text read from an image.".into() } else { w.clone() })
        .collect();
    if let Some(t) = on_disk {
        if quotes.iter().any(|q| !t.contains(&proposals::normalise_quote(&q.text))) {
            out.push("A quote is only in the source with its names corrected, and that correction is held.".into());
        }
    }
    out
}

/// Step 1 for an image: its text (perhaps none) and, for an assistant that sees images, the
/// picture it's shown. With no text and an assistant that can't see it, there's nothing to go on.
fn image_source(
    r: &Run,
    abs: &std::path::Path,
    sha: &str,
    x: brainstead_core::extract::Extracted,
    sees_images: bool,
    picture: &mut Option<PathBuf>,
) -> Result<(Source, String), String> {
    let chars = x.text().chars().count();
    let label = brainstead_core::ask::Cli::route(&r.model).label();
    if sees_images {
        *picture = Some(crate::platform::image_for_model(abs, sha)?);
    } else if chars == 0 {
        return Err(format!(
            "No text was found in the image, and {label} can't see pictures. Ingest it with Claude Code or Codex (Settings › AI assistants)."
        ));
    }
    let text = if chars == 0 { "No text found in it".to_string() } else { format!("{} read from it", plural(chars, "character")) };
    let d = if sees_images { format!("{text}; {label} sees the picture") } else { format!("{text}; {label} gets this text only") };
    Ok((Source { rel: r.source.clone(), text: SourceText::Image(x) }, d))
}

// ── A meeting note from a transcript ──────────────────────────────────────────

fn owner(app: &AppHandle) -> Option<String> {
    app.state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .ui
        .get("ownerName")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A transcript, with what its file name says.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    path: String,
    mtime: i64,
    inferred: brainstead_core::meeting::Inference,
    /// Already dealt with, and how: "ingested" (a wiki page cites it) or "linked" (a note links
    /// to it). None while it's still to do, even when its meeting note exists (it may be empty).
    done: Option<&'static str>,
}

/// A transcript's first few kilobytes (its header lines), or nothing when it can't be read.
fn read_head(abs: &std::path::Path) -> String {
    use std::io::Read;
    let mut buf = Vec::new();
    match std::fs::File::open(abs) {
        Ok(f) => {
            let _ = f.take(4096).read_to_end(&mut buf);
            String::from_utf8_lossy(&buf).into_owned()
        }
        Err(_) => String::new(),
    }
}

/// The Teams transcripts in `sources/`, newest first, each saying whether it's been dealt with.
#[tauri::command]
pub async fn meeting_transcripts(app: AppHandle) -> Res<Vec<Transcript>> {
    blocking(move || {
        let root = root(&app)?;
        let me = owner(&app);
        let files = app.state::<VaultService>().with_index(|ix, _| ix.list(Some("source")))?;
        let notes = app.state::<VaultService>().with_index(|ix, _| ix.list(Some("note")))?;
        let names: Vec<String> = notes.iter().map(|n| brainstead_core::lint::name_of(&n.path).to_string()).collect();
        let files: Vec<_> =
            files.into_iter().filter(|f| brainstead_core::meeting::is_transcript(brainstead_core::lint::name_of(&f.path))).collect();
        let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
        let (ingested, linked) = app.state::<VaultService>().with_index(|ix, _| {
            let ing = ix.ingested(&paths)?;
            let linked = paths.iter().map(|p| ix.linkers(p).map(|l| !l.is_empty())).collect::<Result<Vec<_>, _>>()?;
            Ok((ing, linked))
        })?;
        let mut out: Vec<Transcript> = files
            .into_iter()
            .zip(ingested.into_iter().zip(linked))
            .map(|(f, (ingested, linked))| {
                let name = brainstead_core::lint::name_of(&f.path).to_string();
                let forms = |first: &str| {
                    let mut v: Vec<String> =
                        names.iter().filter(|n| n.starts_with(&format!("1-1. {first}")) && n.ends_with(".md")).cloned().collect();
                    v.sort();
                    v
                };
                let head = read_head(&root.join(&f.path));
                let inferred = brainstead_core::meeting::infer(&name, &head, me.as_deref(), forms, |f| root.join(f).exists());
                // Its meeting note existing isn't enough: it may be the template, still to fill in.
                let done = if ingested {
                    Some("ingested")
                } else if linked {
                    Some("linked")
                } else {
                    None
                };
                Transcript { path: f.path, mtime: f.mtime, inferred, done }
            })
            .collect();
        out.sort_by_key(|t| std::cmp::Reverse(t.mtime));
        Ok(out)
    })
    .await
}

/// Drafts a note from each transcript (one after another, in the background), and makes it.
/// Returns the runs' ids.
#[tauri::command]
pub fn meeting_draft(app: AppHandle, items: Vec<(String, NoteSpec)>, unattended: Option<bool>) -> Res<Vec<String>> {
    let trigger = unattended.unwrap_or(false).then(|| "scheduled".to_string());
    let root = root(&app)?;
    let model = crate::ask::job_model(&app, "meeting");
    let mut runs = Vec::new();
    // A note already being drafted from a transcript isn't drafted twice.
    let busy: Vec<String> = all_runs()
        .into_iter()
        .filter(|r| r.kind == "meeting" && (r.status == "queued" || r.status == "running"))
        .map(|r| r.source)
        .collect();
    for (path, spec) in items {
        brainstead_core::trash::safe_rel(&path).map_err(EditError::from)?;
        if busy.contains(&path) {
            Err(brainstead_core::write::WriteError::Exists("A note is being drafted from this transcript already.".into()))?;
        }
        let bad = |m: &str| -> Res<()> { Err(brainstead_core::write::WriteError::Invalid(m.into()).into()) };
        if chrono::NaiveDate::parse_from_str(spec.date.trim(), "%Y-%m-%d").is_err() {
            bad(&format!("Not a date: {}", spec.date))?;
        }
        let f = spec.filename();
        if spec.name.trim().is_empty() || f.contains(['/', '\\', ':']) {
            bad("Give the note a name.")?;
        }
        if root.join(&f).exists() {
            bad(&format!("{f} exists already: fill it in by hand, or change the name or date."))?;
        }
        let r = Run {
            id: proposals::new_id(),
            kind: "meeting".into(),
            note: Some(spec),
            source: path,
            started: proposals::now_local(),
            finished: None,
            model: model.clone(),
            status: "queued".into(),
            steps: MEETING_STEPS
                .iter()
                .map(|n| Step { name: n.to_string(), status: "waiting".into(), detail: String::new(), ms: 0 })
                .collect(),
            proposals: vec![],
            trigger: trigger.clone(),
            group: None,
            dropped: vec![],
            pages: vec![],
            error: None,
            logged: false,
            planned: 0,
        };
        runs.push(r);
    }
    // Every path is checked before any run is announced: a bad one leaves nothing half queued.
    let ids = runs.iter().map(|r| r.id.clone()).collect();
    enqueue(&app, runs);
    Ok(ids)
}

fn meeting_steps(app: &AppHandle, r: &mut Run, stopped: &dyn Fn(&Run) -> bool) -> Result<(), String> {
    let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
    let spec = r.note.clone().ok_or("No note given.")?;
    let mut text = step(app, r, 0, |r| {
        let t = std::fs::read_to_string(root.join(&r.source)).map_err(|e| format!("Couldn't read it: {e}"))?;
        let n = t.chars().count();
        Ok((t, format!("{n} characters")))
    })?;
    step(app, r, 1, |r| {
        let subs = brainstead_core::names::load(&brainstead_core::names::path(&crate::platform::data_dir()));
        let (fixed, changes) = brainstead_core::names::apply(&text, &subs, brainstead_core::lint::name_of(&r.source), |_| false);
        text = fixed;
        Ok((
            (),
            if changes.is_empty() {
                "None needed".into()
            } else {
                changes.iter().map(|c| format!("{} → {}", c.wrong, c.right)).collect::<Vec<_>>().join(", ")
            },
        ))
    })?;
    if stopped(r) {
        return Err("Stopped.".into());
    }
    let model = r.model.clone();
    let filename = spec.filename();
    let note = step(app, r, 2, |r| {
        let template = std::fs::read_to_string(root.join(format!("Templates/{}.md", spec.note_type.trim()))).unwrap_or_default();
        let sc = brainstead_core::meeting::scaffold(&template, spec.name.trim());
        let prompt = brainstead_core::meeting::prompt(&filename, &sc, &text, owner(app).as_deref());
        let system = brainstead_core::ask::system_prompt(&crate::ask::today(), false);
        let out = crate::ask::run(app, &r.id, &prompt, &model, None, &system, None, &mut |_| {});
        if let Some(e) = out.error {
            return Err(e);
        }
        let n = brainstead_core::meeting::note_from_answer(&out.text)?;
        let lines = n.lines().count();
        Ok((n, format!("{lines} lines")))
    })?;
    if stopped(r) {
        return Err("Stopped.".into());
    }
    step(app, r, 3, |r| {
        let origin = Origin {
            kind: "meeting".into(),
            chat: Some(r.source.clone()),
            label: Some("Meeting note".into()),
            run: Some(r.id.clone()),
            trigger: r.trigger.clone(),
        };
        let mut s = crate::changes::Submit::new(
            &filename,
            proposals::Kind::New,
            &format!("New note: {}", filename.trim_end_matches(".md")),
            &format!("From {}.", r.source),
            origin,
            brainstead_core::changes::Instruction::Page { content: note },
        );
        s.model = Some(model.clone());
        let o = crate::changes::submit(app, s).map_err(|e| e.message().to_string())?;
        r.proposals.push(o.id);
        Ok(((), if o.applied { "Made: ingest it, and trash the transcript".into() } else { "Held in Changes".to_string() }))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use brainstead_core::proposals::Quote;

    #[test]
    fn what_an_ingest_change_is_flagged_for() {
        let q = vec![Quote {
            source: "Steerco".into(),
            text: "Staff launch now targeted for 28 November".into(),
            checked: Some(true),
            ..Default::default()
        }];
        assert!(flags(&[], None, &q).is_empty());
        assert_eq!(flags(&[core_ingest::IMAGE_WARNING.into()], None, &q), ["It rests on text read from an image."]);
        // Names corrected but held: the quote must be in the file as it is on disk.
        let disk = proposals::normalise_quote("Staf launch now targetted for 28 November");
        assert!(flags(&[], Some(&disk), &q)[0].contains("held"));
        let disk = proposals::normalise_quote("Staff launch now targeted for 28 November, pending.");
        assert!(flags(&[], Some(&disk), &q).is_empty());
    }
}
