// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Activity screen's data (§4): `log.md`'s entries newest first, read as the previous app's
//! `vault-log.ts` read them, and a heatmap of file changes by day (each note, wiki page and source
//! counted on the day it last changed, in local time). Also the vault at a glance: notes and wiki
//! pages by type, their top tags, and the most linked files (Activity and Knowledge health).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::{Local, TimeZone};
use regex::Regex;
use serde::Serialize;

use crate::index::{Index, Result};
use crate::read::FileSummary;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LogEntry {
    /// YYYY-MM-DD.
    pub date: String,
    /// HH:MM, when the heading has one.
    pub time: Option<String>,
    pub action: String,
    pub title: String,
    pub description: String,
}

static ENTRY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^##\s+\[(\d{4}-\d{2}-\d{2})(?:\s+(\d{2}:\d{2}))?\]\s+([a-zA-Z][\w-]*)\s*\|\s*(.+)$").unwrap());

/// `log.md`'s entries, newest first (by date and time; entries without a time keep their order).
pub fn parse_log(text: &str) -> Vec<LogEntry> {
    let mut out: Vec<LogEntry> = vec![];
    let mut desc: Vec<&str> = vec![];
    let flush = |out: &mut Vec<LogEntry>, desc: &mut Vec<&str>| {
        if let Some(last) = out.last_mut() {
            last.description = desc.join("\n").trim().to_string();
        }
        desc.clear();
    };
    for line in text.lines() {
        if let Some(c) = ENTRY.captures(line.trim_end_matches('\r')) {
            flush(&mut out, &mut desc);
            out.push(LogEntry {
                date: c[1].to_string(),
                time: c.get(2).map(|m| m.as_str().to_string()),
                action: c[3].to_string(),
                title: c[4].trim().to_string(),
                description: String::new(),
            });
        } else if !out.is_empty() {
            desc.push(line);
        }
    }
    flush(&mut out, &mut desc);
    let key = |e: &LogEntry| match &e.time {
        Some(t) => format!("{}T{t}", e.date),
        None => e.date.clone(),
    };
    out.sort_by_key(|e| std::cmp::Reverse(key(e)));
    out
}

/// One day of the heatmap.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Day {
    pub date: String,
    pub notes: usize,
    pub wiki: usize,
    pub sources: usize,
}

/// The local day a file last changed (its mtime in milliseconds).
pub fn day_of(mtime_ms: i64) -> Option<String> {
    Local.timestamp_millis_opt(mtime_ms).single().map(|t| t.format("%Y-%m-%d").to_string())
}

/// Files changed per day, oldest first; only days with changes.
pub fn heatmap(files: &[FileSummary]) -> Vec<Day> {
    let mut days: BTreeMap<String, Day> = BTreeMap::new();
    for f in files {
        let Some(d) = day_of(f.mtime) else { continue };
        let day = days.entry(d.clone()).or_insert_with(|| Day { date: d, ..Day::default() });
        match f.layer.as_str() {
            "note" => day.notes += 1,
            "wiki" => day.wiki += 1,
            "source" => day.sources += 1,
            _ => {}
        }
    }
    days.into_values().filter(|d| d.notes + d.wiki + d.sources > 0).collect()
}

/// How many files have one value of something (a type, a tag); `""` is none.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Count {
    pub name: String,
    pub n: usize,
}

/// A file and how many other files link to it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Linked {
    pub path: String,
    pub title: String,
    pub layer: String,
    pub links: usize,
}

/// The vault at a glance: per layer (notes, wiki), files by type and by tag, most first;
/// and the notes and wiki pages the most other files link to.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Glance {
    pub note_types: Vec<Count>,
    pub note_tags: Vec<Count>,
    pub wiki_types: Vec<Count>,
    pub wiki_tags: Vec<Count>,
    pub most_linked: Vec<Linked>,
}

fn counts<'a>(items: impl Iterator<Item = &'a str>) -> Vec<Count> {
    let mut m: BTreeMap<&str, usize> = BTreeMap::new();
    for i in items {
        *m.entry(i).or_default() += 1;
    }
    let mut v: Vec<Count> = m.into_iter().map(|(name, n)| Count { name: name.into(), n }).collect();
    v.sort_by(|a, b| b.n.cmp(&a.n).then(a.name.cmp(&b.name)));
    v
}

