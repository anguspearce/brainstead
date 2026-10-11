// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Contradictions between wiki pages (stage 7b), run from the report screen or by the daily check:
//! pages not yet read are read into claims by the model (cached by their content), code
//! finds the clashes, the model judges only the ones not judged before, and a contradiction with
//! a known fix is made as an agent change (held in Changes when a check fails).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use brainstead_core::changes::from_texts;
use brainstead_core::contradictions::{self as c, Claim, Clash, State, Verdict};
use brainstead_core::proposals::{self, Origin, Patch, Quote};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::edits::{EditError, Res};
use crate::vault::VaultService;

const BATCH_WORDS: usize = 30_000;
const BATCH_PAGES: usize = 30;
const JUDGE_BATCH: usize = 20;

static RUNNING: AtomicBool = AtomicBool::new(false);
/// Asked to stop: the run ends at its next batch, and the model call going now is cancelled.
static STOP: AtomicBool = AtomicBool::new(false);

/// Stops a run that's going (Stop, on its screen or the daily check's).
pub fn stop(app: &AppHandle) {
    if RUNNING.load(Ordering::SeqCst) {
        STOP.store(true, Ordering::SeqCst);
        app.state::<std::sync::Arc<crate::ask::Running>>().cancel("contradictions");
    }
}

/// At startup: a run the app quit during is marked stopped (it can't still be going). What it
/// read is kept, page by page, so the next run starts where it left off.
pub fn clear_stale() {
    let mut l = load_last();
    if l.running {
        l.running = false;
        l.doing.clear();
        l.error = Some("Stopped when Brainstead closed; the next check carries on from there.".into());
        if let Ok(j) = serde_json::to_vec_pretty(&l) {
            let _ = brainstead_core::write::write_atomic(&last_path(), &j, false);
        }
    }
}

/// Whether a contradiction check is going.
pub fn going() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

const STOPPED: &str = "Stopped.";

/// What a run is doing now ("Reading pages: batch 3 of 17"), "" when none is.
pub fn doing() -> String {
    let l = load_last();
    if l.running {
        l.doing
    } else {
        String::new()
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| EditError::from(e.to_string()))?
}

fn state() -> State {
    State::new(&crate::platform::data_dir())
}

/// The last run, for the report's figures.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Last {
    pub started: String,
    pub finished: Option<String>,
    pub running: bool,
    /// What it's doing now.
    pub doing: String,
    pub pages: usize,
    pub extracted: usize,
    pub claims: usize,
    pub clashes: usize,
    pub judged: usize,
    pub contradictions: usize,
    pub proposed: usize,
    /// Fixes that couldn't be made or held (the vault read-only, the page changed), and why.
    pub failed: usize,
    pub failures: Vec<String>,
    pub error: Option<String>,
}

fn last_path() -> std::path::PathBuf {
    state().dir.join("last.json")
}

fn load_last() -> Last {
    std::fs::read(last_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_last(app: &AppHandle, l: &Last) {
    let _ = std::fs::create_dir_all(state().dir);
    if let Ok(j) = serde_json::to_vec_pretty(l) {
        let _ = brainstead_core::write::write_atomic(&last_path(), &j, false);
    }
    let _ = app.emit("contradictions-changed", l);
}

fn models(app: &AppHandle) -> (String, String) {
    let m = crate::ask::job_model(app, "contradictions");
    // Reading pages is a cheap job: Claude's smallest model when Claude is the assistant.
    let extract = if m.starts_with("claude") { "claude:haiku".to_string() } else { m.clone() };
    (extract, m)
}

/// Starts a run in the background; false when one is going already.
pub fn start(app: &AppHandle, trigger: Option<&'static str>) -> bool {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return false;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        run_held(&app, trigger, None);
    });
    true
}

/// Runs a check here and now (the daily check's way, `scheduled` when it was, its changes in the
/// job's `group`); None when one is going already.
pub fn run_now(app: &AppHandle, trigger: Option<&str>, group: Option<&str>) -> Option<Last> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return None;
    }
    Some(run_held(app, trigger, group))
}

