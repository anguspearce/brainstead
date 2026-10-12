// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window's edits to the vault: ticking tasks, their dates and order, capture, and undo (of
//! these and of the GTD screens' edits in `gtd.rs`). All refused while Settings › Vault › Read-only is on. After each write the file is re-indexed at
//! once (the watcher can't tell when a write keeps the size and modification time).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use brainstead_core::write::{self, DateKind, TaskRef, WriteError};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::vault::VaultService;
use crate::AppState;

pub const TODO_LIST: &str = "Me. To Do List.md";
pub const SCRATCHPAD: &str = "Me. Scratchpad.md";

/// An error the window can act on: `code` is read-only, stale, conflict, not-found, invalid, io,
/// changed (with `current`, the file as it is now) or exists.
#[derive(Debug, Serialize)]
pub struct EditError {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<Current>,
}

#[derive(Debug, Serialize)]
pub struct Current {
    content: String,
    version: String,
}

impl From<WriteError> for EditError {
    fn from(e: WriteError) -> Self {
        let message = e.to_string();
        let (code, current) = match e {
            WriteError::Stale(_) => ("stale", None),
            WriteError::Conflict(_) => ("conflict", None),
            WriteError::NotFound(_) => ("not-found", None),
            WriteError::Invalid(_) => ("invalid", None),
            WriteError::Io(_) => ("io", None),
            WriteError::Exists(_) => ("exists", None),
            WriteError::Changed { content, version } => ("changed", Some(Current { content, version })),
        };
        EditError { code, message, current }
    }
}

impl EditError {
    /// What went wrong, as the window shows it.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<String> for EditError {
    fn from(message: String) -> Self {
        EditError { code: "io", message, current: None }
    }
}

pub type Res<T> = Result<T, EditError>;

pub fn writable(st: &AppState) -> Res<()> {
    if crate::lock(&st.settings).read_only {
        return Err(EditError {
            code: "read-only",
            message: "Brainstead is set to read-only, so it didn't change the vault. Turn that off in Settings › Vault.".into(),
            current: None,
        });
    }
    Ok(())
}

/// Re-indexes the files just written (or moved away) and tells the window.
/// Adds `entry` (a `reviews::log_entry`) to the vault's `log.md`, newest first, creating it if
/// needed. Returns the undo record for it.
pub fn add_log(root: &Path, entry: &str) -> Result<FileUndo, String> {
    let log = root.join("log.md");
    let before = std::fs::read_to_string(&log).ok();
    let after = brainstead_core::reviews::log_insert(before.as_deref().unwrap_or(""), entry);
    let version = match &before {
        Some(b) => write::save_file(&log, &after, &write::version(b.as_bytes())),
        None => write::create_file(&log, &after),
    }
    .map_err(|e| e.to_string())?;
    Ok(FileUndo { path: "log.md".into(), before, version })
}

/// Vault writes from the window, one at a time and off the main thread: they wait on the index
/// lock (re-indexing what they wrote), and two at once could each read a file before the other's
/// write landed.
static WRITES: Mutex<()> = Mutex::new(());

pub(crate) async fn writing<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    crate::blocking(move || {
        let _one = crate::lock(&WRITES);
        f()
    })
    .await
}

pub fn written(app: &AppHandle, paths: &[PathBuf]) {
    // Knowledge health reads these again (a write that kept the file's time and size is otherwise
    // invisible to its cache), and reruns once the vault is quiet.
    brainstead_core::lint::forget(paths);
    crate::knowledge::vault_changed(app);
    let svc = app.state::<VaultService>();
    match svc.refresh(paths) {
        Ok(ch) if !ch.is_empty() => {
            let _ = app.emit("vault-changed", &ch);
        }
        Ok(_) => {}
        Err(e) => crate::applog!("re-index after write: {e}"),
    }
}

