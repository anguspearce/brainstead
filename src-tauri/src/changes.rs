// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or
// later. See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Agent changes (decided 2026-10-05): agents act, and the user is asked only when it's needed. A
//! change the user started (Ask, a terminal session, a run started by hand) is applied at once,
//! flagged when a check fails; a scheduled one is applied when it passes every check and held
//! otherwise. Renames and moves to the Trash apply at once. Every applied change is recorded in
//! `changes/` (core/src/changes.rs) for the Changes screen and Revert.

use std::path::{Path, PathBuf};

use brainstead_core::changes::{self, Change, Facts, Instruction, Status, Store};
use brainstead_core::proposals::{self, Kind, Origin, Quote};
use brainstead_core::{paths, reviews, write};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::edits::{writable, written, EditError, FileUndo, Res, UndoEntry, UndoStack};
use crate::platform;
use crate::vault::VaultService;
use crate::AppState;

fn store() -> Store {
    Store::new(&platform::data_dir())
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

pub fn emit_changed(app: &AppHandle) {
    let _ = app.emit("changes-changed", ());
}

/// A change an agent or a run asks for.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Submit {
    pub page: String,
    pub kind: Kind,
    pub title: String,
    #[serde(default)]
    pub reason: String,
    pub instruction: Instruction,
    #[serde(default)]
    pub quotes: Vec<Quote>,
    /// Worth knowing, not a problem: a link to a page that doesn't exist.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Checks the maker ran that didn't pass (an ingest's quotes, text read from an image).
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub origin: Origin,
    #[serde(default)]
    pub model: Option<String>,
    /// Held whatever the checks say, and why: a job the user set to hold its changes.
    #[serde(default)]
    pub hold: Option<String>,
    /// An ingest's checked claims, for the page's claims file (D-20261006-16).
    #[serde(skip)]
    pub claims: Option<brainstead_core::claims::Update>,
}

impl Submit {
    pub fn new(page: &str, kind: Kind, title: &str, reason: &str, origin: Origin, instruction: Instruction) -> Submit {
        Submit {
            page: page.into(),
            kind,
            title: title.into(),
            reason: reason.into(),
            instruction,
            quotes: vec![],
            warnings: vec![],
            flags: vec![],
            origin,
            model: None,
            hold: None,
            claims: None,
        }
    }
}

/// What became of it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub id: String,
    pub applied: bool,
    /// The page as it is now (a rename's new path).
    pub page: String,
    /// Checks it didn't pass: why it's held, or what to look at.
    pub flags: Vec<String>,
    /// For the toast and the assistant: "Changed Orbit App: …".
    pub message: String,
}

/// Applies a change, or holds it, by the rule; records it either way.
pub fn submit(app: &AppHandle, s: Submit) -> Res<Outcome> {
    Ok(submit_many(app, vec![s])?.remove(0))
}

/// One change worked out, before it's applied or held.
struct Staged {
    c: Change,
    /// Why only the user can accept it (a template, code that runs, a job set to hold).
    why: Option<String>,
    /// The page as it was, for a whole-page rewrite to be checked against when it's accepted.
    current: Option<String>,
    scheduled: bool,
}

/// Changes that belong together (an Inbox item clarified: a task added to a project and taken out
/// of the To Do list), each worked out on the page as the ones before it leave it. Held together
/// when any of them is held, else applied one after another. Each is recorded in Changes.
pub fn submit_many(app: &AppHandle, list: Vec<Submit>) -> Res<Vec<Outcome>> {
    if list.is_empty() {
        return Err(invalid("No change given."));
    }
    let root = root(app)?;
    let read_only = writable(&app.state::<AppState>()).is_err();
    // The pages as the changes before leave them.
    let mut pages: std::collections::HashMap<String, Option<String>> = std::collections::HashMap::new();
    let mut staged = Vec::new();
    for mut s in list {
        spelled(&root, &mut s.page, &mut s.instruction)?;
        if matches!(s.instruction, Instruction::Image) {
            return Err(invalid("An image is added with add_image."));
        }
        let scheduled = changes::scheduled(&s.origin);
        if read_only && !scheduled {
            writable(&app.state::<AppState>())?;
        }
        let mut c = Change::new(&s.page, s.kind, &s.title, &s.reason, s.origin, s.model, s.instruction);
        c.quotes = s.quotes;
        c.warnings = s.warnings;
        c.flags = s.flags;
        c.claims = s.claims;
        let current = match pages.get(&c.page) {
            Some(t) => t.clone(),
            None => read_opt(&root.join(&c.page)),
        };
        if read_only {
            c.flags.push("Brainstead was read-only when it came.".into());
        }
        if c.instruction.moves() {
            let restore = matches!(c.instruction, Instruction::Restore { .. });
            if restore && root.join(&c.page).exists() {
                return Err(invalid(format!("There's already a file at {}. Restore it under another name.", c.page)));
            }
            if !restore && current.is_none() {
                return Err(invalid(format!("{} isn't in the vault.", c.page)));
            }
            let why = s.hold.or(move_needs_the_user(app, &c)?);
            pages.insert(c.page.clone(), if restore { restored_text(app, &c) } else { None });
            staged.push(Staged { c, why, current: None, scheduled });
            continue;
        }
        if c.kind == Kind::New && current.is_some() {
            return Err(invalid(format!("There's already a page at {}.", c.page)));
        }
        let after = preview(&c, current.as_deref()).map_err(invalid)?;
        if current.as_deref().map(|t| t.replace("\r\n", "\n")) == Some(after.replace("\r\n", "\n")) {
            return Err(invalid(format!("That changes nothing in {}.", brainstead_core::filename::stem(&c.page))));
        }
        let checks = checks(app, &c, current.as_deref(), &after);
        c.flags.extend(checks);
        // Code that runs, or a template: held whoever started it.
        let why = s.hold.or_else(|| changes::needs_the_user(&c.page, current.as_deref(), &after));
        pages.insert(c.page.clone(), Some(after));
        staged.push(Staged { c, why, current, scheduled });
    }
    let held = staged.iter().any(|x| x.why.is_some() || (x.scheduled && !x.c.flags.is_empty()));
    let together = staged.len() > 1;
    staged
        .into_iter()
        .map(|mut x| {
            if held {
                if together && x.why.is_none() && x.c.flags.is_empty() {
                    x.c.flags.push("Held with the rest of this change, which belongs with it.".into());
                }
                hold(app, x.c, x.why, x.current.as_deref())
            } else {
                apply(app, &root, x.c, None)
            }
        })
        .collect()
}