/// A run, with RUNNING already taken.
fn run_held(app: &AppHandle, trigger: Option<&str>, group: Option<&str>) -> Last {
    STOP.store(false, Ordering::SeqCst);
    let mut l = Last { started: proposals::now_local(), running: true, ..Default::default() };
    save_last(app, &l);
    let group = group.map(String::from).unwrap_or_else(|| format!("contradictions-{}", proposals::new_id()));
    if let Err(e) = run(app, &mut l, trigger, &group) {
        l.error = Some(e);
    }
    l.running = false;
    l.doing.clear();
    l.finished = Some(proposals::now_local());
    save_last(app, &l);
    RUNNING.store(false, Ordering::SeqCst);
    l
}

fn run(app: &AppHandle, l: &mut Last, trigger: Option<&str>, group: &str) -> Result<(), String> {
    let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
    let st = state();
    let pages = c::pages(&root);
    let texts: HashMap<String, String> = pages.iter().map(|(r, t, _)| (r.clone(), t.clone())).collect();
    l.pages = pages.len();
    let (extract_model, judge_model) = models(app);

    // 1. Read the pages that changed.
    let (_, missing) = c::current(&st, &pages);
    let batches = c::batches(&missing, &texts, BATCH_WORDS, BATCH_PAGES);
    for (i, b) in batches.iter().enumerate() {
        if STOP.load(Ordering::SeqCst) {
            return Err(STOPPED.into());
        }
        l.doing = format!("Reading pages: batch {} of {}", i + 1, batches.len());
        save_last(app, l);
        let prompt = c::extract_prompt(b, &texts, &pages);
        let out = crate::ask::run(
            app,
            "contradictions",
            &prompt,
            &extract_model,
            None,
            &brainstead_core::ask::system_prompt(&crate::ask::today(), false),
            None,
            &mut |_| {},
        );
        if let Some(e) = out.error {
            return Err(if STOP.load(Ordering::SeqCst) { STOPPED.into() } else { e });
        }
        // An answer with no JSON in it (a refusal, garbled output) caches nothing: the pages are
        // read again next time rather than remembered as having no claims.
        let Some(raw_claims) = c::parse_answer_lines::<Claim>(&out.text) else {
            crate::applog!("contradictions: batch {} of {} gave no JSON; not cached", i + 1, batches.len());
            continue;
        };
        let mut by_page: HashMap<String, Vec<Claim>> = b.iter().map(|p| (p.clone(), vec![])).collect();
        for raw in raw_claims {
            let Some(text) = texts.get(&raw.page) else { continue };
            if !by_page.contains_key(&raw.page) {
                continue;
            }
            if let Ok(ok) = c::validate(&raw, text) {
                by_page.get_mut(&raw.page).unwrap().push(ok);
            }
        }
        for (p, claims) in by_page {
            st.cache(&p, &texts[&p], claims)?;
            l.extracted += 1;
        }
    }

    // 2. The clashes.
    l.doing = "Finding clashes".into();
    save_last(app, l);
    let (claims, _) = c::current(&st, &pages);
    let claims = c::with_kept(claims, &brainstead_core::claims::all(&root), &pages);
    l.claims = claims.len();
    let idx_pages: Vec<(String, Vec<String>)> = pages.iter().map(|(r, _, a)| (r.clone(), a.clone())).collect();
    let clashes = c::find_clashes(&claims, &c::name_index(&idx_pages), c::WINDOW_DAYS);
    l.clashes = clashes.len();
    let _ = std::fs::write(st.dir.join("clashes.json"), serde_json::to_vec_pretty(&clashes).unwrap_or_default());
    let known: HashMap<String, Clash> = clashes.iter().map(|x| (x.id.clone(), x.clone())).collect();

    // 3. Judge the new ones.
    let verdicts = st.verdicts();
    let todo: Vec<Clash> = clashes.iter().filter(|x| !verdicts.contains_key(&x.id)).cloned().collect();
    let today = chrono::Local::now().date_naive().to_string();
    let mut fresh: Vec<Verdict> = Vec::new();
    for (i, chunk) in todo.chunks(JUDGE_BATCH).enumerate() {
        if STOP.load(Ordering::SeqCst) {
            return Err(STOPPED.into());
        }
        l.doing = format!("Judging: batch {} of {}", i + 1, todo.len().div_ceil(JUDGE_BATCH));
        save_last(app, l);
        let out = crate::ask::run(
            app,
            "contradictions",
            &c::judge_prompt(chunk),
            &judge_model,
            None,
            &brainstead_core::ask::system_prompt(&crate::ask::today(), false),
            None,
            &mut |_| {},
        );
        if let Some(e) = out.error {
            return Err(if STOP.load(Ordering::SeqCst) { STOPPED.into() } else { e });
        }
        for v in c::parse_lines::<Verdict>(&out.text) {
            if let Ok(v) = c::check_verdict(v, &known, &today) {
                st.add_verdict(&v)?;
                l.judged += 1;
                fresh.push(v);
            }
        }
    }
    let all = st.verdicts();
    l.contradictions = clashes.iter().filter(|x| all.get(&x.id).is_some_and(|v| v.verdict == "contradiction")).count();

    // 4. Proposals for the new contradictions with a fix, and for history a page hasn't caught up
    // with (an evolution whose older value a page still gives as current).
    for v in fresh.iter().filter(|v| v.verdict == "contradiction" || v.verdict == "evolution") {
        let (Some(p), Some(clash)) = (&v.patch, known.get(&v.id)) else { continue };
        let Some(before) = texts.get(&p.page) else { continue };
        let Ok(after) = proposals::patched(
            before,
            &Patch::Replace { edits: vec![proposals::Edit { find: p.find.clone(), replace: p.replace.clone() }] },
        ) else {
            continue;
        };
        let after = proposals::bump_updated(before, &after, &today);
        let quotes: Vec<Quote> = clash
            .claims
            .iter()
            .map(|x| Quote { source: x.page.clone(), anchor: None, text: x.quote.clone(), checked: Some(true), path: Some(x.page.clone()) })
            .collect();
        let title =
            if v.summary.is_empty() { format!("{}: {}", clash.subject, clash.attribute.replace('_', " ")) } else { v.summary.clone() };
        let origin = Origin {
            kind: "contradiction".into(),
            chat: Some(v.id.clone()),
            label: Some("Contradiction".into()),
            run: Some(group.into()),
            trigger: trigger.map(String::from),
        };
        let mut s = crate::changes::Submit::new(&p.page, proposals::Kind::Edit, &title, &v.fix, origin, from_texts(Some(before), &after));
        s.quotes = quotes;
        s.model = Some(judge_model.clone());
        match crate::changes::submit(app, s) {
            Ok(_) => l.proposed += 1,
            Err(e) => {
                let why = e.message().to_string();
                crate::applog!("contradiction fix for {} not made: {why}", p.page);
                l.failed += 1;
                if !l.failures.contains(&why) {
                    l.failures.push(why);
                }
            }
        }
    }
    Ok(())
}

