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

fn read_any(path: &Path) -> Result<String, String> {
    let rel = path.to_string_lossy();
    let text = if brainstead_core::extract::kind_of(&rel).is_some() {
        brainstead_core::extract::extract(path, Some(crate::platform::pdf_text), Some(crate::platform::image_text))?.text()
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?
    };
    Ok(if text.chars().count() > MAX_CHARS {
        format!("{}\n\n[… cut at {MAX_CHARS} characters]", text.chars().take(MAX_CHARS).collect::<String>())
    } else {
        text
    })
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

fn saved_file(of: &CheckOf) -> PathBuf {
    checks_dir()
        .join(format!("{}.json", brainstead_core::write::version(format!("{}\u{1f}{}\u{1f}{}", of.candidate, of.doc, of.mode).as_bytes())))
}

fn file_version(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| brainstead_core::write::version(&b))
}

/// The two documents' paths for a check: the one checked, and the version in force (none for
/// feedback mode, or when the register doesn't settle it).
fn check_paths(app: &AppHandle, of: &CheckOf) -> Result<(PathBuf, Option<PathBuf>), String> {
    let root = root(app)?;
    let home = dirs::home_dir().unwrap_or_default();
    let cand = canonical::resolve_path(&root, &of.candidate, &home);
    if of.mode == "callouts" {
        return Ok((cand, None));
    }
    if is_compare(&of.mode) {
        return Ok((cand, Some(canonical::resolve_path(&root, &of.doc, &home))));
    }
    let reg = std::fs::read_to_string(root.join(canonical::REGISTER)).map(|t| canonical::parse(&t, &root, &home)).unwrap_or_default();
    Ok((cand, canonical::resolve(&reg.entries, &of.doc).canonical.map(|c| PathBuf::from(c.resolved_path))))
}

/// Compare's modes (two documents, A and B), as against Doc check's (a document and the register).
fn is_compare(mode: &str) -> bool {
    mode == "differences" || mode == "changes"
}

/// At most this much of each document goes to the first look.
const LOOK_CHARS: usize = 4_000;

/// The first look, before anything is compared: the opening of each document, and whether they're
/// about the same thing. A pair that isn't (an apple and a pear) stops here, for the cost of their
/// first pages rather than both whole documents. Some(why) when they aren't.
fn not_alike(app: &AppHandle, chat: &str, a_name: &str, a: &str, b_name: &str, b: &str) -> Result<Option<String>, String> {
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
    let look: Look = json_in(&answer_as(app, chat, &prompt)?, '{', '}')?;
    Ok((!look.same).then_some(look.why))
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
    let of = CheckOf { candidate, doc, mode };
    let Some(mut saved) = std::fs::read(saved_file(&of)).ok().and_then(|b| serde_json::from_slice::<SavedCheck>(&b).ok()) else {
        return Ok(None);
    };
    let (cand, canon) = check_paths(&app, &of)?;
    saved.stale = file_version(&cand).as_deref() != Some(saved.candidate_version.as_str())
        || canon.as_deref().and_then(file_version) != saved.canonical_version;
    Ok(Some(saved))
}