/// The change's page, and a rename's new path, as the file system spells them
/// (`paths::vault_rel`), so the rules see `Templates/Meeting.md` however an agent named it:
/// `templates\meeting.md` or `TEMPLA~1/Meeting.md` on Windows. Refuses a path that isn't in the
/// vault, or on Windows one Win32 would read as another name (`Templates./Meeting.md`).
fn spelled(root: &Path, page: &mut String, instruction: &mut Instruction) -> Res<()> {
    *page = paths::vault_rel(root, page).map_err(invalid)?;
    if let Instruction::Rename { to } = instruction {
        *to = paths::vault_rel_new(root, to).map_err(invalid)?;
    }
    Ok(())
}

/// Why a rename, a move to the Trash or a restore waits for the user: a template, or links
/// rewritten in one; a restore, as a new page's text is checked (into `Templates/`, or code that runs).
fn move_needs_the_user(app: &AppHandle, c: &Change) -> Res<Option<String>> {
    Ok(match &c.instruction {
        Instruction::Rename { to } => {
            let plan = tauri::async_runtime::block_on(crate::notes::rename_preview(app.clone(), c.page.clone(), to.clone()))?;
            changes::move_needs_the_user(&c.page, Some(to), plan.changes.iter().map(|l| l.path.as_str()))
        }
        Instruction::Restore { id } => {
            brainstead_core::trash::entry_file(&root(app)?, id).map_err(EditError::from)?;
            changes::needs_the_user(&c.page, None, &restored_text(app, c).unwrap_or_default())
        }
        _ => changes::move_needs_the_user(&c.page, None, []),
    })
}

/// A restore's file as it lies in the Trash, as text.
fn restored_text(app: &AppHandle, c: &Change) -> Option<String> {
    let Instruction::Restore { id } = &c.instruction else { return None };
    let (_, file) = brainstead_core::trash::entry_file(&root(app).ok()?, id).ok()?;
    read_opt(&file)
}

/// The page's text once the change is made: the instruction run on it, with the assistant's mark
/// on a new note.
fn preview(c: &Change, current: Option<&str>) -> Result<String, String> {
    let after = c.instruction.text(&c.page, current)?;
    if c.kind == Kind::New && proposals::gets_mark(&c.page) {
        return write::with_property(&after, proposals::ASSISTANT_MARK.0, Some(proposals::ASSISTANT_MARK.1)).map_err(|e| e.to_string());
    }
    Ok(after)
}

/// Every check: the text's (core), and the app's own (unsaved edits, a contradiction).
fn checks(app: &AppHandle, c: &Change, before: Option<&str>, after: &str) -> Vec<String> {
    let mut out = changes::checks(&Facts { page: &c.page, before, after: Some(after), quotes: &c.quotes });
    if app.state::<brainstead_core::drafts::Drafts>().read(&c.page).is_some() {
        out.push("The page is open in Brainstead with unsaved edits.".into());
    }
    if c.origin.kind != "contradiction" && crate::contradict::open_contradiction(&c.page) {
        out.push("The contradiction check found this page contradicts another.".into());
    }
    out
}

fn hold(app: &AppHandle, mut c: Change, why: Option<String>, current: Option<&str>) -> Res<Outcome> {
    c.for_user = why.clone();
    c.flags.extend(why);
    c.status = Status::Held;
    // A whole-page rewrite keeps the page as it was, so accepting it can tell whether it's changed.
    if c.instruction.whole_page() {
        c.before = current.map(|t| store().put_text(t)).transpose()?;
    }
    store().save(&c)?;
    emit_changed(app);
    let message = format!("Held for the user in Changes: {}. {}", c.title, c.flags.join(" "));
    Ok(Outcome { id: c.id.clone(), applied: false, page: c.page.clone(), flags: c.flags.clone(), message })
}

