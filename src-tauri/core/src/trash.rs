// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Trash, in the same format as the previous app (bff/src/services/trash.ts), so either app can
//! restore what the other deleted while both run: `.trash/<id>/<basename>` plus a `meta.json`
//! with `id`, `originalRel`, `layer`, `basename` and `deletedAt`. The folder starts with a dot, so
//! the index and the watcher never see it (vault.rs).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::write::{check_conflict_copies, io, lock, Result, WriteError};

pub const TRASH_DIR: &str = ".trash";
/// A wiki page's claims file, in its entry's folder.
const CLAIMS: &str = ".claims.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    id: String,
    original_rel: String,
    layer: String,
    basename: String,
    deleted_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub id: String,
    pub original_rel: String,
    pub layer: String,
    pub basename: String,
    pub deleted_at: String,
    pub size_bytes: u64,
}

fn safe_id(id: &str) -> Result<()> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
        return Err(WriteError::Invalid(format!("Not a trash entry: {id}")));
    }
    Ok(())
}

/// A vault-relative path that stays in the vault.
pub fn safe_rel(rel: &str) -> Result<()> {
    if !crate::paths::is_vault_relative(rel) || rel.split(['/', '\\']).any(|s| s == ".") {
        return Err(WriteError::Invalid(format!("Not a path in the vault: {rel}")));
    }
    Ok(())
}

/// Like the previous app's: milliseconds in base 36, a dash, six hex digits.
fn new_id() -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
    let mut n = ms;
    let mut b36 = Vec::new();
    while n > 0 {
        b36.push(std::char::from_digit((n % 36) as u32, 36).unwrap());
        n /= 36;
    }
    let b36: String = b36.into_iter().rev().collect();
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seed = format!("{ms}-{}-{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    format!("{b36}-{}", &crate::write::version(seed.as_bytes())[..6])
}

/// Renames, or copies and deletes across volumes, keeping the modification time.
fn move_file(from: &Path, to: &Path) -> Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(18) => {
            let mtime = fs::metadata(from).and_then(|m| m.modified()).map_err(io)?;
            fs::copy(from, to).map_err(io)?;
            if let Ok(f) = fs::File::options().write(true).open(to) {
                let _ = f.set_modified(mtime);
            }
            fs::remove_file(from).map_err(io)
        }
        Err(e) => Err(io(e)),
    }
}

fn entry_dir(root: &Path, id: &str) -> PathBuf {
    root.join(TRASH_DIR).join(id)
}

fn read_meta(root: &Path, id: &str) -> Option<Meta> {
    let m: Meta = serde_json::from_slice(&fs::read(entry_dir(root, id).join("meta.json")).ok()?).ok()?;
    // The basename is a file in the entry's folder, nothing more: meta.json may be another app's.
    let one_name = safe_rel(&m.basename).is_ok() && !m.basename.contains(['/', '\\']);
    (m.id == id && one_name).then_some(m)
}

/// Moves a vault file to the trash. `layer` is "note", "wiki", "source" or "template".
pub fn move_to_trash(root: &Path, rel: &str, layer: &str) -> Result<TrashEntry> {
    safe_rel(rel)?;
    crate::rename::check_system(root, rel, "moved to the Trash")?;
    trash_file(root, rel, layer)
}

/// Undoing a run that made a system note (a summaries note it created): moves it to the trash
/// without the system-note check, as the user never had it before.
pub fn undo_created(root: &Path, rel: &str, layer: &str) -> Result<TrashEntry> {
    safe_rel(rel)?;
    trash_file(root, rel, layer)
}

