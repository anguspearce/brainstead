// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only questions to the index: the files in a layer, one document with what it links to and
//! what links to it.

use std::sync::LazyLock;

use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use crate::index::{Index, Result};

fn e<E: std::fmt::Display>(x: E) -> String {
    x.to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    pub path: String,
    pub layer: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub title: String,
    /// From the filename, else the `date` or `created` property; never the modification time.
    pub date: Option<String>,
    pub size: i64,
    pub mtime: i64,
    pub tags: Vec<String>,
    /// The project note it belongs to: itself for a project note, else the one its
    /// `Project: [[…]]` line names (`projects::project_line`).
    pub project: Option<String>,
}

static SUMMARY_COLS: LazyLock<String> = LazyLock::new(|| {
    format!(
        "f.path, f.layer, f.type, f.title, f.date, f.size, f.mtime, (SELECT group_concat(tag, char(31)) FROM tags t WHERE t.file_id = f.id),
         CASE WHEN {} THEN f.path ELSE (SELECT sp.path FROM files sp WHERE sp.id = {}) END",
        crate::projects::is_project_sql("f"),
        crate::projects::member_project_sql("f")
    )
});

fn summary(r: &Row) -> rusqlite::Result<FileSummary> {
    let tags: Option<String> = r.get(7)?;
    Ok(FileSummary {
        path: r.get(0)?,
        layer: r.get(1)?,
        kind: r.get(2)?,
        title: r.get(3)?,
        date: r.get(4)?,
        size: r.get(5)?,
        mtime: r.get(6)?,
        tags: tags.map(|t| t.split('\u{1f}').map(String::from).collect()).unwrap_or_default(),
        project: r.get(8)?,
    })
}

/// A link as written in a document, and where it goes (None: a ghost link).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutLink {
    pub target: String,
    pub kind: String,
    pub heading: Option<String>,
    pub block: Option<String>,
    pub resolved: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backlink {
    pub path: String,
    pub title: String,
    pub layer: String,
    /// The 0-based line the first link is on, and that line's text (filled from the file by
    /// `fill_context`, so it's what's on disk now).
    pub line: Option<i64>,
    pub context: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocMeta {
    pub summary: FileSummary,
    pub frontmatter: serde_json::Value,
    pub frontmatter_error: Option<String>,
    pub aliases: Vec<String>,
    pub links: Vec<OutLink>,
    pub backlinks: Vec<Backlink>,
}

impl Index {
    /// Every file in a layer ("note", "wiki", "source", "template"), or all of them for None.
    pub fn list(&self, layer: Option<&str>) -> Result<Vec<FileSummary>> {
        let sql = format!("SELECT {} FROM files f WHERE ?1 IS NULL OR f.layer = ?1 ORDER BY f.path", *SUMMARY_COLS);
        let mut st = self.conn().prepare(&sql).map_err(e)?;
        let rows = st.query_map([layer], summary).map_err(e)?;
        rows.collect::<std::result::Result<_, _>>().map_err(e)
    }

    pub fn summary(&self, path: &str) -> Result<Option<FileSummary>> {
        let sql = format!("SELECT {} FROM files f WHERE f.path = ?1", *SUMMARY_COLS);
        self.conn().query_row(&sql, [path], summary).optional().map_err(e)
    }

    pub fn summary_by_id(&self, id: i64) -> Result<Option<FileSummary>> {
        let sql = format!("SELECT {} FROM files f WHERE f.id = ?1", *SUMMARY_COLS);
        self.conn().prepare_cached(&sql).map_err(e)?.query_row([id], summary).optional().map_err(e)
    }

    /// The index's view of one document. Its text is read from the vault separately, so it's
    /// always what's on disk now.
    pub fn doc_meta(&self, path: &str) -> Result<Option<DocMeta>> {
        let c = self.conn();
        let Some((id, fm, fm_err)) = c
            .query_row("SELECT id, frontmatter, frontmatter_error FROM files WHERE path = ?1", [path], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?))
            })
            .optional()
            .map_err(e)?
        else {
            return Ok(None);
        };
        let summary = self.summary(path)?.ok_or("gone")?;
        let aliases = c
            .prepare("SELECT alias FROM aliases WHERE file_id = ?1")
            .map_err(e)?
            .query_map([id], |r| r.get(0))
            .map_err(e)?
            .collect::<std::result::Result<Vec<String>, _>>()
            .map_err(e)?;
        let links = c
            .prepare(
                "SELECT l.target, l.kind, l.heading, l.block, t.path FROM links l LEFT JOIN files t ON t.id = l.target_id WHERE l.src = ?1",
            )
            .map_err(e)?
            .query_map([id], |r| {
                Ok(OutLink { target: r.get(0)?, kind: r.get(1)?, heading: r.get(2)?, block: r.get(3)?, resolved: r.get(4)? })
            })
            .map_err(e)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(e)?;
        Ok(Some(DocMeta {
            summary,
            frontmatter: fm.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::Value::Null),
            frontmatter_error: fm_err,
            aliases,
            links,
            backlinks: self.backlinks(id)?,
        }))
    }

    /// Files that link to (or embed) this one, once each, with the first line that does.
    fn backlinks(&self, id: i64) -> Result<Vec<Backlink>> {
        let mut st = self
            .conn()
            .prepare(
                "SELECT f.path, f.title, f.layer, min(l.line)
                 FROM links l JOIN files f ON f.id = l.src
                 WHERE l.target_id = ?1 AND l.src != ?1
                 GROUP BY f.id ORDER BY f.layer, f.title",
            )
            .map_err(e)?;
        let rows = st
            .query_map(params![id], |r| {
                Ok(Backlink { path: r.get(0)?, title: r.get(1)?, layer: r.get(2)?, line: r.get(3)?, context: String::new() })
            })
            .map_err(e)?;
        rows.collect::<std::result::Result<_, _>>().map_err(e)
    }

    /// The files with a link (or embed) that resolves to this one, for a rename.
    pub fn linkers(&self, path: &str) -> Result<Vec<String>> {
        let mut st = self
            .conn()
            .prepare(
                "SELECT DISTINCT f.path FROM links l JOIN files f ON f.id = l.src JOIN files t ON t.id = l.target_id WHERE t.path = ?1",
            )
            .map_err(e)?;
        let rows = st.query_map([path], |r| r.get(0)).map_err(e)?;
        rows.collect::<std::result::Result<_, _>>().map_err(e)
    }

    /// Where each wikilink target goes, for rendering links in text the index hasn't seen (a
    /// document that changed since). Same rules as resolution in the index: file name, then alias.
    pub fn resolve(&self, targets: &[String]) -> Result<Vec<Option<String>>> {
        let mut st = self
            .conn()
            .prepare_cached(
                "SELECT COALESCE(
                   (SELECT path FROM files WHERE key = ?1 ORDER BY length(path), id LIMIT 1),
                   (SELECT f.path FROM aliases a JOIN files f ON f.id = a.file_id WHERE a.key = ?1 ORDER BY a.file_id LIMIT 1))",
            )
            .map_err(e)?;
        targets.iter().map(|t| st.query_row([crate::links::key(t)], |r| r.get(0)).map_err(e)).collect()
    }
}

