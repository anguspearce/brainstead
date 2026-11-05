// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The SQLite index (`index.db`, outside the vault). It's disposable: a schema change drops it and
//! it's rebuilt from the vault. A file is re-read only when its size or modification time differs
//! from what's stored, and re-parsed only when its contents did change — which is also how
//! Brainstead's own writes are recognised and skipped.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::links::key;
use crate::note::{self, Parsed};
use crate::vault::{Vault, VaultFile};

/// Bump when the schema or what's parsed into it changes; the index is then rebuilt.
const SCHEMA_VERSION: i64 = 10;

const SCHEMA: &str = r#"
CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL, key TEXT NOT NULL, layer TEXT NOT NULL,
  sha256 TEXT NOT NULL, size INTEGER, mtime INTEGER, type TEXT, title TEXT, date TEXT, frontmatter TEXT, frontmatter_error TEXT,
  project_line INTEGER, current_state TEXT);
CREATE INDEX files_key ON files(key);
CREATE TABLE links (src INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE, target TEXT NOT NULL, target_key TEXT NOT NULL,
  target_id INTEGER REFERENCES files(id) ON DELETE SET NULL, kind TEXT NOT NULL, heading TEXT, block TEXT, alias TEXT, line INTEGER);
CREATE INDEX links_src ON links(src);
CREATE INDEX links_target ON links(target_id);
CREATE INDEX links_target_key ON links(target_key);
CREATE TABLE tags (file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE, tag TEXT NOT NULL);
CREATE INDEX tags_tag ON tags(tag);
CREATE TABLE aliases (file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE, alias TEXT NOT NULL, key TEXT NOT NULL);
CREATE INDEX aliases_key ON aliases(key);
CREATE TABLE tasks (id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  line INTEGER NOT NULL, line_text TEXT NOT NULL, text TEXT NOT NULL, done INTEGER NOT NULL, due TEXT, scheduled TEXT, start TEXT,
  done_on TEXT, created TEXT, rank INTEGER, tags TEXT, heading TEXT, status TEXT NOT NULL DEFAULT ' ', cancelled TEXT,
  contexts TEXT, effort TEXT, effort_min INTEGER);
CREATE INDEX tasks_file ON tasks(file_id);
CREATE TABLE chunks (id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  heading_path TEXT, start INTEGER, end INTEGER, text TEXT NOT NULL);
CREATE INDEX chunks_file ON chunks(file_id);
CREATE VIRTUAL TABLE chunks_fts USING fts5(title, heading_path, text, tags, content='', contentless_delete=1, tokenize='porter unicode61');
CREATE TABLE meta (k TEXT PRIMARY KEY, v TEXT);
"#;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub files: i64,
    pub notes: i64,
    pub wiki: i64,
    pub sources: i64,
    pub templates: i64,
    pub tasks: i64,
    pub open_tasks: i64,
    pub links: i64,
    pub unresolved_links: i64,
    /// What the indexed files take on disk, in bytes: all of them, then each layer's.
    pub bytes: i64,
    pub note_bytes: i64,
    pub wiki_bytes: i64,
    pub source_bytes: i64,
    pub template_bytes: i64,
    /// When the index last changed, in ms since the epoch.
    pub updated_at: i64,
    /// How long the last full pass over the vault took.
    pub last_sync_ms: i64,
}

/// What one pass changed, by vault-relative path.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }
}

pub struct Index {
    conn: Connection,
    /// Where PDF and Office sources' text is read from and kept; without it they're indexed by
    /// name only.
    text: Option<crate::extract::TextCache>,
}

pub type Result<T> = std::result::Result<T, String>;

