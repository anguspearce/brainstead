// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Brainstead's writes to the vault: one-line task edits and captures, as the previous app makes them
//! (bff/src/services/tasks-index.ts, routes/capture.ts), and the editor's whole-file saves. Every
//! write is atomic (a temporary file in the same folder, renamed over the old one), takes a
//! per-file lock, refuses when OneDrive has left a conflict copy beside the file, and keeps the
//! file's line endings. Task edits also keep its modification time, so "recently edited" means
//! writing. A save refuses when the file changed since it was read (its `version`).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::markdown::heading;
use crate::tasks::{effort_minutes, in_quote, parse_line};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", content = "message", rename_all = "kebab-case")]
pub enum WriteError {
    /// The task's line isn't where it was and can't be found once elsewhere.
    Stale(String),
    /// OneDrive left `name N.ext` beside the file: someone's edits may be in it.
    Conflict(String),
    NotFound(String),
    Invalid(String),
    Io(String),
    /// The file isn't as it was read: someone else (another editor, sync) changed it. What's there now.
    Changed {
        content: String,
        version: String,
    },
    /// A file is already at the path being created or moved to.
    Exists(String),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::Stale(m)
            | WriteError::Conflict(m)
            | WriteError::NotFound(m)
            | WriteError::Invalid(m)
            | WriteError::Io(m)
            | WriteError::Exists(m) => f.write_str(m),
            WriteError::Changed { .. } => f.write_str("This file changed on disk since it was opened, so it wasn't saved."),
        }
    }
}

pub type Result<T> = std::result::Result<T, WriteError>;

pub(crate) fn io(e: std::io::Error) -> WriteError {
    WriteError::Io(e.to_string())
}

static LOCKS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = LazyLock::new(Default::default);

/// One writer at a time per file, within this app (other apps are caught by the line checks).
pub(crate) fn lock(path: &Path) -> Arc<Mutex<()>> {
    LOCKS.lock().unwrap().entry(path.to_path_buf()).or_default().clone()
}

/// Refuses when a OneDrive conflict copy sits beside `path` (`Foo 2.md` for `Foo.md`).
pub fn check_conflict_copies(path: &Path) -> Result<()> {
    let (Some(dir), Some(stem), ext) = (
        path.parent(),
        path.file_stem().map(|s| s.to_string_lossy().into_owned()),
        path.extension().map(|e| e.to_string_lossy().into_owned()),
    ) else {
        return Ok(());
    };
    let Ok(rd) = fs::read_dir(dir) else { return Ok(()) };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let (s, x) = match name.rfind('.') {
            Some(i) if i > 0 => (&name[..i], Some(&name[i + 1..])),
            _ => (name.as_str(), None),
        };
        if x != ext.as_deref() {
            continue;
        }
        if let Some(rest) = s.strip_prefix(&format!("{stem} ")) {
            if (1..=2).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_digit()) {
                return Err(WriteError::Conflict(format!(
                    "OneDrive left a conflict copy, “{name}”, beside this file. Merge or delete it first."
                )));
            }
        }
    }
    Ok(())
}

/// Whether a rename failed only because another program has the file open: on Windows, access
/// denied, a sharing or a lock violation, as antivirus, OneDrive's sync, an editor or the MCP
/// server reading the bridge's files cause for a moment. Never elsewhere: a rename over an open
/// file just works there.
#[cfg(windows)]
fn held_open(e: &std::io::Error) -> bool {
    // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION.
    matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(not(windows))]
fn held_open(_: &std::io::Error) -> bool {
    false
}

/// How many times a rename is tried, and the longest wait between tries: about a second in all.
const RENAME_TRIES: u32 = 10;
const RENAME_WAIT_MAX: std::time::Duration = std::time::Duration::from_millis(200);

/// `op` again, with a doubling wait from 10 ms, while it fails with an error `held` says will pass.
fn retry_held<T>(
    mut op: impl FnMut() -> std::io::Result<T>,
    held: impl Fn(&std::io::Error) -> bool,
    mut sleep: impl FnMut(std::time::Duration),
) -> std::io::Result<T> {
    let mut wait = std::time::Duration::from_millis(10);
    for _ in 1..RENAME_TRIES {
        match op() {
            Err(e) if held(&e) => {
                sleep(wait);
                wait = (wait * 2).min(RENAME_WAIT_MAX);
            }
            r => return r,
        }
    }
    op()
}

/// `fs::rename`, tried again for about a second while another program has the file open
/// (Windows only; elsewhere it's `fs::rename`).
pub fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    retry_held(|| fs::rename(from, to), held_open, std::thread::sleep)
}

/// Moves `from` to `to` only when nothing is at `to`, failing with `AlreadyExists` otherwise. The
/// check and the move are one step, so two renames to one name, or sync dropping a file there
/// meanwhile, never overwrite a note. Tried again while another program has the file open, as
/// `rename` is.
pub fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    retry_held(|| rename_no_replace(from, to), held_open, std::thread::sleep)
}

/// Where the volume can't do it in one step: a check, then a rename.
fn checked_rename(from: &Path, to: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(to).is_ok() {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    fs::rename(from, to)
}

#[cfg(target_os = "macos")]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c = |p: &Path| std::ffi::CString::new(p.as_os_str().as_bytes()).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput));
    let (a, b) = (c(from)?, c(to)?);
    // SAFETY: both are NUL-terminated strings that outlive the call.
    if unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_EXCL) } == 0 {
        return Ok(());
    }
    let e = std::io::Error::last_os_error();
    // Some volumes (network shares, FAT) have no exclusive rename.
    if e.raw_os_error() == Some(libc::ENOTSUP) {
        return checked_rename(from, to);
    }
    Err(e)
}

#[cfg(windows)]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide = |p: &Path| p.as_os_str().encode_wide().chain([0]).collect::<Vec<u16>>();
    let (a, b) = (wide(from), wide(to));
    // No flags, so not MOVEFILE_REPLACE_EXISTING: a file at `to` fails the move.
    // SAFETY: both are NUL-terminated strings that outlive the call.
    if unsafe { windows_sys::Win32::Storage::FileSystem::MoveFileExW(a.as_ptr(), b.as_ptr(), 0) } != 0 {
        return Ok(());
    }
    let e = std::io::Error::last_os_error();
    // ERROR_PATH_NOT_FOUND, ERROR_FILENAME_EXCED_RANGE: a path too long for this call, which
    // `fs::rename` can still take.
    if matches!(e.raw_os_error(), Some(3 | 206)) {
        return checked_rename(from, to);
    }
    Err(e)
}

/// A hard link fails when the name is taken, so linking then unlinking is a move that can't replace.
#[cfg(not(any(target_os = "macos", windows)))]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    match fs::hard_link(from, to) {
        Ok(()) => fs::remove_file(from),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(e),
        Err(_) => checked_rename(from, to),
    }
}

/// Whether two paths are one file: on a disk that ignores case, a name and its other spelling.
pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        matches!((fs::metadata(a), fs::metadata(b)), (Ok(x), Ok(y)) if x.dev() == y.dev() && x.ino() == y.ino())
    }
    #[cfg(not(unix))]
    {
        matches!((crate::paths::canonical(a), crate::paths::canonical(b)), (Ok(x), Ok(y)) if x == y)
    }
}

/// Gives the temporary file the old file's creation time and extended attributes (Finder tags,
/// where it came from), which a file swapped in for it would otherwise lose. As far as it can: a
/// save never fails for these.
#[cfg(target_os = "macos")]
fn keep_attributes(old: &Path, meta: &fs::Metadata, new: &fs::File) {
    use std::os::macos::fs::FileTimesExt;
    use std::os::unix::ffi::OsStrExt;
    use xattr::FileExt;
    extern "C" {
        // <xattr_flags.h>: whether an attribute of this name goes with the file for this purpose.
        fn xattr_preserve_for_intent(name: *const libc::c_char, intent: libc::c_uint) -> libc::c_int;
    }
    // XATTR_OPERATION_INTENT_SAVE: a save that replaces the file, so attributes that describe the
    // old content (a sync provider's own) are left behind.
    const SAVE: libc::c_uint = 2;
    for name in xattr::list(old).into_iter().flatten() {
        let Ok(c) = std::ffi::CString::new(name.as_bytes()) else { continue };
        // SAFETY: `c` is a NUL-terminated string that outlives the call.
        if unsafe { xattr_preserve_for_intent(c.as_ptr(), SAVE) } != 1 {
            continue;
        }
        if let Ok(Some(v)) = xattr::get(old, &name) {
            let _ = new.set_xattr(&name, &v);
        }
    }
    if let Ok(t) = meta.created() {
        let _ = new.set_times(fs::FileTimes::new().set_created(t));
    }
}

#[cfg(not(target_os = "macos"))]
fn keep_attributes(_: &Path, _: &fs::Metadata, _: &fs::File) {}

/// Clears out what a crash left in a vault: deletes this app's temporary files
/// (`.brainstead-<pid>-<n>.tmp`) more than a minute old, so they don't sync to every device, and
/// returns the vault-relative paths of any `.rename-<pid>` files, each a note caught halfway
/// through a change of case, for the user to name again. Those are never deleted. For a vault
/// that's writable only: the caller checks.
pub fn sweep_leftovers(root: &Path) -> Vec<String> {
    let old = |e: &walkdir::DirEntry| {
        e.metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|d| d > std::time::Duration::from_secs(60))
    };
    let mut renames = Vec::new();
    for e in walkdir::WalkDir::new(root).into_iter().flatten().filter(|e| e.file_type().is_file()) {
        let name = e.file_name().to_string_lossy();
        if name.starts_with(".brainstead-") && name.ends_with(".tmp") {
            if old(&e) {
                let _ = fs::remove_file(e.path());
            }
        } else if name.strip_prefix(".rename-").is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit())) {
            renames.extend(crate::vault::rel_of(root, e.path()));
        }
    }
    renames.sort();
    renames
}

static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Writes through a temporary file in the same folder and a rename, so a crash never leaves half
/// a file: the data is flushed to disk before the rename. A symlink is written through to the
/// file it points at, and the file keeps its permissions and, on macOS, its creation time and
/// extended attributes. With `keep_mtime`, the old modification time is put back afterwards.
pub fn write_atomic(path: &Path, data: &[u8], keep_mtime: bool) -> Result<()> {
    use std::io::Write;
    let target;
    let path = match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            target = crate::paths::canonical(path).map_err(io)?;
            target.as_path()
        }
        _ => path,
    };
    let meta = fs::metadata(path).ok();
    let old = if keep_mtime { meta.as_ref().and_then(|m| m.modified().ok()) } else { None };
    let dir = path.parent().ok_or_else(|| WriteError::Invalid("no folder".into()))?;
    // A short name, so a file whose own name is near the 255-byte limit still saves.
    let n = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(".brainstead-{}-{n}.tmp", std::process::id()));
    let written = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        if let Some(m) = &meta {
            f.set_permissions(m.permissions())?;
            keep_attributes(path, m, &f);
        }
        f.sync_all()?;
        drop(f);
        rename(&tmp, path)
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(io(e));
    }
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    if let Some(t) = old {
        if let Ok(f) = fs::File::options().write(true).open(path) {
            let _ = f.set_modified(t);
        }
    }
    Ok(())
}