/// Runs a check, saying each step as it goes (`step(name)` starts one), and keeps its result.
fn check_now(app: &AppHandle, of: &CheckOf, chat: &str, step: &mut dyn FnMut(&str)) -> Result<CheckResult, String> {
    let (candidate, doc, mode) = (of.candidate.clone(), of.doc.clone(), of.mode.clone());
    step("Read the document");
    let root = root(app)?;
    let home = dirs::home_dir().unwrap_or_default();
    let cand_path = canonical::resolve_path(&root, &candidate, &home);
    let cand = read_any(&cand_path)?;
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
        step(&format!("Read {b_name}"));
        let b = read_any(&b_path)?;
        step("Check they're about the same thing");
        if let Some(why) = not_alike(app, chat, &name, &cand, &b_name, &b)? {
            return keep(app, of, unrelated(why));
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
        step(&format!("Read {} {}, the version in force", c.title, c.version));
        let canon = read_any(Path::new(&c.resolved_path))?;
        step("Check they're about the same thing");
        if let Some(why) = not_alike(app, chat, &name, &cand, &format!("{} {}", c.title, c.version), &canon)? {
            let mut out = unrelated(why);
            out.against = Some(c);
            return keep(app, of, out);
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
    step(if mode == "callouts" { "Find your earlier feedback and check it" } else { "Compare them, clause by clause" });
    let text = answer_as(app, chat, &prompt)?;
    let mut out: CheckResult = json_in(&text, '{', '}')?;
    out.against = against;
    out.excluded = excluded;
    if out.unrelated {
        out.findings.clear();
    }
    keep(app, of, out)
}

/// Keeps a finished check, with the versions of what it read, for the screen (and doc_check) to show
/// again until either document changes; gives it back.
fn keep(app: &AppHandle, of: &CheckOf, out: CheckResult) -> Result<CheckResult, String> {
    let (cand, canon) = check_paths(app, of)?;
    let saved = SavedCheck {
        of: of.clone(),
        at: proposals_now(),
        candidate_version: file_version(&cand).unwrap_or_default(),
        canonical_version: canon.as_deref().and_then(file_version),
        result: out.clone(),
        stale: false,
    };
    let _ = std::fs::create_dir_all(checks_dir());
    if let Ok(b) = serde_json::to_vec_pretty(&saved) {
        let _ = brainstead_core::write::write_atomic(&saved_file(of), &b, false);
    }
    Ok(out)
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
        check_now(&app, &CheckOf { candidate, doc, mode }, &chat, &mut |_| {})
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
/// given back instead of a second.
#[tauri::command]
pub fn doc_check_start(app: AppHandle, candidate: String, doc: String, mode: String) -> Result<CheckRun, String> {
    let of = CheckOf { candidate, doc, mode };
    let runs = app.state::<CheckRuns>();
    if let Some(r) = runs.0.lock().unwrap().iter().find(|r| r.of == of && r.status == "running") {
        return Ok(r.clone());
    }
    let id = format!("doccheck-{}", Local::now().format("%H%M%S%3f"));
    let run = CheckRun {
        id: id.clone(),
        of: of.clone(),
        status: "running".into(),
        started: proposals_now(),
        steps: vec![],
        error: None,
        result: None,
    };
    runs.0.lock().unwrap().insert(0, run.clone());
    runs.0.lock().unwrap().truncate(20);
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
        let mut t0 = std::time::Instant::now();
        let mut step = |name: &str| {
            let ms = t0.elapsed().as_millis() as u64;
            t0 = std::time::Instant::now();
            update(&mut |r| {
                if let Some(last) = r.steps.last_mut() {
                    last.status = "done".into();
                    last.ms = ms;
                }
                r.steps.push(CheckStep { name: name.into(), status: "running".into(), ms: 0 });
            });
        };
        let out = check_now(&app2, &of, &id, &mut step);
        let ms = t0.elapsed().as_millis() as u64;
        update(&mut |r| {
            let stopped = r.status == "stopped";
            if let Some(last) = r.steps.last_mut() {
                last.status = if out.is_ok() { "done" } else { "failed" }.into();
                last.ms = ms;
            }
            if stopped {
                return;
            }
            match &out {
                Ok(res) => {
                    r.status = "done".into();
                    r.result = Some(res.clone());
                }
                Err(e) => {
                    r.status = "failed".into();
                    r.error = Some(e.clone());
                }
            }
        });
    });
    emit_run(&app, &run);
    Ok(run)
}

/// The checks this session, newest first (the screen picks up one still going when it opens).
#[tauri::command]
pub fn doc_check_runs(app: AppHandle) -> Vec<CheckRun> {
    app.state::<CheckRuns>().0.lock().unwrap().clone()
}

/// Stops a check that's going: its model call is ended, and nothing is kept from it.
#[tauri::command]
pub fn doc_check_stop(app: AppHandle, id: String) {
    let runs = app.state::<CheckRuns>();
    let mut list = runs.0.lock().unwrap();
    if let Some(r) = list.iter_mut().find(|r| r.id == id && r.status == "running") {
        r.status = "stopped".into();
        emit_run(&app, r);
    }
    drop(list);
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
}
