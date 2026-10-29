// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

mod applog;
mod ask;
mod bridge;
mod capture;
mod changes;
mod contradict;
mod currentstate;
mod daily_check;
mod dataview;
mod edits;
mod find;
mod gtd;
mod ingest;
mod knowledge;
mod listnotes;
mod menu;
mod nativehost;
mod notes;
mod platform;
mod reviews;
mod settings;
mod skills;
mod speech;
mod sync;
mod tray;
mod updates;
mod vault;
mod weekly;
mod weekprep;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use settings::Settings;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};
use vault::VaultService;

// Not DECORATIONS: the window's config decides those (none on Windows), and a state file saved
// while the window still had them would put the native title bar back.
const STATE_FLAGS: StateFlags = StateFlags::all().difference(StateFlags::VISIBLE.union(StateFlags::DECORATIONS));

/// Takes a lock even when a panic poisoned it: what it guards is plain data (settings, an index
/// whose transaction rolled back), and one bad file mustn't take the app down with it.
pub(crate) fn lock<T: ?Sized>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs `f` on a blocking thread, so a command that waits on the index lock (held by the watcher
/// through a long batch) or on the disk never holds up the main thread. A panic there comes back
/// as an error.
pub(crate) async fn blocking<T, E>(f: impl FnOnce() -> Result<T, E> + Send + 'static) -> Result<T, E>
where
    T: Send + 'static,
    E: From<String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| E::from(e.to_string()))?
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    settings_path: PathBuf,
}

#[tauri::command]
fn settings_read(st: State<AppState>) -> Settings {
    lock(&st.settings).clone()
}

/// Copies the vault's old `scripts/substitutions.json` into the app data folder, once.
fn migrate_corrections(vault: &str) {
    match brainstead_core::names::migrate(&platform::data_dir(), std::path::Path::new(vault)) {
        Ok(true) => applog!("names: copied {} from the vault to the app data folder", brainstead_core::names::LEGACY),
        Ok(false) => {}
        Err(e) => applog!("names: couldn't copy the corrections from the vault: {e}"),
    }
}

/// Saves the settings the window sends, and reopens the vault if its folder or exclusions changed.
#[tauri::command]
fn settings_write(app: AppHandle, st: State<AppState>, settings: Settings) -> Result<Settings, String> {
    let next = settings.normalised()?;
    if scene().is_some() {
        return Ok(next);
    }
    // Saved first: a save that fails leaves the settings in use as they were, matching the file.
    let prev = {
        let mut cur = lock(&st.settings);
        settings::save(&st.settings_path, &next)?;
        std::mem::replace(&mut *cur, next.clone())
    };
    // A changed schedule, or taking the reviews over from the previous app, first fires at the next
    // time rather than catching up at once.
    let mut stamp = prev.summaries.changed(&next.summaries);
    if next.summaries_here && !prev.summaries_here {
        stamp = vec![brainstead_core::reviews::ReviewKind::Daily, brainstead_core::reviews::ReviewKind::Weekly];
    }
    if !stamp.is_empty() {
        app.state::<reviews::Reviews>().stamp(&stamp);
        let _ = tauri::Emitter::emit(&app, "reviews-changed", ());
    }
    // The weekly review's preparation, likewise, when the review moves, the jobs come here or it's
    // switched back on.
    let review_moved = prev.weekly_review != next.weekly_review;
    let prep_on = |s: &settings::Settings| s.ui.get("weekprepEnabled").and_then(|v| v.as_bool()).unwrap_or(true);
    if review_moved || (next.summaries_here && !prev.summaries_here) || (prep_on(&next) && !prep_on(&prev)) {
        app.state::<weekprep::WeekPrep>().stamp_now();
    }
    if prev.theme != next.theme {
        let dark = match next.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => app.get_webview_window("main").is_some_and(|w| matches!(w.theme(), Ok(tauri::Theme::Dark))),
        };
        menu::set_about(&app, dark);
    }
    if log_days(&prev) != log_days(&next) {
        applog::prune(&applog::dir(&platform::data_dir()), log_days(&next), &chrono::Local::now().format("%Y-%m-%d").to_string());
    }
    if prev.capture_shortcut != next.capture_shortcut {
        capture::register(&app, &next.capture_shortcut);
    }
    // Show in the menu bar (Settings › General).
    if prev.ui.get("menuBar") != next.ui.get("menuBar") {
        tray::apply(&app);
    }
    if let (Some(v), true) = (&next.vault_path, prev.vault_path != next.vault_path) {
        migrate_corrections(v);
    }
    if prev.vault_path != next.vault_path || prev.excluded != next.excluded {
        app.state::<VaultService>().start(&app, next.vault_path.clone(), next.excluded.clone(), false);
    }
    Ok(next)
}

#[tauri::command]
fn vault_status(svc: State<VaultService>) -> vault::Status {
    svc.status()
}

#[tauri::command]
fn rebuild_index(app: AppHandle, st: State<AppState>, svc: State<VaultService>) {
    let s = lock(&st.settings).clone();
    svc.start(&app, s.vault_path, s.excluded, true);
}

/// The files in one layer ("note", "wiki", "source", "template"), or all of them.
#[tauri::command]
async fn files_list(app: AppHandle, layer: Option<String>) -> Result<Vec<brainstead_core::read::FileSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<VaultService>().with_index(|ix, _| ix.list(layer.as_deref())))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Doc {
    meta: brainstead_core::read::DocMeta,
    /// The file as it is on disk now.
    content: String,
    /// The vault's real path, for image URLs through the asset protocol.
    root: String,
    /// What `doc_save` checks the file against: a hash of the bytes read.
    version: String,
    /// Why the file can't be edited here, when it can't: it isn't UTF-8 text.
    #[serde(skip_serializing_if = "Option::is_none")]
    read_only: Option<String>,
}