/// One undoable change: the line as it was and as it became, or whole files (`files`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoEntry {
    pub path: String,
    pub line: usize,
    pub before: String,
    pub after: String,
    /// What it was, for the toast: "Ticked “Ship it”".
    pub label: String,
    /// When the line became several (a recurring task's next occurrence) or none (deleted on
    /// completion): every line written, in place of `after`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written: Option<Vec<String>>,
    /// A change to one or two whole files (a project created, a task moved between notes): each
    /// put back as it was, only if every one is still as the change left it. `path`, `line`,
    /// `before` and `after` aren't used then.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileUndo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileUndo {
    /// Vault-relative.
    pub path: String,
    /// The file's text before; None when the change created it (undo deletes it).
    pub before: Option<String>,
    /// Its version after the change.
    pub version: String,
}

impl UndoEntry {
    /// An entry for whole-file changes, by vault path.
    pub fn files(label: String, files: Vec<FileUndo>) -> Self {
        let path = files.first().map(|f| f.path.clone()).unwrap_or_default();
        UndoEntry { path, line: 0, before: String::new(), after: String::new(), label, written: None, files }
    }
}

/// The last five task changes, kept in the app data folder so ⌘Z works after a restart.
pub struct UndoStack {
    path: PathBuf,
    entries: Mutex<Vec<UndoEntry>>,
}

const UNDO_MAX: usize = 5;

impl UndoStack {
    pub fn load(path: PathBuf) -> Self {
        let entries = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        UndoStack { path, entries: Mutex::new(entries) }
    }

    fn save(&self, e: &[UndoEntry]) {
        if let Ok(j) = serde_json::to_vec(e) {
            let _ = std::fs::write(&self.path, j);
        }
    }

    pub(crate) fn push(&self, entry: UndoEntry) {
        let mut e = crate::lock(&self.entries);
        // Undoing what the last change on this line did (a tick, then an untick) cancels out.
        let line = |x: &UndoEntry| x.files.is_empty();
        if let Some(i) =
            e.iter().rposition(|x| line(x) && line(&entry) && x.path == entry.path && x.after == entry.before && x.before == entry.after)
        {
            e.remove(i);
        } else {
            e.push(entry);
            let n = e.len();
            if n > UNDO_MAX {
                e.drain(..n - UNDO_MAX);
            }
        }
        self.save(&e);
    }

    /// Drops the entries `f` picks: a change undone another way, which ⌘Z can't undo again.
    pub(crate) fn forget(&self, f: impl Fn(&UndoEntry) -> bool) {
        let mut e = crate::lock(&self.entries);
        let n = e.len();
        e.retain(|x| !f(x));
        if e.len() != n {
            self.save(&e);
        }
    }

    fn pop(&self) -> Option<UndoEntry> {
        let mut e = crate::lock(&self.entries);
        let x = e.pop();
        self.save(&e);
        x
    }

    fn peek(&self) -> Option<UndoEntry> {
        crate::lock(&self.entries).last().cloned()
    }
}

pub(crate) fn task_label(line: &str) -> String {
    let t = brainstead_core::tasks::parse_line(line, 0).map(|t| t.text).unwrap_or_default();
    let t: String =
        t.split([' ']).filter(|w| !w.starts_with('#') && !["📅", "⏳", "✅", "➕", "🛫"].contains(w)).collect::<Vec<_>>().join(" ");
    if t.chars().count() > 60 {
        format!("{}…", t.chars().take(60).collect::<String>())
    } else {
        t
    }
}

pub(crate) fn vault_path(svc: &VaultService, rel: &str) -> Res<PathBuf> {
    Ok(svc.resolve_path(rel)?)
}

/// After a task edit: the line as written, and where it is now.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edited {
    pub(crate) line: usize,
    pub(crate) line_text: String,
    pub(crate) undo: Option<String>,
}

