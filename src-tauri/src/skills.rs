// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The three skill screens §4 lists (stage 8 item 15), each from the vault skill it replaces:
//! Triage bookmarks (`triage-bookmarks`), Draft reply (`draft-reply`) and Doc check (`doc-check`).
//! The model only reads and answers in JSON; whatever changes in the vault, the screen does through
//! the usual safe writes, agent changes (in Changes) or the user's own clicks.

use std::path::{Path, PathBuf};

use brainstead_core::canonical::{self, Entry, Register};
use brainstead_core::lists::BOOKMARKS;
use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::ask;

use crate::vault::VaultService;

fn model(app: &AppHandle) -> String {
    crate::ask::job_model(app, "skills")
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    app.state::<VaultService>().root().ok_or_else(|| "No vault is open.".to_string())
}

/// Runs the default model, read-only in the vault, and returns its answer.
fn answer(app: &AppHandle, tag: &str, prompt: &str) -> Result<String, String> {
    let system = brainstead_core::ask::system_prompt(&ask::today(), false);
    let id = format!("{tag}-{}", Local::now().format("%H%M%S%3f"));
    let out = ask::run(app, &id, prompt, &model(app), None, &system, None, &mut |_| {});
    match out.error {
        Some(e) => Err(e),
        None => Ok(out.text),
    }
}

/// The JSON value in a model's answer: the outermost `open`…`close`, fences and prose around it
/// left out.
fn json_in<T: for<'a> Deserialize<'a>>(answer: &str, open: char, close: char) -> Result<T, String> {
    let (Some(a), Some(b)) = (answer.find(open), answer.rfind(close)) else {
        return Err("The answer had no result in it.".into());
    };
    if b < a {
        return Err("The answer had no result in it.".into());
    }
    serde_json::from_str(&answer[a..=b]).map_err(|e| format!("The answer couldn't be read: {e}"))
}

// ── Triage bookmarks ─────────────────────────────────────────────────────────────────────────

/// A bookmark with what can be known without reading it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkRow {
    pub target: String,
    pub path: Option<String>,
    pub title: String,
    /// When its file last changed (ms): an upper bound on its age, as the pin date isn't kept.
    pub mtime: Option<i64>,
    pub days: Option<i64>,
    /// Untouched for two weeks or more, counting a Keep in triage as a touch.
    pub stale: bool,
    pub missing: bool,
}

/// When each bookmark was last kept in triage (ms), by its target: keeping one says it's still
/// wanted, so it isn't stale again for another two weeks (app data, not the vault).
fn kept_file() -> PathBuf {
    crate::platform::data_dir().join("bookmarks-kept.json")
}