/// Why a file that isn't UTF-8 can be read here but not saved.
pub(crate) fn not_utf8(path: &str) -> String {
    format!("{path} isn't UTF-8 text, so Brainstead shows it read-only: saving it here would replace every character it can't read.")
}

/// One document: its text from disk, and what the index knows about it.
#[tauri::command]
async fn doc_read(app: AppHandle, path: String) -> Result<Doc, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let svc = app.state::<VaultService>();
        let abs = svc.resolve_path(&path)?;
        // Text is sent to the window; anything else (a PDF) is fetched from the asset protocol there.
        let text = matches!(
            path.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref(),
            Some("md" | "txt" | "html" | "htm" | "json" | "csv" | "log" | "yaml" | "yml" | "xml" | "eml" | "vtt" | "srt")
        );
        let bytes = if text { std::fs::read(&abs).map_err(|e| format!("Couldn't read {path}: {e}"))? } else { Vec::new() };
        let mut meta = svc.with_index(|ix, _| ix.doc_meta(&path))?.ok_or_else(|| format!("{path} isn't in the index."))?;
        let root = svc.root().unwrap_or_default();
        brainstead_core::read::fill_context(&root, &mut meta.backlinks);
        let version = brainstead_core::write::version(&bytes);
        // Shown decoded as best it can be, but never saved back: every byte that isn't UTF-8
        // would be replaced (doc_save refuses too).
        let read_only = std::str::from_utf8(&bytes).is_err().then(|| not_utf8(&path));
        Ok(Doc {
            meta,
            content: String::from_utf8_lossy(&bytes).into_owned(),
            root: root.to_string_lossy().into_owned(),
            version,
            read_only,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Where each wikilink target goes (None: a ghost link).
#[tauri::command]
async fn links_resolve(app: AppHandle, targets: Vec<String>) -> Result<Vec<Option<String>>, String> {
    blocking(move || app.state::<VaultService>().with_index(|ix, _| ix.resolve(&targets))).await
}

#[tauri::command]
async fn search(app: AppHandle, req: brainstead_core::search::SearchRequest) -> Result<brainstead_core::search::Results, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<VaultService>().with_index(|ix, _| ix.search(&req)))
        .await
        .map_err(|e| e.to_string())?
}

/// Every task outside the templates, for the query language in the window (src/tasksq/).
#[tauri::command]
async fn tasks_all(app: AppHandle) -> Result<Vec<brainstead_core::taskquery::TaskRow>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<VaultService>().with_index(|ix, _| ix.all_tasks()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tasks_query(
    app: AppHandle,
    query: brainstead_core::taskquery::TaskQuery,
) -> Result<Vec<brainstead_core::taskquery::TaskRow>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<VaultService>().with_index(|ix, _| ix.tasks(&query)))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceRow {
    #[serde(flatten)]
    file: brainstead_core::read::FileSummary,
    /// Cited by a wiki page's `sources:` property.
    ingested: bool,
}

#[tauri::command]
async fn sources_list(app: AppHandle) -> Result<Vec<SourceRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<VaultService>().with_index(|ix, _| {
            let files = ix.list(Some("source"))?;
            let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
            let ing = ix.ingested(&paths)?;
            Ok(files.into_iter().zip(ing).map(|(file, ingested)| SourceRow { file, ingested }).collect())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The Graph screen: the files around `center` to `depth` links, or the whole wiki when there's no
/// centre.
#[tauri::command]
async fn graph(app: AppHandle, center: Option<String>, depth: usize) -> Result<brainstead_core::graph::Graph, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<VaultService>().with_index(|ix, _| match &center {
            Some(c) => ix.graph_around(c, depth),
            None => ix.graph_wiki(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The Activity screen: `log.md`'s entries and the heatmap of file changes by day.
#[derive(Serialize)]
struct Activity {
    log: Vec<brainstead_core::activity::LogEntry>,
    days: Vec<brainstead_core::activity::Day>,
}

#[tauri::command]
async fn activity(app: AppHandle) -> Result<Activity, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let svc = app.state::<VaultService>();
        let days = svc.with_index(|ix, _| ix.activity_files()).map(|f| brainstead_core::activity::heatmap(&f))?;
        let log = brainstead_core::activity::parse_log(&svc.root().map(|r| root_note(&r, "log.md")).unwrap_or_default());
        Ok(Activity { log, days })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The vault at a glance: notes and wiki pages by type, their top tags, and the most linked files.
#[tauri::command]
async fn glance(app: AppHandle) -> Result<brainstead_core::activity::Glance, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<VaultService>().with_index(|ix, _| ix.glance()))
        .await
        .map_err(|e| e.to_string())?
}

/// The files that last changed on one day (YYYY-MM-DD), newest first.
#[tauri::command]
async fn activity_day(app: AppHandle, date: String) -> Result<Vec<brainstead_core::read::FileSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<VaultService>().with_index(|ix, _| ix.changed_on(&date)))
        .await
        .map_err(|e| e.to_string())?
}

/// Each wiki page's sources count and aliases (the wiki list).
#[tauri::command]
async fn wiki_meta(app: AppHandle) -> Result<Vec<brainstead_core::lists::WikiMeta>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<VaultService>().with_index(|ix, _| ix.wiki_meta()))
        .await
        .map_err(|e| e.to_string())?
}

/// The window's maximise button moved (src/WindowControls.tsx): see `platform::set_max_button`.
#[tauri::command]
fn window_max_button(window: tauri::WebviewWindow, rect: Option<(i32, i32, i32, i32)>) {
    platform::set_max_button(&window, rect);
}

