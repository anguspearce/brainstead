// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Knowledge health (stage 7a): runs the lint in the background whenever the index changes, keeps
//! a day-by-day count for the trend, and applies its safe fixes (cross-links, dates, log lines, system-note headers)
//! straight away, keeping the pages' modification times. Its other fixes are agent changes
//! (src/changes.rs).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use brainstead_core::lint::{self, Report};
use brainstead_core::proposals::{self, Kind, Origin};
use brainstead_core::reviews;
use brainstead_core::write::{self, FileChange};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::edits::{writable, written, EditError, FileUndo, Res, UndoEntry, UndoStack};
use crate::platform;
use crate::vault::VaultService;
use crate::AppState;

const LOG: &str = "log.md";

fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

fn invalid(m: impl Into<String>) -> EditError {
    write::WriteError::Invalid(m.into()).into()
}

fn root(app: &AppHandle) -> Res<PathBuf> {
    Ok(app.state::<VaultService>().root().ok_or("No vault is open.".to_string())?)
}

fn read_opt(p: &Path) -> Option<String> {
    std::fs::read(p).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}

// ── Knowledge health ──────────────────────────────────────────────────────────

/// What's kept in `health.json`: duplicate pairs marked as not duplicates, issues ignored, and
/// each day's counts.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Kept {
    dismissed: Vec<String>,
    history: BTreeMap<String, Day>,
    /// Each system note's callout as last seen intact, to put back if it goes.
    callouts: BTreeMap<String, String>,
    /// Issues ignored (`lint::ignore_key`), with their page's version then: back when it changes.
    ignored: BTreeMap<String, String>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Day {
    total: usize,
    decisions: usize,
}

fn kept_path() -> PathBuf {
    platform::data_dir().join("health.json")
}

