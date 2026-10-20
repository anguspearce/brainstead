// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Renaming or moving a file and rewriting the wikilinks to it, in two steps: a plan listing every
//! line that changes, shown to the user, then a commit that refuses unless the plan still holds.
//! The rules follow the previous app (bff/src/services/vault-scan.ts `rewriteWikilinks`): only the
//! target is swapped, so brackets, escapes, the alias and spacing stay as written; a link with a
//! folder stays folder-qualified and a bare one stays bare; an extension on the link (`[[a.md]]`)
//! is kept; code blocks and code spans are left alone; and a link is only rewritten when it
//! resolves to this file (another note may own the same name). Unlike the previous app, links with
//! a heading or block (`[[Old#Part]]`, `[[Old#^b]]`) are rewritten too, and keep that part.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::links::key;
use crate::markdown::{blank_code_spans, lines};
use crate::trash::safe_rel;
use crate::write::{check_conflict_copies, io, join_lines, lock, split_lines, write_atomic, Result, WriteError};

/// Notes other features find by their exact name (the previous app's `PINNED_NOTES`).
pub const PINNED: &[&str] = &["Me. To Do List.md", "Me. Scratchpad.md", "Me. Bookmarks.md", "Me. Smart Lists.md", "Me. Canonical Docs.md"];

/// The daily and weekly summaries' monthly notes, by their new and old names.
static SUMMARY_NOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Me\. (?:Daily|Weekly) (?:Summaries|Reviews) - \d{4}-\d{2}\.md$").unwrap());

/// A system note by its name: one other features find by name, the vault's own `CLAUDE.md`,
/// `index.md` and `log.md`, or a summaries note.
pub fn is_system(rel: &str) -> bool {
    PINNED.contains(&rel) || crate::vault::SCHEMA_FILES.contains(&rel) || SUMMARY_NOTE.is_match(rel)
}

/// Where a note's system-note callout is (the opening quote that says "This is a system note"),
/// as a byte range of its lines without the last line's ending: after a byte-order mark,
/// properties and a `# Title` line, as the Scratchpad's pinned header has it. None when it has none.
pub fn system_callout(text: &str) -> Option<(usize, usize)> {
    let pos = callout_start(text);
    let line_end = |p: usize| text[p..].find('\n').map(|i| p + i + 1).unwrap_or(text.len());
    let mut end = pos;
    while end < text.len() && text[end..].starts_with('>') {
        end = line_end(end);
    }
    if end == pos || !text[pos..end].contains("This is a system note") {
        return None;
    }
    Some((pos, text[pos..end].trim_end_matches(['\n', '\r']).len() + pos))
}

/// `text` with `callout` put back where a system note's callout goes (after its properties and
/// title), with a blank line after it.
pub fn with_callout(text: &str, callout: &str) -> String {
    let at = callout_start(text);
    let rest = &text[at..];
    let gap = if rest.is_empty() || rest.starts_with('\n') || rest.starts_with("\r\n") { "\n" } else { "\n\n" };
    format!("{}{}{gap}{rest}", &text[..at], callout.trim_end())
}

/// Where a note's system-note callout starts, or would: after a byte-order mark, properties, blank
/// lines and a `# Title` line.
fn callout_start(text: &str) -> usize {
    let mut pos = if text.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let line_end = |p: usize| text[p..].find('\n').map(|i| p + i + 1).unwrap_or(text.len());
    let skip_blank = |mut p: usize| {
        while p < text.len() && text[p..line_end(p)].trim().is_empty() {
            p = line_end(p);
        }
        p
    };
    // Properties.
    if text[pos..].starts_with("---\n") || text[pos..].starts_with("---\r\n") {
        let mut p = line_end(pos);
        while p < text.len() && !matches!(text[p..line_end(p)].trim_end(), "---" | "...") {
            p = line_end(p);
        }
        pos = if p < text.len() { line_end(p) } else { pos };
    }
    pos = skip_blank(pos);
    if text[pos..].starts_with("# ") || text[pos..].starts_with("#\t") {
        pos = skip_blank(line_end(pos));
    }
    pos
}