pub(crate) fn edit(
    app: &AppHandle,
    path: &str,
    line: usize,
    expected: &str,
    verb: &str,
    f: impl FnOnce(&str) -> Result<String, WriteError>,
) -> Res<Edited> {
    let st = app.state::<AppState>();
    writable(&st)?;
    let svc = app.state::<VaultService>();
    let abs = vault_path(&svc, path)?;
    let mut err = None;
    // `f` sees the task inside any quote or callout markers, which stay in front.
    let (i, new) = write::edit_task(&abs, line, expected, |l| {
        let (pre, body) = brainstead_core::tasks::split_quote(l);
        match f(body) {
            Ok(n) => format!("{pre}{n}"),
            Err(e) => {
                err = Some(e);
                l.to_string()
            }
        }
    })?;
    if let Some(e) = err {
        return Err(e.into());
    }
    let mut undo = None;
    if new != expected {
        let label = format!("{verb} “{}”", task_label(&new));
        app.state::<UndoStack>().push(UndoEntry {
            path: path.into(),
            line: i,
            before: expected.into(),
            after: new.clone(),
            label: label.clone(),
            written: None,
            files: Vec::new(),
        });
        undo = Some(label);
    }
    written(app, &[abs]);
    Ok(Edited { line: i, line_text: new, undo })
}

fn valid_day(d: &str) -> Res<()> {
    let b = d.as_bytes();
    let ok = b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() });
    if ok {
        Ok(())
    } else {
        Err(EditError { code: "invalid", message: format!("Not a date: {d}"), current: None })
    }
}

#[tauri::command]
/// `today` is the window's local date (it knows the time zone), stamped as the ✅ date.
pub async fn task_toggle(app: AppHandle, path: String, line: usize, line_text: String, done: bool, today: String) -> Res<Edited> {
    writing(move || {
        valid_day(&today)?;
        edit(&app, &path, line, &line_text, if done { "Ticked" } else { "Unticked" }, |l| Ok(write::toggled(l, done, &today)))
    })
    .await
}

#[tauri::command]
pub async fn task_set_date(
    app: AppHandle,
    path: String,
    line: usize,
    line_text: String,
    kind: DateKind,
    date: Option<String>,
) -> Res<Edited> {
    writing(move || {
        let verb = match (kind, date.is_some()) {
            (DateKind::Due, true) => "Set the due date of",
            (DateKind::Due, false) => "Cleared the due date of",
            (DateKind::Scheduled, true) => "Deferred",
            (DateKind::Scheduled, false) => "Cleared the deferral of",
            (DateKind::Start, true) => "Set the start date of",
            (DateKind::Start, false) => "Cleared the start date of",
            (DateKind::Created, true) => "Set the created date of",
            (DateKind::Created, false) => "Cleared the created date of",
        };
        edit(&app, &path, line, &line_text, verb, |l| write::with_date(l, kind, date.as_deref()))
    })
    .await
}

/// Replaces a task's line with `lines`, worked out in the window (src/tasksq/edits.ts): a
/// recurring task done (its next occurrence beside it), deleted on completion, cancelled,
/// reopened, started, or given a priority. `label` is the toast's verb, as "Ticked". Undoable.
#[tauri::command]
pub async fn task_replace(app: AppHandle, path: String, line: usize, line_text: String, lines: Vec<String>, label: String) -> Res<Edited> {
    writing(move || {
        let st = app.state::<AppState>();
        writable(&st)?;
        let svc = app.state::<VaultService>();
        let abs = vault_path(&svc, &path)?;
        if lines.len() > 2 {
            return Err(EditError { code: "invalid", message: "A task becomes at most two lines.".into(), current: None });
        }
        let i = write::replace_task(&abs, line, &line_text, &lines)?;
        let mut undo = None;
        if lines.len() != 1 || lines[0] != line_text {
            let shown = lines
                .iter()
                .find(|l| brainstead_core::tasks::parse_line(l, 0).is_some_and(|t| t.done))
                .or(lines.first())
                .unwrap_or(&line_text);
            let text = format!("{label} “{}”", task_label(shown));
            app.state::<UndoStack>().push(UndoEntry {
                path: path.clone(),
                line: i,
                before: line_text.clone(),
                after: lines.join("\n"),
                label: text.clone(),
                written: Some(lines.clone()),
                files: Vec::new(),
            });
            undo = Some(text);
        }
        written(&app, &[abs]);
        let line_text = lines.last().cloned().unwrap_or_default();
        Ok(Edited { line: i + lines.len().saturating_sub(1), line_text, undo })
    })
    .await
}