// Spelling and grammar for the editor. Not async: AppKit's checker wants the main thread.
#[tauri::command]
fn spell_check(text: String, lang: Option<String>) -> Vec<(usize, usize)> {
    platform::spell_check(&text, lang.as_deref())
}

#[tauri::command]
fn spell_grammar(text: String, lang: Option<String>) -> Vec<platform::GrammarIssue> {
    platform::spell_grammar(&text, lang.as_deref())
}

#[tauri::command]
fn spell_guesses(word: String, lang: Option<String>) -> Vec<String> {
    platform::spell_guesses(&word, lang.as_deref())
}

/// The English spelling languages macOS has, and its own (Settings › Notes).
#[tauri::command]
fn spell_languages() -> (Vec<String>, String) {
    platform::spell_languages()
}

#[tauri::command]
fn spell_accept(word: String, learn: bool) {
    platform::spell_accept(&word, learn)
}

/// A PowerPoint or Excel source's text, slide by slide or sheet by sheet, or the text read from an
/// image, for its preview.
#[tauri::command]
async fn source_parts(app: AppHandle, path: String) -> Result<brainstead_core::extract::Extracted, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let abs = app.state::<VaultService>().resolve_path(&path)?;
        // An image's text is kept by content hash, as the index read it: reading it again is slow.
        if brainstead_core::extract::kind_of(&path) == Some("image") {
            let bytes = std::fs::read(&abs).map_err(|e| e.to_string())?;
            return Ok(vault::text_cache().get(&abs, &brainstead_core::write::version(&bytes)));
        }
        brainstead_core::extract::extract(&abs, Some(platform::pdf_text), None)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reads one of the vault's own list notes at the root, or "" when it isn't there.
fn root_note(root: &std::path::Path, name: &str) -> String {
    std::fs::read_to_string(root.join(name)).unwrap_or_default()
}

#[tauri::command]
async fn bookmarks(app: AppHandle) -> Result<Vec<brainstead_core::lists::Bookmark>, String> {
    blocking(move || app.state::<VaultService>().with_index(|ix, v| ix.bookmarks(&root_note(&v.root, brainstead_core::lists::BOOKMARKS))))
        .await
}

#[tauri::command]
async fn smart_lists(app: AppHandle) -> Result<Vec<brainstead_core::lists::SmartList>, String> {
    blocking(move || {
        let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
        Ok(brainstead_core::lists::parse_smart_lists(&root_note(&root, brainstead_core::lists::SMART_LISTS)))
    })
    .await
}

/// Where a file embedded by name is, vault-relative.
#[tauri::command]
async fn asset_find(app: AppHandle, name: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<VaultService>().find_asset(&name)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// Copies a vault file's markdown, read here so the copy doesn't wait on the window (WebKit
/// drops a clipboard write that comes too long after the click).
#[tauri::command]
fn copy_file(app: AppHandle, svc: State<VaultService>, path: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let text = std::fs::read_to_string(svc.resolve_path(&path)?).map_err(|e| e.to_string())?;
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Permissions {
    /// None when it couldn't be tested.
    full_disk_access: Option<bool>,
    /// Only macOS has Full Disk Access to ask for.
    applies: bool,
}

#[tauri::command]
async fn permissions() -> Permissions {
    let fda = tauri::async_runtime::spawn_blocking(platform::full_disk_access).await.unwrap_or(None);
    Permissions { full_disk_access: fda, applies: cfg!(target_os = "macos") }
}

#[tauri::command]
fn open_full_disk_access() -> Result<(), String> {
    platform::open_full_disk_access_settings()
}

/// Which of these vault files (vault-relative paths) are still there, in the same order: for the
/// buttons that open a file named in a record kept from before (a run, a capture, a log line).
#[tauri::command]
async fn paths_exist(app: AppHandle, paths: Vec<String>) -> Result<Vec<bool>, String> {
    blocking(move || {
        let svc = app.state::<VaultService>();
        Ok(paths.iter().map(|p| svc.resolve_path(p).is_ok_and(|f| f.is_file())).collect())
    })
    .await
}

/// Opens a vault file (vault-relative path) in the app the system uses for it. Only vault files:
/// the window has no way to open any other path.
#[tauri::command]
async fn file_open(app: AppHandle, path: String) -> Result<(), String> {
    blocking(move || {
        use tauri_plugin_opener::OpenerExt;
        let abs = app.state::<VaultService>().resolve_path(&path)?;
        if !abs.is_file() {
            return Err(format!("{path} isn't in the vault any more."));
        }
        app.opener().open_path(abs.to_string_lossy(), None::<&str>).map_err(|e| format!("Couldn't open {path}: {e}"))
    })
    .await
}

/// Shows a vault file (vault-relative path) in the system's Quick Look panel, as Finder's space bar
/// does: Office files look as they do in Office, which the window can't draw.
#[tauri::command]
fn file_quick_look(app: AppHandle, path: String) -> Result<(), String> {
    let abs = app.state::<VaultService>().resolve_path(&path)?;
    if !abs.is_file() {
        return Err(format!("{path} isn't in the vault any more."));
    }
    let w = app.get_webview_window("main").ok_or_else(|| "No window to show Quick Look over.".to_string())?;
    platform::quick_look(&w, &abs)
}

/// Shows a vault file (vault-relative path) or the vault folder itself (empty path) in Finder.
#[tauri::command]
fn reveal(svc: State<VaultService>, path: String) -> Result<(), String> {
    let p = if path.is_empty() { svc.root().ok_or("No vault is open.")? } else { svc.resolve_path(&path)? };
    platform::reveal(&p)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    build: String,
    data_dir: String,
    /// This program, which is also the MCP server (`--mcp`).
    exe: String,
    /// What this system has of the features that aren't everywhere (`platform_features`).
    features: platform::PlatformFeatures,
}

#[tauri::command]
fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        build: option_env!("BRAINSTEAD_BUILD").unwrap_or("dev").to_string(),
        data_dir: platform::data_dir().to_string_lossy().into_owned(),
        exe: std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        features: platform::platform_features(),
    }
}