fn kept() -> std::collections::HashMap<String, i64> {
    std::fs::read_to_string(kept_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Triage's Keep: the bookmark stays, and counts as looked at today.
#[tauri::command]
pub fn bookmark_keep(target: String) -> Result<(), String> {
    let mut k = kept();
    k.insert(target, Local::now().timestamp_millis());
    let f = kept_file();
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&f, serde_json::to_string(&k).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn bookmarks_status(app: AppHandle) -> Result<Vec<BookmarkRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = root(&app)?;
        let text = std::fs::read_to_string(root.join(BOOKMARKS)).unwrap_or_default();
        let now = Local::now().timestamp_millis();
        let kept = kept();
        app.state::<VaultService>().with_index(|ix, _| {
            let mut out = Vec::new();
            for b in ix.bookmarks(&text)? {
                let mtime = match &b.path {
                    Some(p) => ix.summary(p)?.map(|s| s.mtime),
                    None => None,
                };
                let days = mtime.map(|m| (now - m) / 86_400_000);
                let touched = mtime.max(kept.get(&b.target).copied());
                out.push(BookmarkRow {
                    missing: b.path.is_none(),
                    stale: touched.is_some_and(|t| (now - t) / 86_400_000 >= 14),
                    target: b.target,
                    path: b.path,
                    title: b.title,
                    mtime,
                    days,
                });
            }
            Ok(out)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
pub struct BookmarkAsk {
    pub target: String,
    pub path: String,
}

/// What the model suggests for one bookmark.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkSuggestion {
    pub target: String,
    /// keep, promote, task or archive.
    pub decision: String,
    #[serde(default)]
    pub why: String,
    /// Two or three sentences on what it is.
    #[serde(default)]
    pub summary: String,
    /// For promote: the wiki page it should become, and whether that's a concept or an entity.
    #[serde(default)]
    pub page: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    /// For task: the task, starting with a verb.
    #[serde(default)]
    pub task: Option<String>,
}

const DECISIONS: [&str; 4] = ["keep", "promote", "task", "archive"];

#[tauri::command]
pub async fn bookmarks_suggest(app: AppHandle, items: Vec<BookmarkAsk>) -> Result<Vec<BookmarkSuggestion>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if items.is_empty() {
            return Ok(vec![]);
        }
        let list: String = items.iter().map(|i| format!("- \"{}\": {}\n", i.target, i.path)).collect();
        let prompt = format!(
            "Triage these bookmarks: read-later flags kept in `{BOOKMARKS}`. Read each file (paths below are in the vault) and decide what it should become. \
Use the vault's own `triage-bookmarks` skill, if it has one, for how the user likes this done.\n\n\
Decisions: \"keep\" (still worth reading later), \"promote\" (it deserves a wiki page: name the page, an existing one if it fits, and whether it's a concept or an entity), \"task\" (there's something to do about it: write the task starting with a verb), \"archive\" (read and absorbed, or no longer relevant; the bookmark goes, the note stays).\n\
Never decide on age alone, and never invent a page to promote to: if none fits, keep it and say why.\n\
Answer with only a JSON array, one object per bookmark: {{\"target\": \"…\", \"decision\": \"keep|promote|task|archive\", \"why\": \"one short sentence\", \"summary\": \"two sentences on what it is\", \"page\": \"wiki page name or null\", \"kind\": \"concept|entity or null\", \"task\": \"the task or null\"}}.\n\n\
Bookmarks:\n{list}"
        );
        let out: Vec<serde_json::Value> = json_in(&answer(&app, "triage", &prompt)?, '[', ']')?;
        let targets: Vec<&str> = items.iter().map(|i| i.target.as_str()).collect();
        Ok(out
            .into_iter()
            .filter_map(|v| serde_json::from_value::<BookmarkSuggestion>(v).ok())
            .filter(|s| targets.contains(&s.target.as_str()) && DECISIONS.contains(&s.decision.as_str()))
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Draft reply ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Callout {
    pub point: String,
    /// answered, partly or ignored.
    pub status: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    /// yes, partly, no, or n/a when the thread wasn't a reply to the user.
    #[serde(default)]
    pub answered: String,
    /// One or two sentences: what's being asked, and whether the thread settled it.
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub callouts: Vec<Callout>,
    /// Plain text, ready to paste.
    pub draft: String,
    /// Vault pages the draft drew on.
    #[serde(default)]
    pub grounded: Vec<String>,
    /// `[…]` gaps left in the draft, and what each needs.
    #[serde(default)]
    pub gaps: Vec<String>,
    /// A follow-up task for the user, if the reply commits them to one.
    #[serde(default)]
    pub task: Option<String>,
}

const TONES: [(&str, &str); 3] = [
    ("brief", "brief: as short as it can be while still answering"),
    ("warm", "warm: friendly and personal, still to the point"),
    ("formal", "formal: courteous and complete, for someone senior or outside the team"),
];

/// A reply to a captured thread (`thread`, a vault path) or pasted text, grounded in the vault.
/// With `current` (the reply as it stands, the user's edits in it) and `change` (what they asked
/// for), that reply is revised as asked rather than drafted afresh.
#[tauri::command]
pub async fn draft_reply(
    app: AppHandle,
    thread: Option<String>,
    text: Option<String>,
    tone: String,
    current: Option<String>,
    change: Option<String>,
) -> Result<Reply, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let body = match (&thread, &text) {
            (Some(p), _) => {
                brainstead_core::trash::safe_rel(p).map_err(|e| e.to_string())?;
                std::fs::read_to_string(root(&app)?.join(p)).map_err(|e| format!("Couldn't read {p}: {e}"))?
            }
            (None, Some(t)) if !t.trim().is_empty() => t.clone(),
            _ => return Err("Choose a thread or paste one.".into()),
        };
        let tone = TONES.iter().find(|(k, _)| *k == tone).map_or(TONES[0].1, |(_, d)| d);
        // Revising: the reply as it stands and what to change, after the thread (which is still read,
        // and the vault still looked in, for anything the change needs).
        let revise = match (current.as_deref().map(str::trim), change.as_deref().map(str::trim)) {
            (Some(c), Some(ch)) if !c.is_empty() && !ch.is_empty() => format!(
                "\n\nThis isn't a first draft: the user has the reply below (with any edits they made in it) and asks for a change. \
Revise that reply as they ask and give the whole revised reply as the draft. Keep the rest of it, their edits included, as it is unless the change needs otherwise; \
the same rules hold (their voice, plain text, `[…]` for anything not known, nothing invented).\n\nTheir reply:\n\n{c}\n\nThe change they ask for: {ch}"
            ),
            _ => String::new(),
        };
        let prompt = format!(
            "Draft a reply to this thread for the user (they write it in Teams or Outlook; nothing is sent from here). \
Use the vault's own `draft-reply` skill and house rules (its CLAUDE.md), if it has them, for how they write.\n\n\
First read the thread properly: what is actually being asked, and, when it replies to something the user sent, which of their points are answered, partly answered or ignored. \
Then look up the people and topics in the vault (who they are, what was already agreed, what the user already said) so the reply doesn't contradict or repeat it. \
The vault wins over the thread: if they disagree, say so in the verdict.\n\n\
The draft: plain text (no markdown), the length of the original, the user's voice, no throat-clearing and no summarising their message back. \
Tone: {tone}. Never invent a fact, date, name or commitment: leave a `[…]` gap and list it.\n\n\
Answer with only a JSON object: {{\"answered\": \"yes|partly|no|n/a\", \"verdict\": \"one or two sentences\", \"callouts\": [{{\"point\": \"…\", \"status\": \"answered|partly|ignored\"}}], \"draft\": \"the reply\", \"grounded\": [\"vault page names drawn on\"], \"gaps\": [\"each […] and what it needs\"], \"task\": \"a follow-up task for the user, or null\"}}.\n\n\
The thread:\n\n{body}{revise}"
        );
        let mut r: Reply = json_in(&answer(&app, "reply", &prompt)?, '{', '}')?;
        r.task = r.task.filter(|t| !t.trim().is_empty() && t != "null");
        Ok(r)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Doc check ────────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterView {
    #[serde(flatten)]
    pub register: Register,
    /// The register note is there.
    pub exists: bool,
}

#[tauri::command]
pub fn canonical_register(app: AppHandle) -> Result<RegisterView, String> {
    let root = root(&app)?;
    let home = dirs::home_dir().unwrap_or_default();
    match std::fs::read_to_string(root.join(canonical::REGISTER)) {
        Ok(t) => Ok(RegisterView { register: canonical::parse(&t, &root, &home), exists: true }),
        Err(_) => Ok(RegisterView { register: Register::default(), exists: false }),
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// diverges, not-covered, superseded-term, beyond-scope or aligned.
    pub kind: String,
    pub title: String,
    /// Where in each document: "§3.2 vs §4".
    #[serde(default)]
    pub r#where: String,
    #[serde(default)]
    pub candidate: String,
    #[serde(default)]
    pub canonical: String,
    /// Changes ownership, funding, sequence or how it's been positioned.
    #[serde(default)]
    pub material: bool,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub verdict: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub findings: Vec<Finding>,
    /// The two documents turned out not to be about the same thing: no findings, nothing compared.
    #[serde(default)]
    pub unrelated: bool,
    /// Filled here, not by the model (a kept check reads them back).
    #[serde(default)]
    pub against: Option<Entry>,
    #[serde(default)]
    pub excluded: Vec<Entry>,
}

/// At most this much of each document goes to the model.
const MAX_CHARS: usize = 150_000;

/// A document's text for the model, and the version of the file it was read from (kept with the
/// result, so a later change to the file shows it stale). A PDF or Office file's version is taken
/// before its text is extracted: a change while it's read shows stale rather than passing unseen.
fn read_any(path: &Path) -> Result<(String, String), String> {
    let rel = path.to_string_lossy();
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let version = brainstead_core::write::version(&bytes);
    let text = if brainstead_core::extract::kind_of(&rel).is_some() {
        brainstead_core::extract::extract(path, Some(crate::platform::pdf_text), Some(crate::platform::image_text))?.text()
    } else {
        String::from_utf8(bytes).map_err(|_| format!("Couldn't read {}: it isn't text", path.display()))?
    };
    let text = if text.chars().count() > MAX_CHARS {
        format!("{}\n\n[… cut at {MAX_CHARS} characters]", text.chars().take(MAX_CHARS).collect::<String>())
    } else {
        text
    };
    Ok((text, version))
}

/// What a check is of: the document, the governing document (its key in the register) and the mode.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckOf {
    pub candidate: String,
    pub doc: String,
    pub mode: String,
}

/// A step of a check, as the screen ticks them off.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CheckStep {
    pub name: String,
    /// running, done or failed.
    pub status: String,
    pub ms: u64,
}

/// A check under way or done this session (Doc check's progress), by id.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CheckRun {
    pub id: String,
    #[serde(flatten)]
    pub of: CheckOf,
    /// running, done, failed or stopped.
    pub status: String,
    pub started: String,
    pub steps: Vec<CheckStep>,
    pub error: Option<String>,
    pub result: Option<CheckResult>,
}

/// The checks this session, newest first.
#[derive(Default)]
pub struct CheckRuns(std::sync::Mutex<Vec<CheckRun>>);

/// A finished check, kept (in the app data folder, not the vault) with the versions of the two
/// documents it read, so it's shown again until either changes.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedCheck {
    #[serde(flatten)]
    pub of: CheckOf,
    pub at: String,
    pub candidate_version: String,
    #[serde(default)]
    pub canonical_version: Option<String>,
    pub result: CheckResult,
    /// Filled when it's read: either document has changed since.
    #[serde(default)]
    pub stale: bool,
}

fn checks_dir() -> PathBuf {
    crate::platform::data_dir().join("doc-checks")
}

fn saved_file(dir: &Path, of: &CheckOf) -> PathBuf {
    dir.join(format!("{}.json", brainstead_core::write::version(format!("{}\u{1f}{}\u{1f}{}", of.candidate, of.doc, of.mode).as_bytes())))
}

fn file_version(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| brainstead_core::write::version(&b))
}

/// The two documents' paths for a check: the one checked, and the version in force (none for
/// feedback mode, or when the register doesn't settle it).
fn check_paths(root: &Path, home: &Path, of: &CheckOf) -> (PathBuf, Option<PathBuf>) {
    let cand = canonical::resolve_path(root, &of.candidate, home);
    if of.mode == "callouts" {
        return (cand, None);
    }
    if is_compare(&of.mode) {
        return (cand, Some(canonical::resolve_path(root, &of.doc, home)));
    }
    let reg = std::fs::read_to_string(root.join(canonical::REGISTER)).map(|t| canonical::parse(&t, root, home)).unwrap_or_default();
    (cand, canonical::resolve(&reg.entries, &of.doc).canonical.map(|c| PathBuf::from(c.resolved_path)))
}

/// Compare's modes (two documents, A and B), as against Doc check's (a document and the register).
fn is_compare(mode: &str) -> bool {
    mode == "differences" || mode == "changes"
}

/// At most this much of each document goes to the first look.
const LOOK_CHARS: usize = 4_000;

/// The first look, before anything is compared: the opening of each document, and whether they're
/// about the same thing. A pair that isn't (an apple and a pear) stops here, for the cost of their
/// first pages rather than both whole documents. Some(why) when they aren't; an answer that can't be
/// read is taken as alike, so the full comparison (which can still say unrelated) decides.
fn not_alike(env: &mut CheckEnv, a_name: &str, a: &str, b_name: &str, b: &str) -> Result<Option<String>, String> {
    let head = |t: &str| t.chars().take(LOOK_CHARS).collect::<String>();
    let prompt = format!(
        "Before anything is compared: are these two documents about the same thing (the same subject, programme, policy or document in another version), so that comparing them clause by clause makes sense? \
Judge only from their openings below; don't look anything up. Two versions of one document, or a document and the policy it must follow, are the same thing; two documents on unrelated subjects are not.\n\n\
Answer with only a JSON object: {{\"same\": true|false, \"why\": \"one sentence\"}}.\n\n\
The opening of \"{a_name}\":\n\n{}\n\n---\n\nThe opening of \"{b_name}\":\n\n{}",
        head(a),
        head(b)
    );
    #[derive(Deserialize)]
    struct Look {
        same: bool,
        #[serde(default)]
        why: String,
    }
    let look = json_in::<Look>(&env.ask(&prompt)?, '{', '}');
    Ok(look.ok().and_then(|l| (!l.same).then_some(l.why)))
}

/// What a check gives when the two documents aren't about the same thing.
fn unrelated(why: String) -> CheckResult {
    CheckResult { verdict: "Not comparable: they aren't about the same thing".into(), summary: why, unrelated: true, ..Default::default() }
}

/// Said in each full comparison too: if, read in full, they aren't about the same thing after all.
const STOP_IF_UNRELATED: &str = "If, read in full, the two turn out not to be about the same thing, stop: give \"unrelated\": true, a verdict saying so and no findings, rather than comparing them anyway.";

/// The kept result for a check, marked stale when either document changed since.
#[tauri::command]
pub fn doc_check_saved(app: AppHandle, candidate: String, doc: String, mode: String) -> Result<Option<SavedCheck>, String> {
    Ok(saved_in(&checks_dir(), &root(&app)?, &dirs::home_dir().unwrap_or_default(), &CheckOf { candidate, doc, mode }))
}

fn saved_in(dir: &Path, root: &Path, home: &Path, of: &CheckOf) -> Option<SavedCheck> {
    let mut saved = std::fs::read(saved_file(dir, of)).ok().and_then(|b| serde_json::from_slice::<SavedCheck>(&b).ok())?;
    let (cand, canon) = check_paths(root, home, of);
    saved.stale = file_version(&cand).as_deref() != Some(saved.candidate_version.as_str())
        || canon.as_deref().and_then(file_version) != saved.canonical_version;
    Some(saved)
}

/// What a check needs from the app, so it runs (and is tested) without one.
struct CheckEnv<'a> {
    root: PathBuf,
    home: PathBuf,
    /// The model's answer to a prompt.
    answer: &'a mut dyn FnMut(&str) -> Result<String, String>,
    /// Starts a step, as the screen ticks them off.
    step: &'a mut dyn FnMut(&str),
    /// The check's been stopped (doc_check_stop).
    stopped: &'a dyn Fn() -> bool,
}