/// A file's line endings as `split_lines` read them: each line's own, so a file that mixes CRLF
/// and LF keeps both when some of its lines change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Eol {
    /// The lines as read, to line the changed ones up against (empty when they all end alike).
    old: Vec<String>,
    /// Each line's ending as read; the last, unended line gets `new`.
    ends: Vec<&'static str>,
    /// For lines that weren't there: the file's commoner ending, CRLF on a tie.
    new: &'static str,
}

impl Eol {
    /// Every line ending in `\n`.
    pub fn lf() -> Self {
        Eol { old: vec![], ends: vec![], new: "\n" }
    }

    /// The ending new lines get.
    pub fn new_line(&self) -> &'static str {
        self.new
    }
}

/// The file's lines without their endings, the endings to put back, and whether the last line
/// has one.
pub fn split_lines(text: &str) -> (Vec<String>, Eol, bool) {
    let trailing = text.ends_with('\n');
    let mut lines = Vec::new();
    let mut ends: Vec<&'static str> = Vec::new();
    if !text.is_empty() {
        let body = text.strip_suffix('\n').unwrap_or(text);
        let mut parts = body.split('\n').peekable();
        while let Some(l) = parts.next() {
            // A CR counts as part of the ending only before a LF.
            let ended = parts.peek().is_some() || trailing;
            match l.strip_suffix('\r').filter(|_| ended) {
                Some(t) => {
                    lines.push(t.to_string());
                    ends.push("\r\n");
                }
                None => {
                    lines.push(l.to_string());
                    ends.push(if ended { "\n" } else { "" });
                }
            }
        }
    }
    let crlf = ends.iter().filter(|e| **e == "\r\n").count();
    let lf = ends.iter().filter(|e| **e == "\n").count();
    let new = if crlf > 0 && crlf >= lf { "\r\n" } else { "\n" };
    if let Some(last) = ends.last_mut().filter(|e| e.is_empty()) {
        *last = new;
    }
    let mixed = crlf > 0 && lf > 0;
    let old = if mixed { lines.clone() } else { vec![] };
    (lines, Eol { old, ends, new }, trailing)
}

/// The lines joined again: a line that was in the file (lined up by a diff against the lines as
/// read) keeps its own ending; a new or changed one takes the ending of the line it replaced, or
/// else the one before it.
pub fn join_lines(lines: &[String], eol: &Eol, trailing: bool) -> String {
    let mut ends: Vec<&'static str> = vec![eol.new; lines.len()];
    if !eol.old.is_empty() {
        let ops = similar::capture_diff_slices(similar::Algorithm::Myers, &eol.old, lines);
        for op in ops {
            let (o, n) = (op.old_range(), op.new_range());
            for (k, j) in n.enumerate() {
                let i = if o.is_empty() { o.start.saturating_sub(1) } else { (o.start + k).min(o.end - 1) };
                if let Some(e) = eol.ends.get(i) {
                    ends[j] = e;
                }
            }
        }
    }
    let mut s = String::with_capacity(lines.iter().map(|l| l.len() + 2).sum());
    for (i, l) in lines.iter().enumerate() {
        s.push_str(l);
        if i + 1 < lines.len() || trailing {
            s.push_str(ends[i]);
        }
    }
    s
}

static RANK_TAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\^rank-\d+\s*$").unwrap());
static DONE_DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*✅\s*\d{4}-\d{2}-\d{2}").unwrap());

/// A line as compared when re-finding a task: without its rank and done date, which other edits
/// (a drag, a tick in another editor) change without changing which task it is.
fn normalise(line: &str) -> String {
    let l = RANK_TAIL.replace(line, "");
    DONE_DATE.replace_all(&l, "").trim_end().to_string()
}

/// Where the task is now: the given line if it still matches, else the one line in the file that
/// does. None (stale) when it's gone or there are several.
pub fn find_line(lines: &[String], line: usize, expected: &str) -> Option<usize> {
    let want = normalise(expected);
    if lines.get(line).is_some_and(|l| normalise(l) == want) {
        return Some(line);
    }
    let mut hits = lines.iter().enumerate().filter(|(_, l)| normalise(l) == want).map(|(i, _)| i);
    match (hits.next(), hits.next()) {
        (Some(i), None) => Some(i),
        _ => None,
    }
}

/// Edits one task line: finds it (as `find_line`), applies `f`, and writes the file if the line
/// changed. Returns the line number and the line as written.
pub fn edit_task(path: &Path, line: usize, expected: &str, f: impl FnOnce(&str) -> String) -> Result<(usize, String)> {
    edit_line_where(path, line, expected, true, f)
}

/// Changes one line, found as `edit_task` finds it, whatever the line is: an initiative's action,
/// which is a plain list item, not a task (D-20261118-08), or a line an undo puts back.
pub fn edit_line(path: &Path, line: usize, expected: &str, f: impl FnOnce(&str) -> String) -> Result<(usize, String)> {
    edit_line_where(path, line, expected, false, f)
}

fn edit_line_where(path: &Path, line: usize, expected: &str, task: bool, f: impl FnOnce(&str) -> String) -> Result<(usize, String)> {
    let l = lock(path);
    let _g = l.lock().unwrap();
    check_conflict_copies(path)?;
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(WriteError::NotFound("The note isn't there any more.".into())),
        Err(e) => return Err(io(e)),
    };
    let (mut lines, eol, trailing) = split_lines(&text);
    let i = find_line(&lines, line, expected)
        .ok_or_else(|| WriteError::Stale("This line changed in the file since it was shown. It's been reloaded; try again.".into()))?;
    if task && parse_line(&lines[i], i).is_none() {
        return Err(WriteError::Stale("That line isn't a task any more.".into()));
    }
    let new = f(&lines[i]);
    if new != lines[i] {
        lines[i] = new.clone();
        write_atomic(path, join_lines(&lines, &eol, trailing).as_bytes(), true)?;
    }
    Ok((i, new))
}

/// The lines either side of a task as `replace_task` found it (None at the top or the end of the
/// file): where a deleted task goes back to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Around {
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Replaces one task line with `new` (none, to delete it; two, for a recurring task's next
/// occurrence beside the done one), found as `edit_task` finds it, in one atomic write that keeps
/// the modification time. Returns where the task was, and the lines either side of it.
pub fn replace_task(path: &Path, line: usize, expected: &str, new: &[String]) -> Result<(usize, Around)> {
    let l = lock(path);
    let _g = l.lock().unwrap();
    check_conflict_copies(path)?;
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(WriteError::NotFound("The note isn't there any more.".into())),
        Err(e) => return Err(io(e)),
    };
    let (mut lines, eol, trailing) = split_lines(&text);
    let i = find_line(&lines, line, expected)
        .ok_or_else(|| WriteError::Stale("This task changed in the file since it was shown. It's been reloaded; try again.".into()))?;
    if parse_line(&lines[i], i).is_none() {
        return Err(WriteError::Stale("That line isn't a task any more.".into()));
    }
    if new.iter().any(|n| n.contains('\n') || n.contains('\r')) {
        return Err(WriteError::Invalid("A task is one line.".into()));
    }
    let around = Around { before: i.checked_sub(1).map(|p| lines[p].clone()), after: lines.get(i + 1).cloned() };
    lines.splice(i..=i, new.iter().cloned());
    write_atomic(path, join_lines(&lines, &eol, trailing).as_bytes(), true)?;
    Ok((i, around))
}

/// Undoes `replace_task`: the lines it wrote, found at `line` (or once elsewhere), become `before`
/// again. Stale when they've changed since. A deleted task left no lines to find, so it goes back
/// between the lines that were `around` it, at `line` if they still meet there, else at the one
/// place they do; stale when they meet nowhere, or in several places, or weren't recorded.
pub fn restore_task(path: &Path, line: usize, written: &[String], before: &str, around: Option<&Around>) -> Result<()> {
    let l = lock(path);
    let _g = l.lock().unwrap();
    check_conflict_copies(path)?;
    let text = fs::read_to_string(path).map_err(io)?;
    let (mut lines, eol, trailing) = split_lines(&text);
    let n = written.len();
    let stale = || WriteError::Stale("That line has changed since, so it can't be undone.".into());
    let same = |now: Option<&String>, then: &Option<String>| match (now, then) {
        (Some(a), Some(b)) => normalise(a) == normalise(b),
        (None, None) => true,
        _ => false,
    };
    let at = |i: usize| match (n, around) {
        // Without the lines around it, any line number would do: lines added above since would put
        // the task under another heading.
        (0, None) => false,
        (0, Some(a)) => i <= lines.len() && same(i.checked_sub(1).and_then(|p| lines.get(p)), &a.before) && same(lines.get(i), &a.after),
        _ => i + n <= lines.len() && lines[i..i + n].iter().zip(written).all(|(a, b)| normalise(a) == normalise(b)),
    };
    let i = if at(line) {
        line
    } else {
        let mut hits = (0..=lines.len().saturating_sub(n)).filter(|&i| at(i));
        match (hits.next(), hits.next()) {
            (Some(i), None) => i,
            _ => return Err(stale()),
        }
    };
    lines.splice(i..i + n, [before.to_string()]);
    write_atomic(path, join_lines(&lines, &eol, trailing).as_bytes(), true)?;
    Ok(())
}

static CHECKBOX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*(?:[-*+]|\d+[.)])\s+\[)[^\]](\]\s+)").unwrap());
static RANK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\^rank-(\d+)\s*$").unwrap());

fn split_rank(line: &str) -> (String, Option<String>) {
    match RANK.captures(line) {
        Some(c) => (line[..c.get(0).unwrap().start()].to_string(), Some(c[1].to_string())),
        None => (line.to_string(), None),
    }
}

/// Ticks (`[x]` and ` ✅ today`, rank kept) or unticks (`[ ]`, ✅ and rank dropped, so the task goes
/// back to the top of a manual list) a task line.
pub fn toggled(line: &str, done: bool, today: &str) -> String {
    let (body, rank) = split_rank(line);
    let mut body = CHECKBOX.replace(&body, |c: &regex::Captures| format!("{}{}{}", &c[1], if done { "x" } else { " " }, &c[2])).to_string();
    body = DONE_DATE.replace_all(&body, "").trim_end().to_string();
    if done {
        body.push_str(&format!(" ✅ {today}"));
        if let Some(r) = rank {
            body.push_str(&format!(" ^rank-{r}"));
        }
    }
    body
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateKind {
    /// 📅
    Due,
    /// ⏳ ("defer")
    Scheduled,
    /// 🛫
    Start,
    /// ➕
    Created,
}

/// Sets (or with None clears) a task's 📅, ⏳, 🛫 or ➕ date, at the end of the line before `^rank-N`.
pub fn with_date(line: &str, kind: DateKind, date: Option<&str>) -> Result<String> {
    if let Some(d) = date {
        let b = d.as_bytes();
        let ok = b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() });
        if !ok {
            return Err(WriteError::Invalid(format!("Not a date: {d}")));
        }
    }
    let emoji = match kind {
        DateKind::Due => "📅",
        DateKind::Scheduled => "⏳",
        DateKind::Start => "🛫",
        DateKind::Created => "➕",
    };
    let re = Regex::new(&format!(r"\s*{emoji}\u{{fe0f}}?\s*\d{{4}}-\d{{2}}-\d{{2}}")).unwrap();
    let (body, rank) = split_rank(line);
    let mut body = re.replace_all(&body, "").trim_end().to_string();
    if let Some(d) = date {
        body.push_str(&format!(" {emoji} {d}"));
    }
    if let Some(r) = rank {
        body.push_str(&format!(" ^rank-{r}"));
    }
    Ok(body)
}