/// Which of the features that aren't on every system this one has, so the window hides the rest.
#[tauri::command]
fn platform_features() -> platform::PlatformFeatures {
    platform::platform_features()
}

/// Set once the window has dealt with unsaved edits, so the next quit goes through.
static QUIT_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Quits, after the window has asked about unsaved edits.
#[tauri::command]
fn app_quit(app: AppHandle) {
    QUIT_OK.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}

/// Restarts into an update just installed (src-tauri/src/updates.rs) by ⌘Q's road: the window
/// asks about unsaved edits and saves its settings, then calls app_restart. Choosing Stay there
/// keeps this copy running; the update is already in place for the next start.
pub(crate) fn restart_for_update(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = tauri::Emitter::emit(app, "restart-requested", ());
}

/// Restarts, after the window has asked about unsaved edits. Through the event loop, so the
/// quit's own work (the window's place, stopping what's running) is done as for ⌘Q. On Windows the
/// update is installed only now (`updates::install_downloaded`): its installer closes the app and
/// opens the new version itself.
#[tauri::command]
fn app_restart(app: AppHandle) {
    QUIT_OK.store(true, std::sync::atomic::Ordering::SeqCst);
    if updates::install_downloaded(&app) {
        // Still here: the installer didn't start, and the user was told.
        QUIT_OK.store(false, std::sync::atomic::Ordering::SeqCst);
        return;
    }
    app.request_restart();
}

/// What quitting does before the app goes: the window's size and place saved, assistant runs
/// stopped. On every quit, and before Windows' update installer closes the app without one.
pub(crate) fn before_quit(app: &AppHandle) {
    let _ = app.save_window_state(STATE_FLAGS);
    app.state::<std::sync::Arc<ask::Running>>().kill_all();
}

/// What would be cut short by restarting into an update now, or None.
fn busy(app: &AppHandle) -> Option<String> {
    let why = if app.state::<std::sync::Arc<ask::Running>>().any() {
        "An assistant is answering."
    } else if ingest::going() {
        "An ingest run is going."
    } else if daily_check::going() {
        "The daily check is running."
    } else if contradict::going() {
        "A contradiction check is running."
    } else if app.state::<reviews::Reviews>().running() {
        "A summary is being written."
    } else if app.state::<weekprep::WeekPrep>().running() {
        "The weekly review's suggestions are being prepared."
    } else if app.state::<find::Find>().running() {
        "Find tasks and projects is running."
    } else if currentstate::going() {
        "Write Current state is running."
    } else {
        return None;
    };
    Some(why.into())
}

/// Screenshot mode (tools/screenshots.py): the scene to set up, as JSON, from BRAINSTEAD_SCENE.
/// Settings aren't saved while it's set.
#[tauri::command]
fn scene() -> Option<String> {
    std::env::var("BRAINSTEAD_SCENE").ok().filter(|s| !s.is_empty())
}

/// Whether this copy takes part in the one-copy-per-session lock. Not a screenshot scene, which
/// runs beside the app on purpose, nor a copy with its own data folder (`BRAINSTEAD_DATA`, a dev
/// copy): the lock is the app identifier's, shared by every build, and that copy has its own index
/// and bridge.
fn shares_instance_lock() -> bool {
    scene().is_none() && std::env::var_os("BRAINSTEAD_DATA").is_none()
}