fn kept() -> Kept {
    std::fs::read(kept_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_kept(k: &Kept) {
    if let Ok(j) = serde_json::to_vec_pretty(k) {
        let _ = write::write_atomic(&kept_path(), &j, false);
    }
}

/// The last report, and the index's state it was made from.
#[derive(Default)]
pub struct Health {
    report: Mutex<Option<Report>>,
    seen: Mutex<i64>,
    /// Held while the checks run, so two runs never overlap.
    running: Mutex<()>,
    /// When the index last changed (`vault_changed`), until the checks have run since.
    changed_at: Mutex<Option<std::time::Instant>>,
}

/// The checks rerun once the vault has been quiet this long after a change.
const QUIET: Duration = Duration::from_millis(400);

/// The index changed (vault.rs): the checks rerun once it's quiet for `QUIET`.
pub fn vault_changed(app: &AppHandle) {
    *app.state::<Health>().changed_at.lock().unwrap() = Some(std::time::Instant::now());
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthView {
    report: Option<Report>,
    /// Day by day, oldest first, the last 8 weeks.
    history: Vec<(String, Day)>,
    ran_at: Option<String>,
}

pub(crate) fn run_lint(app: &AppHandle) -> Option<Report> {
    let h = app.state::<Health>();
    let _one = h.running.lock().unwrap();
    let root = app.state::<VaultService>().root()?;
    let k = kept();
    let dismissed: HashSet<String> = k.dismissed.iter().cloned().collect();
    let mut r = lint::run(&root, today(), &dismissed);
    // Claims with no citation, from the contradiction check's cached claims (none until it runs).
    r.checks.push(crate::contradict::uncited_check(&root));
    let mut k = kept();
    r.checks.push(callout_check(&root, &mut k.callouts));
    lint::without_ignored(&mut r, &mut k.ignored, &|p| page_version(&root, p), &|p| read_opt(&root.join(p)));
    k.history.insert(today().to_string(), Day { total: r.total(), decisions: r.needs_decision() });
    let cutoff = (today() - chrono::Duration::days(56)).to_string();
    k.history.retain(|d, _| *d >= cutoff);
    save_kept(&k);
    *app.state::<Health>().report.lock().unwrap() = Some(r.clone());
    let _ = app.emit("health-changed", Counts { decisions: r.needs_decision(), total: r.total() });
    Some(r)
}

/// System notes (the To Do list, the summaries' notes and the rest, by name) whose system-note
/// callout has gone or no longer reads as one: other parts of Brainstead look for it. Each callout
/// seen intact is remembered (`known`), so the fix can put it back.
fn callout_check(root: &Path, known: &mut BTreeMap<String, String>) -> lint::Check {
    let mut items = Vec::new();
    let names = std::fs::read_dir(root).into_iter().flatten().flatten().filter_map(|e| e.file_name().into_string().ok());
    for name in names.filter(|n| {
        n.ends_with(".md") && brainstead_core::rename::is_system(n) && !brainstead_core::vault::SCHEMA_FILES.contains(&n.as_str())
    }) {
        let Some(text) = read_opt(&root.join(&name)) else { continue };
        match brainstead_core::rename::system_callout(&text) {
            Some((a, b)) => {
                known.insert(name, text[a..b].to_string());
            }
            None if known.contains_key(&name) => items.push(lint::Item {
                text: format!("{}: its system-note header is gone", brainstead_core::filename::stem(&name)),
                page: Some(name.clone()),
                detail: Some("Put it back as it was".into()),
                safe: true,
                ..Default::default()
            }),
            None => {}
        }
    }
    lint::Check { id: "system-callouts", title: "System notes missing their header", classic: false, ignored: 0, items }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Counts {
    decisions: usize,
    total: usize,
}

/// Reruns the checks once the vault is quiet after a change (`vault_changed`), so an edit made
/// anywhere (Changes, Ask, another editor) shows within a second; and at start, and whenever the
/// index's stamp moves without a change being heard, looked at every 5 seconds.
pub fn watch_health(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut looked = std::time::Instant::now() - Duration::from_secs(5);
        loop {
            let h = app.state::<Health>();
            let heard = h.changed_at.lock().unwrap().is_some_and(|t| t.elapsed() >= QUIET);
            if heard || looked.elapsed() >= Duration::from_secs(5) {
                looked = std::time::Instant::now();
                let stamp = app.state::<VaultService>().with_index(|ix, _| Ok(ix.stats().map(|s| s.updated_at).unwrap_or(0))).unwrap_or(0);
                let moved = stamp != 0 && *h.seen.lock().unwrap() != stamp;
                if heard || moved {
                    *h.changed_at.lock().unwrap() = None;
                    *h.seen.lock().unwrap() = stamp;
                    run_lint(&app);
                }
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    });
}

#[tauri::command]
pub async fn health_report(app: AppHandle, fresh: bool) -> Result<HealthView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cached = app.state::<Health>().report.lock().unwrap().clone();
        let report = if fresh || cached.is_none() { run_lint(&app) } else { cached };
        let history = kept().history.into_iter().collect();
        Ok(HealthView { report, history, ran_at: Some(proposals::now_local()) })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A safe fix the window asks for: a check's item, by its page and name.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    check: String,
    page: String,
    name: Option<String>,
    detail: Option<String>,
}

/// Applies safe fixes: a link at a name's first plain mention, `updated:` set to the file's date,
/// and a `log.md` line for the fixes (which also logs any unlogged writes). Pages keep their
/// modification times. One undo for all of them.
fn health_fix_now(app: AppHandle, fixes: Vec<Fix>) -> Res<String> {
    writable(&app.state::<AppState>())?;
    let root = root(&app)?;
    let mut by_page: BTreeMap<String, Vec<&Fix>> = BTreeMap::new();
    let mut unlogged = 0;
    for f in &fixes {
        match f.check.as_str() {
            "missing-crossrefs" | "stale-dates" | "system-callouts" => by_page.entry(f.page.clone()).or_default().push(f),
            "unlogged-writes" => unlogged += 1,
            other => return Err(invalid(format!("{other} has no safe fix."))),
        }
    }
    let mut changes: Vec<FileChange> = Vec::new();
    let mut what: Vec<String> = Vec::new();
    for (page, fs) in &by_page {
        brainstead_core::trash::safe_rel(page).map_err(EditError::from)?;
        let abs = root.join(page);
        let Some(before) = read_opt(&abs) else { continue };
        let mut text = before.clone();
        for f in fs {
            let next = match (f.check.as_str(), f.name.as_deref(), f.detail.as_deref()) {
                ("missing-crossrefs", Some(n), _) => lint::link_first_mention(&text, n).inspect(|_| what.push(format!("linked {n}"))),
                ("stale-dates", _, Some(d)) => lint::fix_updated(&text, d).inspect(|_| what.push("updated date".into())),
                ("system-callouts", _, _) => kept()
                    .callouts
                    .get(page)
                    .filter(|_| brainstead_core::rename::system_callout(&text).is_none())
                    .map(|c| brainstead_core::rename::with_callout(&text, c))
                    .inspect(|_| what.push("system-note header put back".into())),
                _ => None,
            };
            if let Some(t) = next {
                text = t;
            }
        }
        if text != before {
            let v = write::save_file_as(&abs, &text, &write::version(before.as_bytes()), true)?;
            changes.push(FileChange { path: abs, before: Some(before), version: v });
        }
    }
    if changes.is_empty() && unlogged == 0 {
        return Err(invalid("Nothing needed fixing any more."));
    }
    let pages = changes.len();
    if unlogged > 0 {
        what.push(format!("logged {unlogged} wiki write{}", if unlogged == 1 { "" } else { "s" }));
    }
    let summary = what.join(", ");
    let log = root.join(LOG);
    let log_existed = log.exists();
    let log_before = read_opt(&log).unwrap_or_default();
    let entry = reviews::log_entry("lint-fix", "Knowledge health", Some(&summary), chrono::Local::now().naive_local());
    let log_after = reviews::log_insert(&log_before, &entry);
    let v = if log_existed {
        write::save_file(&log, &log_after, &write::version(log_before.as_bytes()))?
    } else {
        write::create_file(&log, &log_after)?
    };
    changes.push(FileChange { path: log.clone(), before: log_existed.then_some(log_before), version: v });
    let label = if pages == 0 {
        "Logged the wiki's writes".to_string()
    } else {
        format!("Fixed {pages} page{}", if pages == 1 { "" } else { "s" })
    };
    push_undo(&app, &root, label.clone(), &changes);
    written(&app, &changes.iter().map(|c| c.path.clone()).collect::<Vec<_>>());
    settled(&app, |check, i| {
        fixes.iter().any(|f| f.check == check && i.page.as_deref() == Some(f.page.as_str()) && i.name == f.name && i.detail == f.detail)
    });
    Ok(label)
}

/// What Reshape pages did.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reshaped {
    /// The run's group in Changes, to revert it all.
    run: String,
    applied: usize,
    /// Pages it couldn't change, and why.
    failed: Vec<String>,
    /// Pages left for the user: they need a decision first.
    left: usize,
}

/// Reshape pages (D-20261006-06): every page not in the shape that can be reshaped by itself,
/// or the pages named (by path or name, reshaped as proposed even when they'd need the user), each
/// one change in Changes in one run, revertable alone or together. One `log.md` line for the run.
fn health_reshape_now(app: AppHandle, pages: Option<Vec<String>>, unattended: bool) -> Res<Reshaped> {
    use brainstead_core::pageshape;
    writable(&app.state::<AppState>())?;
    let root = root(&app)?;
    let named = |path: &str| {
        pages.as_ref().map(|ps| {
            ps.iter().any(|p| {
                let p = p.trim().trim_start_matches("[[").trim_end_matches("]]");
                p == path || brainstead_core::filename::stem(path).eq_ignore_ascii_case(p)
            })
        })
    };
    let run = format!("{}{}", crate::changes::RESHAPE_RUN, proposals::new_id());
    let origin = Origin {
        kind: "lint".into(),
        label: Some("Reshape pages".into()),
        run: Some(run.clone()),
        trigger: unattended.then(|| "scheduled".into()),
        ..Default::default()
    };
    let mut done = Reshaped { run, applied: 0, failed: vec![], left: 0 };
    let mut found = 0;
    for (r, new) in pageshape::survey(&root) {
        let want = named(&r.path);
        found += usize::from(want == Some(true));
        let (Some(new), true) = (new, want.unwrap_or(r.auto) && r.report.broken.is_empty()) else {
            done.left += usize::from(want.is_none() && !r.in_shape);
            continue;
        };
        let Some(before) = read_opt(&root.join(&r.path)) else { continue };
        let rp = &r.report;
        let mut reason = format!(
            "Put in the page shape: {} Timeline entries, {} headings rewritten, {} Source lines added.",
            rp.entries,
            rp.headings.len(),
            rp.added.iter().filter(|a| a.starts_with("Source:")).count()
        );
        if !rp.reasons.is_empty() {
            reason.push_str(&format!(" Reshaped as proposed although {}.", rp.reasons.join("; ")));
        }
        let s = crate::changes::Submit::new(
            &r.path,
            Kind::Edit,
            "Reshaped to the page shape",
            &reason,
            origin.clone(),
            brainstead_core::changes::from_texts(Some(&before), &new),
        );
        match crate::changes::submit(&app, s) {
            Ok(o) if o.applied => done.applied += 1,
            Ok(o) => done.failed.push(format!("{}: {}", r.path, o.message)),
            Err(e) => done.failed.push(format!("{}: {}", r.path, e.message())),
        }
    }
    if let Some(ps) = &pages {
        if found < ps.len() {
            return Err(invalid(format!("Only {found} of the {} pages named are wiki entity or concept pages.", ps.len())));
        }
    }
    if done.applied > 0 {
        let n = done.applied;
        let detail = format!("reshaped {n} page{}", if n == 1 { "" } else { "s" });
        let entry = reviews::log_entry("lint-fix", "Knowledge health", Some(&detail), chrono::Local::now().naive_local());
        if let Err(e) = crate::edits::add_log(&root, &entry) {
            crate::applog!("reshape log line: {e}");
        }
        written(&app, &[root.join(LOG)]);
    }
    // Every page it was asked for reshaped: they leave the list at once.
    if done.failed.is_empty() && done.left == 0 {
        settled(&app, |check, i| check == "page-shape" && i.page.as_deref().is_none_or(|p| named(p).unwrap_or(true)));
    } else {
        relint(&app);
    }
    Ok(done)
}

/// A page's version, as an ignored issue remembers it.
fn page_version(root: &Path, page: &str) -> Option<String> {
    std::fs::read(root.join(page)).ok().map(|b| write::version(&b))
}

/// Ignores an issue (its check and its line, as the report gives them) until its page changes,
/// or for an uncited claim until its line does.
fn health_ignore_now(app: AppHandle, check: String, text: String) -> Res<()> {
    let root = root(&app)?;
    let report = app.state::<Health>().report.lock().unwrap().clone();
    let item = report
        .as_ref()
        .and_then(|r| r.check(&check))
        .and_then(|c| c.items.iter().find(|i| i.text == text))
        .cloned()
        .ok_or_else(|| invalid("That issue isn't in Knowledge health any more."))?;
    let version = item.page.as_deref().and_then(|p| page_version(&root, p)).unwrap_or_default();
    let mut k = kept();
    k.ignored.insert(lint::ignore_key(&check, &text), version);
    save_kept(&k);
    // Taken out of the last report at once: running every check again takes seconds.
    let h = app.state::<Health>();
    let counts = {
        let mut report = h.report.lock().unwrap();
        report.as_mut().map(|r| {
            if let Some(c) = r.checks.iter_mut().find(|c| c.id == check) {
                let n = c.items.len();
                c.items.retain(|i| i.text != text);
                c.ignored += n - c.items.len();
            }
            Counts { decisions: r.needs_decision(), total: r.total() }
        })
    };
    match counts {
        Some(c) => {
            let _ = app.emit("health-changed", c);
        }
        None => relint(&app),
    }
    Ok(())
}

/// Knowledge health's Ignore all, on the checks that offer it (`IGNORE_ALL`): every issue it lists
/// ignored, each until its page (or a claim's line) changes, as Ignore does one. How many.
fn health_ignore_all_now(app: AppHandle, check: String) -> Res<usize> {
    if !IGNORE_ALL.contains(&check.as_str()) {
        return Err(invalid("Ignore all is for stale pages and claims with no citation."));
    }
    let root = root(&app)?;
    let items =
        app.state::<Health>().report.lock().unwrap().as_ref().and_then(|r| r.check(&check)).map(|c| c.items.clone()).unwrap_or_default();
    let mut k = kept();
    for i in &items {
        let version = i.page.as_deref().and_then(|p| page_version(&root, p)).unwrap_or_default();
        k.ignored.insert(lint::ignore_key(&check, &i.text), version);
    }
    save_kept(&k);
    let h = app.state::<Health>();
    let counts = {
        let mut report = h.report.lock().unwrap();
        report.as_mut().map(|r| {
            if let Some(c) = r.checks.iter_mut().find(|c| c.id == check) {
                c.ignored += c.items.len();
                c.items.clear();
            }
            Counts { decisions: r.needs_decision(), total: r.total() }
        })
    };
    if let Some(c) = counts {
        let _ = app.emit("health-changed", c);
    }
    Ok(items.len())
}

/// The checks with Ignore all: advisory, with no fix of their own.
pub const IGNORE_ALL: [&str; 2] = ["stale-pages", "uncited-claims"];

/// Shows a check's ignored issues again (every check's when None).
fn health_unignore_now(app: AppHandle, check: Option<String>) -> Res<usize> {
    let mut k = kept();
    let before = k.ignored.len();
    match &check {
        Some(c) => k.ignored.retain(|key, _| !key.starts_with(&format!("{c}|"))),
        None => k.ignored.clear(),
    }
    let n = before - k.ignored.len();
    save_kept(&k);
    relint(&app);
    Ok(n)
}

/// Marks two pages as not duplicates.
fn health_dismiss_now(app: AppHandle, a: String, b: String) -> Res<()> {
    let mut k = kept();
    let key = lint::pair_key(&a, &b);
    if !k.dismissed.contains(&key) {
        k.dismissed.push(key.clone());
    }
    save_kept(&k);
    settled(&app, |check, i| {
        check == "duplicates" && i.page.as_deref().zip(i.pages.first()).is_some_and(|(x, y)| lint::pair_key(x, y) == key)
    });
    Ok(())
}

/// Moves an image nothing uses to the Trash (asked first in the window).
fn health_trash_image_now(app: AppHandle, path: String) -> Res<String> {
    writable(&app.state::<AppState>())?;
    if !path.starts_with("images/") {
        return Err(invalid("Only images go to the Trash from here."));
    }
    let root = root(&app)?;
    let e = brainstead_core::trash::move_to_trash(&root, &path, "image")?;
    written(&app, &[root.join(&path)]);
    settled(&app, |check, i| check == "unreferenced-images" && i.page.as_deref() == Some(path.as_str()));
    Ok(e.id)
}

/// A missing page, made: `wiki/<folder>/<Name>.md` from `Templates/Wiki page.md` when there is
/// one, else the ingest skill's starting page. Returns its path.
fn health_create_page_now(app: AppHandle, name: String, folder: String) -> Res<String> {
    writable(&app.state::<AppState>())?;
    let name = name.trim().to_string();
    if !["entities", "concepts"].contains(&folder.as_str()) {
        return Err(invalid("A new page goes in entities or concepts."));
    }
    let rel = format!("wiki/{folder}/{name}.md");
    proposals::valid_new_page(&rel).map_err(invalid)?;
    let root = root(&app)?;
    let kind = if folder == "entities" { "entity" } else { "concept" };
    let text = match read_opt(&root.join("Templates/Wiki page.md")) {
        Some(t) => t.replace("{{title}}", &name),
        None => format!("---\ntype: {kind}\nname: {name}\ndescription: \nsources: []\n---\n\n## Current state\n\n"),
    };
    let abs = root.join(&rel);
    let v = write::create_file(&abs, &text)?;
    push_undo(&app, &root, format!("Created {name}"), &[FileChange { path: abs.clone(), before: None, version: v }]);
    written(&app, &[abs]);
    settled(&app, |check, i| check == "missing-pages" && i.name.as_deref() == Some(name.as_str()));
    Ok(rel)
}

fn push_undo(app: &AppHandle, root: &Path, label: String, changes: &[FileChange]) {
    let files = changes
        .iter()
        .map(|c| FileUndo {
            path: brainstead_core::vault::rel_of(root, &c.path).unwrap_or_default(),
            before: c.before.clone(),
            version: c.version.clone(),
        })
        .collect();
    app.state::<UndoStack>().push(UndoEntry::files(label, files));
}

/// Points a missing page's links at a page that exists: one change per page linking it,
/// `[[Ghost]]` becoming `[[Page|Ghost]]`, applied at once.
fn health_link_ghost_now(app: AppHandle, target: String, to: String, pages: Vec<String>, unattended: bool) -> Res<usize> {
    let root = root(&app)?;
    let to_name = brainstead_core::filename::stem(&to).to_string();
    let mut n = 0;
    for page in pages {
        brainstead_core::trash::safe_rel(&page).map_err(EditError::from)?;
        let Some(before) = read_opt(&root.join(&page)) else { continue };
        let after = relink(&before, &target, &to_name);
        if after == before {
            continue;
        }
        let origin = Origin {
            kind: "lint".into(),
            label: Some("Missing page".into()),
            trigger: unattended.then(|| "scheduled".into()),
            ..Default::default()
        };
        let title = format!("Link [[{target}]] to {to_name}");
        let s = crate::changes::Submit::new(
            &page,
            Kind::Edit,
            &title,
            "A missing page from Knowledge health.",
            origin,
            brainstead_core::changes::from_texts(Some(&before), &after),
        );
        crate::changes::submit(&app, s)?;
        n += 1;
    }
    // Claims made from a file of the missing name (renamed since, their source left behind) name
    // the page it's pointed at now, so a later ingest doesn't link the old name again.
    if let Err(e) = brainstead_core::claims::rename_source(&root, &format!("{target}.md"), &to) {
        crate::applog!("claims for {target}: {e}");
    }
    // As every fix does, so the missing page leaves the list once the links point elsewhere.
    if n > 0 {
        settled(&app, |check, i| check == "missing-pages" && i.name.as_deref() == Some(target.as_str()));
    }
    Ok(n)
}

/// `[[target]]`, `[[target|x]]` and `[[target#h]]` pointed at `to`, keeping what they show.
fn relink(text: &str, target: &str, to: &str) -> String {
    let re = regex::Regex::new(r"(!?)\[\[([^\]|#\n]+?)((?:#[^\]|\n]*)?)((?:\|[^\]\n]*)?)\]\]").unwrap();
    re.replace_all(text, |c: &regex::Captures| {
        if c[2].trim() != target {
            return c[0].to_string();
        }
        let shown = if c[4].is_empty() { format!("|{}", c[2].trim()) } else { c[4].to_string() };
        format!("{}[[{to}{}{shown}]]", &c[1], &c[3])
    })
    .into_owned()
}

/// Runs a command's work off the main thread, so the window never stalls on the vault.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| EditError::from(e.to_string()))?
}

/// Reruns the checks in the background after a fix; the window hears `health-changed`.
fn relint(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        run_lint(&app);
    });
}