/// Runs the change's instruction on the page as it is, writes it through the safe write with its
/// `log.md` line and a ⌘Z entry, and records it as applied. `base` is the page's version it must
/// still be at (a held whole-page rewrite's), else the write refuses.
fn apply(app: &AppHandle, root: &Path, mut c: Change, base: Option<String>) -> Res<Outcome> {
    writable(&app.state::<AppState>())?;
    let name = brainstead_core::filename::stem(&c.page).to_string();
    let message = match c.instruction.clone() {
        Instruction::Rename { to } => {
            let plan = tauri::async_runtime::block_on(crate::notes::rename_preview(app.clone(), c.page.clone(), to))?;
            let to = tauri::async_runtime::block_on(crate::notes::rename_commit(app.clone(), plan))?;
            c.to = Some(to.clone());
            format!("Renamed {} to {to}", c.page)
        }
        Instruction::Trash => {
            let e = tauri::async_runtime::block_on(crate::notes::trash_move(app.clone(), c.page.clone()))?;
            c.trash = Some(e.id);
            format!("Moved {} to the Trash", c.page)
        }
        Instruction::Restore { id } => {
            let to = tauri::async_runtime::block_on(crate::notes::trash_restore(app.clone(), id, Some(c.page.clone())))?;
            format!("Restored {to} from the Trash")
        }
        _ => {
            let abs = root.join(&c.page);
            let current = read_opt(&abs);
            let after = preview(&c, current.as_deref()).map_err(invalid)?;
            let version = match &current {
                Some(t) => {
                    let now = write::version(t.as_bytes());
                    if base.as_ref().is_some_and(|b| *b != now) {
                        return Err(invalid(format!(
                            "{name} has changed since this change was held, and it rewrites the whole page, so accepting it would write over those edits. Use Edit before accepting, or reject it."
                        )));
                    }
                    write::save_file(&abs, &after, base.as_deref().unwrap_or(&now))?
                }
                None => {
                    if let Some(d) = abs.parent() {
                        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
                    }
                    write::create_file(&abs, &after)?
                }
            };
            // The text as written: a CRLF page keeps its endings.
            let written_text = read_opt(&abs).unwrap_or(after);
            let st = store();
            c.before = current.as_deref().map(|t| st.put_text(t)).transpose()?;
            c.after = Some(st.put_text(&written_text)?);
            let mut files = vec![FileUndo { path: c.page.clone(), before: current, version }];
            let mut paths = vec![abs, root.join("log.md")];
            match write_claims(&st, root, &mut c) {
                Ok(Some(f)) => {
                    paths.push(root.join(&f.path));
                    files.push(f);
                }
                Ok(None) => {}
                Err(e) => crate::applog!("claims for {}: {}", c.page, e.message()),
            }
            if let Some(line) = log_line(&c) {
                match crate::edits::add_log(root, &line) {
                    Ok(f) => files.push(f),
                    Err(e) => crate::applog!("change log line: {e}"),
                }
            }
            let label = format!("{} {name}: {}", if c.kind == Kind::New { "Made" } else { "Changed" }, c.title);
            app.state::<UndoStack>().push(UndoEntry::files(label.clone(), files));
            written(app, &paths);
            label
        }
    };
    c.status = Status::Applied;
    c.decided = Some(proposals::now_local());
    store().save(&c)?;
    emit_changed(app);
    let mut message = format!("{message}. Revert it in Changes.");
    if !c.flags.is_empty() {
        message.push_str(&format!(" Flagged: {}", c.flags.join(" ")));
    }
    Ok(Outcome { id: c.id.clone(), applied: true, page: c.to.clone().unwrap_or(c.page.clone()), flags: c.flags.clone(), message })
}

/// An image an assistant added (add_image), already saved at `rel`: recorded as applied, with
/// Revert (to Brainstead's Trash). Never held: a new file in `images/` changes no page.
pub fn record_image(app: &AppHandle, rel: &str, reason: &str, origin: Origin, model: Option<String>) -> Res<Outcome> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let mut c = Change::new(rel, Kind::Image, &format!("Add image {name}"), reason, origin, model, Instruction::Image);
    c.status = Status::Applied;
    c.decided = Some(proposals::now_local());
    store().save(&c)?;
    emit_changed(app);
    let message = format!("Added {rel}. Embed it in a note with ![[{rel}]]. Revert it in Changes.");
    Ok(Outcome { id: c.id.clone(), applied: true, page: rel.to_string(), flags: vec![], message })
}

/// Writes the change's claims into its page's claims file (an ingest's set, or a source renamed),
/// recording the file before and after. Returns the file's ⌘Z entry when it changed and is still there.
fn write_claims(st: &Store, root: &Path, c: &mut Change) -> Res<Option<FileUndo>> {
    let page = c.page.clone();
    let (Some(u), Some(rel)) = (c.claims.as_mut(), brainstead_core::claims::path(&page)) else { return Ok(None) };
    let abs = root.join(&rel);
    let current = read_opt(&abs);
    let next = brainstead_core::claims::updated(current.as_deref(), &page, u);
    if next == current {
        return Ok(None);
    }
    u.before = current.as_deref().map(|t| st.put_text(t)).transpose()?;
    u.after = next.as_deref().map(|t| st.put_text(t)).transpose()?;
    brainstead_core::claims::put(&abs, next.as_deref()).map_err(|e| e.to_string())?;
    Ok(next.map(|t| FileUndo { path: rel, before: current, version: write::version(t.as_bytes()) }))
}

/// A reverted change's claims: each source it touched has its claims on the page put back as they
/// were before it, whatever other sources have added since, following a source renamed since.
/// Nothing when it didn't change the claims file.
fn revert_claims(st: &Store, root: &Path, c: &Change) {
    let (Some(u), Some(rel)) = (c.claims.as_ref(), brainstead_core::claims::path(&c.page)) else { return };
    if u.before.is_none() && u.after.is_none() {
        return;
    }
    let before = u.before.as_deref().and_then(|h| st.text(h));
    let abs = root.join(&rel);
    let current = read_opt(&abs);
    let next = brainstead_core::claims::reverted(before.as_deref(), current.as_deref(), &c.page, u);
    if next != current {
        if let Err(e) = brainstead_core::claims::put(&abs, next.as_deref()) {
            crate::applog!("claims for {} not reverted: {e}", c.page);
        }
    }
}

/// The start of a Reshape pages run's group.
pub const RESHAPE_RUN: &str = "reshape-";

/// The log line for an applied change: one per ingest run (with its first page), else one each.
fn log_line(c: &Change) -> Option<String> {
    let now = chrono::Local::now().naive_local();
    Some(match c.origin.kind.as_str() {
        "ingest" => return c.origin.run.as_deref().or(c.origin.chat.as_deref()).and_then(crate::ingest::take_log),
        "review" => reviews::log_entry("review", c.origin.label.as_deref().unwrap_or(&c.title), None, now),
        // Reshape pages and Write Current state write one line for their run.
        "lint" if c.origin.run.as_deref().is_some_and(|r| r.starts_with(RESHAPE_RUN) || r.starts_with(crate::currentstate::RUN)) => {
            return None
        }
        "lint" => reviews::log_entry("lint-fix", "Knowledge health", Some(&c.title), now),
        _ => reviews::log_entry(
            if c.kind == Kind::New { "create" } else { "update" },
            brainstead_core::filename::stem(&c.page),
            Some(&c.title),
            now,
        ),
    })
}