/// A note that says it's a system note: its opening quote starts "This is a system note".
pub fn says_system(text: &str) -> bool {
    text[..crate::write::pinned_header_len(text)].contains("This is a system note")
}

/// Refuses to delete or rename a system note, by its name or by what it says of itself.
pub fn check_pinned(rel: &str, verb: &str) -> Result<()> {
    if is_system(rel) {
        return Err(WriteError::Invalid(format!(
            "“{}” can't be {verb}: it's a system note, which other parts of the app look for by name.",
            crate::filename::stem(rel)
        )));
    }
    Ok(())
}

/// [`check_pinned`], and the file's own system-note callout.
pub fn check_system(root: &Path, rel: &str, verb: &str) -> Result<()> {
    check_pinned(rel, verb)?;
    if rel.ends_with(".md") && fs::read_to_string(root.join(rel)).is_ok_and(|t| says_system(&t)) {
        return Err(WriteError::Invalid(format!("“{}” can't be {verb}: it's a system note.", crate::filename::stem(rel))));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkChange {
    pub path: String,
    /// 0-based.
    pub line: usize,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePlan {
    pub from: String,
    pub to: String,
    pub changes: Vec<LinkChange>,
}

static WIKILINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!?\\?\[\\?\[([^\[\]\n]+?)\\?\]\\?\]").unwrap());

/// The note-extension (`.md`, `.txt`) a link target carries, as written.
fn note_ext(t: &str) -> &str {
    for ext in [".md", ".txt"] {
        // `get`, not slicing: the cut can fall inside a character of a CJK or emoji name.
        let tail = t.len().checked_sub(ext.len()).filter(|&i| i > 0).and_then(|i| t.get(i..));
        if let Some(tail) = tail.filter(|e| e.eq_ignore_ascii_case(ext)) {
            return tail;
        }
    }
    ""
}

/// A path compared as a link names it: `/` for `\\`, no note extension, NFC, lower case.
fn path_key(p: &str) -> String {
    let p = p.trim().replace('\\', "/");
    let p = &p[..p.len() - note_ext(&p).len()];
    crate::links::nfc(p).to_lowercase()
}

/// One line with its links to `from` pointed at `to`. `resolves(target)` says whether a link
/// target goes to `from` under the index's rules.
pub fn rewrite_line(line: &str, from: &str, to: &str, resolves: &dyn Fn(&str) -> bool) -> String {
    if !line.contains("[[") && !line.contains("\\[\\[") {
        return line.to_string();
    }
    let old_key = key(from);
    let new_stem = crate::filename::stem(to);
    let new_path = to.rsplit_once('/').map(|(d, _)| format!("{d}/{new_stem}"));
    let blanked = blank_code_spans(line);
    let mut out = String::new();
    let mut last = 0;
    for c in WIKILINK.captures_iter(&blanked) {
        let inner = c.get(1).unwrap();
        let text = &line[inner.start()..inner.end()];
        // The note part: up to an alias bar (`|`, or `\|` in a table) or a `#`.
        let end = text.find(['|', '#']).map(|i| if i > 0 && text.as_bytes()[i - 1] == b'\\' { i - 1 } else { i }).unwrap_or(text.len());
        let dest = &text[..end];
        let target = dest.trim();
        if target.is_empty() || key(target) != old_key {
            continue;
        }
        // A link with a folder names one file: it must be this one. A bare one goes wherever the
        // index sends that name.
        let qualified = target.contains(['/', '\\']);
        let ok = if qualified { path_key(target) == path_key(from) } else { resolves(target) };
        if !ok {
            continue;
        }
        let lead = dest.len() - dest.trim_start().len();
        let stem = if qualified { new_path.clone().unwrap_or_else(|| new_stem.to_string()) } else { new_stem.to_string() };
        let at = inner.start() + lead;
        out.push_str(&line[last..at]);
        out.push_str(&stem);
        out.push_str(note_ext(target));
        last = at + target.len();
    }
    out.push_str(&line[last..]);
    out
}

/// Every change in one file's text.
fn changes_in(path: &str, text: &str, from: &str, to: &str, resolves: &dyn Fn(&str) -> bool) -> Vec<LinkChange> {
    let mut out = Vec::new();
    for l in lines(text, 0).iter().filter(|l| !l.code) {
        let after = rewrite_line(l.text, from, to, resolves);
        if after != l.text {
            out.push(LinkChange { path: path.to_string(), line: l.no, before: l.text.to_string(), after });
        }
    }
    out
}

/// What renaming `from` to `to` (both vault-relative) would change. `linkers` are the files that
/// link to `from` (from the index); `from` itself is checked too, for links to its own headings.
pub fn plan(root: &Path, from: &str, to: &str, linkers: &[String], resolves: &dyn Fn(&str) -> bool) -> Result<RenamePlan> {
    safe_rel(from)?;
    safe_rel(to)?;
    check_system(root, from, "renamed")?;
    if from == to {
        return Err(WriteError::Invalid("That's the name it has already.".into()));
    }
    let src = root.join(from);
    if !src.is_file() {
        return Err(WriteError::NotFound(format!("{from} isn't there any more.")));
    }
    let case_only = from.to_lowercase() == to.to_lowercase();
    if !case_only && root.join(to).exists() {
        return Err(WriteError::Exists(format!("There's already a file at {to}.")));
    }
    check_conflict_copies(&src)?;
    let mut files: Vec<&str> = linkers.iter().map(String::as_str).filter(|p| *p != from).collect();
    files.sort();
    files.dedup();
    files.insert(0, from);
    let mut changes = Vec::new();
    for f in files {
        let Ok(text) = fs::read_to_string(root.join(f)) else { continue };
        changes.extend(changes_in(f, &text, from, to, resolves));
    }
    Ok(RenamePlan { from: from.into(), to: to.into(), changes })
}

/// Carries out a plan the user saw: works it out again and refuses (stale) if anything differs,
/// checks every file it touches and the destination before writing any, rewrites the links
/// (keeping each file's modification time: a rename isn't an edit) and the claims files that name
/// it, then moves the file. When a write or the move fails, the files already rewritten are put back.
pub fn commit(root: &Path, shown: &RenamePlan, linkers: &[String], resolves: &dyn Fn(&str) -> bool) -> Result<()> {
    let now = plan(root, &shown.from, &shown.to, linkers, resolves)?;
    if now != *shown {
        return Err(WriteError::Stale("Files changed since the preview. Look at the new preview and try again.".into()));
    }
    let mut by_file: Vec<(&str, Vec<&LinkChange>)> = Vec::new();
    for c in &now.changes {
        match by_file.iter_mut().find(|(p, _)| *p == c.path) {
            Some((_, v)) => v.push(c),
            None => by_file.push((&c.path, vec![c])),
        }
    }
    let (src, dst) = (root.join(&now.from), root.join(&now.to));
    let case_only = now.from.to_lowercase() == now.to.to_lowercase();
    // Every file this touches, locked in one order (as `move_task` does) for the whole commit.
    let mut paths: Vec<std::path::PathBuf> = by_file.iter().map(|(p, _)| root.join(p)).collect();
    paths.push(src.clone());
    paths.sort();
    paths.dedup();
    let locks: Vec<_> = paths.iter().map(|p| lock(p)).collect();
    let _guards: Vec<_> = locks.iter().map(|l| l.lock().unwrap()).collect();

    // Every check before any write.
    let mut writes: Vec<(std::path::PathBuf, String, String)> = Vec::new();
    for (p, cs) in &by_file {
        let abs = root.join(p);
        check_conflict_copies(&abs)?;
        let text = fs::read_to_string(&abs).map_err(io)?;
        let (mut ls, eol, trailing) = split_lines(&text);
        for c in cs {
            if ls.get(c.line) != Some(&c.before) {
                return Err(WriteError::Stale(format!("{p} changed since the preview. Look at the new preview and try again.")));
            }
            ls[c.line] = c.after.clone();
        }
        let after = join_lines(&ls, &eol, trailing);
        writes.push((abs, text, after));
    }
    if !case_only && dst.exists() {
        return Err(WriteError::Exists(format!("There's already a file at {}.", now.to)));
    }
    if let Some(d) = dst.parent() {
        fs::create_dir_all(d).map_err(io)?;
    }
    // A wiki page's claims go with it (D-20261006-08), and the claims made from it, wherever they
    // are, name it by its new name: worked out now, written before the move, put back with the rest.
    let claims = crate::claims::on_rename(root, &now.from, &now.to);

    let mut written: Vec<&(std::path::PathBuf, String, String)> = Vec::new();
    let mut claims_written: Vec<&crate::claims::Rewrite> = Vec::new();
    let undo =
        |written: &[&(std::path::PathBuf, String, String)], claims_written: &[&crate::claims::Rewrite], e: WriteError| -> WriteError {
            let rel = |abs: &Path| abs.strip_prefix(root).unwrap_or(abs).display().to_string();
            let mut failed: Vec<String> = claims_written
                .iter()
                .filter(|(abs, before, _)| crate::claims::put(abs, before.as_deref()).is_err())
                .map(|(abs, _, _)| rel(abs))
                .collect();
            failed.extend(
                written.iter().filter(|(abs, before, _)| write_atomic(abs, before.as_bytes(), true).is_err()).map(|(abs, _, _)| rel(abs)),
            );
            if failed.is_empty() {
                e
            } else {
                WriteError::Io(format!(
                    "{e}; and putting back {} failed too, so some links or claims point at the new name.",
                    failed.join(", ")
                ))
            }
        };
    for w in &writes {
        if let Err(e) = write_atomic(&w.0, w.2.as_bytes(), true) {
            return Err(undo(&written, &claims_written, e));
        }
        written.push(w);
    }
    for c in &claims {
        if let Err(e) = crate::claims::put(&c.0, c.2.as_deref()) {
            return Err(undo(&written, &claims_written, io(e)));
        }
        claims_written.push(c);
    }
    let moved = if case_only {
        // A change of case only: on a case-insensitive disk a direct rename does nothing. If the
        // second step fails the note goes back to its name, not left under the hidden one.
        let tmp = src.with_file_name(format!(".rename-{}", std::process::id()));
        crate::write::rename(&src, &tmp).and_then(|_| {
            crate::write::rename(&tmp, &dst).inspect_err(|_| {
                let _ = crate::write::rename(&tmp, &src);
            })
        })
    } else {
        crate::write::rename(&src, &dst)
    };
    // The note itself may have been rewritten (links to its own headings); a failed move leaves
    // it at `src`, where `undo` puts it back.
    moved.map_err(|e| undo(&written, &claims_written, io(e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_system_callout() {
        let t = "# To Do\n\n> **This is a system note** - Tasks.\n> More.\n\n- [ ] x\n";
        let (a, b) = system_callout(t).unwrap();
        assert_eq!(&t[a..b], "> **This is a system note** - Tasks.\n> More.");
        let fm = "---\ntags: [x]\n---\n> **This is a system note** - Log.\n";
        let (a, b) = system_callout(fm).unwrap();
        assert_eq!(&fm[a..b], "> **This is a system note** - Log.");
        assert!(system_callout("> A quote.\n").is_none());
        assert!(system_callout("Text\n> **This is a system note**\n").is_none(), "not at the top");
        // Put back where it was.
        let gone = "# To Do\n\n- [ ] x\n";
        assert_eq!(
            with_callout(gone, "> **This is a system note** - Tasks."),
            "# To Do\n\n> **This is a system note** - Tasks.\n\n- [ ] x\n"
        );
    }

    #[test]
    fn system_notes_by_name_and_by_what_they_say() {
        for n in ["Me. To Do List.md", "log.md", "index.md", "Me. Daily Summaries - 2026-10.md", "Me. Weekly Reviews - 2026-09.md"] {
            assert!(is_system(n), "{n}");
        }
        for n in ["Me. Daily Summaries - 2026-10 2.md", "wiki/log.md", "Meeting. Orbit App - 2026-10-02.md"] {
            assert!(!is_system(n), "{n}");
        }
        assert!(says_system("# Me. Ideas\n\n> **This is a system note** - kept by the app.\n\nBody"));
        assert!(!says_system("# Notes\n\nA line that mentions This is a system note in passing.\n"));
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("Me. Ideas.md"), "> **This is a system note** - x\n").unwrap();
        std::fs::write(d.path().join("Idea. Pricing.md"), "Plain.\n").unwrap();
        assert!(check_system(d.path(), "Me. Ideas.md", "renamed").is_err());
        assert!(check_system(d.path(), "Idea. Pricing.md", "renamed").is_ok());
        assert!(crate::trash::move_to_trash(d.path(), "Me. Ideas.md", "note").is_err());
        assert!(crate::trash::move_to_trash(d.path(), "Idea. Pricing.md", "note").is_ok());
    }

    fn always(_: &str) -> bool {
        true
    }

    #[test]
    fn rewrites_link_shapes() {
        let r = |l: &str| rewrite_line(l, "wiki/entities/Orbit App.md", "wiki/entities/Orbit Platform.md", &always);
        assert_eq!(
            r("See [[Orbit App]], [[orbit app|the app]], [[Orbit App#Current state]], [[Orbit App#^q3]], ![[Orbit App]]."),
            "See [[Orbit Platform]], [[Orbit Platform|the app]], [[Orbit Platform#Current state]], [[Orbit Platform#^q3]], ![[Orbit Platform]]."
        );
        assert_eq!(
            r("[[ Orbit App.md |x]] and [[wiki/entities/Orbit App]]"),
            "[[ Orbit Platform.md |x]] and [[wiki/entities/Orbit Platform]]"
        );
        assert_eq!(r("| [[Orbit App\\|alias]] |"), "| [[Orbit Platform\\|alias]] |");
        assert_eq!(r("\\[\\[Orbit App\\]\\]"), "\\[\\[Orbit Platform\\]\\]");
        assert_eq!(r("`[[Orbit App]]` [[Orbit Apps]] [[OA]]"), "`[[Orbit App]]` [[Orbit Apps]] [[OA]]");
        // Owned by another note of the same name: left alone.
        assert_eq!(rewrite_line("[[Orbit App]]", "x/Orbit App.md", "x/New.md", &|_| false), "[[Orbit App]]");
        assert_eq!(r("[[other/Orbit App]]"), "[[other/Orbit App]]");
    }

    fn copy_dir(from: &Path, to: &Path) {
        for e in walkdir::WalkDir::new(from) {
            let e = e.unwrap();
            let rel = e.path().strip_prefix(from).unwrap();
            let dst = to.join(rel);
            if e.file_type().is_dir() {
                fs::create_dir_all(&dst).unwrap();
            } else {
                fs::copy(e.path(), &dst).unwrap();
            }
        }
    }

    fn fixture() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault"), d.path());
        d
    }

    fn snapshot(root: &Path) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = walkdir::WalkDir::new(root)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                let rel = e.path().strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                fs::read_to_string(e.path()).ok().map(|t| (rel, t))
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn commit_changes_only_the_planned_lines() {
        let d = fixture();
        let root = d.path();
        let from = "wiki/entities/Orbit App.md";
        assert!(root.join(from).exists());
        let linkers = vec!["Meeting. Orbit App Steerco - 2026-09-30.md".to_string()];
        let resolves = |t: &str| key(t) == "orbit app";
        // The fixture's OneDrive conflict copy beside the meeting stops the rename.
        let p = plan(root, from, "wiki/entities/Orbit Platform.md", &linkers, &resolves).unwrap();
        assert!(matches!(commit(root, &p, &linkers, &resolves), Err(WriteError::Conflict(_))));
        fs::remove_file(root.join("Meeting. Orbit App Steerco - 2026-09-30 2.md")).unwrap();
        let before = snapshot(root);
        let p = plan(root, from, "wiki/entities/Orbit Platform.md", &linkers, &resolves).unwrap();
        assert!(!p.changes.is_empty());
        assert!(p.changes.iter().all(|c| c.path != from || c.before.contains("[[")));
        commit(root, &p, &linkers, &resolves).unwrap();
        let after = snapshot(root);
        // Every file is as it was, except the planned lines and the move.
        for (rel, text) in &before {
            let now_rel = if rel == from { "wiki/entities/Orbit Platform.md".to_string() } else { rel.clone() };
            let now = &after.iter().find(|(r, _)| *r == now_rel).unwrap_or_else(|| panic!("{now_rel} gone")).1;
            let mut expect: Vec<String> = text.split('\n').map(String::from).collect();
            for c in p.changes.iter().filter(|c| c.path == *rel) {
                assert_eq!(expect[c.line].trim_end_matches('\r'), c.before);
                expect[c.line] = c.after.clone();
            }
            assert_eq!(*now, expect.join("\n"), "{rel}");
        }
        assert!(!root.join(from).exists());
        assert_eq!(before.len(), after.len());
    }

    #[test]
    fn refuses_a_stale_plan() {
        let d = fixture();
        let root = d.path();
        let from = "wiki/entities/Orbit App.md";
        let linkers = vec!["Meeting. Orbit App Steerco - 2026-09-30.md".to_string()];
        let resolves = |t: &str| key(t) == "orbit app";
        fs::remove_file(root.join("Meeting. Orbit App Steerco - 2026-09-30 2.md")).unwrap();
        let p = plan(root, from, "wiki/entities/Orbit Platform.md", &linkers, &resolves).unwrap();
        let m = root.join(&linkers[0]);
        let t = fs::read_to_string(&m).unwrap();
        fs::write(&m, format!("{t}\nAnother [[Orbit App]].\n")).unwrap();
        assert!(matches!(commit(root, &p, &linkers, &resolves), Err(WriteError::Stale(_))));
        assert!(root.join(from).exists());
        assert!(fs::read_to_string(&m).unwrap().contains("Another [[Orbit App]]"));
    }

    #[test]
    fn non_ascii_names_dont_panic() {
        assert_eq!(note_ext("会議メモ"), "");
        assert_eq!(note_ext("会議.md"), ".md");
        assert_eq!(note_ext("🚀🚀"), "");
        assert_eq!(path_key("ノート/会議.MD"), "ノート/会議");
        let r = rewrite_line("[[x/会議]] [[x/🚀]]", "x/会議.md", "x/新.md", &always);
        assert_eq!(r, "[[x/新]] [[x/🚀]]");
    }

    #[test]
    fn a_failed_move_puts_the_links_back() {
        let d = fixture();
        let root = d.path();
        let from = "wiki/entities/Orbit App.md";
        let linkers = vec!["Meeting. Orbit App Steerco - 2026-09-30.md".to_string()];
        let resolves = |t: &str| key(t) == "orbit app";
        fs::remove_file(root.join("Meeting. Orbit App Steerco - 2026-09-30 2.md")).unwrap();
        let before = snapshot(root);
        // The destination's folder can't be made: refused before any link is rewritten.
        fs::write(root.join("blocked"), "a file, not a folder").unwrap();
        let p = plan(root, from, "blocked/Orbit Platform.md", &linkers, &resolves).unwrap();
        assert!(!p.changes.is_empty());
        assert!(commit(root, &p, &linkers, &resolves).is_err());
        fs::remove_file(root.join("blocked")).unwrap();
        assert_eq!(snapshot(root), before);
        // The move itself fails (a folder that can't be written to): the links go back.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked = root.join("locked");
            fs::create_dir(&locked).unwrap();
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
            let p = plan(root, from, "locked/Orbit Platform.md", &linkers, &resolves).unwrap();
            let r = commit(root, &p, &linkers, &resolves);
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(matches!(r, Err(WriteError::Io(_))), "{r:?}");
            assert_eq!(snapshot(root), before);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_claims_write_puts_everything_back() {
        use std::os::unix::fs::PermissionsExt;
        let d = fixture();
        let root = d.path();
        let from = "wiki/entities/Orbit App.md";
        let to = "wiki/entities/Orbit Platform.md";
        let linkers = vec!["Meeting. Orbit App Steerco - 2026-09-30.md".to_string()];
        let resolves = |t: &str| key(t) == "orbit app";
        fs::remove_file(root.join("Meeting. Orbit App Steerco - 2026-09-30 2.md")).unwrap();
        // The page's own claims, and a claim on another page made from it.
        let k = |source: &str| crate::claims::Kept {
            subject: "Orbit App".into(),
            attribute: "owner".into(),
            value: "Maya".into(),
            source: source.into(),
            ..Default::default()
        };
        let own = crate::claims::with_source(None, from, "a.md", &[k("a.md")]).unwrap();
        let other = crate::claims::with_source(None, "wiki/people/Maya.md", from, &[k(from)]).unwrap();
        for (page, text) in [(from, &own), ("wiki/people/Maya.md", &other)] {
            let p = root.join(crate::claims::path(page).unwrap());
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        let before = snapshot(root);
        // The other page's claims can't be written (its folder is locked): the page's own claims,
        // already carried, and the links go back, and the note stays where it was.
        let people = root.join("wiki/.claims/people");
        fs::set_permissions(&people, fs::Permissions::from_mode(0o555)).unwrap();
        let p = plan(root, from, to, &linkers, &resolves).unwrap();
        assert!(!p.changes.is_empty());
        let r = commit(root, &p, &linkers, &resolves);
        fs::set_permissions(&people, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(r, Err(WriteError::Io(_))), "{r:?}");
        assert_eq!(snapshot(root), before);
        // Once it can be written, all of it moves together.
        commit(root, &p, &linkers, &resolves).unwrap();
        assert!(root.join(to).exists() && !root.join(from).exists());
        assert!(root.join("wiki/.claims/entities/Orbit Platform.json").exists());
        let maya = fs::read_to_string(people.join("Maya.json")).unwrap();
        assert_eq!(crate::claims::parse(&maya).claims[0].source, to);
    }

    #[test]
    fn refusals() {
        let d = fixture();
        let root = d.path();
        let none: Vec<String> = vec![];
        assert!(matches!(plan(root, "Me. To Do List.md", "Todo.md", &none, &always), Err(WriteError::Invalid(_))));
        assert!(matches!(plan(root, "Idea. Images.md", "Idea. Accents.md", &none, &always), Err(WriteError::Exists(_))));
        assert!(matches!(plan(root, "Missing.md", "B.md", &none, &always), Err(WriteError::NotFound(_))));
        assert!(plan(root, "Idea. Images.md", "../out.md", &none, &always).is_err());
        // Case only is allowed.
        let p = plan(root, "Idea. Images.md", "Idea. images.md", &none, &always).unwrap();
        commit(root, &p, &none, &always).unwrap();
        assert!(fs::read_dir(root).unwrap().flatten().any(|e| e.file_name() == "Idea. images.md"));
    }
}