/// After an action: what it settled (`gone`, by check and item) taken out of the last report at
/// once and the window told, as Ignore does, since running every check again takes a second or
/// so; then the checks rerun to be sure.
fn settled(app: &AppHandle, gone: impl Fn(&str, &lint::Item) -> bool) {
    let h = app.state::<Health>();
    let counts = {
        let mut report = h.report.lock().unwrap();
        report.as_mut().map(|r| {
            for c in &mut r.checks {
                let id = c.id;
                c.items.retain(|i| !gone(id, i));
            }
            Counts { decisions: r.needs_decision(), total: r.total() }
        })
    };
    if let Some(c) = counts {
        let _ = app.emit("health-changed", c);
    }
    relint(app);
}

// The window's commands, each running its `_now` on a blocking thread.
#[tauri::command]
pub async fn health_fix(app: AppHandle, fixes: Vec<Fix>) -> Res<String> {
    blocking(move || health_fix_now(app, fixes)).await
}

#[tauri::command]
pub async fn health_reshape(app: AppHandle, pages: Option<Vec<String>>, unattended: Option<bool>) -> Res<Reshaped> {
    blocking(move || health_reshape_now(app, pages, unattended.unwrap_or(false))).await
}

#[tauri::command]
pub async fn health_dismiss(app: AppHandle, a: String, b: String) -> Res<()> {
    blocking(move || health_dismiss_now(app, a, b)).await
}