/// Reads each backlink's line from its file under `root`, trimmed of list markers and cut to a
/// readable length.
pub fn fill_context(root: &std::path::Path, backlinks: &mut [Backlink]) {
    for b in backlinks {
        let (Some(n), Ok(text)) = (b.line, std::fs::read_to_string(root.join(&b.path))) else { continue };
        if let Some(l) = text.lines().nth(n as usize) {
            let t = l.trim().trim_start_matches(['-', '*', '+', '>']).trim();
            b.context = if t.chars().count() > 240 { format!("{}…", t.chars().take(240).collect::<String>()) } else { t.to_string() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;
    use std::path::PathBuf;

    fn ix() -> Index {
        let v = Vault::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault"), vec!["archived".into()]);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&v).unwrap();
        ix
    }

    #[test]
    fn lists_layers() {
        let ix = ix();
        let wiki = ix.list(Some("wiki")).unwrap();
        assert!(wiki.iter().any(|f| f.title == "Orbit App" && f.tags == ["project", "hub"]));
        assert!(ix.list(Some("note")).unwrap().iter().all(|f| f.layer == "note"));
        assert!(ix.list(None).unwrap().len() > wiki.len());
    }

    #[test]
    fn doc_with_links_and_backlinks() {
        let ix = ix();
        let m = ix.doc_meta("wiki/entities/Orbit App.md").unwrap().unwrap();
        assert_eq!(m.aliases, ["OA", "The Orbit App"]);
        assert_eq!(m.frontmatter["type"], "entity");
        assert!(m.backlinks.iter().any(|b| b.path == "Meeting. Orbit App Steerco - 2026-09-30.md"));
        assert!(m.backlinks.iter().any(|b| b.path == "wiki/concepts/Hub Platform.md"));
        let meeting = ix.doc_meta("Meeting. Orbit App Steerco - 2026-09-30.md").unwrap().unwrap();
        let ghost = meeting.links.iter().find(|l| l.target == "Nowhere Note").unwrap();
        assert!(ghost.resolved.is_none());
        let local = meeting.links.iter().find(|l| l.target.is_empty()).unwrap();
        assert_eq!(local.resolved.as_deref(), Some("Meeting. Orbit App Steerco - 2026-09-30.md"));
        assert!(ix.doc_meta("nope.md").unwrap().is_none());
        let mut bl = m.backlinks.clone();
        fill_context(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault"), &mut bl);
        let b = bl.iter().find(|b| b.path.starts_with("Meeting.")).unwrap();
        assert!(b.context.starts_with("Discussed [[Orbit App]]"), "{}", b.context);
    }

    #[test]
    fn resolves_names_and_aliases() {
        let ix = ix();
        let r =
            ix.resolve(&["orbit app".into(), "The Orbit App".into(), "Nowhere".into(), "wiki/concepts/Hub Platform.md".into()]).unwrap();
        assert_eq!(r[0].as_deref(), Some("wiki/entities/Orbit App.md"));
        assert_eq!(r[1].as_deref(), Some("wiki/entities/Orbit App.md"));
        assert_eq!(r[2], None);
        assert_eq!(r[3].as_deref(), Some("wiki/concepts/Hub Platform.md"));
    }
}