fn e<E: std::fmt::Display>(x: E) -> String {
    x.to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// A file read and parsed, ready to store.
struct Read {
    file: VaultFile,
    sha: String,
    parsed: Option<Parsed>,
}

impl Index {
    pub fn open(path: &Path) -> Result<Index> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(e)?;
        }
        Self::setup(Connection::open(path).map_err(e)?)
    }

    /// Opens the app's index for reading only, as the MCP server does: never rebuilt from here,
    /// so an index from another version of the schema is an error.
    pub fn open_read_only(path: &Path) -> Result<Index> {
        use rusqlite::OpenFlags;
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
            .map_err(|_| "Brainstead hasn't indexed the vault yet: open Brainstead once.".to_string())?;
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(e)?;
        if v != SCHEMA_VERSION {
            return Err("Brainstead's index is from another version: open Brainstead once to rebuild it.".into());
        }
        Ok(Index { conn, text: None })
    }

    pub fn open_in_memory() -> Result<Index> {
        Self::setup(Connection::open_in_memory().map_err(e)?)
    }

    fn setup(conn: Connection) -> Result<Index> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;").map_err(e)?;
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(e)?;
        if v != SCHEMA_VERSION {
            Self::recreate(&conn)?;
        }
        Ok(Index { conn, text: None })
    }

    fn recreate(conn: &Connection) -> Result<()> {
        let names: Vec<(String, String)> = conn
            .prepare("SELECT type, name FROM sqlite_master WHERE type IN ('table') AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'chunks_fts_%'")
            .map_err(e)?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(e)?
            .collect::<std::result::Result<_, _>>()
            .map_err(e)?;
        conn.execute_batch("PRAGMA foreign_keys=OFF;").map_err(e)?;
        for (_, n) in names {
            conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{n}\";")).map_err(e)?;
        }
        conn.execute_batch(SCHEMA).map_err(e)?;
        conn.execute_batch(&format!("PRAGMA user_version={SCHEMA_VERSION}; PRAGMA foreign_keys=ON;")).map_err(e)?;
        Ok(())
    }

    /// Empties the index, for Rebuild index and when the vault folder changes.
    pub fn clear(&mut self) -> Result<()> {
        Self::recreate(&self.conn)
    }

    /// Reads PDF and Office sources' text through this cache (stage 7b).
    pub fn set_text_cache(&mut self, c: crate::extract::TextCache) {
        self.text = Some(c);
    }

    /// A PDF or Office file's text, as indexed: from the cache by the file's content hash.
    pub fn extracted(&self, path: &str, dir: &Path) -> Result<Option<crate::extract::Extracted>> {
        let sha: Option<String> =
            self.conn.query_row("SELECT sha256 FROM files WHERE path = ?1", [path], |r| r.get(0)).optional().map_err(e)?;
        Ok(sha.and_then(|s| crate::extract::TextCache::lookup(dir, &s)))
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Brings the whole index up to date with the vault: new and changed files are read, files
    /// that have gone are dropped.
    pub fn sync(&mut self, vault: &Vault) -> Result<Changes> {
        let t0 = Instant::now();
        let files = vault.walk();
        let known = self.fingerprints()?;
        let mut removed: Vec<String> = Vec::new();
        {
            let present: std::collections::HashSet<&str> = files.iter().map(|f| f.rel.as_str()).collect();
            for p in known.keys() {
                if !present.contains(p.as_str()) {
                    removed.push(p.clone());
                }
            }
        }
        let stale: Vec<VaultFile> =
            files.into_iter().filter(|f| known.get(&f.rel).is_none_or(|k| k.size != f.size as i64 || k.mtime != f.mtime)).collect();
        let changes = self.apply(stale, removed, &known)?;
        self.set_meta("last_sync_ms", &(t0.elapsed().as_millis() as i64).to_string())?;
        Ok(changes)
    }

    /// Re-checks just these paths (from the file watcher). A path that's gone, or no longer
    /// indexed, is dropped; so is one whose name now differs in case only (renamed in another
    /// editor: on a case-insensitive disk the old spelling still opens).
    pub fn update(&mut self, vault: &Vault, paths: &[PathBuf]) -> Result<Changes> {
        let known = self.fingerprints()?;
        let mut stale = Vec::new();
        let mut removed = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut listings = Listings::default();
        for p in paths {
            let Some(rel) = crate::vault::rel_of(&vault.root, p) else { continue };
            if !seen.insert(rel.clone()) {
                continue;
            }
            match vault.file(p).filter(|_| listings.named(p)) {
                Some(f) => {
                    if known.get(&f.rel).is_none_or(|k| k.size != f.size as i64 || k.mtime != f.mtime) {
                        stale.push(f);
                    }
                }
                // A folder that's still there (a file in it changed): walked below.
                None if p.is_dir() && listings.named(p) => {}
                None => {
                    // A folder that was removed or renamed: everything under it.
                    let prefix = format!("{rel}/");
                    for k in known.keys() {
                        if *k == rel || k.starts_with(&prefix) {
                            removed.push(k.clone());
                        }
                    }
                }
            }
        }
        // A folder renamed or moved in: walk it (only it) for files the events didn't name one by one.
        for p in paths {
            if p.is_dir() && listings.named(p) {
                for f in vault.walk_under(p) {
                    if seen.insert(f.rel.clone()) && known.get(&f.rel).is_none_or(|k| k.size != f.size as i64 || k.mtime != f.mtime) {
                        stale.push(f);
                    }
                }
            }
        }
        removed.sort();
        removed.dedup();
        self.apply(stale, removed, &known)
    }

    /// Re-reads these files whatever their fingerprints say: after Brainstead's own write, which
    /// keeps the modification time and may keep the size (one date for another). Paths that are
    /// gone (renamed, trashed, or renamed in case only) are dropped.
    pub fn refresh(&mut self, vault: &Vault, paths: &[PathBuf]) -> Result<Changes> {
        let known = self.fingerprints()?;
        let mut listings = Listings::default();
        let stale: Vec<VaultFile> = paths.iter().filter(|p| listings.named(p)).filter_map(|p| vault.file(p)).collect();
        let removed: Vec<String> = paths
            .iter()
            .filter(|p| !p.exists() || !listings.named(p))
            .filter_map(|p| crate::vault::rel_of(&vault.root, p))
            .filter(|r| known.contains_key(r))
            .collect();
        let mut forced = known;
        for f in &stale {
            forced.remove(&f.rel);
        }
        self.apply(stale, removed, &forced)
    }

    fn fingerprints(&self) -> Result<HashMap<String, Fingerprint>> {
        let mut st = self.conn.prepare("SELECT path, size, mtime, sha256 FROM files").map_err(e)?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, Fingerprint { size: r.get(1)?, mtime: r.get(2)?, sha: r.get(3)? })))
            .map_err(e)?;
        rows.collect::<std::result::Result<HashMap<_, _>, _>>().map_err(e)
    }

    fn apply(&mut self, stale: Vec<VaultFile>, removed: Vec<String>, known: &HashMap<String, Fingerprint>) -> Result<Changes> {
        // Reading and parsing is most of the work, so it runs on every core.
        let text = self.text.clone();
        let reads: Vec<Read> = stale
            .into_par_iter()
            .filter_map(|f| {
                let bytes = std::fs::read(&f.abs).ok()?;
                let sha = hex::encode(Sha256::digest(&bytes));
                let same = known.get(&f.rel).is_some_and(|k| k.sha == sha);
                let parsed = (!same).then(|| {
                    if crate::vault::is_note_ext(&f.rel) {
                        note::parse(&f.rel, f.layer, &String::from_utf8_lossy(&bytes))
                    } else if let (Some(t), Some(_)) = (&text, crate::extract::kind_of(&f.rel)) {
                        note::parse_extracted(&f.rel, &t.get(&f.abs, &sha))
                    } else {
                        note::file_only(&f.rel)
                    }
                });
                Some(Read { file: f, sha, parsed })
            })
            .collect();
        let mut changes = Changes::default();
        let tx = self.conn.transaction().map_err(e)?;
        // The names (file keys and aliases) this pass may have given or taken away, and the files
        // whose links are new: only links to those names, or from those files, are resolved again.
        let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut sources: Vec<i64> = Vec::new();
        let alias_keys = |tx: &rusqlite::Transaction, path: &str| -> Result<Vec<String>> {
            let mut st = tx.prepare_cached("SELECT a.key FROM aliases a JOIN files f ON f.id = a.file_id WHERE f.path = ?1").map_err(e)?;
            let rows = st.query_map([path], |r| r.get::<_, String>(0)).map_err(e)?;
            rows.collect::<std::result::Result<Vec<_>, _>>().map_err(e)
        };
        for p in &removed {
            names.insert(key(p));
            names.extend(alias_keys(&tx, p)?);
            tx.execute(
                "DELETE FROM chunks_fts WHERE rowid IN (SELECT c.id FROM chunks c JOIN files f ON f.id = c.file_id WHERE f.path = ?1)",
                [p],
            )
            .map_err(e)?;
            tx.execute("DELETE FROM files WHERE path = ?1", [p]).map_err(e)?;
            changes.removed.push(p.clone());
        }
        for r in reads {
            match r.parsed {
                // Touched but not changed (or Brainstead's own write coming back): only the fingerprint.
                None => {
                    tx.execute(
                        "UPDATE files SET size = ?2, mtime = ?3 WHERE path = ?1",
                        params![r.file.rel, r.file.size as i64, r.file.mtime],
                    )
                    .map_err(e)?;
                }
                Some(p) => {
                    names.insert(key(&r.file.rel));
                    names.extend(alias_keys(&tx, &r.file.rel)?);
                    names.extend(p.aliases.iter().map(|a| key(a)));
                    sources.push(store(&tx, &r.file, &r.sha, &p)?);
                    changes.changed.push(r.file.rel);
                }
            }
        }
        if !changes.is_empty() {
            const RESOLVE: &str = "UPDATE links SET target_id = CASE WHEN target = '' THEN src ELSE COALESCE(
                   (SELECT f.id FROM files f WHERE f.key = links.target_key ORDER BY length(f.path), f.id LIMIT 1),
                   (SELECT a.file_id FROM aliases a WHERE a.key = links.target_key ORDER BY a.file_id LIMIT 1)) END";
            if names.len() + sources.len() > RESOLVE_ALL_ABOVE {
                tx.execute_batch(&format!("{RESOLVE};")).map_err(e)?;
            } else {
                let mut by_src = tx.prepare_cached(&format!("{RESOLVE} WHERE src = ?1")).map_err(e)?;
                for id in &sources {
                    by_src.execute([id]).map_err(e)?;
                }
                let mut by_name = tx.prepare_cached(&format!("{RESOLVE} WHERE target_key = ?1")).map_err(e)?;
                for k in &names {
                    by_name.execute([k]).map_err(e)?;
                }
            }
            tx.execute(
                "INSERT INTO meta(k, v) VALUES('updated_at', ?1) ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                [now_ms().to_string()],
            )
            .map_err(e)?;
        }
        tx.commit().map_err(e)?;
        Ok(changes)
    }

    pub fn set_meta(&self, k: &str, v: &str) -> Result<()> {
        self.conn.execute("INSERT INTO meta(k, v) VALUES(?1, ?2) ON CONFLICT(k) DO UPDATE SET v = excluded.v", [k, v]).map_err(e)?;
        Ok(())
    }

    pub fn meta(&self, k: &str) -> Result<Option<String>> {
        self.conn.query_row("SELECT v FROM meta WHERE k = ?1", [k], |r| r.get(0)).optional().map_err(e)
    }

    pub fn stats(&self) -> Result<IndexStats> {
        let c = &self.conn;
        let one = |sql: &str| -> Result<i64> { c.query_row(sql, [], |r| r.get(0)).map_err(e) };
        let mut by_layer: HashMap<String, (i64, i64)> = HashMap::new();
        {
            let mut st = c.prepare("SELECT layer, count(*), coalesce(sum(size), 0) FROM files GROUP BY layer").map_err(e)?;
            for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))).map_err(e)? {
                let (l, n, b) = row.map_err(e)?;
                by_layer.insert(l, (n, b));
            }
        }
        let g = |k: &str| by_layer.get(k).map_or(0, |v| v.0);
        let gb = |k: &str| by_layer.get(k).map_or(0, |v| v.1);
        Ok(IndexStats {
            files: by_layer.values().map(|v| v.0).sum(),
            bytes: by_layer.values().map(|v| v.1).sum(),
            note_bytes: gb("note"),
            wiki_bytes: gb("wiki"),
            source_bytes: gb("source"),
            template_bytes: gb("template"),
            notes: g("note"),
            wiki: g("wiki"),
            sources: g("source"),
            templates: g("template"),
            tasks: one("SELECT count(*) FROM tasks")?,
            open_tasks: one("SELECT count(*) FROM tasks WHERE done = 0")?,
            links: one("SELECT count(*) FROM links WHERE kind = 'link'")?,
            // A template's links are made when it runs (`[[<% … %>]]`), so they don't count.
            unresolved_links: one(
                "SELECT count(*) FROM links l JOIN files f ON f.id = l.src WHERE l.kind = 'link' AND l.target_id IS NULL AND f.layer != 'template'",
            )?,
            updated_at: self.meta("updated_at")?.and_then(|v| v.parse().ok()).unwrap_or(0),
            last_sync_ms: self.meta("last_sync_ms")?.and_then(|v| v.parse().ok()).unwrap_or(0),
        })
    }
}