pub const RANK_SPACING: i64 = 1024;

/// A rank between two neighbours (None: no neighbour that side), or None when there's no room
/// and the list has to be renumbered.
pub fn rank_in_gap(prev: Option<i64>, next: Option<i64>) -> Option<i64> {
    match (prev, next) {
        (None, None) => Some(RANK_SPACING),
        (None, Some(n)) => Some(n / 2).filter(|r| *r > 0 && *r < n),
        (Some(p), None) => Some(p + RANK_SPACING),
        (Some(p), Some(n)) => Some(p + (n - p) / 2).filter(|r| *r > p && *r < n),
    }
}

/// The line with `^rank-N` set. On an unticked line a leftover ✅ goes too, or the next read
/// would throw the new rank away (the unticked-elsewhere rule in `tasks::parse_line`).
pub fn with_rank(line: &str, rank: i64) -> String {
    let (mut body, _) = split_rank(line);
    let done = parse_line(line, 0).is_some_and(|t| t.done);
    if !done {
        body = DONE_DATE.replace_all(&body, "").to_string();
    }
    format!("{} ^rank-{rank}", body.trim_end())
}

/// A task in a list being renumbered.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRef {
    pub path: String,
    pub line: usize,
    pub line_text: String,
}

/// Gives the tasks ranks 1024, 2048… in the order given, one write per file. Tasks that can't
/// be found are skipped and returned.
pub fn bulk_rank(root: &Path, tasks: &[TaskRef]) -> Result<Vec<TaskRef>> {
    let mut by_file: Vec<(String, Vec<(usize, &TaskRef)>)> = Vec::new();
    for (i, t) in tasks.iter().enumerate() {
        match by_file.iter_mut().find(|(p, _)| *p == t.path) {
            Some((_, v)) => v.push((i, t)),
            None => by_file.push((t.path.clone(), vec![(i, t)])),
        }
    }
    let mut missed = Vec::new();
    for (rel, items) in by_file {
        let path = root.join(&rel);
        let l = lock(&path);
        let _g = l.lock().unwrap();
        check_conflict_copies(&path)?;
        let text = fs::read_to_string(&path).map_err(io)?;
        let (mut lines, eol, trailing) = split_lines(&text);
        let mut changed = false;
        for (i, t) in items {
            match find_line(&lines, t.line, &t.line_text) {
                Some(k) => {
                    let new = in_quote(&lines[k], |b| with_rank(b, (i as i64 + 1) * RANK_SPACING));
                    if new != lines[k] {
                        lines[k] = new;
                        changed = true;
                    }
                }
                None => missed.push(t.clone()),
            }
        }
        if changed {
            write_atomic(&path, join_lines(&lines, &eol, trailing).as_bytes(), true)?;
        }
    }
    Ok(missed)
}

/// Collapses whitespace, as capture does for a task.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A captured task: `- [ ] text` at the top of the `#### Other` section of the To Do list, which
/// is added at the end when missing. The list itself must exist.
pub fn capture_task(path: &Path, text: &str) -> Result<String> {
    let t = one_line(text);
    if t.is_empty() {
        return Err(WriteError::Invalid("Nothing to capture.".into()));
    }
    let l = lock(path);
    let _g = l.lock().unwrap();
    check_conflict_copies(path)?;
    let src = fs::read_to_string(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            WriteError::NotFound(format!("{} isn't in the vault.", path.file_name().unwrap_or_default().to_string_lossy()))
        }
        _ => io(e),
    })?;
    let (mut lines, eol, trailing) = split_lines(&src);
    let item = format!("- [ ] {t}");
    insert_other(&mut lines, &item);
    write_atomic(path, join_lines(&lines, &eol, trailing).as_bytes(), false)?;
    Ok(item)
}

/// Puts a task line at the top of the To Do list's `#### Other` section, adding the section at the
/// end when there isn't one.
pub fn insert_other(lines: &mut Vec<String>, item: &str) {
    match lines
        .iter()
        .position(|l| l.trim_end().starts_with("####") && l.trim_start_matches('#').trim() == "Other" && l.starts_with("#### "))
    {
        Some(h) => {
            // After the heading and any blank lines, so the newest is first.
            let mut at = h + 1;
            while at < lines.len() && lines[at].trim().is_empty() {
                at += 1;
            }
            lines.insert(at, item.to_string());
        }
        None => {
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            lines.extend([String::new(), "#### Other".into(), String::new(), item.to_string()]);
        }
    }
}

/// Where the Scratchpad's pinned header ends: a `# Title` line, then a leading quote only if it's
/// the system note, then blank lines (the previous app's `pinnedHeaderLength`).
pub fn pinned_header_len(text: &str) -> usize {
    // After a byte-order mark, so nothing is put in front of it.
    let mut pos = if text.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let rest = |p: usize| &text[p..];
    if rest(pos).starts_with("# ") || rest(pos).starts_with("#\t") {
        pos += rest(pos).find('\n').map(|i| i + 1).unwrap_or(rest(pos).len());
        while let Some(l) = rest(pos).split_inclusive('\n').next().filter(|l| l.trim().is_empty() && !l.is_empty()) {
            pos += l.len();
        }
    }
    let mut q = pos;
    for l in rest(pos).split_inclusive('\n') {
        if !l.starts_with('>') {
            break;
        }
        q += l.len();
    }
    if q > pos && text[pos..q].contains("This is a system note") {
        pos = q;
        while let Some(l) = rest(pos).split_inclusive('\n').next().filter(|l| l.trim().is_empty() && !l.is_empty()) {
            pos += l.len();
        }
    }
    pos
}

/// A captured thought: a `## YYYY-MM-DD HH:MM` block below the Scratchpad's pinned header, newest
/// first. The Scratchpad is started when it's missing.
pub fn capture_thought(path: &Path, text: &str, stamp: &str) -> Result<String> {
    let t = text.trim();
    if t.is_empty() {
        return Err(WriteError::Invalid("Nothing to capture.".into()));
    }
    let l = lock(path);
    let _g = l.lock().unwrap();
    check_conflict_copies(path)?;
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => NEW_SCRATCHPAD.to_string(),
        Err(e) => return Err(io(e)),
    };
    let (out, block) = with_thought(&src, t, stamp);
    write_atomic(path, out.as_bytes(), false)?;
    Ok(block)
}

/// The Scratchpad with a thought added as Quick capture adds it: a `## stamp` block below its
/// pinned header, newest first; and the block. An assistant's capture is a change in Changes
/// (changes::Instruction::AddThought) made the same way.
pub fn with_thought(src: &str, text: &str, stamp: &str) -> (String, String) {
    let eol = split_lines(src).1.new_line();
    let at = pinned_header_len(src);
    let text = text.trim().replace("\r\n", "\n");
    // A level 1 or 2 heading in the thought would end its block there (inbox.rs), so the rest
    // would be left behind when it's clarified: its `#` is escaped, and it stays as text.
    let body: Vec<String> = crate::markdown::lines(&text, 0)
        .iter()
        .map(|l| match crate::markdown::heading(l.text) {
            Some((n, _)) if n <= 2 && !l.code => l.text.replacen('#', "\\#", 1),
            _ => l.text.to_string(),
        })
        .collect();
    let block = format!("## {stamp}{eol}{eol}{}{eol}{eol}", body.join(eol));
    (format!("{}{block}{}", &src[..at], &src[at..]), block)
}

/// What a Scratchpad that isn't there yet starts as.
pub const NEW_SCRATCHPAD: &str = "# Me. Scratchpad\n\n";

static TASK_HEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:[-*+]|\d+[.)])\s+\[[^\]]\]\s+").unwrap());
/// The Tasks plugin's signifiers: dates, recurrence, priorities, ids. Its fields are read from the
/// end of the line, so text goes before the first of them.
static SIGNIFIER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[📅⏳⌛🛫➕✅❌🔁⏫🔼🔽🔺⏬🆔⛔🏁]").unwrap());
static CONTEXT_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s+)#context/[\p{L}\p{N}_\-/]+").unwrap());
static CONTEXT_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\p{L}\p{N}_\-]+(?:/[\p{L}\p{N}_\-]+)*$").unwrap());
static EFFORT_FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s+)(?:\[effort::[^\]]*\]|\(effort::[^)]*\))").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s+)(!?)\[\[([^\[\]\n]+?)\]\]").unwrap());

/// Takes the `found` ranges out of a task's text and puts `value` where the first of them was, or
/// before the first Tasks signifier (else `^rank-N`, else the end) when there was none. Only the
/// spaces where it joins change; the rest of the line is as it was.
fn set_in_task(line: &str, found: impl Fn(&str) -> Vec<std::ops::Range<usize>>, value: Option<&str>) -> String {
    let head = TASK_HEAD.find(line).map(|m| m.end()).unwrap_or(0);
    let (body, rank) = split_rank(&line[head..]);
    let mut text = String::new();
    let mut at = None;
    let mut last = 0;
    for r in found(&body) {
        text.push_str(&body[last..r.start]);
        at.get_or_insert(text.len());
        last = r.end;
    }
    text.push_str(&body[last..]);
    let at = at.unwrap_or_else(|| SIGNIFIER.find(&text).map(|m| m.start()).unwrap_or(text.len()));
    let (a, b) = text.split_at(at);
    let mut out = a.trim_end().to_string();
    for part in [value.unwrap_or(""), b.trim()] {
        if !part.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(part);
        }
    }
    let mut line = format!("{}{out}", &line[..head]);
    if let Some(r) = rank {
        line.push_str(&format!(" ^rank-{r}"));
    }
    line
}

fn ranges(re: &Regex, text: &str) -> Vec<std::ops::Range<usize>> {
    re.find_iter(text).map(|m| m.range()).collect()
}

/// A context as typed (`calls`, `@calls`, `#context/calls`) as the name in its tag.
pub fn context_name(s: &str) -> Result<String> {
    let t = s.trim().trim_start_matches(['@', '#']);
    let t = t.strip_prefix("context/").unwrap_or(t);
    if !CONTEXT_NAME.is_match(t) || t.chars().all(|c| c.is_ascii_digit()) {
        return Err(WriteError::Invalid(format!("Not a context: {s}")));
    }
    Ok(t.to_string())
}

/// Sets a task's contexts (`#context/<name>` tags), or with none clears them.
pub fn with_contexts(line: &str, names: &[String]) -> Result<String> {
    let mut tags: Vec<String> = Vec::new();
    for n in names {
        let t = format!("#context/{}", context_name(n)?);
        if !tags.contains(&t) {
            tags.push(t);
        }
    }
    let v = tags.join(" ");
    Ok(set_in_task(line, |t| ranges(&CONTEXT_TAG, t), Some(&v).filter(|v| !v.is_empty()).map(|v| v.as_str())))
}

/// Sets (`[effort:: 15m]`) or with None clears a task's effort. It has to read as a duration.
pub fn with_effort(line: &str, effort: Option<&str>) -> Result<String> {
    let v = match effort.map(str::trim) {
        Some(e) if effort_minutes(e).is_some() => Some(format!("[effort:: {e}]")),
        Some(e) => return Err(WriteError::Invalid(format!("Not an effort: {e}. Try 15m, 1h or 1h30m."))),
        None => None,
    };
    Ok(set_in_task(line, |t| ranges(&EFFORT_FIELD, t), v.as_deref()))
}