impl Index {
    /// The vault at a glance (see `Glance`).
    pub fn glance(&self) -> Result<Glance> {
        let files = self.list(None)?;
        let layer = |l: &'static str| files.iter().filter(move |f| f.layer == l);
        let types = |l| counts(layer(l).map(|f| f.kind.as_deref().unwrap_or("")));
        let tags = |l| counts(layer(l).flat_map(|f| f.tags.iter().map(String::as_str)));
        let mut st = self
            .conn()
            .prepare(
                "SELECT t.path, t.title, t.layer, count(DISTINCT l.src) AS n
                 FROM links l JOIN files t ON t.id = l.target_id
                 WHERE l.src != l.target_id AND t.layer IN ('note', 'wiki')
                 GROUP BY t.id ORDER BY n DESC, t.title LIMIT 10",
            )
            .map_err(|x| x.to_string())?;
        let most_linked = st
            .query_map([], |r| Ok(Linked { path: r.get(0)?, title: r.get(1)?, layer: r.get(2)?, links: r.get::<_, i64>(3)? as usize }))
            .map_err(|x| x.to_string())?
            .collect::<std::result::Result<_, _>>()
            .map_err(|x| x.to_string())?;
        Ok(Glance { note_types: types("note"), note_tags: tags("note"), wiki_types: types("wiki"), wiki_tags: tags("wiki"), most_linked })
    }

    /// Every note, wiki page and source, for the heatmap.
    pub fn activity_files(&self) -> Result<Vec<FileSummary>> {
        Ok(self.list(None)?.into_iter().filter(|f| f.layer != "template").collect())
    }

    /// The files that last changed on `date` (YYYY-MM-DD, local), newest first.
    pub fn changed_on(&self, date: &str) -> Result<Vec<FileSummary>> {
        let mut out: Vec<FileSummary> = self.activity_files()?.into_iter().filter(|f| day_of(f.mtime).as_deref() == Some(date)).collect();
        out.sort_by_key(|f| std::cmp::Reverse(f.mtime));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_log_newest_first() {
        let log = "# Log\n\nAppend-only record.\n\n## [2026-09-28 06:45] review | daily-review Mon 28 Sep\n\n## [2026-09-29 17:42] update | Orbit App\nQ4 milestones\n\n## [2026-09-29] ingest | Roadmap Update\n3 pages\nand a summary\n\n## not an entry\n## [2026-09-29 09:10] lint-fix | Knowledge health\n";
        let e = parse_log(log);
        assert_eq!(
            e.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
            ["Orbit App", "Knowledge health", "Roadmap Update", "daily-review Mon 28 Sep"]
        );
        assert_eq!(e[0].time.as_deref(), Some("17:42"));
        assert_eq!(e[0].description, "Q4 milestones");
        assert_eq!(e[1].action, "lint-fix");
        // A line that isn't a heading stays in the entry above it.
        assert_eq!(e[2].description, "3 pages\nand a summary\n\n## not an entry");
        assert_eq!(e[2].time, None);
        assert!(parse_log("# Log\n\nNothing yet.\n").is_empty());
    }

    #[test]
    fn counts_files_by_the_day_they_changed() {
        let at = |s: &str| {
            let n = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
            Local.from_local_datetime(&n).single().unwrap().timestamp_millis()
        };
        let f = |path: &str, layer: &str, when: &str| FileSummary {
            path: path.into(),
            layer: layer.into(),
            kind: None,
            title: path.into(),
            date: None,
            size: 1,
            mtime: at(when),
            tags: vec![],
            project: None,
        };
        let files = [
            f("a.md", "note", "2026-09-29 08:00"),
            f("b.md", "note", "2026-09-29 23:59"),
            f("wiki/c.md", "wiki", "2026-09-29 12:00"),
            f("sources/d.md", "source", "2026-09-30 00:01"),
        ];
        let h = heatmap(&files);
        assert_eq!(h.len(), 2);
        assert_eq!((h[0].date.as_str(), h[0].notes, h[0].wiki, h[0].sources), ("2026-09-29", 2, 1, 0));
        assert_eq!((h[1].date.as_str(), h[1].sources), ("2026-09-30", 1));
    }

    #[test]
    fn counts_types_and_tags_and_the_most_linked() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault");
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&crate::vault::Vault::new(fixture, vec![])).unwrap();
        let g = ix.glance().unwrap();
        let notes = ix.list(None).unwrap().into_iter().filter(|f| f.layer == "note").count();
        assert_eq!(g.note_types.iter().map(|c| c.n).sum::<usize>(), notes);
        assert!(g.note_types.windows(2).all(|w| w[0].n >= w[1].n));
        assert!(g.wiki_tags.iter().any(|c| c.name == "project"));
        assert!(!g.most_linked.is_empty() && g.most_linked.windows(2).all(|w| w[0].links >= w[1].links));
        assert!(g.most_linked.iter().all(|l| l.layer == "note" || l.layer == "wiki"));
    }

    #[test]
    fn lists_a_days_files_from_the_index() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault");
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&crate::vault::Vault::new(fixture, vec![])).unwrap();
        let all = ix.activity_files().unwrap();
        assert!(!all.is_empty() && all.iter().all(|f| f.layer != "template"));
        let day = day_of(all[0].mtime).unwrap();
        let on = ix.changed_on(&day).unwrap();
        assert!(on.iter().any(|f| f.path == all[0].path));
        assert!(ix.changed_on("1999-01-01").unwrap().is_empty());
    }
}