/// What a stopped check ends with: it's said as stopped, not as this error.
const STOPPED: &str = "Stopped.";

impl CheckEnv<'_> {
    /// Starts the next step, unless the check's been stopped: then nothing more starts.
    fn step(&mut self, name: &str) -> Result<(), String> {
        if (self.stopped)() {
            return Err(STOPPED.into());
        }
        (self.step)(name);
        Ok(())
    }
    /// The model's answer, unless the check's been stopped (before the call, or during it).
    fn ask(&mut self, prompt: &str) -> Result<String, String> {
        if (self.stopped)() {
            return Err(STOPPED.into());
        }
        let out = (self.answer)(prompt);
        if (self.stopped)() {
            return Err(STOPPED.into());
        }
        out
    }
}

/// A finished check, with the versions of the two documents as it read them, to keep.
struct Checked {
    result: CheckResult,
    candidate_version: String,
    canonical_version: Option<String>,
}

/// Runs a check, saying each step as it goes. Nothing is kept here: the caller keeps it (keep),
/// unless it's been stopped.
fn check_in(env: &mut CheckEnv, of: &CheckOf) -> Result<Checked, String> {
    let (candidate, doc, mode) = (of.candidate.clone(), of.doc.clone(), of.mode.clone());
    env.step("Read the document")?;
    let (root, home) = (env.root.clone(), env.home.clone());
    let cand_path = canonical::resolve_path(&root, &candidate, &home);
    let (cand, candidate_version) = read_any(&cand_path)?;
    let mut canonical_version = None;
    let name = cand_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(candidate.clone());
    let reg = std::fs::read_to_string(root.join(canonical::REGISTER)).map(|t| canonical::parse(&t, &root, &home)).unwrap_or_default();
    let prompt;
    let mut against = None;
    let mut excluded = vec![];
    if mode == "callouts" {
        prompt = format!(
                "Check whether this revised document, \"{name}\", took in the feedback the user sent on the last version. \
Find what they said before in the vault (notes, sources, sent captures about this document) and list it point by point; then judge each point against this revision. \
Use the vault's own `doc-check` skill, if it has one, for how they like this done.\n\n\
Answer with only a JSON object: {{\"verdict\": \"one line\", \"summary\": \"two sentences\", \"findings\": [{{\"kind\": \"aligned|diverges|not-covered\", \"title\": \"the callout\", \"where\": \"where in the revision\", \"candidate\": \"what the revision says, quoted\", \"canonical\": \"what the user asked for, quoted\", \"material\": true|false}}]}} \
(aligned: taken in; diverges: answered differently; not-covered: ignored). If you find no feedback from the user on this document, stop there: say so in the verdict, with no findings, rather than inventing points to check.\n\n\
The revision:\n\n{cand}"
            );
    } else if is_compare(&mode) {
        // Compare: A (the one picked first) and B, any two documents.
        let b_path = canonical::resolve_path(&root, &doc, &home);
        let b_name = b_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(doc.clone());
        env.step(&format!("Read {b_name}"))?;
        let (b, b_version) = read_any(&b_path)?;
        canonical_version = Some(b_version);
        env.step("Check they're about the same thing")?;
        if let Some(why) = not_alike(env, &name, &cand, &b_name, &b)? {
            return Ok(Checked { result: unrelated(why), candidate_version, canonical_version });
        }
        prompt = if mode == "changes" {
            format!(
                "\"{name}\" (A) is a newer version of \"{b_name}\" (B). Say what changed from B to A: \"changed\" (both say it, differently: quote both), \"added\" (in A, not in B), \"removed\" (in B, not in A). \
Leave out pure rewording that keeps the meaning. Mark each material or not: material if it changes a decision, number, date, owner, scope or commitment. Don't invent changes to look thorough. {STOP_IF_UNRELATED}\n\n\
Answer with only a JSON object: {{\"verdict\": \"one line: what changed, in short\", \"summary\": \"two sentences\", \"unrelated\": false, \"findings\": [{{\"kind\": \"changed|added|removed\", \"title\": \"short\", \"where\": \"§ in A vs § in B\", \"candidate\": \"quote from A\", \"canonical\": \"quote from B\", \"material\": true|false}}]}}.\n\n\
A, the newer version:\n\n{cand}\n\n---\n\nB, the earlier version:\n\n{b}"
            )
        } else {
            format!(
                "Compare \"{name}\" (A) and \"{b_name}\" (B), neither taking precedence. For each point either makes: \"differs\" (they say different things about it: quote both), \"only-a\" (A says it, B is silent), \"only-b\" (B says it, A is silent), \"aligned\" (they agree, even in other words). \
Mark each difference material or not: material if it's a decision, number, date, owner, scope or commitment. Don't invent findings to look thorough. {STOP_IF_UNRELATED}\n\n\
Answer with only a JSON object: {{\"verdict\": \"one line: how far they agree\", \"summary\": \"two sentences\", \"unrelated\": false, \"findings\": [{{\"kind\": \"differs|only-a|only-b|aligned\", \"title\": \"short\", \"where\": \"§ in A vs § in B\", \"candidate\": \"quote from A\", \"canonical\": \"quote from B\", \"material\": true|false}}]}}.\n\n\
A:\n\n{cand}\n\n---\n\nB:\n\n{b}"
            )
        };
    } else {
        let r = canonical::resolve(&reg.entries, &doc);
        if !r.ambiguous.is_empty() {
            return Err(format!(
                "More than one version of “{doc}” is marked canonical in {} ({}). Say which is in force there, then check again.",
                canonical::REGISTER,
                r.ambiguous.iter().map(|e| e.version.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        let Some(c) = r.canonical else {
            return Err(format!("Nothing in {} says which version of “{doc}” is in force. Add it there first.", canonical::REGISTER));
        };
        if !c.exists {
            return Err(format!("{} lists {} {} at {}, and that file isn't there.", canonical::REGISTER, c.title, c.version, c.path));
        }
        env.step(&format!("Read {} {}, the version in force", c.title, c.version))?;
        let (canon, canon_version) = read_any(Path::new(&c.resolved_path))?;
        canonical_version = Some(canon_version);
        env.step("Check they're about the same thing")?;
        if let Some(why) = not_alike(env, &name, &cand, &format!("{} {}", c.title, c.version), &canon)? {
            let mut result = unrelated(why);
            result.against = Some(c);
            return Ok(Checked { result, candidate_version, canonical_version });
        }
        let drafts: Vec<String> = r.drafts.iter().map(|e| format!("{} {}", e.title, e.version)).collect();
        let gone: Vec<String> = r.superseded.iter().map(|e| format!("{} {}", e.title, e.version)).collect();
        prompt = format!(
                "Check the document \"{name}\" against {} {}, the version in force. Not in force, so not to be used: drafts {}; superseded {}. \
Use the vault's own `doc-check` skill, if it has one, for how the user likes this done.\n\n\
Go clause by clause through what the document claims, assigns or sequences. For each, one of: \"aligned\" (agrees, even in other words), \"diverges\" (contradicts a specific clause: quote both), \"not-covered\" (the canonical document requires something this one is silent on), \"beyond-scope\" (it asserts something the canonical document doesn't speak to), \"superseded-term\" (it uses a term a superseded version used and the version in force replaced). \
Mark each divergence material or not: material if it changes ownership, funding, sequence or how it's been positioned. Don't invent findings to look thorough, and don't soften a real divergence. {STOP_IF_UNRELATED}\n\n\
Answer with only a JSON object: {{\"verdict\": \"one line: ready, or what must change\", \"summary\": \"two sentences\", \"unrelated\": false, \"findings\": [{{\"kind\": \"…\", \"title\": \"short\", \"where\": \"§ in this document vs § in the canonical one\", \"candidate\": \"quote from this document\", \"canonical\": \"quote from the canonical one\", \"material\": true|false}}]}}.\n\n\
The document to check:\n\n{cand}\n\n---\n\nThe version in force, {} {}:\n\n{canon}",
                c.title,
                c.version,
                if drafts.is_empty() { "none".into() } else { drafts.join(", ") },
                if gone.is_empty() { "none".into() } else { gone.join(", ") },
                c.title,
                c.version,
            );
        excluded = r.drafts.into_iter().chain(r.superseded).collect();
        against = Some(c);
    }
    env.step(if mode == "callouts" { "Find your earlier feedback and check it" } else { "Compare them, clause by clause" })?;
    let text = env.ask(&prompt)?;
    let mut result: CheckResult = json_in(&text, '{', '}')?;
    result.against = against;
    result.excluded = excluded;
    if result.unrelated {
        result.findings.clear();
    }
    Ok(Checked { result, candidate_version, canonical_version })
}

/// Runs a check in the app (its vault, its model under `chat`, so doc_check_stop can end the call).
fn check_now(app: &AppHandle, of: &CheckOf, chat: &str, step: &mut dyn FnMut(&str), stopped: &dyn Fn() -> bool) -> Result<Checked, String> {
    let mut answer = |prompt: &str| answer_as(app, chat, prompt);
    let mut env = CheckEnv { root: root(app)?, home: dirs::home_dir().unwrap_or_default(), answer: &mut answer, step, stopped };
    check_in(&mut env, of)
}

/// Keeps a finished check, with the versions of the documents as it read them, for the screen (and
/// doc_check) to show again until either changes; gives its result back.
fn keep(dir: &Path, of: &CheckOf, done: Checked) -> CheckResult {
    let saved = SavedCheck {
        of: of.clone(),
        at: proposals_now(),
        candidate_version: done.candidate_version,
        canonical_version: done.canonical_version,
        result: done.result,
        stale: false,
    };
    let _ = std::fs::create_dir_all(dir);
    if let Ok(b) = serde_json::to_vec_pretty(&saved) {
        let _ = brainstead_core::write::write_atomic(&saved_file(dir, of), &b, false);
    }
    saved.result
}

fn proposals_now() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// A model's answer under a known id, so the run can be stopped (ask::Running::cancel).
fn answer_as(app: &AppHandle, id: &str, prompt: &str) -> Result<String, String> {
    let system = brainstead_core::ask::system_prompt(&ask::today(), false);
    let out = ask::run(app, id, prompt, &model(app), None, &system, None, &mut |_| {});
    match out.error {
        Some(e) => Err(e),
        None => Ok(out.text),
    }
}

/// Checks `candidate` (a vault path or a full path) against the version of `doc` in force, or, in
/// `callouts` mode, whether it took in the user's earlier feedback; waited on (doc_check, for an
/// assistant). The screen starts one in the background with doc_check_start.
#[tauri::command]
pub async fn doc_check(app: AppHandle, candidate: String, doc: String, mode: String) -> Result<CheckResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let chat = format!("doccheck-{}", Local::now().format("%H%M%S%3f"));
        let of = CheckOf { candidate, doc, mode };
        check_now(&app, &of, &chat, &mut |_| {}, &|| false).map(|done| keep(&checks_dir(), &of, done))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn emit_run(app: &AppHandle, r: &CheckRun) {
    use tauri::Emitter;
    let _ = app.emit("doccheck-changed", r);
}

/// Starts a check in the background and gives its run; its steps and result arrive as
/// `doccheck-changed`. One already going for the same document, governing document and mode is
/// given back instead of a second. The run is listed and said as running before its thread starts,
/// so no step of it can arrive ahead of that.
#[tauri::command]
pub fn doc_check_start(app: AppHandle, candidate: String, doc: String, mode: String) -> Result<CheckRun, String> {
    let of = CheckOf { candidate, doc, mode };
    let id = format!("doccheck-{}", Local::now().format("%H%M%S%3f"));
    let runs = app.state::<CheckRuns>();
    let (run, new) = {
        let mut list = runs.0.lock().unwrap();
        let (run, new) = start_run(&mut list, &of, &id, &proposals_now());
        if new {
            emit_run(&app, &run);
        }
        (run, new)
    };
    if !new {
        return Ok(run);
    }
    let app2 = app.clone();
    std::thread::spawn(move || {
        let update = |f: &mut dyn FnMut(&mut CheckRun)| {
            let st = app2.state::<CheckRuns>();
            let mut list = st.0.lock().unwrap();
            if let Some(r) = list.iter_mut().find(|r| r.id == id) {
                f(r);
                emit_run(&app2, r);
            }
        };
        let stopped = || app2.state::<CheckRuns>().0.lock().unwrap().iter().any(|r| r.id == id && r.status == "stopped");
        let mut t0 = std::time::Instant::now();
        let mut step = |name: &str| {
            let ms = t0.elapsed().as_millis() as u64;
            t0 = std::time::Instant::now();
            update(&mut |r| r.next_step(name, ms));
        };
        let out = check_now(&app2, &of, &id, &mut step, &stopped);
        let ms = t0.elapsed().as_millis() as u64;
        // Kept under the list's lock, so a Stop can't land between seeing it isn't stopped and keeping it.
        let mut out = Some(out);
        update(&mut |r| {
            if let Some(out) = out.take() {
                let out = if r.status == "stopped" { Err(STOPPED.into()) } else { out.map(|done| keep(&checks_dir(), &of, done)) };
                r.finish(out, ms);
            }
        });
    });
    Ok(run)
}

/// Lists a new run as running, newest first, unless one for the same check is going: that one is
/// given back instead (and false).
fn start_run(list: &mut Vec<CheckRun>, of: &CheckOf, id: &str, started: &str) -> (CheckRun, bool) {
    if let Some(r) = list.iter().find(|r| r.of == *of && r.status == "running") {
        return (r.clone(), false);
    }
    let run = CheckRun {
        id: id.into(),
        of: of.clone(),
        status: "running".into(),
        started: started.into(),
        steps: vec![],
        error: None,
        result: None,
    };
    list.insert(0, run.clone());
    list.truncate(20);
    (run, true)
}

/// Marks a run that's going as stopped; gives it back, to say so.
fn stop_run(list: &mut [CheckRun], id: &str) -> Option<CheckRun> {
    let r = list.iter_mut().find(|r| r.id == id && r.status == "running")?;
    r.status = "stopped".into();
    Some(r.clone())
}

impl CheckRun {
    /// The step going is done (in `ms`), and `name` starts.
    fn next_step(&mut self, name: &str, ms: u64) {
        if let Some(last) = self.steps.last_mut() {
            last.status = "done".into();
            last.ms = ms;
        }
        self.steps.push(CheckStep { name: name.into(), status: "running".into(), ms: 0 });
    }

    /// The check ended: done with its result, or failed. A stopped run stays stopped, with no result:
    /// its last step is marked failed, since it didn't finish.
    fn finish(&mut self, out: Result<CheckResult, String>, ms: u64) {
        let stopped = self.status == "stopped";
        if let Some(last) = self.steps.last_mut() {
            last.status = if out.is_ok() && !stopped { "done" } else { "failed" }.into();
            last.ms = ms;
        }
        if stopped {
            return;
        }
        match out {
            Ok(res) => {
                self.status = "done".into();
                self.result = Some(res);
            }
            Err(e) => {
                self.status = "failed".into();
                self.error = Some(e);
            }
        }
    }
}

/// The checks this session, newest first (the screen picks up one still going when it opens).
#[tauri::command]
pub fn doc_check_runs(app: AppHandle) -> Vec<CheckRun> {
    app.state::<CheckRuns>().0.lock().unwrap().clone()
}

/// Stops a check that's going: its model call is ended, no step after it starts, and nothing is
/// kept from it.
#[tauri::command]
pub fn doc_check_stop(app: AppHandle, id: String) {
    let runs = app.state::<CheckRuns>();
    if let Some(r) = stop_run(&mut runs.0.lock().unwrap(), &id) {
        emit_run(&app, &r);
    }
    app.state::<std::sync::Arc<ask::Running>>().cancel(&id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_json_out_of_an_answer() {
        let a = "Here you go:\n```json\n[{\"target\": \"A\", \"decision\": \"keep\"}]\n```\nDone.";
        let v: Vec<BookmarkSuggestion> = json_in(a, '[', ']').unwrap();
        assert_eq!((v[0].target.as_str(), v[0].decision.as_str()), ("A", "keep"));
        let r: Reply = json_in("{\"draft\": \"Hi Maya, yes.\", \"answered\": \"partly\"}", '{', '}').unwrap();
        assert_eq!((r.draft.as_str(), r.answered.as_str(), r.callouts.len()), ("Hi Maya, yes.", "partly", 0));
        assert!(json_in::<Reply>("no json here", '{', '}').is_err());
    }

    const FINDINGS: &str =
        r#"{"verdict": "Mostly aligned", "summary": "Two differences.", "findings": [{"kind": "differs", "title": "Launch date"}]}"#;

    /// A vault with two plans to compare, and Compare's check of them.
    fn plans() -> (tempfile::TempDir, CheckOf) {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("Orbit plan v2.md"), "# Orbit plan v2\n\nLaunch in May.\n").unwrap();
        std::fs::write(d.path().join("Orbit plan v1.md"), "# Orbit plan v1\n\nLaunch in April.\n").unwrap();
        (d, CheckOf { candidate: "Orbit plan v2.md".into(), doc: "Orbit plan v1.md".into(), mode: "differences".into() })
    }

    /// Runs a check over `root` with the model answering through `answer`; gives the outcome and the
    /// steps it started.
    fn check(
        root: &Path,
        of: &CheckOf,
        answer: &mut dyn FnMut(&str) -> Result<String, String>,
        stopped: &dyn Fn() -> bool,
    ) -> (Result<Checked, String>, Vec<String>) {
        let mut steps = vec![];
        let mut step = |n: &str| steps.push(n.to_string());
        let mut env = CheckEnv { root: root.into(), home: root.into(), answer, step: &mut step, stopped };
        let out = check_in(&mut env, of);
        (out, steps)
    }

    #[test]
    fn a_first_look_that_cant_be_read_goes_on_to_the_full_comparison() {
        let (d, of) = plans();
        let mut asked = 0;
        let mut answer = |_: &str| {
            asked += 1;
            Ok(if asked == 1 { "They look like the same plan.".into() } else { FINDINGS.into() })
        };
        let (out, _) = check(d.path(), &of, &mut answer, &|| false);
        assert_eq!(asked, 2);
        let done = out.unwrap();
        assert_eq!((done.result.verdict.as_str(), done.result.findings.len()), ("Mostly aligned", 1));
        // And a first look that says they're unrelated stops there.
        let mut answer = |_: &str| Ok(r#"{"same": false, "why": "A plan and a recipe."}"#.to_string());
        let done = check(d.path(), &of, &mut answer, &|| false).0.unwrap();
        assert!(done.result.unrelated);
        assert_eq!(done.result.summary, "A plan and a recipe.");
    }

    #[test]
    fn a_stopped_check_starts_nothing_more() {
        let (d, of) = plans();
        let stop = std::cell::Cell::new(false);
        // Stopped during the first look: no full comparison is asked for, and nothing comes back to keep.
        let mut asked = 0;
        let mut answer = |_: &str| {
            asked += 1;
            stop.set(true);
            Ok(r#"{"same": true}"#.to_string())
        };
        let (out, steps) = check(d.path(), &of, &mut answer, &|| stop.get());
        assert_eq!(out.err().as_deref(), Some(STOPPED));
        assert_eq!(asked, 1);
        assert_eq!(steps.last().map(String::as_str), Some("Check they're about the same thing"));
        // Stopped before it starts: nothing is read and the model isn't asked.
        let mut answer = |_: &str| -> Result<String, String> { panic!("asked after a stop") };
        let (out, steps) = check(d.path(), &of, &mut answer, &|| true);
        assert_eq!(out.err().as_deref(), Some(STOPPED));
        assert!(steps.is_empty());
    }

    #[test]
    fn a_kept_check_holds_the_versions_it_read_and_goes_stale_when_either_changes() {
        let (d, of) = plans();
        let kept = tempfile::tempdir().unwrap();
        let mut answer = |_: &str| Ok(FINDINGS.to_string());
        let done = check(d.path(), &of, &mut answer, &|| false).0.unwrap();
        assert_eq!(keep(kept.path(), &of, done).verdict, "Mostly aligned");
        let saved = saved_in(kept.path(), d.path(), d.path(), &of).unwrap();
        assert!(!saved.stale);
        assert_eq!(saved.result.findings.len(), 1);
        // Another mode of the same two is a check of its own, not kept yet.
        assert!(saved_in(kept.path(), d.path(), d.path(), &CheckOf { mode: "changes".into(), ..of.clone() }).is_none());
        std::fs::write(d.path().join("Orbit plan v1.md"), "# Orbit plan v1\n\nLaunch in June.\n").unwrap();
        assert!(saved_in(kept.path(), d.path(), d.path(), &of).unwrap().stale);

        // A document edited while the model reads it: the kept check is of what was read, so stale at once.
        let (d, of) = plans();
        let mut answer = |_: &str| {
            std::fs::write(d.path().join("Orbit plan v2.md"), "# Orbit plan v2\n\nLaunch in July.\n").unwrap();
            Ok(FINDINGS.to_string())
        };
        let done = check(d.path(), &of, &mut answer, &|| false).0.unwrap();
        keep(kept.path(), &of, done);
        assert!(saved_in(kept.path(), d.path(), d.path(), &of).unwrap().stale);
    }

    #[test]
    fn runs_start_once_and_a_stopped_run_stays_stopped() {
        let of = CheckOf { candidate: "Orbit plan v2.md".into(), doc: "Orbit plan v1.md".into(), mode: "differences".into() };
        let mut list = vec![];
        let (run, new) = start_run(&mut list, &of, "doccheck-1", "2026-10-12T09:00:00+02:00");
        assert!(new && run.status == "running");
        // The same check again while it's going gives that one back.
        let (again, new) = start_run(&mut list, &of, "doccheck-2", "2026-10-12T09:00:01+02:00");
        assert!(!new);
        assert_eq!((again.id.as_str(), list.len()), ("doccheck-1", 1));

        list[0].next_step("Read the document", 0);
        list[0].next_step("Read Orbit plan v1.md", 12);
        assert_eq!(list[0].steps.iter().map(|s| s.status.as_str()).collect::<Vec<_>>(), ["done", "running"]);
        assert_eq!(stop_run(&mut list, "doccheck-1").map(|r| r.status).as_deref(), Some("stopped"));
        assert!(stop_run(&mut list, "doccheck-1").is_none());
        // Its thread ending afterwards, even with a result, doesn't bring it back.
        list[0].finish(Ok(CheckResult { verdict: "Mostly aligned".into(), ..Default::default() }), 40);
        assert_eq!(list[0].status, "stopped");
        assert!(list[0].result.is_none());
        assert_eq!(list[0].steps[1].status, "failed");
        // A stopped check can be started afresh.
        assert!(start_run(&mut list, &of, "doccheck-3", "2026-10-12T09:01:00+02:00").1);

        list[0].finish(Err("The model didn't answer.".into()), 5);
        assert_eq!((list[0].status.as_str(), list[0].error.as_deref()), ("failed", Some("The model didn't answer.")));
    }
}