/// Links a task to a project (`[[Project. Name]]`, by the note's file name without `.md`), or
/// with None unlinks it. Links for which `is_project` says yes (given the target) are replaced;
/// other links stay.
pub fn with_project_link(line: &str, project: Option<&str>, is_project: impl Fn(&str) -> bool) -> String {
    let found = |t: &str| {
        LINK.captures_iter(t)
            .filter(|c| c[1].is_empty())
            .filter(|c| {
                let target = c[2].split(['|', '#']).next().unwrap_or("").trim();
                !target.is_empty() && is_project(target)
            })
            .map(|c| c.get(0).unwrap().range())
            .collect()
    };
    let v = project.map(|p| format!("[[{p}]]"));
    set_in_task(line, found, v.as_deref())
}

/// A whole-file change, as undo needs it: the file's text before (None when the change created
/// it) and its version after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub before: Option<String>,
    pub version: String,
}

fn read_note(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            WriteError::NotFound(format!("{} isn't in the vault any more.", path.file_name().unwrap_or_default().to_string_lossy()))
        }
        _ => io(e),
    })
}

/// Changes one file's lines with `f`, under its lock, in one atomic write.
fn rewrite<T>(path: &Path, f: impl FnOnce(&mut Vec<String>) -> Result<T>) -> Result<(T, FileChange)> {
    let l = lock(path);
    let _g = l.lock().unwrap();
    check_conflict_copies(path)?;
    let src = read_note(path)?;
    let (mut lines, eol, trailing) = split_lines(&src);
    let out = f(&mut lines)?;
    let text = join_lines(&lines, &eol, trailing || src.is_empty());
    write_atomic(path, text.as_bytes(), false)?;
    Ok((out, FileChange { path: path.to_path_buf(), before: Some(src), version: version(text.as_bytes()) }))
}

/// Which lines are inside fenced code.
fn code_lines(lines: &[String]) -> Vec<bool> {
    let text = lines.join("\n");
    crate::markdown::lines(&text, 0).iter().map(|l| l.code).collect()
}

/// Puts `item` at the end of the `## heading` section (before the blank lines that end it), adding
/// the heading at the end of the file when there isn't one. Returns the item's line.
pub fn append_under(lines: &mut Vec<String>, name: &str, item: &str) -> usize {
    let code = code_lines(lines);
    let level = |i: usize| if code.get(i).copied().unwrap_or(false) { None } else { heading(&lines[i]) };
    let h = (0..lines.len()).find(|&i| level(i).is_some_and(|(n, t)| n == 2 && t.eq_ignore_ascii_case(name.trim())));
    let Some(h) = h else {
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend([format!("## {}", name.trim()), String::new(), item.to_string()]);
        return lines.len() - 1;
    };
    let end = (h + 1..lines.len()).find(|&i| level(i).is_some_and(|(n, _)| n <= 2)).unwrap_or(lines.len());
    match (h + 1..end).rev().find(|&i| !lines[i].trim().is_empty()) {
        Some(last) => {
            lines.insert(last + 1, item.to_string());
            last + 1
        }
        // An empty section: a blank line, the item, and a blank line before the next heading.
        None => {
            let mut fill = vec![String::new(), item.to_string()];
            if end < lines.len() {
                fill.push(String::new());
            }
            lines.splice(h + 1..end, fill);
            h + 2
        }
    }
}

/// Adds `item` at the top of the To Do list's `name` section, whatever its level (the To Do list's
/// headings are `####`): above its first list item, or after what the section holds (a query) when
/// it has none. With no such heading, one is made just above Other, at Other's level (or at the end,
/// `## name`, with no Other either). Returns the item's line.
pub fn append_under_todo(lines: &mut Vec<String>, name: &str, item: &str) -> usize {
    let code = code_lines(lines);
    let level = |i: usize| if code.get(i).copied().unwrap_or(false) { None } else { heading(&lines[i]) };
    let named = |want: &str| (0..lines.len()).find_map(|i| level(i).filter(|(_, t)| t.eq_ignore_ascii_case(want)).map(|(n, _)| (i, n)));
    let Some((h, n)) = named(name.trim()) else {
        let Some((o, n)) = named("Other") else { return append_under(lines, name, item) };
        let marks = "#".repeat(n);
        lines.splice(o..o, [format!("{marks} {}", name.trim()), String::new(), item.to_string(), String::new()]);
        return o + 2;
    };
    let end = (h + 1..lines.len()).find(|&i| level(i).is_some_and(|(m, _)| m <= n)).unwrap_or(lines.len());
    let listed = (h + 1..end).find(|&i| !code.get(i).copied().unwrap_or(false) && LIST_ITEM.is_match(&lines[i]));
    if let Some(first) = listed {
        lines.insert(first, item.to_string());
        return first;
    }
    match (h + 1..end).rev().find(|&i| !lines[i].trim().is_empty()) {
        Some(last) => {
            lines.splice(last + 1..last + 1, [String::new(), item.to_string()]);
            last + 2
        }
        None => {
            let mut fill = vec![String::new(), item.to_string()];
            if end < lines.len() {
                fill.push(String::new());
            }
            lines.splice(h + 1..end, fill);
            h + 2
        }
    }
}

/// A list item's line: `- …`, `* …`, `+ …` or `1. …`, indented or not.
static LIST_ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:[-*+]|\d+[.)])\s").unwrap());

/// Adds a task line at the top of the To Do list's `name` section ([`append_under_todo`]).
pub fn add_task_under_todo(path: &Path, name: &str, task_line: &str) -> Result<(usize, FileChange)> {
    one_task_line(task_line)?;
    rewrite(path, |lines| Ok(append_under_todo(lines, name, task_line)))
}

/// Moves a task within the To Do list to the top of its `name` section, as `new_line`: its old
/// line (found as `edit_task` finds it) goes and the new one is added in one write, so a failure
/// leaves the task where it was. Returns the task's new line and the change.
pub fn move_task_under_todo(path: &Path, line: usize, expected: &str, name: &str, new_line: &str) -> Result<(usize, FileChange)> {
    one_task_line(new_line)?;
    rewrite(path, |lines| {
        let i = find_line(lines, line, expected)
            .filter(|&i| parse_line(&lines[i], i).is_some())
            .ok_or_else(|| WriteError::Stale("This task changed in the file since it was shown. It's been reloaded; try again.".into()))?;
        lines.remove(i);
        Ok(append_under_todo(lines, name, new_line))
    })
}

fn one_task_line(task_line: &str) -> Result<()> {
    if task_line.contains(['\n', '\r']) || parse_line(task_line, 0).is_none() {
        return Err(WriteError::Invalid("That isn't one task line.".into()));
    }
    Ok(())
}

/// Adds a task line at the end of a note's `## heading` section. Returns its line and the change.
pub fn add_task_under(path: &Path, name: &str, task_line: &str) -> Result<(usize, FileChange)> {
    one_task_line(task_line)?;
    rewrite(path, |lines| Ok(append_under(lines, name, task_line)))
}