#[tauri::command]
pub async fn health_ignore(app: AppHandle, check: String, text: String) -> Res<()> {
    blocking(move || health_ignore_now(app, check, text)).await
}

#[tauri::command]
pub async fn health_ignore_all(app: AppHandle, check: String) -> Res<usize> {
    blocking(move || health_ignore_all_now(app, check)).await
}

#[tauri::command]
pub async fn health_unignore(app: AppHandle, check: Option<String>) -> Res<usize> {
    blocking(move || health_unignore_now(app, check)).await
}

#[tauri::command]
pub async fn health_trash_image(app: AppHandle, path: String) -> Res<String> {
    blocking(move || health_trash_image_now(app, path)).await
}

#[tauri::command]
pub async fn health_create_page(app: AppHandle, name: String, folder: String) -> Res<String> {
    blocking(move || health_create_page_now(app, name, folder)).await
}

#[tauri::command]
pub async fn health_link_ghost(app: AppHandle, target: String, to: String, pages: Vec<String>, unattended: Option<bool>) -> Res<usize> {
    blocking(move || health_link_ghost_now(app, target, to, pages, unattended.unwrap_or(false))).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_note_that_lost_its_header() {
        let t = tempfile::tempdir().unwrap();
        let todo = t.path().join("Me. To Do List.md");
        std::fs::write(&todo, "> **This is a system note** - Tasks.\n\n- [ ] a\n").unwrap();
        std::fs::write(t.path().join("Idea. X.md"), "No header\n").unwrap();
        let mut known = BTreeMap::new();
        // Seen intact: remembered, nothing to say.
        assert!(callout_check(t.path(), &mut known).items.is_empty());
        assert_eq!(known["Me. To Do List.md"], "> **This is a system note** - Tasks.");
        // Gone: flagged, with the safe fix.
        std::fs::write(&todo, "- [ ] a\n").unwrap();
        let c = callout_check(t.path(), &mut known);
        assert_eq!((c.items.len(), c.items[0].safe, c.items[0].page.as_deref()), (1, true, Some("Me. To Do List.md")));
    }

    #[test]
    fn relinking_a_ghost() {
        let t = "See [[Steerco Charter]], [[Steerco Charter|the charter]], [[Steerco Charter#Scope]] and [[Other]].";
        assert_eq!(
            relink(t, "Steerco Charter", "Orbit App"),
            "See [[Orbit App|Steerco Charter]], [[Orbit App|the charter]], [[Orbit App#Scope|Steerco Charter]] and [[Other]]."
        );
    }
}