/// The window's background before the page paints, in step with --ground in src/styles.css and
/// the splash in index.html.
fn ground(dark: bool) -> (u8, u8, u8) {
    if dark {
        (0x16, 0x18, 0x1c)
    } else {
        (0xfa, 0xfb, 0xfc)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// `Brainstead --mcp …`: the MCP server a CLI starts, on stdin and stdout, before any window; or
/// `Brainstead chrome-extension://…`, the capture extensions' host Chrome starts.
/// None when the program was started as the app.
pub fn mcp_main() -> Option<i32> {
    // Started by Chrome as the capture extensions' host.
    if let Some(code) = nativehost::host_main() {
        return Some(code);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--mcp") {
        return None;
    }
    Some(brainstead_mcp::main_with(&args[1..], platform::data_dir()))
}

/// How many days of the app's logs are kept (Settings › About).
fn log_days(s: &Settings) -> u64 {
    s.ui.get("logDays").and_then(|v| v.as_u64()).filter(|d| (1..=365).contains(d)).unwrap_or(applog::DEFAULT_DAYS)
}

/// What the switch-over checklist (Settings › General) can find out for itself.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Switchover {
    /// Signatures in `automated.json`; None when there's no such file.
    automated: Option<usize>,
    /// The vault has `.claude/skills`, the skills Claude Code uses there.
    skills: bool,
    /// The previous app's own files are in the vault, or were retired from it here, so the
    /// checklist is worth showing.
    previous: bool,
    /// The vault has the previous app's `scripts/` folder.
    scripts: bool,
    /// The vault's `CLAUDE.md` tells agents to use Brainstead's MCP tools (`AGENT_BLOCK`).
    claude_md: bool,
}

/// Files only the previous app kept in the vault: its `scripts/` folder with the name
/// corrections and the scripts Brainstead's reviews, ingest and Knowledge health replaced.
const PREVIOUS_APP_FILES: [&str; 6] = [
    brainstead_core::names::LEGACY,
    "scripts/lint_wiki.py",
    "scripts/upsert_review_block.py",
    "scripts/log_append.py",
    "scripts/review_window.py",
    "scripts/session_logs.py",
];

/// Whether the previous app has been used on this vault.
fn previous_app_found(root: &std::path::Path) -> bool {
    PREVIOUS_APP_FILES.iter().any(|f| root.join(f).is_file())
}

#[tauri::command]
fn switchover_status(svc: State<VaultService>) -> Switchover {
    let automated = std::fs::read_to_string(reviews::automated_file())
        .ok()
        .map(|t| serde_json::from_str::<Vec<serde_json::Value>>(&t).map_or(0, |v| v.len()));
    let root = svc.root();
    let skills = root.as_ref().is_some_and(|r| r.join(".claude").join("skills").is_dir());
    let scripts = root.as_ref().is_some_and(|r| r.join("scripts").is_dir());
    let claude_md = root.as_ref().and_then(|r| std::fs::read_to_string(r.join("CLAUDE.md")).ok()).is_some_and(|t| t.contains(AGENT_BEGIN));
    let previous = root.as_deref().is_some_and(previous_app_found) || claude_md;
    Switchover { automated, skills, previous, scripts, claude_md }
}

const AGENT_BEGIN: &str = "<!-- brainstead:begin -->";
const AGENT_END: &str = "<!-- brainstead:end -->";

/// What the vault's `CLAUDE.md` tells an agent working in the vault once the previous app's skills
/// and scripts are retired: change it only through Brainstead's MCP tools, so every change is
/// checked and recorded in Changes (decided 2026-10-05).
const AGENT_BLOCK: &str = "## Changing the vault\n\n\
Brainstead manages this vault. Change it only through the `brainstead` MCP tools, never with your own file tools: \
edit_page (any page, or a new wiki page), create_note, add_image (an image for a note to embed), create_task, delete_task, rename_note, trash_note, and the task, Inbox and project tools. \
Brainstead checks each change and records it in its Changes screen, where the user can revert it. \
To ingest a source, use start_run with run ingest; to fix a misspelt name, fix_name; for the wiki's health, lint and fix_health. \
Read with search, read_section, backlinks and resolve_entity. \
The skills and `scripts/` that anything below mentions are retired: Brainstead does that work now, so don't look for them.";

/// `text` with the block between its markers, after the first heading (or at the top), in place of
/// one there already.
fn with_agent_block(text: &str) -> String {
    let block = format!("{AGENT_BEGIN}\n{AGENT_BLOCK}\n{AGENT_END}");
    if let (Some(a), Some(b)) = (text.find(AGENT_BEGIN), text.find(AGENT_END)) {
        if a < b {
            return format!("{}{block}{}", &text[..a], &text[b + AGENT_END.len()..]);
        }
    }
    let first = text.lines().next().unwrap_or("");
    if first.starts_with("# ") {
        let rest = text[first.len()..].trim_start_matches(['\r', '\n']);
        return format!("{first}\n\n{block}\n\n{rest}");
    }
    if text.trim().is_empty() {
        return format!("{block}\n");
    }
    format!("{block}\n\n{text}")
}

/// Retires the previous app's agent files once it's stopped: `.claude/skills/` and `scripts/` go
/// to Brainstead's Trash (restorable from there), and `CLAUDE.md` gets the block telling agents to
/// use the MCP tools (undoable with ⌘Z). The name corrections are copied out of `scripts/` first.
#[tauri::command]
async fn switchover_retire(app: AppHandle) -> Result<String, edits::EditError> {
    blocking(move || {
        edits::writable(&app.state::<AppState>())?;
        let root = app.state::<VaultService>().root().ok_or("No vault is open.".to_string())?;
        brainstead_core::names::migrate(&platform::data_dir(), &root).map_err(|e| e.to_string())?;
        let mut done = Vec::new();
        for rel in [".claude/skills", "scripts"] {
            if root.join(rel).is_dir() {
                brainstead_core::trash::move_to_trash(&root, rel, "folder")?;
                done.push(format!("{rel}/"));
            }
        }
        let md = root.join("CLAUDE.md");
        let before = std::fs::read_to_string(&md).ok();
        let after = with_agent_block(before.as_deref().unwrap_or(""));
        if before.as_deref() != Some(after.as_str()) {
            let version = match &before {
                Some(b) => brainstead_core::write::save_file(&md, &after, &brainstead_core::write::version(b.as_bytes()))?,
                None => brainstead_core::write::create_file(&md, &after)?,
            };
            let mut files = vec![edits::FileUndo { path: "CLAUDE.md".into(), before, version }];
            let entry = brainstead_core::reviews::log_entry(
                "update",
                "CLAUDE",
                Some("Agents use Brainstead's MCP tools"),
                chrono::Local::now().naive_local(),
            );
            if let Ok(f) = edits::add_log(&root, &entry) {
                files.push(f);
            }
            app.state::<edits::UndoStack>().push(edits::UndoEntry::files("Told agents in CLAUDE.md to use Brainstead".into(), files));
            done.push("CLAUDE.md updated".into());
        }
        edits::written(&app, &[md, root.join("log.md")]);
        Ok(if done.is_empty() {
            "Nothing left to retire.".into()
        } else {
            format!("Retired: {}. The folders are in the Trash.", done.join(", "))
        })
    })
    .await
}

#[cfg(test)]
mod switchover_tests {
    #[test]
    fn finds_the_previous_app_only_by_its_own_files() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join(".claude/skills/x")).unwrap();
        std::fs::create_dir_all(t.path().join("scripts")).unwrap();
        assert!(!super::previous_app_found(t.path()), "skills and an empty scripts folder aren't enough");
        std::fs::write(t.path().join("scripts/lint_wiki.py"), "").unwrap();
        assert!(super::previous_app_found(t.path()));
    }

    #[test]
    fn claude_md_gets_the_block_once() {
        let t = "# Vault\n\nThe wiki is in wiki/.\n";
        let a = super::with_agent_block(t);
        assert!(a.starts_with("# Vault\n\n<!-- brainstead:begin -->\n## Changing the vault"), "{a}");
        assert!(a.ends_with("<!-- brainstead:end -->\n\nThe wiki is in wiki/.\n"));
        // Again: the same, not a second block.
        assert_eq!(super::with_agent_block(&a), a);
        assert!(super::with_agent_block("").starts_with("<!-- brainstead:begin -->"));
        assert!(super::with_agent_block("No heading.\n").ends_with("<!-- brainstead:end -->\n\nNo heading.\n"));
    }
}