fn trash_file(root: &Path, rel: &str, layer: &str) -> Result<TrashEntry> {
    let abs = root.join(rel);
    let l = lock(&abs);
    let _g = l.lock().unwrap();
    let md = fs::metadata(&abs).map_err(|_| WriteError::NotFound(format!("{rel} isn't there any more.")))?;
    // OneDrive's `Name 2.md` usually holds the real edits: sort that out first.
    check_conflict_copies(&abs)?;
    let id = new_id();
    let basename = rel.rsplit('/').next().unwrap_or(rel).to_string();
    let dir = entry_dir(root, &id);
    let deleted_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let meta = Meta { id: id.clone(), original_rel: rel.to_string(), layer: layer.to_string(), basename: basename.clone(), deleted_at };
    let json = serde_json::to_string_pretty(&meta).map_err(|e| WriteError::Io(e.to_string()))?;
    // meta.json first, written whole: a file in the trash is never without it. If the move then
    // fails the entry goes, and the file stays where it was.
    fs::create_dir_all(&dir).map_err(io)?;
    if let Err(e) =
        crate::write::write_atomic(&dir.join("meta.json"), json.as_bytes(), false).and_then(|_| move_file(&abs, &dir.join(&basename)))
    {
        let _ = fs::remove_dir_all(&dir);
        return Err(e);
    }
    // A wiki page's claims go into the trash with it, and come back if it's restored.
    if let Some(c) = crate::claims::path(rel).map(|c| root.join(c)).filter(|c| c.exists()) {
        let _ = move_file(&c, &dir.join(CLAIMS));
    }
    Ok(TrashEntry { id, original_rel: meta.original_rel, layer: meta.layer, basename, deleted_at: meta.deleted_at, size_bytes: md.len() })
}

/// What's in the trash, most recently deleted first. Entries whose file is gone are skipped.
pub fn list(root: &Path) -> Vec<TrashEntry> {
    let Ok(rd) = fs::read_dir(root.join(TRASH_DIR)) else { return vec![] };
    let mut out: Vec<TrashEntry> = rd
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            safe_id(&id).ok()?;
            let m = read_meta(root, &id)?;
            let size = fs::metadata(entry_dir(root, &id).join(&m.basename)).ok()?.len();
            Some(TrashEntry {
                id,
                original_rel: m.original_rel,
                layer: m.layer,
                basename: m.basename,
                deleted_at: m.deleted_at,
                size_bytes: size,
            })
        })
        .collect();
    out.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    out
}

/// Where an entry goes back to (the path it had) and its file as it lies in the trash, so a restore
/// can be checked before it's made.
pub fn entry_file(root: &Path, id: &str) -> Result<(String, PathBuf)> {
    safe_id(id)?;
    let m = read_meta(root, id).ok_or_else(|| WriteError::NotFound("That's no longer in the trash.".into()))?;
    Ok((m.original_rel, entry_dir(root, id).join(&m.basename)))
}

/// Puts an entry back where it was, or at `as_rel`. Refuses when a file is already there.
/// Returns the vault-relative path it's at now.
pub fn restore(root: &Path, id: &str, as_rel: Option<&str>) -> Result<String> {
    safe_id(id)?;
    let m = read_meta(root, id).ok_or_else(|| WriteError::NotFound("That's no longer in the trash.".into()))?;
    let to = as_rel.unwrap_or(&m.original_rel).to_string();
    safe_rel(&to)?;
    let target = root.join(&to);
    let l = lock(&target);
    let _g = l.lock().unwrap();
    if target.exists() {
        return Err(WriteError::Exists(format!("There's already a file at {to}. Restore it under another name.")));
    }
    if let Some(d) = target.parent() {
        fs::create_dir_all(d).map_err(io)?;
    }
    let dir = entry_dir(root, id);
    move_file(&dir.join(&m.basename), &target)?;
    if let Some(c) = crate::claims::path(&to).map(|c| root.join(c)).filter(|c| dir.join(CLAIMS).exists() && !c.exists()) {
        let _ = c.parent().map(fs::create_dir_all);
        let _ = move_file(&dir.join(CLAIMS), &c);
    }
    let _ = fs::remove_dir_all(&dir);
    Ok(to)
}

/// Deletes one entry for good.
pub fn delete(root: &Path, id: &str) -> Result<()> {
    safe_id(id)?;
    let dir = entry_dir(root, id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(io)?;
    }
    Ok(())
}