/// One clash as the report shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    #[serde(flatten)]
    clash: Clash,
    verdict: Option<Verdict>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    last: Last,
    items: Vec<Item>,
}

fn clashes() -> Vec<Clash> {
    std::fs::read(state().dir.join("clashes.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

#[tauri::command]
pub async fn contradictions_report() -> Res<Report> {
    blocking(|| {
        let v = state().verdicts();
        let items = clashes().into_iter().map(|x| Item { verdict: v.get(&x.id).cloned(), clash: x }).collect();
        Ok(Report { last: load_last(), items })
    })
    .await
}

#[tauri::command]
pub fn contradictions_run(app: AppHandle, unattended: Option<bool>) -> bool {
    start(&app, unattended.unwrap_or(false).then_some("scheduled"))
}

/// Marks a clash resolved or ignored (or back to a judge's verdict word).
#[tauri::command]
pub async fn contradictions_mark(app: AppHandle, id: String, verdict: String) -> Res<()> {
    blocking(move || {
        if !["resolved", "ignored", "contradiction", "evolution", "compatible", "unclear"].contains(&verdict.as_str()) {
            return Err(brainstead_core::write::WriteError::Invalid(format!("Not a verdict: {verdict}")).into());
        }
        let st = state();
        let mut v = st.verdicts().remove(&id).unwrap_or(Verdict {
            id: id.clone(),
            verdict: String::new(),
            severity: String::new(),
            summary: String::new(),
            correct: String::new(),
            fix: String::new(),
            patch: None,
            judged: String::new(),
        });
        v.verdict = verdict;
        v.judged = chrono::Local::now().date_naive().to_string();
        st.add_verdict(&v)?;
        let _ = app.emit("contradictions-changed", load_last());
        Ok(())
    })
    .await
}

/// The clashes whose fix is applied in Changes (a revert opens them again).
fn fixed() -> std::collections::HashSet<String> {
    c::fixed(&brainstead_core::changes::Store::new(&crate::platform::data_dir()).list())
}

/// The open contradictions a page is part of, for its banner.
#[tauri::command]
pub async fn page_contradictions(path: String) -> Res<Vec<Item>> {
    blocking(move || {
        let v = state().verdicts();
        let all = clashes();
        Ok(c::open_on(&path, &all, &v, &fixed()).into_iter().map(|x| Item { verdict: v.get(&x.id).cloned(), clash: x.clone() }).collect())
    })
    .await
}

/// A wiki page's kept facts (D-20261006-08), for its side pane: the latest per subject and
/// attribute, with what each superseded.
#[tauri::command]
pub async fn page_facts(app: AppHandle, path: String) -> Res<Vec<brainstead_core::claims::Fact>> {
    blocking(move || {
        let root = app.state::<VaultService>().root().ok_or_else(|| EditError::from("No vault is open.".to_string()))?;
        let Some(rel) = brainstead_core::claims::path(&path) else { return Ok(vec![]) };
        let text = std::fs::read_to_string(root.join(rel)).unwrap_or_default();
        Ok(brainstead_core::claims::facts(&brainstead_core::claims::parse(&text), None, None))
    })
    .await
}

/// Whether a page is part of an open contradiction, for Changes' checks. One whose fix has been
/// applied isn't, though clashes.json keeps it until the next run reads the page again.
pub fn open_contradiction(path: &str) -> bool {
    let all = clashes();
    let v = state().verdicts();
    // Only a page in a contradiction needs the changes that fixed one, and reading those means
    // reading every change: a change to any other page (each one a link fix makes) skips it.
    if c::open_on(path, &all, &v, &Default::default()).is_empty() {
        return false;
    }
    !c::open_on(path, &all, &v, &fixed()).is_empty()
}

/// Knowledge health's "Claims with no citation", from the cached claims.
pub fn uncited_check(root: &std::path::Path) -> brainstead_core::lint::Check {
    let pages = c::pages(root);
    let texts: HashMap<String, String> = pages.iter().map(|(r, t, _)| (r.clone(), t.clone())).collect();
    let (claims, _) = c::current(&state(), &pages);
    let items = c::uncited(&claims, &texts)
        .into_iter()
        .map(|(page, line)| brainstead_core::lint::Item {
            text: format!("{page}: {line}"),
            page: Some(page),
            detail: Some(line),
            ..Default::default()
        })
        .collect();
    brainstead_core::lint::Check { id: "uncited-claims", title: "Claims with no citation", classic: false, ignored: 0, items }
}

#[tauri::command]
pub fn contradictions_stop(app: AppHandle) {
    stop(&app);
}