/// Moves a task between two ranked neighbours. Returns false when there's no room, and the
/// window renumbers the list with `tasks_rank_all` instead.
#[tauri::command]
pub async fn task_rank(app: AppHandle, path: String, line: usize, line_text: String, prev: Option<i64>, next: Option<i64>) -> Res<bool> {
    writing(move || {
        let Some(rank) = write::rank_in_gap(prev, next) else { return Ok(false) };
        edit(&app, &path, line, &line_text, "Moved", |l| Ok(write::with_rank(l, rank)))?;
        Ok(true)
    })
    .await
}

/// Ranks a whole list in the order shown (1024, 2048…). Not undoable.
#[tauri::command]
pub async fn tasks_rank_all(app: AppHandle, tasks: Vec<TaskRef>) -> Res<usize> {
    writing(move || {
        let st = app.state::<AppState>();
        writable(&st)?;
        let svc = app.state::<VaultService>();
        let root = svc.root().ok_or("No vault is open.".to_string())?;
        let missed = write::bulk_rank(&root, &tasks)?;
        let mut paths: Vec<PathBuf> = tasks.iter().map(|t| root.join(&t.path)).collect();
        paths.dedup();
        written(&app, &paths);
        Ok(missed.len())
    })
    .await
}

#[tauri::command]
pub fn undo_peek(stack: State<UndoStack>) -> Option<UndoEntry> {
    stack.peek()
}

/// Puts back the last change, if its line is still as the change left it.
#[tauri::command]
pub async fn undo(app: AppHandle) -> Res<Option<String>> {
    writing(move || {
        let st = app.state::<AppState>();
        writable(&st)?;
        let stack = app.state::<UndoStack>();
        let Some(e) = stack.pop() else { return Ok(None) };
        let svc = app.state::<VaultService>();
        // The corrections file lives in the app data folder (Fix name's remembered corrections).
        let paths = apply_undo(
            |rel| {
                if rel == brainstead_core::names::UNDO_NAME {
                    Ok(brainstead_core::names::path(&crate::platform::data_dir()))
                } else {
                    vault_path(&svc, rel)
                }
            },
            &e,
        )?;
        written(&app, &paths);
        Ok(Some(format!("Undid: {}", e.label)))
    })
    .await
}