/// Deletes every entry for good. Returns how many.
pub fn empty(root: &Path) -> Result<usize> {
    let all = list(root);
    for e in &all {
        delete(root, &e.id)?;
    }
    Ok(all.len())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_folder_goes_to_the_trash_and_back() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join(".claude/skills/ingest")).unwrap();
        std::fs::write(t.path().join(".claude/skills/ingest/SKILL.md"), "x").unwrap();
        let e = move_to_trash(t.path(), ".claude/skills", "folder").unwrap();
        assert!(!t.path().join(".claude/skills").exists());
        assert_eq!(list(t.path())[0].basename, "skills");
        assert_eq!(restore(t.path(), &e.id, None).unwrap(), ".claude/skills");
        assert_eq!(std::fs::read_to_string(t.path().join(".claude/skills/ingest/SKILL.md")).unwrap(), "x");
    }

    use super::*;

    #[test]
    fn round_trip() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir_all(root.join("People")).unwrap();
        fs::write(root.join("People/Ann.md"), "# Ann\n").unwrap();
        let mtime = fs::metadata(root.join("People/Ann.md")).unwrap().modified().unwrap();
        let e = move_to_trash(root, "People/Ann.md", "note").unwrap();
        assert!(!root.join("People/Ann.md").exists());
        assert_eq!(e.size_bytes, 6);
        // The same meta.json the previous app writes.
        let meta: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(".trash").join(&e.id).join("meta.json")).unwrap()).unwrap();
        let keys: Vec<&str> = meta.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["id", "originalRel", "layer", "basename", "deletedAt"]);
        assert_eq!(meta["basename"], "Ann.md");
        assert!(e.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        assert_eq!(list(root), vec![e.clone()]);

        // Where it goes back to, and its file, for a restore to be checked before it's made.
        let (back, file) = entry_file(root, &e.id).unwrap();
        assert_eq!(back, "People/Ann.md");
        assert_eq!(fs::read_to_string(file).unwrap(), "# Ann\n");
        assert!(entry_file(root, "abc-000000").is_err());

        // Something new took its place: restore refuses, restore as works.
        fs::write(root.join("People/Ann.md"), "new").unwrap();
        assert!(matches!(restore(root, &e.id, None), Err(WriteError::Exists(_))));
        assert_eq!(restore(root, &e.id, Some("People/Ann (old).md")).unwrap(), "People/Ann (old).md");
        assert_eq!(fs::read_to_string(root.join("People/Ann (old).md")).unwrap(), "# Ann\n");
        assert_eq!(fs::metadata(root.join("People/Ann (old).md")).unwrap().modified().unwrap(), mtime);
        assert!(list(root).is_empty());
        assert!(!root.join(".trash").join(&e.id).exists());

        let a = move_to_trash(root, "People/Ann.md", "note").unwrap();
        fs::write(root.join("B.md"), "b").unwrap();
        move_to_trash(root, "B.md", "note").unwrap();
        assert_eq!(list(root).len(), 2);
        delete(root, &a.id).unwrap();
        assert_eq!(list(root).len(), 1);
        assert_eq!(empty(root).unwrap(), 1);
        assert!(list(root).is_empty());
        assert!(restore(root, "../x", None).is_err());
        // A meta.json whose basename reaches outside its entry is no entry.
        let bad = root.join(".trash/abc-123456");
        fs::create_dir_all(&bad).unwrap();
        fs::write(root.join("Outside.md"), "x").unwrap();
        let meta = r#"{"id":"abc-123456","originalRel":"Moved.md","layer":"note","basename":"../../Outside.md","deletedAt":"2026-10-03T00:00:00.000Z"}"#;
        fs::write(bad.join("meta.json"), meta).unwrap();
        assert!(list(root).is_empty());
        assert!(matches!(restore(root, "abc-123456", None), Err(WriteError::NotFound(_))));
        assert!(root.join("Outside.md").exists() && !root.join("Moved.md").exists());
        assert!(move_to_trash(root, "../etc", "note").is_err());
    }
}