/// A property value as YAML: as it is when it reads back as the same string, else double-quoted.
/// Words that older YAML readers take for true, false or nothing are quoted too.
pub fn yaml_scalar(v: &str) -> String {
    let old_yaml = ["y", "n", "yes", "no", "on", "off", "true", "false", "null", "~"].contains(&v.to_lowercase().as_str());
    let plain = !old_yaml
        && serde_yaml_ng::from_str::<serde_json::Value>(&format!("k: {v}"))
            .ok()
            .is_some_and(|y| y.get("k").and_then(|x| x.as_str()) == Some(v) && y.as_object().is_some_and(|o| o.len() == 1));
    if plain {
        v.to_string()
    } else {
        format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// The text with one frontmatter property changed, added (before the closing `---`) or removed
/// (with None), and every other byte as it was. Frontmatter is started when there's none.
pub fn with_property(src: &str, key: &str, value: Option<&str>) -> Result<String> {
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(WriteError::Invalid(format!("Not a property name: {key}")));
    }
    if value.is_some_and(|v| v.contains(['\n', '\r'])) {
        return Err(WriteError::Invalid("A property here is one line.".into()));
    }
    let bom = if src.starts_with('\u{feff}') { "\u{feff}" } else { "" };
    let (mut lines, eol, trailing) = split_lines(&src[bom.len()..]);
    let entry = value.map(|v| format!("{key}: {}", yaml_scalar(v.trim())));
    let opened = lines.first().is_some_and(|l| l.trim_end() == "---");
    let close = if opened { (1..lines.len()).find(|&i| matches!(lines[i].trim_end(), "---" | "...")) } else { None };
    match (opened, close) {
        (true, None) => return Err(WriteError::Invalid("This note's properties aren't closed with ---, so they weren't changed.".into())),
        (false, _) => {
            let Some(e) = entry else { return Ok(src.to_string()) };
            lines.splice(0..0, ["---".to_string(), e, "---".to_string()]);
        }
        (true, Some(c)) => {
            let at = (1..c).find(|&i| lines[i].strip_prefix(key).is_some_and(|r| r.starts_with(':')));
            match (at, entry) {
                (Some(i), e) => {
                    // A value that goes on below (a list, a block) goes with it.
                    let end = (i + 1..c).find(|&j| !lines[j].starts_with([' ', '\t', '-'])).unwrap_or(c);
                    lines.splice(i..end, e);
                }
                (None, Some(e)) => lines.insert(c, e),
                (None, None) => return Ok(src.to_string()),
            }
        }
    }
    Ok(format!("{bom}{}", join_lines(&lines, &eol, trailing || src.is_empty())))
}

/// Changes one frontmatter property of a note, as `with_property`.
pub fn set_property(path: &Path, key: &str, value: Option<&str>) -> Result<FileChange> {
    let l = lock(path);
    let _g = l.lock().unwrap();
    check_conflict_copies(path)?;
    let src = read_note(path)?;
    let text = with_property(&src, key, value)?;
    if text != src {
        write_atomic(path, text.as_bytes(), false)?;
    }
    Ok(FileChange { path: path.to_path_buf(), before: Some(src), version: version(text.as_bytes()) })
}

/// Moves a task from one note to the end of another's `## heading` section, as `new_line`: the
/// destination is written first, and put back if the source then can't be. Both or neither.
/// Returns the task's line in the destination and the two changes.
pub fn move_task(from: &Path, line: usize, expected: &str, to: &Path, name: &str, new_line: &str) -> Result<(usize, [FileChange; 2])> {
    one_task_line(new_line)?;
    if from == to {
        return Err(WriteError::Invalid("The task is already in that note.".into()));
    }
    let (a, b) = (lock(from), lock(to));
    let (first, second) = if from < to { (&a, &b) } else { (&b, &a) };
    let _g1 = first.lock().unwrap();
    let _g2 = second.lock().unwrap();
    check_conflict_copies(from)?;
    check_conflict_copies(to)?;
    let src_from = read_note(from)?;
    let src_to = read_note(to)?;
    let (mut from_lines, from_eol, from_trailing) = split_lines(&src_from);
    let i = find_line(&from_lines, line, expected)
        .filter(|&i| parse_line(&from_lines[i], i).is_some())
        .ok_or_else(|| WriteError::Stale("This task changed in the file since it was shown. It's been reloaded; try again.".into()))?;
    from_lines.remove(i);
    let (mut to_lines, to_eol, to_trailing) = split_lines(&src_to);
    let at = append_under(&mut to_lines, name, new_line);
    let new_from = join_lines(&from_lines, &from_eol, from_trailing);
    let new_to = join_lines(&to_lines, &to_eol, to_trailing || src_to.is_empty());
    write_atomic(to, new_to.as_bytes(), false)?;
    if let Err(e) = write_atomic(from, new_from.as_bytes(), false) {
        write_atomic(to, src_to.as_bytes(), false)
            .map_err(|r| WriteError::Io(format!("{e}; and putting {} back failed too: {r}", to.display())))?;
        return Err(e);
    }
    Ok((
        at,
        [
            FileChange { path: to.to_path_buf(), version: version(new_to.as_bytes()), before: Some(src_to) },
            FileChange { path: from.to_path_buf(), version: version(new_from.as_bytes()), before: Some(src_from) },
        ],
    ))
}

/// Rewrites two notes together: `f` gets their texts and gives what they become. Both are written,
/// or neither (the first is put back when the second fails).
pub fn rewrite_pair(a: &Path, b: &Path, f: impl FnOnce(&str, &str) -> Result<(String, String)>) -> Result<[FileChange; 2]> {
    if a == b {
        return Err(WriteError::Invalid("Both are the same note.".into()));
    }
    let (la, lb) = (lock(a), lock(b));
    let (first, second) = if a < b { (&la, &lb) } else { (&lb, &la) };
    let _g1 = first.lock().unwrap();
    let _g2 = second.lock().unwrap();
    check_conflict_copies(a)?;
    check_conflict_copies(b)?;
    let (src_a, src_b) = (read_note(a)?, read_note(b)?);
    let (new_a, new_b) = f(&src_a, &src_b)?;
    write_atomic(b, new_b.as_bytes(), false)?;
    if let Err(e) = write_atomic(a, new_a.as_bytes(), false) {
        write_atomic(b, src_b.as_bytes(), false)
            .map_err(|r| WriteError::Io(format!("{e}; and putting {} back failed too: {r}", b.display())))?;
        return Err(e);
    }
    Ok([
        FileChange { path: a.to_path_buf(), version: version(new_a.as_bytes()), before: Some(src_a) },
        FileChange { path: b.to_path_buf(), version: version(new_b.as_bytes()), before: Some(src_b) },
    ])
}

/// Undoes whole-file changes: each file goes back to its text before (a file the change created
/// is deleted), but only when every one of them is still as the change left it. All or none.
pub fn restore_files(changes: &[FileChange]) -> Result<()> {
    let mut paths: Vec<&PathBuf> = changes.iter().map(|c| &c.path).collect();
    paths.sort();
    paths.dedup();
    let locks: Vec<Arc<Mutex<()>>> = paths.iter().map(|p| lock(p)).collect();
    let _guards: Vec<_> = locks.iter().map(|l| l.lock().unwrap()).collect();
    let stale = || WriteError::Stale("That note has changed since, so it can't be undone.".into());
    let mut now = Vec::new();
    for c in changes {
        check_conflict_copies(&c.path)?;
        let bytes = fs::read(&c.path).map_err(|_| stale())?;
        if version(&bytes) != c.version {
            return Err(stale());
        }
        now.push(bytes);
    }
    for (k, c) in changes.iter().enumerate() {
        let done = match &c.before {
            Some(t) => write_atomic(&c.path, t.as_bytes(), false),
            None => fs::remove_file(&c.path).map_err(io),
        };
        if let Err(e) = done {
            for (j, d) in changes.iter().enumerate().take(k) {
                let _ = write_atomic(&d.path, &now[j], false);
            }
            return Err(e);
        }
    }
    Ok(())
}

/// What `save_file` compares: the SHA-256 of the file's bytes, in hex.
pub fn version(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// Saves the editor's text over the whole file, if the file is still the one read (`base`, its
/// `version`). A file read with CRLF endings keeps them even when the editor sends LF. Returns the
/// new version. Saving the same text writes nothing.
pub fn save_file(path: &Path, content: &str, base: &str) -> Result<String> {
    save_file_as(path, content, base, false)
}

/// `save_file`, keeping the file's modification time when `keep_mtime`: for Wiki health's
/// safe fixes, which shouldn't make a page look newly written.
pub fn save_file_as(path: &Path, content: &str, base: &str, keep_mtime: bool) -> Result<String> {
    let l = lock(path);
    let _g = l.lock().unwrap();
    check_conflict_copies(path)?;
    let old = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WriteError::NotFound("The file isn't there any more: it was moved or deleted.".into()))
        }
        Err(e) => return Err(io(e)),
    };
    let now = version(&old);
    if now != base {
        return Err(WriteError::Changed { content: String::from_utf8_lossy(&old).into_owned(), version: now });
    }
    // The editor sends LF: each line the file had keeps its own ending (CRLF, or a mix), and new
    // lines take the file's commoner one.
    let text = if !content.contains("\r\n") && old.windows(2).any(|w| w == b"\r\n") {
        let (_, eol, _) = split_lines(&String::from_utf8_lossy(&old));
        let (ls, _, trailing) = split_lines(content);
        join_lines(&ls, &eol, trailing)
    } else {
        content.to_string()
    };
    if text.as_bytes() == old.as_slice() {
        return Ok(now);
    }
    write_atomic(path, text.as_bytes(), keep_mtime)?;
    Ok(version(text.as_bytes()))
}

/// Creates a new file, and its folders, refusing when something is already there. Returns its
/// version.
pub fn create_file(path: &Path, content: &str) -> Result<String> {
    create_file_bytes(path, content.as_bytes())?;
    Ok(version(content.as_bytes()))
}