/// Puts one entry's change back. Returns the files written.
fn apply_undo(resolve: impl Fn(&str) -> Res<PathBuf>, e: &UndoEntry) -> Res<Vec<PathBuf>> {
    if !e.files.is_empty() {
        let changes = e
            .files
            .iter()
            .map(|f| Ok(write::FileChange { path: resolve(&f.path)?, before: f.before.clone(), version: f.version.clone() }))
            .collect::<Res<Vec<_>>>()?;
        write::restore_files(&changes)?;
        return Ok(changes.into_iter().map(|c| c.path).collect());
    }
    let abs = resolve(&e.path)?;
    let before = e.before.clone();
    match &e.written {
        Some(lines) => write::restore_task(&abs, e.line, lines, &before)?,
        None => {
            write::edit_task(&abs, e.line, &e.after, move |_| before).map_err(|err| match err {
                WriteError::Stale(_) => WriteError::Stale("That line has changed since, so it can't be undone.".into()),
                other => other,
            })?;
        }
    }
    Ok(vec![abs])
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureKind {
    Task,
    Thought,
}

/// Quick capture: a task under `#### Other` in the To Do list, or a dated thought at the top of the
/// Scratchpad. `stamp` is the window's local "YYYY-MM-DD HH:MM", the thought's heading.
#[tauri::command]
pub async fn capture(app: AppHandle, kind: CaptureKind, text: String, stamp: String) -> Res<String> {
    writing(move || {
        let st = app.state::<AppState>();
        writable(&st)?;
        let svc = app.state::<VaultService>();
        let root = svc.root().ok_or("No vault is open.".to_string())?;
        let (path, out) = match kind {
            CaptureKind::Task => {
                let p = root.join(TODO_LIST);
                let line = write::capture_task(&p, &text)?;
                (p, format!("Added to the To Do list: {}", line.trim_start_matches("- [ ] ")))
            }
            CaptureKind::Thought => {
                let p = root.join(SCRATCHPAD);
                write::capture_thought(&p, &text, &stamp)?;
                (p, "Added to the Scratchpad".to_string())
            }
        };
        written(&app, &[path]);
        Ok(out)
    })
    .await
}

pub fn undo_file(data: &Path) -> PathBuf {
    data.join("undo.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(path: &str, before: &str, after: &str) -> UndoEntry {
        UndoEntry {
            path: path.into(),
            line: 0,
            before: before.into(),
            after: after.into(),
            label: "x".into(),
            written: None,
            files: Vec::new(),
        }
    }

    fn stack() -> (tempfile::TempDir, UndoStack) {
        let d = tempfile::tempdir().unwrap();
        let s = UndoStack::load(d.path().join("undo.json"));
        (d, s)
    }

    #[test]
    fn keeps_five_and_cancels_a_line_undone_by_hand() {
        let (d, s) = stack();
        for i in 0..7 {
            s.push(line("A.md", &format!("- [ ] {i}"), &format!("- [x] {i}")));
        }
        assert_eq!(crate::lock(&s.entries).len(), 5);
        s.push(line("A.md", "- [x] 6", "- [ ] 6"));
        assert_eq!(s.peek().unwrap().before, "- [ ] 5");
        // File entries never cancel each other, though their line fields are alike.
        let f =
            |v: &str| UndoEntry::files("Moved".into(), vec![FileUndo { path: "B.md".into(), before: Some("b".into()), version: v.into() }]);
        s.push(f("1"));
        s.push(f("2"));
        assert_eq!(s.peek().unwrap().files[0].version, "2");
        assert_eq!(crate::lock(&s.entries).len(), 5);
        // Kept across a restart, and entries saved before `files` existed still load.
        assert_eq!(UndoStack::load(d.path().join("undo.json")).peek(), s.peek());
        std::fs::write(
            d.path().join("old.json"),
            r#"[{"path":"A.md","line":3,"before":"- [ ] a","after":"- [x] a","label":"Ticked “a”"}]"#,
        )
        .unwrap();
        let old = UndoStack::load(d.path().join("old.json")).pop().unwrap();
        assert_eq!((old.line, old.files.len()), (3, 0));
    }

    #[test]
    fn undoes_lines_and_files() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().to_path_buf();
        let resolve = |rel: &str| Ok(root.join(rel));
        std::fs::write(root.join("A.md"), "# A\r\n- [x] one ✅ 2026-10-02\r\n").unwrap();
        let mut e = line("A.md", "- [ ] one", "- [x] one ✅ 2026-10-02");
        e.line = 1;
        apply_undo(resolve, &e).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("A.md")).unwrap(), "# A\r\n- [ ] one\r\n");

        // A task moved from the To Do list into a project, then undone: both files as they were.
        let todo = "#### Other\n- [ ] Call Sam\n";
        let proj = "## Next actions\n";
        std::fs::write(root.join("Me. To Do List.md"), todo).unwrap();
        std::fs::write(root.join("Project. A.md"), proj).unwrap();
        let (_, chs) = write::move_task(
            &root.join("Me. To Do List.md"),
            1,
            "- [ ] Call Sam",
            &root.join("Project. A.md"),
            "Next actions",
            "- [ ] Call Sam",
        )
        .unwrap();
        let files = chs
            .iter()
            .map(|c| FileUndo {
                path: c.path.file_name().unwrap().to_string_lossy().into_owned(),
                before: c.before.clone(),
                version: c.version.clone(),
            })
            .collect();
        let m = UndoEntry::files("Moved “Call Sam”".into(), files);
        let written = apply_undo(resolve, &m).unwrap();
        assert_eq!(written.len(), 2);
        assert_eq!(std::fs::read_to_string(root.join("Me. To Do List.md")).unwrap(), todo);
        assert_eq!(std::fs::read_to_string(root.join("Project. A.md")).unwrap(), proj);
        // Again: the files aren't as the move left them now, so nothing changes.
        assert_eq!(apply_undo(resolve, &m).unwrap_err().code, "stale");
        assert_eq!(std::fs::read_to_string(root.join("Project. A.md")).unwrap(), proj);
    }
}