/// A run's applied changes, undone by its own Undo (a summary's): marked reverted.
pub fn mark_reverted(app: &AppHandle, group: &str) {
    let st = store();
    for mut c in st.list().into_iter().filter(|c| c.status == Status::Applied && c.group() == group) {
        c.status = Status::Reverted;
        c.decided = Some(proposals::now_local());
        let _ = st.save(&c);
    }
    emit_changed(app);
}

// ── The Changes screen ────────────────────────────────────────────────────────

/// A change in the feed: everything but the page's text.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    id: String,
    created: String,
    origin: Origin,
    model: Option<String>,
    page: String,
    kind: Kind,
    title: String,
    reason: String,
    status: Status,
    decided: Option<String>,
    flags: Vec<String>,
    warnings: Vec<String>,
    to: Option<String>,
    /// The run it belongs to.
    group: String,
    /// Has the texts Revert needs (a move is reverted by moving it back).
    revertable: bool,
}

fn row(c: &Change) -> Row {
    Row {
        id: c.id.clone(),
        created: c.created.clone(),
        origin: c.origin.clone(),
        model: c.model.clone(),
        page: c.page.clone(),
        kind: c.kind,
        title: c.title.clone(),
        reason: c.reason.clone(),
        status: c.status,
        decided: c.decided.clone(),
        flags: c.flags.clone(),
        warnings: c.warnings.clone(),
        to: c.to.clone(),
        group: c.group().to_string(),
        revertable: c.status == Status::Applied && (c.after.is_some() || c.instruction.moves()),
    }
}

/// One change opened: its diff (for a held one, what it would do to the page as it is now, or why
/// it can't any more) and its quotes.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    #[serde(flatten)]
    row: Row,
    quotes: Vec<Quote>,
    changes: Vec<changes::Hunk>,
    /// A held change that can't be applied any more, and why.
    problem: Option<String>,
}

fn view(app: &AppHandle, id: &str) -> Res<View> {
    let st = store();
    let c = st.get(id)?;
    let (mut hunks, mut problem) = (vec![], None);
    if c.status == Status::Held && matches!(c.instruction, Instruction::Restore { .. }) {
        // What a held restore would put back.
        match restored_text(app, &c) {
            Some(t) => hunks = changes::hunks("", &t),
            None => problem = Some("That's no longer in the Trash.".into()),
        }
    } else if !c.instruction.moves() {
        if c.status == Status::Held {
            let current = read_opt(&root(app)?.join(&c.page));
            match preview(&c, current.as_deref()) {
                Ok(after) => hunks = changes::hunks(current.as_deref().unwrap_or(""), &after),
                Err(e) => problem = Some(e),
            }
        } else if let Some(a) = c.after.as_deref().and_then(|h| st.text(h)) {
            let b = c.before.as_deref().and_then(|h| st.text(h)).unwrap_or_default();
            hunks = changes::hunks(&b, &a);
        } else if c.status == Status::Rejected {
            // A rejected change was never applied: what it asked for, against nothing.
            if let Ok(a) = c.instruction.text(&c.page, Some("")) {
                hunks = changes::hunks("", &a);
            }
        }
    }
    Ok(View { row: row(&c), quotes: c.quotes.clone(), changes: hunks, problem })
}

/// Deciding on a change (accept, reject, revert, edit before accepting), one at a time: two at
/// once could both find it waiting, or applied, and act on it twice.
static DECIDING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Accepts a held change. `assistant`: an assistant asked (the MCP changes tool), which may accept
/// only a change held because a check failed (D-20261005-09), never one only the user can accept.
fn accept_one(app: &AppHandle, id: &str, assistant: bool) -> Res<Outcome> {
    let _one = crate::lock(&DECIDING);
    let c = store().get(id)?;
    if c.status != Status::Held {
        return Err(invalid("That change isn't waiting any more."));
    }
    let root = root(app)?;
    if c.kind == Kind::New && root.join(&c.page).exists() {
        return Err(invalid(format!("There's already a page at {}.", c.page)));
    }
    if assistant {
        if let Some(why) = for_the_user(app, &root, &c)? {
            return Err(invalid(format!(
                "Only the user can accept “{}”, on Brainstead's Changes screen: {why} Ask them to look at it there.",
                c.title
            )));
        }
    }
    let base = if c.instruction.whole_page() && root.join(&c.page).exists() {
        Some(c.before.clone().ok_or_else(|| {
            invalid(format!(
                "“{}” rewrites the whole of {}, and the page as it was when it was held wasn't kept, so accepting it could write over later edits. Use Edit before accepting, or reject it.",
                c.title,
                brainstead_core::filename::stem(&c.page)
            ))
        })?)
    } else {
        None
    };
    apply(app, &root, c, base)
}

/// Why only the user can accept a held change: what it was held for, or, for one held before that
/// was kept, what it would do to the page as it is now.
fn for_the_user(app: &AppHandle, root: &Path, c: &Change) -> Res<Option<String>> {
    if c.for_user.is_some() {
        return Ok(c.for_user.clone());
    }
    // By the file's own spelling, as `submit_many` decides.
    let mut c = c.clone();
    spelled(root, &mut c.page, &mut c.instruction)?;
    let c = &c;
    if c.instruction.moves() {
        return move_needs_the_user(app, c);
    }
    let current = read_opt(&root.join(&c.page));
    let after = preview(c, current.as_deref()).map_err(invalid)?;
    Ok(changes::needs_the_user(&c.page, current.as_deref(), &after))
}

fn reject_one(app: &AppHandle, id: &str) -> Res<()> {
    let _one = crate::lock(&DECIDING);
    let st = store();
    let mut c = st.get(id)?;
    if c.status != Status::Held {
        return Err(invalid("That change isn't waiting any more."));
    }
    c.status = Status::Rejected;
    c.decided = Some(proposals::now_local());
    st.save(&c)?;
    emit_changed(app);
    Ok(())
}