pub fn create_file_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let l = lock(path);
    let _g = l.lock().unwrap();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let exists = || WriteError::Exists(format!("There's already a file called “{name}” there."));
    if path.exists() {
        return Err(exists());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io)?;
    }
    check_conflict_copies(path)?;
    let mut f = fs::File::options().write(true).create_new(true).open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => exists(),
        _ => io(e),
    })?;
    if let Err(e) = f.write_all(bytes) {
        drop(f);
        let _ = fs::remove_file(path);
        return Err(io(e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rename that fails while the file is held open is tried again, with a growing wait, up to
    /// ten times; any other error, or success, returns at once.
    #[test]
    fn a_rename_held_up_by_an_open_file_is_tried_again() {
        use std::cell::RefCell;
        use std::io::{Error, ErrorKind};
        use std::time::Duration;
        let busy = || Error::new(ErrorKind::PermissionDenied, "held open");
        let held = |e: &Error| e.kind() == ErrorKind::PermissionDenied;
        // Held open twice, then free.
        let (calls, waits) = (RefCell::new(0), RefCell::new(vec![]));
        let op = || {
            *calls.borrow_mut() += 1;
            if *calls.borrow() <= 2 {
                Err(busy())
            } else {
                Ok(7)
            }
        };
        assert_eq!(retry_held(op, held, |d| waits.borrow_mut().push(d)).unwrap(), 7);
        assert_eq!((*calls.borrow(), waits.borrow().clone()), (3, vec![Duration::from_millis(10), Duration::from_millis(20)]));
        // Held open throughout: ten tries, about a second of waiting, then the error.
        let (calls, waits) = (RefCell::new(0), RefCell::new(vec![]));
        let r = retry_held(
            || {
                *calls.borrow_mut() += 1;
                Err::<(), _>(busy())
            },
            held,
            |d| waits.borrow_mut().push(d),
        );
        assert_eq!(r.unwrap_err().kind(), ErrorKind::PermissionDenied);
        assert_eq!(*calls.borrow(), RENAME_TRIES);
        let total: Duration = waits.borrow().iter().sum();
        assert!(waits.borrow().iter().all(|d| *d <= RENAME_WAIT_MAX));
        assert!((Duration::from_millis(800)..Duration::from_millis(1500)).contains(&total), "{total:?}");
        // Any other error isn't waited on.
        let (calls, waits) = (RefCell::new(0), RefCell::new(vec![]));
        let r = retry_held(
            || {
                *calls.borrow_mut() += 1;
                Err::<(), _>(Error::new(ErrorKind::NotFound, "gone"))
            },
            held,
            |d| waits.borrow_mut().push(d),
        );
        assert_eq!((r.unwrap_err().kind(), *calls.borrow(), waits.borrow().len()), (ErrorKind::NotFound, 1, 0));
    }

    /// Only Windows' access denied, sharing and lock violations count as held open, and only there.
    #[test]
    fn only_windows_lock_errors_are_waited_on() {
        let os = |n| std::io::Error::from_raw_os_error(n);
        assert_eq!([5, 32, 33].map(|n| held_open(&os(n))), [cfg!(windows); 3]);
        assert!(!held_open(&os(2)));
        assert!(!held_open(&std::io::Error::other("x")));
    }

    // From the previous app's tasks-index.test.ts and mutations-workflow.test.ts.
    #[test]
    fn toggles() {
        assert_eq!(toggled("- [ ] Ship it ^rank-1024", true, "2026-10-02"), "- [x] Ship it ✅ 2026-10-02 ^rank-1024");
        assert_eq!(toggled("  * [x] Ship it ✅ 2026-09-30 ^rank-2048", false, "2026-10-02"), "  * [ ] Ship it");
        assert_eq!(toggled("+ [X] Done ✅ 2026-09-30", true, "2026-10-02"), "+ [x] Done ✅ 2026-10-02");
        assert_eq!(toggled("- [ ] Due 📅 2026-10-05", true, "2026-10-02"), "- [x] Due 📅 2026-10-05 ✅ 2026-10-02");
    }

    #[test]
    fn dates() {
        assert_eq!(with_date("- [ ] A ^rank-5", DateKind::Due, Some("2026-10-05")).unwrap(), "- [ ] A 📅 2026-10-05 ^rank-5");
        assert_eq!(with_date("- [ ] A 📅 2026-10-01 #x", DateKind::Due, Some("2026-10-05")).unwrap(), "- [ ] A #x 📅 2026-10-05");
        assert_eq!(with_date("- [ ] A ⏳ 2026-10-01 📅 2026-10-09", DateKind::Scheduled, None).unwrap(), "- [ ] A 📅 2026-10-09");
        assert!(with_date("- [ ] A", DateKind::Due, Some("next week")).is_err());
    }

    #[test]
    fn ranks() {
        assert_eq!(rank_in_gap(None, None), Some(1024));
        assert_eq!(rank_in_gap(None, Some(1024)), Some(512));
        assert_eq!(rank_in_gap(Some(1024), None), Some(2048));
        assert_eq!(rank_in_gap(Some(1024), Some(2048)), Some(1536));
        assert_eq!(rank_in_gap(Some(5), Some(6)), None);
        assert_eq!(rank_in_gap(None, Some(1)), None);
        assert_eq!(with_rank("- [ ] A ^rank-9", 2048), "- [ ] A ^rank-2048");
        assert_eq!(with_rank("- [ ] Reopened ✅ 2026-09-01", 1024), "- [ ] Reopened ^rank-1024");
        assert_eq!(with_rank("- [x] Done ✅ 2026-09-01", 1024), "- [x] Done ✅ 2026-09-01 ^rank-1024");
    }

    #[test]
    fn finds_moved_lines() {
        let lines: Vec<String> = ["# H", "- [ ] A ^rank-1", "- [ ] B", "- [ ] B"].iter().map(|s| s.to_string()).collect();
        assert_eq!(find_line(&lines, 1, "- [ ] A ^rank-2048"), Some(1));
        assert_eq!(find_line(&lines, 0, "- [ ] A"), Some(1));
        // Ambiguous or gone: stale.
        assert_eq!(find_line(&lines, 0, "- [ ] B"), None);
        assert_eq!(find_line(&lines, 1, "- [ ] C"), None);
        assert_eq!(find_line(&lines, 2, "- [ ] B"), Some(2));
    }

    #[test]
    fn line_endings_round_trip() {
        for src in ["a\nb\n", "a\r\nb\r\n", "a\nb", "a\r\nb", ""] {
            let (l, eol, t) = split_lines(src);
            assert_eq!(join_lines(&l, &eol, t), src);
        }
    }

    #[test]
    fn mixed_line_endings_stay_mixed() {
        let src = "a\r\nb\nc\r\nd\n";
        let (mut l, eol, t) = split_lines(src);
        assert_eq!(l, ["a", "b", "c", "d"]);
        assert_eq!(join_lines(&l, &eol, t), src);
        l[1] = "B".into();
        assert_eq!(join_lines(&l, &eol, t), "a\r\nB\nc\r\nd\n");
        l.insert(3, "new".into());
        assert_eq!(join_lines(&l, &eol, t), "a\r\nB\nc\r\nnew\r\nd\n");
        let (mut l, eol, t) = split_lines(src);
        l.remove(0);
        assert_eq!(join_lines(&l, &eol, t), "b\nc\r\nd\n");
        // A lone CR at the very end is text, not an ending.
        let (l, eol, t) = split_lines("a\r\nb\r");
        assert_eq!(l, ["a", "b\r"]);
        assert_eq!(join_lines(&l, &eol, t), "a\r\nb\r");
    }

    #[test]
    fn task_edit_in_a_mixed_file_touches_one_line() {
        let src = "# H\r\n- [ ] one\n- [ ] two\r\nend\n";
        let (_d, p) = temp("Mixed.md", src);
        edit_task(&p, 2, "- [ ] two", |l| toggled(l, true, "2026-10-03")).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "# H\r\n- [ ] one\n- [x] two ✅ 2026-10-03\r\nend\n");
    }

    #[test]
    fn tasks_in_a_callout_keep_their_markers() {
        let src = "> [!todo] This week\n> - [ ] Call Lena\n> > - [ ] Nested 📅 2026-10-01\n";
        let (_d, p) = temp("Callout.md", src);
        edit_task(&p, 1, "> - [ ] Call Lena", |l| in_quote(l, |b| toggled(b, true, "2026-10-03"))).unwrap();
        edit_task(&p, 2, "> > - [ ] Nested 📅 2026-10-01", |l| in_quote(l, |b| with_date(b, DateKind::Due, Some("2026-10-09")).unwrap()))
            .unwrap();
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            "> [!todo] This week\n> - [x] Call Lena ✅ 2026-10-03\n> > - [ ] Nested 📅 2026-10-09\n"
        );
        let root = p.parent().unwrap();
        let t = |l: usize, s: &str| TaskRef { path: "Callout.md".into(), line: l, line_text: s.into() };
        bulk_rank(root, &[t(2, "> > - [ ] Nested 📅 2026-10-09")]).unwrap();
        assert!(fs::read_to_string(&p).unwrap().contains("\n> > - [ ] Nested 📅 2026-10-09 ^rank-1024\n"));
    }

    #[test]
    fn save_keeps_each_lines_ending() {
        let src = "a\r\nb\nc\r\n";
        let (_d, p) = temp("Mixed.md", src);
        save_file(&p, "a\nb2\nc\nd\n", &version(src.as_bytes())).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "a\r\nb2\nc\r\nd\r\n");
    }

    #[test]
    fn scratchpad_capture_goes_after_a_bom() {
        let (_d, p) = temp("Me. Scratchpad.md", "\u{feff}# Me. Scratchpad\n\n## old\n");
        capture_thought(&p, "new", "2026-10-03 09:00").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "\u{feff}# Me. Scratchpad\n\n## 2026-10-03 09:00\n\nnew\n\n## old\n");
        assert_eq!(pinned_header_len("\u{feff}body"), 3);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_writes_keep_links_permissions_and_long_names() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real.md");
        fs::write(&real, "old").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        let link = d.path().join("link.md");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_atomic(&link, b"new", false).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        assert_eq!(fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o600);
        let long = d.path().join(format!("{}.md", "n".repeat(250)));
        fs::write(&long, "old").unwrap();
        write_atomic(&long, b"new", false).unwrap();
        assert_eq!(fs::read_to_string(&long).unwrap(), "new");
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 3);
    }

    fn temp(name: &str, text: &str) -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(name);
        fs::write(&p, text).unwrap();
        (d, p)
    }

    #[test]
    fn replaces_a_task_with_two_lines_and_undoes_it() {
        let src = "# H\r\n- [ ] trash 🔁 every Sunday 📅 2021-04-25\r\n- [ ] other\r\n";
        let (_d, p) = temp("A.md", src);
        let lines = vec![
            "- [ ] trash 🔁 every Sunday 📅 2021-05-02".to_string(),
            "- [x] trash 🔁 every Sunday 📅 2021-04-25 ✅ 2021-04-24".to_string(),
        ];
        let (i, _) = replace_task(&p, 1, "- [ ] trash 🔁 every Sunday 📅 2021-04-25", &lines).unwrap();
        assert_eq!(i, 1);
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            "# H\r\n- [ ] trash 🔁 every Sunday 📅 2021-05-02\r\n- [x] trash 🔁 every Sunday 📅 2021-04-25 ✅ 2021-04-24\r\n- [ ] other\r\n"
        );
        restore_task(&p, 1, &lines, "- [ ] trash 🔁 every Sunday 📅 2021-04-25", None).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), src);
        // Deleted on completion, then put back.
        let (_, around) = replace_task(&p, 2, "- [ ] other", &[]).unwrap();
        assert_eq!(around, Around { before: Some("- [ ] trash 🔁 every Sunday 📅 2021-04-25".into()), after: None });
        assert_eq!(fs::read_to_string(&p).unwrap(), "# H\r\n- [ ] trash 🔁 every Sunday 📅 2021-04-25\r\n");
        restore_task(&p, 2, &[], "- [ ] other", Some(&around)).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), src);
        // Stale once the written lines have changed.
        replace_task(&p, 2, "- [ ] other", &["- [-] other ❌ 2026-10-02".into()]).unwrap();
        fs::write(&p, "# H\n- [ ] changed\n").unwrap();
        assert!(matches!(restore_task(&p, 2, &["- [-] other ❌ 2026-10-02".into()], "- [ ] other", None), Err(WriteError::Stale(_))));
        assert!(matches!(replace_task(&p, 1, "- [ ] a\nb", &["x\ny".into()]), Err(WriteError::Stale(_))));
    }

    #[test]
    fn a_deleted_task_goes_back_between_the_lines_it_was_between() {
        let src = "# Orbit App\n- [ ] Call Maya\n- [ ] Book the room\n- [ ] Send the notes\n\n# Acme\n\n```\ncode\n```\n";
        let (_d, p) = temp("A.md", src);
        let (i, around) = replace_task(&p, 2, "- [ ] Book the room", &[]).unwrap();
        assert_eq!((i, around.before.as_deref(), around.after.as_deref()), (2, Some("- [ ] Call Maya"), Some("- [ ] Send the notes")));
        // Lines added above since: it goes back beside its neighbours, not at the old line number.
        let gone = fs::read_to_string(&p).unwrap();
        fs::write(&p, format!("# Lena\n- [ ] Plan the week\n\n{gone}")).unwrap();
        restore_task(&p, i, &[], "- [ ] Book the room", Some(&around)).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), format!("# Lena\n- [ ] Plan the week\n\n{src}"));
        // Its neighbours no longer meet anywhere: nothing is written.
        let (i, around) = replace_task(&p, 5, "- [ ] Book the room", &[]).unwrap();
        let apart = "# Orbit App\n- [ ] Call Maya\n\n# Acme\n- [ ] Send the notes\n";
        fs::write(&p, apart).unwrap();
        assert!(matches!(restore_task(&p, i, &[], "- [ ] Book the room", Some(&around)), Err(WriteError::Stale(_))));
        // They meet in two places, neither at its line: not guessed.
        fs::write(&p, "intro\nmore\n- [ ] Call Maya\n- [ ] Send the notes\n\n- [ ] Call Maya\n- [ ] Send the notes\n").unwrap();
        assert!(matches!(restore_task(&p, 0, &[], "- [ ] Book the room", Some(&around)), Err(WriteError::Stale(_))));
        // An undo remembered before the neighbours were kept can't be placed.
        fs::write(&p, src).unwrap();
        assert!(matches!(restore_task(&p, 2, &[], "- [ ] Book the room", None), Err(WriteError::Stale(_))));
        assert_eq!(fs::read_to_string(&p).unwrap(), src);
        // The only line of a note comes back to an empty one.
        let (_d2, q) = temp("B.md", "- [ ] Call Maya\n");
        let (i, around) = replace_task(&q, 0, "- [ ] Call Maya", &[]).unwrap();
        assert_eq!(around, Around::default());
        restore_task(&q, i, &[], "- [ ] Call Maya", Some(&around)).unwrap();
        assert_eq!(fs::read_to_string(&q).unwrap().trim_end(), "- [ ] Call Maya");
    }

    #[test]
    fn a_move_never_replaces_a_file() {
        let d = tempfile::tempdir().unwrap();
        let (a, b, c) = (d.path().join("A.md"), d.path().join("B.md"), d.path().join("C.md"));
        fs::write(&a, "a").unwrap();
        fs::write(&b, "b").unwrap();
        assert_eq!(rename_new(&a, &b).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!((fs::read_to_string(&a).unwrap(), fs::read_to_string(&b).unwrap()), ("a".into(), "b".into()));
        rename_new(&a, &c).unwrap();
        assert!(!a.exists());
        assert_eq!(fs::read_to_string(&c).unwrap(), "a");
        assert_eq!(checked_rename(&c, &b).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        assert!(same_file(&b, &b) && !same_file(&b, &c) && !same_file(&b, &a));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_save_keeps_the_creation_time_and_extended_attributes() {
        use std::os::macos::fs::FileTimesExt;
        let (_d, p) = temp("A.md", "one\n");
        let born = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        fs::File::options().write(true).open(&p).unwrap().set_times(fs::FileTimes::new().set_created(born)).unwrap();
        xattr::set(&p, "com.apple.metadata:_kMDItemUserTags", b"tags").unwrap();
        xattr::set(&p, "org.example.note", b"kept").unwrap();
        write_atomic(&p, b"two\n", true).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "two\n");
        assert_eq!(fs::metadata(&p).unwrap().created().unwrap(), born);
        assert_eq!(xattr::get(&p, "com.apple.metadata:_kMDItemUserTags").unwrap().as_deref(), Some(&b"tags"[..]));
        assert_eq!(xattr::get(&p, "org.example.note").unwrap().as_deref(), Some(&b"kept"[..]));
    }

    #[test]
    fn sweeps_old_temporary_files_and_reports_half_renamed_notes() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        fs::create_dir_all(r.join("wiki/entities")).unwrap();
        let aged = |rel: &str| {
            fs::write(r.join(rel), "x").unwrap();
            let then = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
            fs::File::options().write(true).open(r.join(rel)).unwrap().set_modified(then).unwrap();
        };
        aged(".brainstead-4242-7.tmp");
        aged("wiki/entities/.brainstead-4242-8.tmp");
        aged("wiki/entities/.rename-4242");
        aged("wiki/entities/Orbit App.md");
        aged(".rename-notes");
        // One being written right now stays.
        fs::write(r.join(".brainstead-4243-0.tmp"), "x").unwrap();
        assert_eq!(sweep_leftovers(r), vec!["wiki/entities/.rename-4242".to_string()]);
        assert!(!r.join(".brainstead-4242-7.tmp").exists() && !r.join("wiki/entities/.brainstead-4242-8.tmp").exists());
        for kept in [".brainstead-4243-0.tmp", "wiki/entities/.rename-4242", "wiki/entities/Orbit App.md", ".rename-notes"] {
            assert!(r.join(kept).exists(), "{kept}");
        }
    }

    #[test]
    fn ticks_any_status() {
        assert_eq!(toggled("- [/] half way ^rank-3", true, "2026-10-02"), "- [x] half way ✅ 2026-10-02 ^rank-3");
        assert_eq!(toggled("1. [-] dropped ❌ 2026-10-01", false, "2026-10-02"), "1. [ ] dropped ❌ 2026-10-01");
    }

    #[test]
    fn edits_only_the_line_and_keeps_mtime() {
        let (_d, p) = temp("Note.md", "# Note\r\n\r\n- [ ] One ^rank-1024\r\n- [ ] Two\r\n");
        let old = fs::metadata(&p).unwrap().modified().unwrap() - std::time::Duration::from_secs(3600);
        fs::File::options().write(true).open(&p).unwrap().set_modified(old).unwrap();
        let (i, new) = edit_task(&p, 2, "- [ ] One ^rank-1024", |l| toggled(l, true, "2026-10-02")).unwrap();
        assert_eq!((i, new.as_str()), (2, "- [x] One ✅ 2026-10-02 ^rank-1024"));
        assert_eq!(fs::read_to_string(&p).unwrap(), "# Note\r\n\r\n- [x] One ✅ 2026-10-02 ^rank-1024\r\n- [ ] Two\r\n");
        assert_eq!(fs::metadata(&p).unwrap().modified().unwrap(), old);
        // Moved down two lines by an edit elsewhere: still found.
        fs::write(&p, "# Note\r\nnew\r\nnew\r\n\r\n- [x] One ✅ 2026-10-02 ^rank-1024\r\n- [ ] Two\r\n").unwrap();
        assert_eq!(edit_task(&p, 2, "- [x] One ✅ 2026-10-02 ^rank-1024", |l| toggled(l, false, "x")).unwrap().0, 4);
        // Changed text: stale, file untouched.
        let before = fs::read_to_string(&p).unwrap();
        assert!(matches!(edit_task(&p, 5, "- [ ] Two changed", |l| l.to_string()), Err(WriteError::Stale(_))));
        assert_eq!(fs::read_to_string(&p).unwrap(), before);
    }

    #[test]
    fn refuses_beside_conflict_copies() {
        let (d, p) = temp("Note.md", "- [ ] A\n");
        fs::write(d.path().join("Note 2.md"), "- [ ] A\n").unwrap();
        assert!(matches!(edit_task(&p, 0, "- [ ] A", |l| toggled(l, true, "2026-10-02")), Err(WriteError::Conflict(_))));
        fs::remove_file(d.path().join("Note 2.md")).unwrap();
        fs::write(d.path().join("Note 123.md"), "").unwrap();
        assert!(edit_task(&p, 0, "- [ ] A", |l| toggled(l, true, "2026-10-02")).is_ok());
    }

    #[test]
    fn captures_tasks_under_other() {
        let (_d, p) = temp("Me. To Do List.md", "# To Do\n\n#### Other\n\n- [ ] Older\n\n#### Done\n");
        capture_task(&p, "  Call   Lena\nabout comms ").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "# To Do\n\n#### Other\n\n- [ ] Call Lena about comms\n- [ ] Older\n\n#### Done\n");
        let (_d2, p2) = temp("Me. To Do List.md", "# To Do\n\nno section\n\n");
        capture_task(&p2, "New").unwrap();
        assert_eq!(fs::read_to_string(&p2).unwrap(), "# To Do\n\nno section\n\n#### Other\n\n- [ ] New\n");
        assert!(matches!(capture_task(&p2.with_file_name("missing.md"), "x"), Err(WriteError::NotFound(_))));
        assert!(matches!(capture_task(&p2, "   "), Err(WriteError::Invalid(_))));
    }

    // The previous app's seven pinnedHeaderLength cases (routes/capture.test.ts).
    #[test]
    fn scratchpad_header() {
        assert_eq!(pinned_header_len(""), 0);
        assert_eq!(pinned_header_len("## 2026-01-01 10:00\n"), 0);
        assert_eq!(pinned_header_len("# Me. Scratchpad\n\n## x"), 18);
        let sys = "# Me. Scratchpad\n\n> **This is a system note** — x\n> more\n\n## old";
        assert_eq!(&sys[pinned_header_len(sys)..], "## old");
        let quote = "# T\n\n> just a quote\n\nbody";
        assert_eq!(&quote[pinned_header_len(quote)..], "> just a quote\n\nbody");
        let only = "> **This is a system note** — y\n";
        assert_eq!(pinned_header_len(only), only.len());
        assert_eq!(pinned_header_len("# T"), 3);
    }

    #[test]
    fn captures_thoughts() {
        let (_d, p) = temp("Me. Scratchpad.md", "# Me. Scratchpad\n\n> **This is a system note** — notes.\n\n## 2026-10-01 09:00\n\nold\n");
        capture_thought(&p, "first line\nsecond", "2026-10-02 10:15").unwrap();
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            "# Me. Scratchpad\n\n> **This is a system note** — notes.\n\n## 2026-10-02 10:15\n\nfirst line\nsecond\n\n## 2026-10-01 09:00\n\nold\n"
        );
        let missing = p.with_file_name("new.md");
        capture_thought(&missing, "hi", "2026-10-02 10:16").unwrap();
        assert_eq!(fs::read_to_string(&missing).unwrap(), "# Me. Scratchpad\n\n## 2026-10-02 10:16\n\nhi\n\n");
    }

    #[test]
    fn bulk_ranks_in_order() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("A.md"), "- [ ] a1\n- [ ] a2 ^rank-5\n").unwrap();
        fs::write(d.path().join("B.md"), "- [ ] b1\n").unwrap();
        let t = |p: &str, l: usize, s: &str| TaskRef { path: p.into(), line: l, line_text: s.into() };
        let missed = bulk_rank(
            d.path(),
            &[t("B.md", 0, "- [ ] b1"), t("A.md", 1, "- [ ] a2 ^rank-5"), t("A.md", 0, "- [ ] a1"), t("A.md", 9, "- [ ] gone")],
        )
        .unwrap();
        assert_eq!(missed.len(), 1);
        assert_eq!(fs::read_to_string(d.path().join("A.md")).unwrap(), "- [ ] a1 ^rank-3072\n- [ ] a2 ^rank-2048\n");
        assert_eq!(fs::read_to_string(d.path().join("B.md")).unwrap(), "- [ ] b1 ^rank-1024\n");
    }

    #[test]
    fn saves_only_what_was_read() {
        let (d, p) = temp("Note.md", "one\ntwo\n");
        let v = version(b"one\ntwo\n");
        let v2 = save_file(&p, "one\n2\n", &v).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "one\n2\n");
        assert_eq!(v2, version(b"one\n2\n"));
        // Changed on disk since: refused, with what's there now.
        fs::write(&p, "theirs\n").unwrap();
        match save_file(&p, "mine\n", &v2) {
            Err(WriteError::Changed { content, version: cv }) => {
                assert_eq!(content, "theirs\n");
                assert_eq!(cv, version(b"theirs\n"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(fs::read_to_string(&p).unwrap(), "theirs\n");
        // A conflict copy beside it.
        fs::write(d.path().join("Note 2.md"), "x").unwrap();
        assert!(matches!(save_file(&p, "mine\n", &version(b"theirs\n")), Err(WriteError::Conflict(_))));
        fs::remove_file(d.path().join("Note 2.md")).unwrap();
        fs::remove_file(&p).unwrap();
        assert!(matches!(save_file(&p, "mine\n", &v2), Err(WriteError::NotFound(_))));
    }

    #[test]
    fn save_keeps_crlf() {
        let (_d, p) = temp("Win.md", "a\r\nb\r\n");
        save_file(&p, "a\nB\n", &version(b"a\r\nb\r\n")).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"a\r\nB\r\n");
    }

    #[test]
    fn creates_without_overwriting() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("People/New.md");
        let v = create_file(&p, "# New\n").unwrap();
        assert_eq!(v, version(b"# New\n"));
        assert_eq!(fs::read_to_string(&p).unwrap(), "# New\n");
        assert!(matches!(create_file(&p, "other"), Err(WriteError::Exists(_))));
        assert_eq!(fs::read_to_string(&p).unwrap(), "# New\n");
    }

    #[test]
    fn sets_start_and_created_dates() {
        let l = "- [ ] ship 📅 2026-10-09 ^rank-2048";
        let a = with_date(l, DateKind::Start, Some("2026-10-05")).unwrap();
        assert_eq!(a, "- [ ] ship 📅 2026-10-09 🛫 2026-10-05 ^rank-2048");
        let b = with_date(&a, DateKind::Created, Some("2026-10-02")).unwrap();
        assert_eq!(b, "- [ ] ship 📅 2026-10-09 🛫 2026-10-05 ➕ 2026-10-02 ^rank-2048");
        assert_eq!(with_date(&b, DateKind::Start, None).unwrap(), "- [ ] ship 📅 2026-10-09 ➕ 2026-10-02 ^rank-2048");
        // With the emoji's variation selector, as some editors write it.
        assert_eq!(with_date("- [ ] x ➕\u{fe0f} 2026-01-01", DateKind::Created, Some("2026-02-02")).unwrap(), "- [ ] x ➕ 2026-02-02");
    }
    #[test]
    fn sets_contexts() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let l = "- [ ] Call Sam 📅 2026-10-05 ^rank-1024";
        let a = with_contexts(l, &names(&["calls", "@office/desk", "#context/calls"])).unwrap();
        assert_eq!(a, "- [ ] Call Sam #context/calls #context/office/desk 📅 2026-10-05 ^rank-1024");
        // Replaced where they were, the rest untouched.
        let b = "* [ ] Call #context/personal Sam  about it #x 📅 2026-10-05";
        assert_eq!(with_contexts(b, &names(&["calls"])).unwrap(), "* [ ] Call #context/calls Sam  about it #x 📅 2026-10-05");
        assert_eq!(with_contexts(b, &[]).unwrap(), "* [ ] Call Sam  about it #x 📅 2026-10-05");
        assert_eq!(with_contexts("- [ ] #context/a lead", &names(&["b"])).unwrap(), "- [ ] #context/b lead");
        assert_eq!(with_contexts("- [ ] plain", &names(&["b"])).unwrap(), "- [ ] plain #context/b");
        assert!(with_contexts(l, &names(&["two words"])).is_err());
        assert!(with_contexts(l, &names(&["123"])).is_err());
    }

    #[test]
    fn sets_effort() {
        let l = "- [ ] Draft 🔁 every week 📅 2026-10-05";
        let a = with_effort(l, Some("1h30m")).unwrap();
        assert_eq!(a, "- [ ] Draft [effort:: 1h30m] 🔁 every week 📅 2026-10-05");
        assert_eq!(with_effort(&a, Some("15m")).unwrap(), "- [ ] Draft [effort:: 15m] 🔁 every week 📅 2026-10-05");
        assert_eq!(with_effort("- [ ] x (effort:: 2h) #t", Some("2d")).unwrap(), "- [ ] x [effort:: 2d] #t");
        assert_eq!(with_effort(&a, None).unwrap(), l);
        assert!(matches!(with_effort(l, Some("a while")), Err(WriteError::Invalid(_))));
    }

    #[test]
    fn sets_project_links() {
        let is_project = |t: &str| t.starts_with("Project. ") || t == "OA launch";
        let l = "- [ ] Book the room [[Sam Carter]] ⏳ 2026-10-05";
        let a = with_project_link(l, Some("Project. Orbit App launch"), is_project);
        assert_eq!(a, "- [ ] Book the room [[Sam Carter]] [[Project. Orbit App launch]] ⏳ 2026-10-05");
        assert_eq!(
            with_project_link("- [ ] x [[OA launch|launch]] y ^rank-3", Some("Project. Garden"), is_project),
            "- [ ] x [[Project. Garden]] y ^rank-3"
        );
        assert_eq!(with_project_link(&a, None, is_project), l);
        // An embed isn't a link to it.
        assert_eq!(with_project_link("- [ ] ![[Project. A]]", None, is_project), "- [ ] ![[Project. A]]");
    }

    #[test]
    fn appends_under_a_heading() {
        let v = |s: &str| s.split('\n').map(String::from).collect::<Vec<_>>();
        let mut a = v("---\nstatus: active\n---\n\n## Next actions\n\n## Waiting for\n\n## Notes");
        assert_eq!(append_under(&mut a, "Next actions", "- [ ] one"), 6);
        assert_eq!(append_under(&mut a, "next actions", "- [ ] two"), 7);
        assert_eq!(a.join("\n"), "---\nstatus: active\n---\n\n## Next actions\n\n- [ ] one\n- [ ] two\n\n## Waiting for\n\n## Notes");
        // A ### inside the section stays in it; a fence isn't a heading.
        let mut b = v("## Next actions\n- [ ] a\n### Later\n- [ ] b\n```\n## Next actions\n```\n\n## Notes\n");
        assert_eq!(append_under(&mut b, "Next actions", "- [ ] c"), 7);
        assert_eq!(b[6..8], ["```".to_string(), "- [ ] c".to_string()]);
        let mut c = v("# Title\n\ntext\n\n");
        append_under(&mut c, "Waiting for", "- [ ] w");
        assert_eq!(c.join("\n"), "# Title\n\ntext\n\n## Waiting for\n\n- [ ] w");
        let mut d = v("## Notes");
        append_under(&mut d, "Notes", "- [ ] n");
        assert_eq!(d.join("\n"), "## Notes\n\n- [ ] n");
    }

    #[test]
    fn adds_a_task_to_a_note() {
        let src = "---\r\nstatus: active\r\n---\r\n\r\n## Next actions\r\n\r\n- [ ] one\r\n\r\n## Notes\r\n\r\nSee it.\r\n";
        let (_d, p) = temp("Project. A.md", src);
        let (i, ch) = add_task_under(&p, "Next actions", "- [ ] two").unwrap();
        assert_eq!(i, 7);
        let after = "---\r\nstatus: active\r\n---\r\n\r\n## Next actions\r\n\r\n- [ ] one\r\n- [ ] two\r\n\r\n## Notes\r\n\r\nSee it.\r\n";
        assert_eq!(fs::read_to_string(&p).unwrap(), after);
        assert_eq!((ch.before.as_deref(), ch.version.clone()), (Some(src), version(after.as_bytes())));
        assert!(matches!(add_task_under(&p, "Next actions", "not a task"), Err(WriteError::Invalid(_))));
        assert!(matches!(add_task_under(&p, "Next actions", "- [ ] a\n- [ ] b"), Err(WriteError::Invalid(_))));
        restore_files(&[ch]).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), src);
    }

    #[test]
    fn quotes_yaml_only_when_needed() {
        for v in ["Work", "Orbit App is live", "on-hold", "a/b"] {
            assert_eq!(yaml_scalar(v), v);
        }
        for (v, q) in [
            ("Done: it works", "\"Done: it works\""),
            ("yes", "\"yes\""),
            ("12", "\"12\""),
            ("#1", "\"#1\""),
            ("", "\"\""),
            ("say \"hi\": x", "\"say \\\"hi\\\": x\""),
        ] {
            assert_eq!(yaml_scalar(v), q, "{v}");
        }
    }

    #[test]
    fn sets_one_property() {
        let src = "---\ntitle: X\nstatus: active\ntags:\n  - a\n  - b\narea: Work\n---\nbody\n";
        assert_eq!(with_property(src, "status", Some("on-hold")).unwrap(), src.replace("status: active", "status: on-hold"));
        assert_eq!(with_property(src, "tags", Some("c")).unwrap(), "---\ntitle: X\nstatus: active\ntags: c\narea: Work\n---\nbody\n");
        assert_eq!(with_property(src, "area", None).unwrap(), "---\ntitle: X\nstatus: active\ntags:\n  - a\n  - b\n---\nbody\n");
        assert_eq!(
            with_property(src, "outcome", Some("It: ships")).unwrap(),
            src.replace("area: Work\n", "area: Work\noutcome: \"It: ships\"\n")
        );
        assert_eq!(with_property(src, "outcome", None).unwrap(), src);
        // `status_old` isn't `status`.
        let near = "---\nstatus_old: x\n---\n";
        assert_eq!(with_property(near, "status", Some("done")).unwrap(), "---\nstatus_old: x\nstatus: done\n---\n");
        assert_eq!(
            with_property("# Note\r\n\r\ntext", "status", Some("done")).unwrap(),
            "---\r\nstatus: done\r\n---\r\n# Note\r\n\r\ntext"
        );
        assert_eq!(with_property("\u{feff}---\nstatus: a\n---\n", "status", Some("done")).unwrap(), "\u{feff}---\nstatus: done\n---\n");
        assert_eq!(with_property("", "status", Some("done")).unwrap(), "---\nstatus: done\n---\n");
        assert_eq!(with_property("plain\n", "status", None).unwrap(), "plain\n");
        assert!(matches!(with_property("---\nopen: x\nbody\n", "status", Some("done")), Err(WriteError::Invalid(_))));
        assert!(with_property(src, "bad key", Some("x")).is_err());
        assert!(with_property(src, "status", Some("two\nlines")).is_err());
    }

    #[test]
    fn set_property_writes_and_undoes() {
        let src = "---\r\nstatus: active\r\narea: Work\r\n---\r\n\r\n## Next actions\r\n";
        let (_d, p) = temp("Project. A.md", src);
        let ch = set_property(&p, "status", Some("done")).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), src.replace("active", "done"));
        restore_files(&[ch]).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), src);
    }

    #[test]
    fn moves_a_task_between_notes() {
        let d = tempfile::tempdir().unwrap();
        let todo = d.path().join("Me. To Do List.md");
        let proj = d.path().join("Project. A.md");
        let todo_src = "#### Other\r\n\r\n* [ ] Call Sam\r\n* [ ] Other thing\r\n\r\n#### Done\r\n";
        let proj_src = "---\nstatus: active\n---\n\n## Next actions\n\n- [ ] one\n\n## Waiting for\n\n## Notes\n";
        fs::write(&todo, todo_src).unwrap();
        fs::write(&proj, proj_src).unwrap();
        let (at, chs) = move_task(&todo, 2, "* [ ] Call Sam", &proj, "Waiting for", "- [ ] Call Sam #waiting-for").unwrap();
        assert_eq!(at, 10);
        assert_eq!(fs::read_to_string(&todo).unwrap(), "#### Other\r\n\r\n* [ ] Other thing\r\n\r\n#### Done\r\n");
        assert_eq!(
            fs::read_to_string(&proj).unwrap(),
            "---\nstatus: active\n---\n\n## Next actions\n\n- [ ] one\n\n## Waiting for\n\n- [ ] Call Sam #waiting-for\n\n## Notes\n"
        );
        // Undo puts both back, but only while both are as the move left them.
        fs::write(&todo, "changed\n").unwrap();
        assert!(matches!(restore_files(&chs), Err(WriteError::Stale(_))));
        assert_eq!(fs::read_to_string(&proj).unwrap().matches("Call Sam").count(), 1);
        fs::write(&todo, "#### Other\r\n\r\n* [ ] Other thing\r\n\r\n#### Done\r\n").unwrap();
        restore_files(&chs).unwrap();
        assert_eq!((fs::read_to_string(&todo).unwrap(), fs::read_to_string(&proj).unwrap()), (todo_src.into(), proj_src.into()));
        // Stale source, or a conflict copy beside either note: neither is written.
        assert!(matches!(move_task(&todo, 2, "* [ ] Gone", &proj, "Next actions", "- [ ] Gone"), Err(WriteError::Stale(_))));
        fs::write(d.path().join("Project. A 2.md"), "").unwrap();
        assert!(matches!(move_task(&todo, 2, "* [ ] Call Sam", &proj, "Next actions", "- [ ] Call Sam"), Err(WriteError::Conflict(_))));
        assert_eq!((fs::read_to_string(&todo).unwrap(), fs::read_to_string(&proj).unwrap()), (todo_src.into(), proj_src.into()));
        assert!(matches!(move_task(&todo, 2, "* [ ] Call Sam", &todo, "Next actions", "- [ ] Call Sam"), Err(WriteError::Invalid(_))));
    }

    #[cfg(unix)]
    #[test]
    fn a_move_that_cant_finish_puts_the_destination_back() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let src_dir = d.path().join("locked");
        fs::create_dir(&src_dir).unwrap();
        let todo = src_dir.join("Me. To Do List.md");
        let proj = d.path().join("Project. A.md");
        fs::write(&todo, "#### Other\n- [ ] Call Sam\n").unwrap();
        fs::write(&proj, "## Next actions\n").unwrap();
        // The source's folder can't be written, so its temporary file can't be made.
        fs::set_permissions(&src_dir, fs::Permissions::from_mode(0o555)).unwrap();
        let r = move_task(&todo, 1, "- [ ] Call Sam", &proj, "Next actions", "- [ ] Call Sam");
        fs::set_permissions(&src_dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(r, Err(WriteError::Io(_))), "{r:?}");
        assert_eq!(fs::read_to_string(&proj).unwrap(), "## Next actions\n");
        assert_eq!(fs::read_to_string(&todo).unwrap(), "#### Other\n- [ ] Call Sam\n");
    }

    #[test]
    fn undoes_a_created_file_by_deleting_it() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("New.md");
        let v = create_file(&p, "x\n").unwrap();
        let ch = FileChange { path: p.clone(), before: None, version: v };
        fs::write(&p, "edited\n").unwrap();
        assert!(matches!(restore_files(std::slice::from_ref(&ch)), Err(WriteError::Stale(_))));
        fs::write(&p, "x\n").unwrap();
        restore_files(&[ch]).unwrap();
        assert!(!p.exists());
    }
}