#[tauri::command]
fn login_item() -> Option<bool> {
    platform::open_at_login()
}

/// Turns Open at login on or off and tells every window (Settings and the menu-bar window both have the switch).
#[tauri::command]
fn login_item_set(app: AppHandle, on: bool) -> Result<Option<bool>, String> {
    platform::set_open_at_login(on)?;
    let now = platform::open_at_login();
    let _ = tauri::Emitter::emit(&app, "login-item-changed", now);
    Ok(now)
}

/// Shows the app's log folder in Finder.
#[tauri::command]
fn logs_reveal() -> Result<(), String> {
    let d = applog::dir(&platform::data_dir());
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    platform::reveal(&d)
}

pub fn run() {
    let context = tauri::generate_context!();
    // One copy per session, settled before the log, settings and index are touched: on Windows a
    // second copy hands over here and exits. The plugin below is the channel it hands over through,
    // and decides on its own only on Linux (`platform::claim_single_instance`).
    if shares_instance_lock() {
        platform::claim_single_instance(&context.config().identifier);
    }
    let data = platform::data_dir();
    let settings_path = data.join("settings.json");
    let (settings, settings_warning) = settings::load_checked(&settings_path);
    applog::init(&data, log_days(&settings));
    if let Some(w) = &settings_warning {
        crate::applog!("settings: {w}");
    }
    let vault_service = VaultService::opening(settings.vault_path.clone());
    let state = AppState { settings: Mutex::new(settings), settings_path };

    let builder = tauri::Builder::default();
    // First of the plugins, as the plugin requires: a second launch hands over to this copy, which
    // shows its window, rather than running a second copy on the same index, bridge and capture
    // shortcut. On Windows the plugin's own check comes too late to be relied on (its mutex is taken
    // before its window exists, and a copy started in between runs on), so `claim_single_instance`
    // above has settled it by now and the plugin is the hand-over channel.
    #[cfg(any(windows, target_os = "linux"))]
    let builder = if shares_instance_lock() {
        builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| tray::second_launch(app, &args)))
    } else {
        builder
    };
    builder
        .plugin(tauri_plugin_window_state::Builder::default().with_state_flags(STATE_FLAGS).with_denylist(&["capture", "tray"]).build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(capture::plugin())
        .manage(std::sync::Arc::new(ask::Running::default()))
        .manage(std::sync::Arc::new(ask::Turns::default()))
        .manage(skills::CheckRuns::default())
        .manage(reviews::Reviews::load())
        .manage(weekprep::WeekPrep::load())
        .manage(find::Find::load())
        .manage(capture::CaptureState::default())
        .manage(state)
        .manage(vault_service)
        .manage(menu::NoteMenu::default())
        .manage(edits::UndoStack::load(edits::undo_file(&data)))
        .manage(brainstead_core::drafts::Drafts::new(notes::drafts_dir(&data)))
        .manage(knowledge::Health::default())
        .manage(ingest::Stops::default())
        .manage(bridge::Waiting::default())
        .manage(tray::TrayState::default())
        .invoke_handler(tauri::generate_handler![
            graph,
            updates::update_offered,
            updates::update_install,
            updates::update_later,
            updates::update_skip,
            updates::update_check,
            source_parts,
            spell_check,
            window_max_button,
            spell_grammar,
            spell_guesses,
            spell_accept,
            spell_languages,
            skills::bookmarks_status,
            skills::bookmark_keep,
            skills::bookmarks_suggest,
            skills::draft_reply,
            skills::canonical_register,
            skills::doc_check,
            skills::doc_check_start,
            skills::doc_check_runs,
            skills::doc_check_stop,
            skills::doc_check_saved,
            listnotes::bookmark_remove,
            wiki_meta,
            logs_reveal,
            login_item,
            login_item_set,
            switchover_status,
            switchover_retire,
            reviews::automated_list,
            reviews::automated_save,
            reviews::automated_suggest,
            paths_exist,
            file_open,
            file_quick_look,
            activity,
            activity_day,
            listnotes::bookmark_toggle,
            listnotes::smart_list_save,
            listnotes::smart_list_delete,
            ingest::fixname_plan,
            ingest::fixname_apply,
            ingest::ingest_start,
            ingest::source_provenance,
            ingest::meeting_transcripts,
            ingest::meeting_draft,
            daily_check::daily_check_status,
            daily_check::daily_check_stop,
            reviews::reviews_stop,
            contradict::contradictions_stop,
            daily_check::daily_check_run_now,
            contradict::contradictions_report,
            contradict::contradictions_run,
            contradict::contradictions_mark,
            contradict::page_contradictions,
            contradict::page_facts,
            ingest::ingest_runs,
            ingest::ingest_stop,
            changes::changes_list,
            changes::change_get,
            changes::change_submit,
            changes::change_accept,
            changes::change_reject,
            changes::changes_accept_all,
            changes::changes_reject_all,
            changes::change_revert,
            changes::changes_revert_all,
            changes::change_for_editing,
            changes::changes_submit_many,
            changes::change_task_line,
            changes::change_project_note,
            knowledge::health_report,
            knowledge::health_fix,
            knowledge::health_ignore,
            knowledge::health_ignore_all,
            daily_check::catalogue_update,
            knowledge::health_unignore,
            knowledge::health_reshape,
            currentstate::current_state_start,
            currentstate::current_state_status,
            currentstate::current_state_stop,
            knowledge::health_dismiss,
            knowledge::health_trash_image,
            knowledge::health_create_page,
            knowledge::health_link_ghost,
            app_quit,
            app_restart,
            settings_read,
            settings_write,
            vault_status,
            rebuild_index,
            permissions,
            open_full_disk_access,
            reveal,
            files_list,
            glance,
            doc_read,
            links_resolve,
            search,
            tasks_query,
            sources_list,
            notes::sources_import,
            nativehost::capture_status,
            nativehost::capture_reveal,
            sync::sync_sign_in_state,
            sync::sync_sign_out,
            sync::sync_now,
            sync::sync_today_meetings,
            sync::sync_cached_meetings,
            sync::sync_list_channels,
            sync::sync_pull,
            bookmarks,
            smart_lists,
            asset_find,
            copy_text,
            copy_file,
            edits::task_toggle,
            edits::task_set_date,
            edits::task_replace,
            tasks_all,
            edits::task_rank,
            edits::tasks_rank_all,
            edits::undo,
            edits::undo_peek,
            edits::capture,
            notes::doc_save,
            notes::doc_create,
            notes::vault_create,
            notes::vault_add_examples,
            notes::draft_write,
            notes::draft_read,
            notes::draft_discard,
            notes::drafts_list,
            notes::image_save,
            notes::image_import,
            notes::image_add,
            notes::rename_preview,
            notes::rename_commit,
            notes::trash_move,
            notes::trash_list,
            notes::trash_quick_look,
            notes::trash_restore,
            notes::trash_delete,
            notes::trash_empty,
            notes::copy_rich,
            notes::clipboard_read,
            notes::user_scripts,
            notes::export_pdf,
            capture::capture_close,
            capture::capture_show,
            capture::capture_shortcut_status,
            menu::menu_note_open,
            dataview::dataview_pages,
            dataview::dataview_read,
            ask::ask_clis,
            ask::ask_send,
            ask::ask_cancel,
            ask::ask_outcome,
            ask::ask_log,
            ask::chat_title,
            ask::ask_next,
            ask::chats_list,
            ask::chat_read,
            ask::chat_save,
            ask::chat_rename,
            ask::chat_trash,
            ask::skills_list,
            reviews::reviews_status,
            reviews::review_run_now,
            reviews::review_undo,
            reviews::review_latest,
            gtd::projects_list,
            gtd::inbox_list,
            gtd::inbox_capture_done,
            gtd::task_set_contexts,
            gtd::task_set_effort,
            gtd::task_set_project,
            gtd::project_create,
            gtd::project_add_task,
            gtd::task_add,
            gtd::project_set,
            gtd::projects_set_area,
            gtd::inbox_remove_thought,
            gtd::inbox_move_task,
            weekly::clarify_suggest,
            weekly::weekly_context,
            weekly::weekly_state_read,
            weekly::weekly_state_write,
            weekly::weekly_finish,
            weekly::weekly_review_note,
            find::find_status,
            find::find_run,
            find::find_stop,
            find::find_decide,
            weekprep::weekprep_status,
            weekprep::weekprep_run,
            weekprep::weekprep_job,
            weekprep::weekprep_stop,
            weekprep::weekprep_ensure,
            weekprep::weekprep_link,
            app_info,
            platform_features,
            bridge::mcp_reply,
            bridge::mcp_ready,
            tray::tray_set_state,
            tray::main_show,
            tray::tray_quit,
            speech::tts_voices,
            speech::tts_speak,
            speech::tts_pause,
            speech::tts_stop,
            speech::media_state,
            speech::keep_awake,
            scene
        ])
        .setup(move |app| {
            // `Brainstead --quit` (the Windows install script) with no copy running to hand it to
            // (`tray::second_launch`): there is nothing to quit, so this copy doesn't start either.
            if std::env::args().skip(1).any(|a| a == "--quit") {
                std::process::exit(0);
            }
            // Runs an earlier session left waiting: nothing will take them now.
            ingest::settle_stale();
            // The capture extensions' host, registered with each browser (not by a screenshot demo,
            // and not by a dev build: that would point the browsers at target/debug).
            if shares_instance_lock() && !cfg!(debug_assertions) {
                std::thread::spawn(nativehost::register);
            }
            nativehost::watch(app.handle().clone());
            // Teams and Outlook sync's timer (D-20261025-01): off unless the user turns it on, and a
            // no-op without a sign-in. Not in a screenshot demo.
            if scene().is_none() {
                sync::watch(app.handle().clone());
            }
            // The menu-bar icon and its menu window (stage 10). A scene with "tray": true shows the
            // menu window alone, somewhere on screen, for its screenshot.
            tray::setup_window(app.handle());
            let tray_scene = scene().and_then(|sc| serde_json::from_str::<serde_json::Value>(&sc).ok()).is_some_and(|v| v["tray"] == true);
            if scene().is_none() {
                tray::apply(app.handle());
            } else if tray_scene {
                if let Some(t) = app.get_webview_window("tray") {
                    let _ = t.set_position(tauri::LogicalPosition::new(200.0, 120.0));
                    platform::make_unseen(&t);
                    let _ = t.show();
                }
            }
            // Actions from the MCP server (stage 9); not in a screenshot demo.
            if scene().is_none() {
                bridge::start(app.handle().clone());
            }
            // tools/screenshots.py: the scene's snapshot to BRAINSTEAD_SNAPSHOT, once it has settled.
            if let (Some(_), Some(out)) = (scene(), std::env::var_os("BRAINSTEAD_SNAPSHOT")) {
                let label = if tray_scene { "tray" } else { "main" };
                let after = std::env::var("BRAINSTEAD_SNAPSHOT_AFTER").ok().and_then(|s| s.parse().ok()).unwrap_or(5.0);
                let app = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs_f64(after));
                    if let Some(w) = app.get_webview_window(label) {
                        let _ = platform::snapshot(&w, std::path::Path::new(&out));
                    }
                });
            }
            // A bad settings file: say what was put back, once the app is up.
            if let Some(w) = settings_warning {
                use tauri_plugin_notification::NotificationExt;
                let _ = app.notification().builder().title("Brainstead's settings").body(w).show();
            }
            if let Err(e) = menu::install(app.handle()) {
                crate::applog!("menu: {e}");
            }
            platform::spell_warm(
                lock(&app.state::<AppState>().settings).ui.get("spellLanguage").and_then(|v| v.as_str()).map(String::from),
            );
            let s = lock(&app.state::<AppState>().settings).clone();
            if let Some(w) = app.get_webview_window("main") {
                // Screenshots are taken at one size, whatever size the window was left at (a scene's
                // "window": [w, h] for another, a large display's). They're taken unseen and out of
                // the Dock, without taking the focus.
                if let Some(sc) = scene() {
                    let size = serde_json::from_str::<serde_json::Value>(&sc)
                        .ok()
                        .and_then(|v| Some((v["window"][0].as_f64()?, v["window"][1].as_f64()?)))
                        .unwrap_or((1440.0, 900.0));
                    let _ = w.set_size(tauri::LogicalSize::new(size.0, size.1));
                    let _ = w.center();
                    platform::make_unseen(&w);
                    platform::set_in_dock(app.handle(), false);
                }
                let dark = match s.theme.as_str() {
                    "dark" => true,
                    "light" => false,
                    _ => matches!(w.theme(), Ok(tauri::Theme::Dark)),
                };
                // The menu window's scene shows it alone; opened at login with "Only in the menu bar
                // when the window is closed" on, Brainstead starts in the menu bar.
                let ui = |k: &str| s.ui.get(k).and_then(serde_json::Value::as_bool);
                let menu_bar_only = ui("menuBar").unwrap_or(true) && ui("menuBarOnly").unwrap_or(false);
                if menu_bar_only && scene().is_none() && platform::launched_at_login() {
                    platform::set_in_dock(app.handle(), false);
                } else if !tray_scene {
                    platform::show_painted(&w, ground(dark), scene().is_none());
                }
                menu::set_about(app.handle(), dark);
                // Closing the window hides it: the app keeps running for the capture shortcut,
                // and clicking the Dock or tray icon brings the window back. ⌘Q quits. On Windows
                // with no tray icon (`tray::close_quits`) closing quits, through the window's own
                // quit so it asks about unsaved edits first; the "unasked" payload tells src/Quit.tsx
                // not to ask "Quit Brainstead?" as well, since closing is how a Windows app quits.
                let w2 = w.clone();
                w.on_window_event(move |ev| match ev {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let app = w2.app_handle();
                        let _ = app.save_window_state(STATE_FLAGS);
                        if tray::close_quits(app) {
                            let _ = tauri::Emitter::emit_to(app, "main", "quit-requested", "unasked");
                        } else {
                            let _ = w2.hide();
                            tray::main_closed(app);
                        }
                    }
                    // An update a background check found is offered once the window has the focus.
                    tauri::WindowEvent::Focused(true) => updates::focused(w2.app_handle()),
                    // The system went light or dark: About follows when the app's theme is System.
                    tauri::WindowEvent::ThemeChanged(t) => {
                        let app = w2.app_handle();
                        if lock(&app.state::<AppState>().settings).theme == "system" {
                            menu::set_about(app, matches!(t, tauri::Theme::Dark));
                        }
                    }
                    _ => {}
                });
            }
            match scene() {
                None => capture::register(app.handle(), &s.capture_shortcut),
                // Screenshot mode: a scene with "capture": true opens the capture window as the shortcut would.
                Some(sc) if sc.contains("\"capture\":true") => capture::show(app.handle()),
                Some(_) => {}
            }
            // Fix name's remembered corrections moved from the vault's scripts/ to here (2026-10-03).
            if let (Some(v), None) = (&s.vault_path, scene()) {
                migrate_corrections(v);
            }
            app.state::<VaultService>().start(app.handle(), s.vault_path, s.excluded, false);
            changes::start(app.handle());
            knowledge::watch_health(app.handle());
            if scene().is_none() {
                reviews::start_scheduler(app.handle());
                daily_check::start_scheduler(app.handle());
                weekprep::start_scheduler(app.handle());
                updates::start(app.handle(), busy);
            }
            Ok(())
        })
        .build(context)
        .expect("error while building tauri application")
        .run(|app, ev| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = &ev {
                // ⌘Q: the window first asks about unsaved edits (Save, Discard, Stay), then calls
                // app_quit. A quit the app asks for itself (code set) goes ahead.
                if code.is_none() && !QUIT_OK.load(std::sync::atomic::Ordering::SeqCst) && scene().is_none() {
                    api.prevent_exit();
                    let _ = tauri::Emitter::emit(app, "quit-requested", ());
                    return;
                }
                before_quit(app);
            }
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = ev {
                if !tray::reopen_from_icon() {
                    tray::show_main(app);
                }
            }
        });
}