/// Every held change in a run, oldest first. Accepting them one after another runs each on the
/// page as the one before left it.
fn held_in(group: &str) -> Vec<Change> {
    let mut v: Vec<Change> = store().list().into_iter().filter(|c| c.status == Status::Held && c.group() == group).collect();
    v.reverse();
    v
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Many {
    done: usize,
    /// Those that couldn't be applied, and why.
    failed: Vec<String>,
}

/// What Revert did: put back, or why it couldn't and the page as it was before, to copy from.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reverted {
    ok: bool,
    message: String,
    before: Option<String>,
}

/// Undoes an applied change on the page as it is now: its lines put back, wherever they are. When
/// they've been edited since, it says so and gives the page as it was before the change.
/// The transcript a meeting run's new note was made from, when this change is that note.
fn meeting_transcript(c: &Change) -> Option<&str> {
    (c.origin.kind == "meeting" && c.kind == Kind::New).then_some(c.origin.chat.as_deref()).flatten()
}

fn revert_one(app: &AppHandle, id: &str) -> Res<Reverted> {
    writable(&app.state::<AppState>())?;
    let _one = crate::lock(&DECIDING);
    let st = store();
    let mut c = st.get(id)?;
    if c.status != Status::Applied {
        return Err(invalid("Only an applied change can be reverted."));
    }
    let root = root(app)?;
    let name = brainstead_core::filename::stem(&c.page).to_string();
    let message = match c.instruction.clone() {
        Instruction::Rename { .. } => {
            let to = c.to.clone().ok_or_else(|| invalid("This rename says nowhere it went."))?;
            let plan = tauri::async_runtime::block_on(crate::notes::rename_preview(app.clone(), to.clone(), c.page.clone()))?;
            tauri::async_runtime::block_on(crate::notes::rename_commit(app.clone(), plan))?;
            format!("Renamed {to} back to {}", c.page)
        }
        Instruction::Trash => {
            let id = c.trash.clone().ok_or_else(|| invalid("This move to the Trash has no Trash entry to restore."))?;
            tauri::async_runtime::block_on(crate::notes::trash_restore(app.clone(), id, None))?;
            format!("Restored {name} from the Trash")
        }
        Instruction::Restore { .. } => {
            let e = tauri::async_runtime::block_on(crate::notes::trash_move(app.clone(), c.page.clone()))?;
            c.trash = Some(e.id);
            format!("Moved {name} back to the Trash")
        }
        Instruction::Image => {
            if !root.join(&c.page).is_file() {
                return Ok(Reverted { ok: false, message: format!("{} isn't in the vault any more.", c.page), before: None });
            }
            let e = tauri::async_runtime::block_on(crate::notes::trash_move(app.clone(), c.page.clone()))?;
            c.trash = Some(e.id);
            format!("Moved {} to the Trash", c.page)
        }
        _ => {
            let after = c.after.as_deref().and_then(|h| st.text(h)).ok_or_else(|| invalid("This change's history is gone."))?;
            let before = c.before.as_deref().and_then(|h| st.text(h));
            let abs = root.join(&c.page);
            let Some(current) = read_opt(&abs) else {
                return Ok(Reverted { ok: false, message: format!("{name} isn't in the vault any more."), before });
            };
            let undo = match &before {
                Some(b) => changes::revert_text(b, &after, &current),
                // A page it made: still as it made it, it goes to the Trash.
                None => (current.replace("\r\n", "\n") == after.replace("\r\n", "\n")).then(String::new),
            };
            let Some(text) = undo else {
                return Ok(Reverted {
                    ok: false,
                    message: format!("The lines this change made in {name} have been edited since, so it can't be undone line by line. Here's the page as it was before it, to copy from."),
                    before,
                });
            };
            // Its ⌘Z entry would undo it a second time.
            let made = c.after.clone();
            app.state::<UndoStack>().forget(|e| e.files.iter().any(|f| f.path == c.page && Some(&f.version) == made.as_ref()));
            match &before {
                Some(_) => {
                    let v = write::save_file(&abs, &text, &write::version(current.as_bytes()))?;
                    app.state::<UndoStack>().push(UndoEntry::files(
                        format!("Reverted {name}: {}", c.title),
                        vec![FileUndo { path: c.page.clone(), before: Some(current), version: v }],
                    ));
                }
                None => {
                    brainstead_core::trash::undo_created(&root, &c.page, "note").map_err(EditError::from)?;
                }
            }
            revert_claims(&st, &root, &c);
            written(app, &[abs]);
            format!("Reverted {name}: {}", c.title)
        }
    };
    c.status = Status::Reverted;
    c.decided = Some(proposals::now_local());
    st.save(&c)?;
    // A summary's block reverted here is its run undone, as Recent runs' Undo leaves it.
    if let Some(run) = c.origin.run.as_deref().filter(|r| c.origin.kind == "review" && r.starts_with("summary-")) {
        crate::reviews::reverted_in_changes(app, run);
    }
    // A meeting note reverted: its transcript, if it wasn't trashed, is back in the Inbox to make
    // the note again (src/meetingFlow.ts took it out once the note was made).
    if let Some(t) = meeting_transcript(&c).filter(|t| root.join(t).is_file()) {
        crate::gtd::forget_clarified(t);
    }
    emit_changed(app);
    Ok(Reverted { ok: true, message, before: None })
}