/// Past this many names and files in one pass (a first sync, a big folder moved in), every link
/// is resolved in one statement instead of a few at a time.
const RESOLVE_ALL_ABOVE: usize = 2000;

/// Folder listings, read once per pass, to tell a file's own name from another spelling of it
/// that a case-insensitive disk also opens.
#[derive(Default)]
struct Listings(HashMap<PathBuf, Option<std::collections::HashSet<std::ffi::OsString>>>);

impl Listings {
    /// Whether `p`'s last part is spelt as on disk (true when that can't be told).
    fn named(&mut self, p: &Path) -> bool {
        let (Some(dir), Some(name)) = (p.parent(), p.file_name()) else { return true };
        let names = self
            .0
            .entry(dir.to_path_buf())
            .or_insert_with(|| std::fs::read_dir(dir).ok().map(|rd| rd.flatten().map(|e| e.file_name()).collect()));
        names.as_ref().is_none_or(|n| n.contains(name))
    }
}

struct Fingerprint {
    size: i64,
    mtime: i64,
    sha: String,
}

fn store(tx: &rusqlite::Transaction, f: &VaultFile, sha: &str, p: &Parsed) -> Result<i64> {
    let fm = if p.frontmatter.is_null() { None } else { Some(p.frontmatter.to_string()) };
    let project_line = p.project_line.map(|l| l as i64);
    let existing: Option<i64> = tx.query_row("SELECT id FROM files WHERE path = ?1", [&f.rel], |r| r.get(0)).optional().map_err(e)?;
    let id = match existing {
        // Kept, so links from other files still point at it.
        Some(id) => {
            tx.execute("DELETE FROM chunks_fts WHERE rowid IN (SELECT id FROM chunks WHERE file_id = ?1)", [id]).map_err(e)?;
            for t in ["links", "tags", "aliases", "tasks", "chunks"] {
                let col = if t == "links" { "src" } else { "file_id" };
                tx.execute(&format!("DELETE FROM {t} WHERE {col} = ?1"), [id]).map_err(e)?;
            }
            tx.execute(
                "UPDATE files SET key=?2, layer=?3, sha256=?4, size=?5, mtime=?6, type=?7, title=?8, date=?9, frontmatter=?10, frontmatter_error=?11, project_line=?12, current_state=?13 WHERE id=?1",
                params![id, key(&f.rel), f.layer.as_str(), sha, f.size as i64, f.mtime, p.kind, p.title, p.date, fm, p.frontmatter_error, project_line, p.current_state],
            )
            .map_err(e)?;
            id
        }
        None => {
            tx.execute(
                "INSERT INTO files(path, key, layer, sha256, size, mtime, type, title, date, frontmatter, frontmatter_error, project_line, current_state) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                params![f.rel, key(&f.rel), f.layer.as_str(), sha, f.size as i64, f.mtime, p.kind, p.title, p.date, fm, p.frontmatter_error, project_line, p.current_state],
            )
            .map_err(e)?;
            tx.last_insert_rowid()
        }
    };
    {
        let mut st = tx
            .prepare_cached("INSERT INTO links(src, target, target_key, kind, heading, block, alias, line) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")
            .map_err(e)?;
        for l in &p.links {
            st.execute(params![id, l.target, key(&l.target), l.kind.as_str(), l.heading, l.block, l.alias, l.line as i64]).map_err(e)?;
        }
        let mut st = tx.prepare_cached("INSERT INTO tags(file_id, tag) VALUES(?1, ?2)").map_err(e)?;
        for t in &p.tags {
            st.execute(params![id, t]).map_err(e)?;
        }
        let mut st = tx.prepare_cached("INSERT INTO aliases(file_id, alias, key) VALUES(?1, ?2, ?3)").map_err(e)?;
        for a in &p.aliases {
            st.execute(params![id, a, key(a)]).map_err(e)?;
        }
        let mut st = tx
            .prepare_cached(
                "INSERT INTO tasks(file_id, line, line_text, text, done, due, scheduled, start, done_on, created, rank, tags, heading, status, cancelled, contexts, effort, effort_min) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            )
            .map_err(e)?;
        for t in &p.tasks {
            st.execute(params![
                id,
                t.line as i64,
                t.line_text,
                t.text,
                t.done,
                t.due,
                t.scheduled,
                t.start,
                t.done_on,
                t.created,
                t.rank,
                t.tags.join(" "),
                t.heading,
                t.status,
                t.cancelled,
                t.contexts.join(" "),
                t.effort,
                t.effort_min
            ])
            .map_err(e)?;
        }
        let tags = p.tags.join(" ");
        let mut st = tx.prepare_cached("INSERT INTO chunks(file_id, heading_path, start, end, text) VALUES(?1,?2,?3,?4,?5)").map_err(e)?;
        let mut fts =
            tx.prepare_cached("INSERT INTO chunks_fts(rowid, title, heading_path, text, tags) VALUES(?1,?2,?3,?4,?5)").map_err(e)?;
        for c in &p.chunks {
            st.execute(params![id, c.heading_path, c.start as i64, c.end as i64, c.text]).map_err(e)?;
            fts.execute(params![tx.last_insert_rowid(), p.title, c.heading_path, c.text, tags]).map_err(e)?;
        }
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault")
    }

    /// The fixture vault copied to a temp dir, so tests can change it.
    fn temp_vault() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        copy_dir(&fixture(), t.path());
        t
    }

    fn copy_dir(from: &Path, to: &Path) {
        for e in walkdir::WalkDir::new(from) {
            let e = e.unwrap();
            let dest = to.join(e.path().strip_prefix(from).unwrap());
            if e.file_type().is_dir() {
                std::fs::create_dir_all(&dest).unwrap();
            } else {
                std::fs::copy(e.path(), &dest).unwrap();
            }
        }
    }

    fn count(ix: &Index, sql: &str) -> i64 {
        ix.conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn indexes_the_fixture_vault() {
        let v = Vault::new(fixture(), vec!["archived".into()]);
        let mut ix = Index::open_in_memory().unwrap();
        let ch = ix.sync(&v).unwrap();
        assert!(!ch.changed.is_empty());
        let s = ix.stats().unwrap();
        assert!(s.notes > 5 && s.wiki > 0 && s.sources > 0 && s.templates > 0, "{s:?}");
        assert!(s.note_bytes > 0 && s.bytes == s.note_bytes + s.wiki_bytes + s.source_bytes + s.template_bytes, "{s:?}");
        // System and excluded folders, schema files and conflict copies stay out.
        for p in ["CLAUDE.md", "index.md", ".trash/", ".obsidian/", "images/", "archived/", "2026-09-30 2.md"] {
            assert_eq!(count(&ix, &format!("SELECT count(*) FROM files WHERE path LIKE '%{p}%'")), 0, "{p}");
        }
        // A second pass with nothing changed reads nothing.
        assert!(ix.sync(&v).unwrap().is_empty());
        // Links resolve by name and by alias; ghost links stay unresolved.
        let resolved = |t: &str| count(&ix, &format!("SELECT count(*) FROM links WHERE target = '{t}' AND target_id IS NOT NULL"));
        assert!(resolved("Orbit App") > 0);
        assert!(resolved("OA") > 0);
        assert_eq!(resolved("Nowhere Note"), 0);
        // Search finds words from the body, stemmed.
        let hits = count(&ix, "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH 'launching'");
        assert!(hits > 0);
    }

    #[test]
    fn follows_changes() {
        let t = temp_vault();
        let v = Vault::new(t.path(), vec![]);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&v).unwrap();
        let before = ix.stats().unwrap();

        let new = t.path().join("Idea. Watcher test.md");
        std::fs::write(&new, "- [ ] try it 📅 2026-10-05\nlinks to [[Orbit App]] zebrafish").unwrap();
        let ch = ix.update(&v, std::slice::from_ref(&new)).unwrap();
        assert_eq!(ch.changed, ["Idea. Watcher test.md"]);
        assert_eq!(ix.stats().unwrap().notes, before.notes + 1);
        assert_eq!(count(&ix, "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH 'zebrafish'"), 1);

        // Rewritten with the same bytes: only the fingerprint is updated.
        std::fs::write(&new, std::fs::read(&new).unwrap()).unwrap();
        assert!(ix.update(&v, std::slice::from_ref(&new)).unwrap().changed.is_empty());

        std::fs::remove_file(&new).unwrap();
        let ch = ix.update(&v, &[new]).unwrap();
        assert_eq!(ch.removed, ["Idea. Watcher test.md"]);
        assert_eq!(ix.stats().unwrap().notes, before.notes);
        assert_eq!(count(&ix, "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH 'zebrafish'"), 0);

        // A linked-to page that goes leaves its links unresolved, and they resolve again when it's back.
        let target = t.path().join("wiki/entities/Orbit App.md");
        let saved = std::fs::read(&target).unwrap();
        std::fs::remove_file(&target).unwrap();
        ix.update(&v, std::slice::from_ref(&target)).unwrap();
        assert_eq!(count(&ix, "SELECT count(*) FROM links WHERE target = 'Orbit App' AND target_id IS NOT NULL"), 0);
        std::fs::write(&target, saved).unwrap();
        ix.update(&v, &[target]).unwrap();
        assert!(count(&ix, "SELECT count(*) FROM links WHERE target = 'Orbit App' AND target_id IS NOT NULL") > 0);
    }

    #[test]
    fn a_folder_event_for_a_folder_still_there_keeps_its_files() {
        // A synced folder reports the folder itself when a file is added to it.
        let t = temp_vault();
        let v = Vault::new(t.path(), vec![]);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&v).unwrap();
        let sources = count(&ix, "SELECT count(*) FROM files WHERE layer = 'source'");
        assert!(sources > 0);
        let new = t.path().join("sources/Email. Thread. Launch - 2026-10-03.md");
        std::fs::write(&new, "# Launch\n").unwrap();
        let ch = ix.update(&v, &[t.path().join("sources"), new]).unwrap();
        assert!(ch.removed.is_empty(), "{:?}", ch.removed);
        assert_eq!(count(&ix, "SELECT count(*) FROM files WHERE layer = 'source'"), sources + 1);
        // The folder going does remove them.
        std::fs::remove_dir_all(t.path().join("sources")).unwrap();
        ix.update(&v, &[t.path().join("sources")]).unwrap();
        assert_eq!(count(&ix, "SELECT count(*) FROM files WHERE layer = 'source'"), 0);
    }

    #[test]
    fn a_pass_resolves_only_the_links_it_affects() {
        let t = temp_vault();
        let v = Vault::new(t.path(), vec![]);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&v).unwrap();
        let resolved =
            |ix: &Index, t: &str| count(ix, &format!("SELECT count(*) FROM links WHERE target = '{t}' AND target_id IS NOT NULL"));
        assert!(resolved(&ix, "Orbit App") > 0);
        // Marked unresolved by hand: a pass that changes nothing about that name leaves it so.
        ix.conn.execute_batch("UPDATE links SET target_id = NULL WHERE target = 'Orbit App'").unwrap();
        let other = t.path().join("Idea. Accents.md");
        let text = std::fs::read_to_string(&other).unwrap();
        std::fs::write(&other, format!("{text}\nOne more line.\n")).unwrap();
        assert_eq!(ix.update(&v, std::slice::from_ref(&other)).unwrap().changed, ["Idea. Accents.md"]);
        assert_eq!(resolved(&ix, "Orbit App"), 0);
        // A new alias, and a new file, resolve the links to their names elsewhere.
        let linker = t.path().join("Idea. Linker.md");
        std::fs::write(&linker, "[[Zebra Name]] and [[Fresh Page]]\n").unwrap();
        ix.update(&v, std::slice::from_ref(&linker)).unwrap();
        assert_eq!(resolved(&ix, "Zebra Name") + resolved(&ix, "Fresh Page"), 0);
        std::fs::write(&other, format!("---\naliases: [Zebra Name]\n---\n{text}")).unwrap();
        std::fs::write(t.path().join("Fresh Page.md"), "x\n").unwrap();
        ix.update(&v, &[other.clone(), t.path().join("Fresh Page.md")]).unwrap();
        assert_eq!((resolved(&ix, "Zebra Name"), resolved(&ix, "Fresh Page")), (1, 1));
        // And taking the alias away unresolves them again.
        std::fs::write(&other, &text).unwrap();
        ix.update(&v, std::slice::from_ref(&other)).unwrap();
        assert_eq!(resolved(&ix, "Zebra Name"), 0);
    }

    #[test]
    fn a_folder_moved_in_is_walked_by_itself() {
        let t = temp_vault();
        let v = Vault::new(t.path(), vec![]);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&v).unwrap();
        let dir = t.path().join("Moved in");
        std::fs::create_dir_all(dir.join("deeper/.hidden")).unwrap();
        std::fs::write(dir.join("A.md"), "a\n").unwrap();
        std::fs::write(dir.join("deeper/B.md"), "b\n").unwrap();
        std::fs::write(dir.join("deeper/.hidden/C.md"), "c\n").unwrap();
        let mut ch = ix.update(&v, &[dir]).unwrap().changed;
        ch.sort();
        assert_eq!(ch, ["Moved in/A.md", "Moved in/deeper/B.md"]);
    }

    #[test]
    fn a_case_only_rename_elsewhere_isnt_a_second_file() {
        let t = temp_vault();
        let v = Vault::new(t.path(), vec![]);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&v).unwrap();
        let files = ix.stats().unwrap().files;
        let (old, new) = (t.path().join("Idea. Images.md"), t.path().join("Idea. images.md"));
        let tmp = t.path().join("tmp-rename.md");
        std::fs::rename(&old, &tmp).unwrap();
        std::fs::rename(&tmp, &new).unwrap();
        let ch = ix.update(&v, &[old.clone(), new.clone()]).unwrap();
        assert_eq!(
            (ch.changed.as_slice(), ch.removed.as_slice()),
            (&["Idea. images.md".to_string()][..], &["Idea. Images.md".to_string()][..])
        );
        assert_eq!(ix.stats().unwrap().files, files);
        // Brainstead's own rename, refreshed: the same.
        std::fs::rename(&new, &tmp).unwrap();
        std::fs::rename(&tmp, &old).unwrap();
        ix.refresh(&v, &[new, old]).unwrap();
        assert_eq!(ix.stats().unwrap().files, files);
        assert_eq!(count(&ix, "SELECT count(*) FROM files WHERE path = 'Idea. Images.md'"), 1);
    }

    #[test]
    fn schema_change_rebuilds() {
        let t = tempfile::tempdir().unwrap();
        let db = t.path().join("index.db");
        {
            let ix = Index::open(&db).unwrap();
            ix.conn.execute_batch("PRAGMA user_version = 0;").unwrap();
        }
        let ix = Index::open(&db).unwrap();
        assert_eq!(ix.stats().unwrap().files, 0);
    }
}