/// "Edit before accepting": the page with the held change made becomes its draft, which the editor
/// offers to restore when it opens; saving there is the write. Refused when the page already has
/// unsaved edits, which the draft would replace. The change keeps the page before and the text it
/// offered, so Revert can still take its lines out once it's saved. Returns the page.
fn for_editing(app: &AppHandle, id: &str) -> Res<String> {
    let _one = crate::lock(&DECIDING);
    let st = store();
    let mut c = st.get(id)?;
    if c.status != Status::Held {
        return Err(invalid("That change isn't waiting any more."));
    }
    let Some(current) = read_opt(&root(app)?.join(&c.page)) else {
        return Err(invalid("The page doesn't exist yet: accept it, then edit it."));
    };
    let drafts = app.state::<brainstead_core::drafts::Drafts>();
    if drafts.read(&c.page).is_some_and(|d| d.content.replace("\r\n", "\n") != current.replace("\r\n", "\n")) {
        return Err(invalid(format!(
            "{} has unsaved edits in Brainstead's editor. Save or discard them there first, then Edit before accepting.",
            brainstead_core::filename::stem(&c.page)
        )));
    }
    let content = preview(&c, Some(&current)).map_err(invalid)?;
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    c.before = Some(st.put_text(&current)?);
    c.after = Some(st.put_text(&content)?);
    drafts.write(&brainstead_core::drafts::Draft { path: c.page.clone(), content, base: write::version(current.as_bytes()), at })?;
    // Yours from here: the editor's save is the write. Revert takes out the lines it offered,
    // where they still are.
    c.status = Status::Applied;
    c.decided = Some(proposals::now_local());
    c.flags.push("Opened to edit before accepting: it's made when the page is saved in the editor.".into());
    st.save(&c)?;
    emit_changed(app);
    Ok(c.page)
}

/// How long history is kept: Settings › AI assistants.
fn retention(app: &AppHandle) -> (i64, u64) {
    let s = app.state::<AppState>();
    let ui = &crate::lock(&s.settings).ui;
    let days = ui.get("changesKeepDays").and_then(|v| v.as_i64()).filter(|d| *d > 0).unwrap_or(changes::KEEP_DAYS);
    let mb = ui.get("changesKeepMb").and_then(|v| v.as_u64()).filter(|m| *m > 0).unwrap_or(changes::KEEP_MB);
    (days, mb)
}

/// Once the vault is open: the old review queue moved in (once), and history past its time or size
/// pruned, then again each day.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        let (days, mb) = retention(&app);
        let today = chrono::Local::now().date_naive();
        if let Some(root) = app.state::<VaultService>().root() {
            match changes::migrate(&platform::data_dir(), &root, today, days) {
                Ok(0) => {}
                Ok(n) => {
                    crate::applog!("changes: moved {n} from the old review queue");
                    emit_changed(&app);
                }
                Err(e) => crate::applog!("changes: the old review queue wasn't moved: {e}"),
            }
        }
        if store().prune(today, days, mb * 1024 * 1024) > 0 {
            emit_changed(&app);
        }
        std::thread::sleep(std::time::Duration::from_secs(if app.state::<VaultService>().root().is_some() { 24 * 3600 } else { 30 }));
    });
}

// The window's commands, each off the main thread.

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| EditError::from(e.to_string()))?
}

/// The feed and the held changes, newest first; with `page`, only that page's.
#[tauri::command]
pub async fn changes_list(page: Option<String>) -> Res<Vec<Row>> {
    blocking(move || {
        Ok(store().list().iter().filter(|c| page.as_ref().is_none_or(|p| &c.page == p || c.to.as_ref() == Some(p))).map(row).collect())
    })
    .await
}

#[tauri::command]
pub async fn change_get(app: AppHandle, id: String) -> Res<View> {
    blocking(move || view(&app, &id)).await
}

#[tauri::command]
pub async fn change_submit(app: AppHandle, change: Submit) -> Res<Outcome> {
    blocking(move || submit(&app, change)).await
}

#[tauri::command]
pub async fn change_accept(app: AppHandle, id: String, assistant: Option<bool>) -> Res<Outcome> {
    blocking(move || accept_one(&app, &id, assistant == Some(true))).await
}

/// Changes that belong together, made or held together (src/mcpActions.ts's task, Inbox and
/// project tools).
#[tauri::command]
pub async fn changes_submit_many(app: AppHandle, changes: Vec<Submit>) -> Res<Vec<Outcome>> {
    blocking(move || submit_many(&app, changes)).await
}

#[tauri::command]
pub async fn change_reject(app: AppHandle, id: String) -> Res<()> {
    blocking(move || reject_one(&app, &id)).await
}

/// Of these changes, only those `ids` names when it names any: a run's Accept all, Reject all or
/// Revert all while Changes' search is on acts on the changes the search shows, no others.
fn only(todo: Vec<Change>, ids: &Option<Vec<String>>) -> Vec<Change> {
    match ids {
        Some(ids) => todo.into_iter().filter(|c| ids.contains(&c.id)).collect(),
        None => todo,
    }
}

/// Accepts every held change in a run (`group`), or every held change when None; only those in
/// `ids` when given.
#[tauri::command]
pub async fn changes_accept_all(app: AppHandle, group: Option<String>, assistant: Option<bool>, ids: Option<Vec<String>>) -> Res<Many> {
    blocking(move || {
        let todo: Vec<Change> = match &group {
            Some(g) => held_in(g),
            None => store().list().into_iter().rev().filter(|c| c.status == Status::Held).collect(),
        };
        let todo = only(todo, &ids);
        let mut m = Many { done: 0, failed: vec![] };
        for c in todo {
            match accept_one(&app, &c.id, assistant == Some(true)) {
                Ok(_) => m.done += 1,
                Err(e) => m.failed.push(format!("{}: {}", c.title, e.message())),
            }
        }
        Ok(m)
    })
    .await
}

#[tauri::command]
pub async fn changes_reject_all(app: AppHandle, group: Option<String>, ids: Option<Vec<String>>) -> Res<Many> {
    blocking(move || {
        let todo: Vec<Change> = match &group {
            Some(g) => held_in(g),
            None => store().list().into_iter().filter(|c| c.status == Status::Held).collect(),
        };
        let todo = only(todo, &ids);
        let mut m = Many { done: 0, failed: vec![] };
        for c in todo {
            reject_one(&app, &c.id)?;
            m.done += 1;
        }
        Ok(m)
    })
    .await
}

/// Reverts every change a run made (only those in `ids` when given), newest first; those whose
/// lines have been edited since are listed in `failed`, the rest still reverted.
pub fn revert_run(app: &AppHandle, group: &str, ids: &Option<Vec<String>>) -> Res<Many> {
    let todo = only(store().list().into_iter().filter(|c| c.status == Status::Applied && c.group() == group).collect(), ids);
    if todo.is_empty() {
        return Err(invalid("Nothing that run made is left to revert."));
    }
    let mut m = Many { done: 0, failed: vec![] };
    for c in todo {
        match revert_one(app, &c.id) {
            Ok(r) if r.ok => m.done += 1,
            Ok(r) => m.failed.push(format!("{}: {}", c.title, r.message)),
            Err(e) => m.failed.push(format!("{}: {}", c.title, e.message())),
        }
    }
    Ok(m)
}

#[tauri::command]
pub async fn changes_revert_all(app: AppHandle, group: String, ids: Option<Vec<String>>) -> Res<Many> {
    blocking(move || revert_run(&app, &group, &ids)).await
}

#[tauri::command]
pub async fn change_revert(app: AppHandle, id: String) -> Res<Reverted> {
    blocking(move || revert_one(&app, &id)).await
}

#[tauri::command]
pub async fn change_for_editing(app: AppHandle, id: String) -> Res<String> {
    blocking(move || for_editing(&app, &id)).await
}

// ── Working out an assistant's task and project edits, without writing ─────────────────────────
// The MCP tools' task, Inbox and project edits go through Changes (D-20261005-10): the window works
// out the new lines with these and src/tasksq/edits.ts, then submits them as instructions.

/// One edit to a task's line, as the task screens make it.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum LineEdit {
    /// Ticked (with `today`'s ✅) or unticked; a recurring task's next occurrence is the window's.
    Toggle {
        done: bool,
        today: String,
    },
    Date {
        kind: write::DateKind,
        date: Option<String>,
    },
    Contexts {
        names: Vec<String>,
    },
    Effort {
        effort: Option<String>,
    },
    /// Linked to a project note (its vault path), in place of any project it links; None unlinks.
    Project {
        project: Option<String>,
    },
    /// Ranked between two neighbours in the manual order.
    Rank {
        prev: Option<i64>,
        next: Option<i64>,
    },
}

/// The task's line once `edit` is made: what the matching task command would write.
#[tauri::command]
pub async fn change_task_line(app: AppHandle, line: String, edit: LineEdit) -> Res<String> {
    blocking(move || {
        Ok(match edit {
            LineEdit::Toggle { done, today } => {
                if chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d").is_err() {
                    return Err(invalid(format!("Not a date: {today}")));
                }
                write::toggled(&line, done, &today)
            }
            LineEdit::Date { kind, date } => write::with_date(&line, kind, date.as_deref())?,
            LineEdit::Contexts { names } => write::with_contexts(&line, &names)?,
            LineEdit::Effort { effort } => write::with_effort(&line, effort.as_deref().filter(|e| !e.trim().is_empty()))?,
            LineEdit::Rank { prev, next } => {
                let rank = write::rank_in_gap(prev, next).ok_or_else(|| {
                    invalid("There's no room between those two in the manual order; give tasks that are next to each other, or drag it in the Tasks screen.")
                })?;
                write::with_rank(&line, rank)
            }
            LineEdit::Project { project } => {
                use brainstead_core::projects;
                if let Some(p) = &project {
                    if !projects::is_project_path(p) {
                        return Err(invalid(format!("Not a project note: {p}")));
                    }
                }
                // Which of the line's links go to a project note, by the index (as task_set_project).
                let targets: Vec<String> = brainstead_core::links::parse_links(&brainstead_core::markdown::lines(&line, 0))
                    .into_iter()
                    .map(|l| l.target)
                    .filter(|t| !t.is_empty())
                    .collect();
                let resolved = app.state::<VaultService>().with_index(|ix, _| ix.resolve(&targets))?;
                let to_project: Vec<String> = targets
                    .into_iter()
                    .zip(resolved)
                    .filter(|(t, r)| r.as_deref().is_some_and(projects::is_project_path) || projects::is_project_path(&format!("{t}.md")))
                    .map(|(t, _)| t)
                    .collect();
                let stem = project.as_deref().map(|p| brainstead_core::filename::stem(p).to_string());
                write::with_project_link(&line, stem.as_deref(), |t| to_project.iter().any(|x| x == &brainstead_core::links::nfc(t)))
            }
        })
    })
    .await
}

/// A new project note, as the Projects screen makes it: its path and text.
#[derive(Serialize)]
pub struct ProjectNote {
    path: String,
    content: String,
}

#[tauri::command]
pub fn change_project_note(name: String, status: String, area: Option<String>, outcome: Option<String>) -> Res<ProjectNote> {
    use brainstead_core::projects;
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() || n.starts_with('.') || n.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '[', ']', '#', '^']) {
        return Err(invalid(format!("“{name}” can't be a project name: leave out / \\ : * ? \" < > | [ ] # ^")));
    }
    let status = projects::parse_status(&status).ok_or_else(|| invalid(format!("Not a project status: {status}")))?;
    if area.iter().chain(outcome.iter()).any(|v| v.contains(['\n', '\r'])) {
        return Err(invalid("Area and outcome are one line each."));
    }
    Ok(ProjectNote {
        path: format!("{}{n}.md", projects::PREFIX),
        content: projects::project_note(status, area.as_deref(), outcome.as_deref()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use brainstead_core::claims::{self, Kept, Update};

    fn kept(value: &str, source: &str) -> Kept {
        Kept { subject: "Orbit App".into(), attribute: "owner".into(), value: value.into(), source: source.into(), ..Default::default() }
    }

    #[test]
    fn a_reverted_meeting_note_names_its_transcript() {
        let t = "sources/Teams. Transcript. Steerco - 2026-10-02.md";
        let origin = Origin { kind: "meeting".into(), chat: Some(t.into()), ..Default::default() };
        let note =
            Change::new("Meeting. Steerco - 2026-10-02.md", Kind::New, "t", "", origin.clone(), None, changes::from_texts(None, "x\n"));
        assert_eq!(meeting_transcript(&note), Some(t));
        // Not an edit a meeting run made, nor another run's new page.
        let edit = Change::new("wiki/x.md", Kind::Edit, "t", "", origin, None, changes::from_texts(Some("a\n"), "b\n"));
        assert_eq!(meeting_transcript(&edit), None);
        let other = Change::new("x.md", Kind::New, "t", "", Origin::default(), None, changes::from_texts(None, "x\n"));
        assert_eq!(meeting_transcript(&other), None);
    }

    /// A change to `page` carrying `u`, as `submit` records it.
    fn change(page: &str, u: Update) -> Change {
        let mut c = Change::new(page, Kind::Edit, "t", "", Origin::default(), None, changes::from_texts(Some("a\n"), "b\n"));
        c.claims = Some(u);
        c
    }

    #[test]
    fn a_source_renamed_by_a_change_is_recorded_and_reverted_with_it() {
        let (d, t) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (st, root) = (Store::new(d.path()), t.path());
        let page = "wiki/entities/Orbit App.md";
        let abs = root.join(claims::path(page).unwrap());
        let before = claims::with_source(None, page, "sources/Steerco.md", &[kept("Maya", "sources/Steerco.md")]).unwrap();
        claims::put(&abs, Some(&before)).unwrap();
        let u = Update { source: "wiki/entities/Steerco.md".into(), from: Some("Steerco.md".into()), ..Default::default() };
        let mut c = change(page, u);
        let undo = write_claims(&st, root, &mut c).unwrap().unwrap();
        assert_eq!(undo.before.as_deref(), Some(before.as_str()));
        let now = std::fs::read_to_string(&abs).unwrap();
        assert_eq!(claims::parse(&now).claims[0].source, "wiki/entities/Steerco.md");
        // Kept with the change: the file before and after.
        let u = c.claims.as_ref().unwrap();
        assert_eq!((st.text(u.before.as_deref().unwrap()), st.text(u.after.as_deref().unwrap())), (Some(before.clone()), Some(now)));
        revert_claims(&st, root, &c);
        assert_eq!(std::fs::read_to_string(&abs).unwrap(), before);
    }

    #[test]
    fn reverting_an_ingest_after_its_source_was_renamed() {
        let (d, t) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (st, root) = (Store::new(d.path()), t.path());
        let page = "wiki/entities/Orbit App.md";
        let abs = root.join(claims::path(page).unwrap());
        let a = "sources/Steerco.md";
        let mut c = change(page, Update { source: a.into(), claims: vec![kept("Maya", a)], ..Default::default() });
        write_claims(&st, root, &mut c).unwrap();
        // The source renamed since (a rename's claims step).
        let to = "sources/Steerco notes.md";
        for (p, _, after) in claims::on_rename(root, a, to) {
            claims::put(&p, after.as_deref()).unwrap();
        }
        assert_eq!(claims::parse(&std::fs::read_to_string(&abs).unwrap()).claims[0].source, to);
        // Its revert takes them out under the new name: nothing left, the file goes.
        revert_claims(&st, root, &c);
        assert!(!abs.exists());
        // A change that didn't touch the claims file leaves it alone when reverted.
        let other = claims::with_source(None, page, "b.md", &[kept("Lena", "b.md")]).unwrap();
        claims::put(&abs, Some(&other)).unwrap();
        let mut c = change(page, Update { source: "b.md".into(), claims: vec![kept("Lena", "b.md")], ..Default::default() });
        assert!(write_claims(&st, root, &mut c).unwrap().is_none());
        revert_claims(&st, root, &c);
        assert_eq!(std::fs::read_to_string(&abs).unwrap(), other);
    }

    #[test]
    fn a_change_is_ruled_on_by_the_files_own_path() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        std::fs::create_dir_all(root.join("Templates")).unwrap();
        std::fs::write(root.join("Templates/Meeting.md"), "# Meeting\n").unwrap();
        let spell = |page: &str, mut i: Instruction| {
            let mut page = page.to_string();
            spelled(root, &mut page, &mut i).map(|_| (page, i)).map_err(|e| e.message().to_string())
        };
        let mut names = vec!["Templates/Meeting.md"];
        if root.join("TEMPLATES/MEETING.MD").is_file() {
            names.push("templates/MEETING.md");
        }
        if cfg!(windows) {
            names.extend(["Templates\\Meeting.md", "templates\\meeting.md"]);
        }
        for n in names {
            let (page, _) = spell(n, Instruction::Trash).unwrap();
            assert_eq!(page, "Templates/Meeting.md", "{n}");
            assert!(changes::needs_the_user(&page, Some("a"), "b").is_some(), "{n}");
            // A rename into the folder, however it's spelled, keeps the name it's given.
            let folder = n.rsplit_once(['/', '\\']).unwrap().0;
            let (_, i) = spell("Idea. A.md", Instruction::Rename { to: format!("{folder}/Idea. A.md") }).unwrap();
            assert_eq!(i, Instruction::Rename { to: "Templates/Idea. A.md".into() }, "{n}");
        }
        // Another name Win32 reads for the template, and paths out of the vault, are refused.
        let bad: &[&str] =
            if cfg!(windows) { &["Templates./Meeting.md", "Templates/Meeting.md.", "../x.md"] } else { &["../x.md", "/x.md"] };
        for n in bad {
            assert!(spell(n, Instruction::Trash).unwrap_err().contains("Not a path in the vault"), "{n}");
        }
    }
}
