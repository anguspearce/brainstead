// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Projects: a `Project. <name>.md` note whose properties hold `status` (active, on-hold, someday,
//! done), `area` and `outcome`. Its tasks are those written in it and any task elsewhere whose line
//! links it (see `taskquery`). A missing or unknown status reads as active.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::params;
use serde::Serialize;

use crate::index::{Index, Result};
use crate::write::{self, FileChange, WriteError};

fn e<E: std::fmt::Display>(x: E) -> String {
    x.to_string()
}

pub const PREFIX: &str = "Project. ";

/// The headings a new project note starts with.
pub const NEXT_ACTIONS: &str = "Next actions";
pub const WAITING_FOR: &str = "Waiting for";
pub const NOTES: &str = "Notes";

/// Whether a vault path is a project note: its file name is `Project. <name>.md`.
pub fn is_project_path(rel: &str) -> bool {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    base.len() > PREFIX.len() + 3 && base.starts_with(PREFIX) && base.ends_with(".md")
}

/// The same test in SQL, for the `files` row aliased `f`. Notes only, so templates don't count.
pub(crate) fn is_project_sql(f: &str) -> String {
    format!("({f}.layer = 'note' AND substr({f}.path, length(rtrim({f}.path, replace({f}.path, '/', ''))) + 1) GLOB 'Project. ?*.md')")
}

/// A project's name: its file name without `Project. ` and `.md`.
pub fn project_name(rel: &str) -> String {
    let stem = crate::filename::stem(rel);
    stem.strip_prefix(PREFIX).unwrap_or(stem).to_string()
}

/// A status as written, as one of the four, or None when it isn't one.
pub fn parse_status(s: &str) -> Option<&'static str> {
    match s.trim().to_lowercase().replace(['_', ' '], "-").as_str() {
        "active" => Some("active"),
        "on-hold" | "hold" | "onhold" | "paused" => Some("on-hold"),
        "someday" | "someday-maybe" | "someday/maybe" | "maybe" => Some("someday"),
        "done" | "completed" | "complete" => Some("done"),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    pub path: String,
    pub name: String,
    /// active, on-hold, someday or done.
    pub status: String,
    pub area: Option<String>,
    /// What done looks like.
    pub outcome: Option<String>,
    /// Open tasks that aren't waiting-for or someday-maybe.
    pub next: usize,
    /// Open `#waiting-for` tasks.
    pub waiting: usize,
    /// Open `#someday-maybe` tasks.
    pub someday: usize,
    /// Ticked tasks (not cancelled ones).
    pub done: usize,
    /// When the note, or a file holding one of its tasks, last changed, in ms since the epoch.
    pub last_touched: i64,
    /// Notes that link it, by path.
    pub links: Vec<String>,
}

impl Index {
    /// Every project note, by name.
    pub fn projects(&self) -> Result<Vec<ProjectRow>> {
        let notes: Vec<(i64, String, Option<String>, i64)> = {
            let mut st = self
                .conn()
                .prepare(&format!("SELECT f.id, f.path, f.frontmatter, f.mtime FROM files f WHERE {}", is_project_sql("f")))
                .map_err(e)?;
            let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(e)?;
            rows.collect::<std::result::Result<_, _>>().map_err(e)?
        };
        let mut by_project: HashMap<String, Vec<crate::taskquery::TaskRow>> = HashMap::new();
        for t in self.all_tasks()? {
            if let Some(p) = t.project.clone() {
                by_project.entry(p).or_default().push(t);
            }
        }
        let mut backlinks = self
            .conn()
            .prepare_cached(
                "SELECT DISTINCT s.path FROM links l JOIN files s ON s.id = l.src
                 WHERE l.target_id = ?1 AND l.kind = 'link' AND l.src != ?1 ORDER BY s.path",
            )
            .map_err(e)?;
        let mut out = Vec::new();
        for (id, path, fm, mtime) in notes {
            let fm: serde_json::Value = fm.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
            let prop = |k: &str| crate::frontmatter::string(&fm, k);
            let tasks = by_project.remove(&path).unwrap_or_default();
            let has = |t: &crate::taskquery::TaskRow, tag: &str| t.tags.iter().any(|g| g == tag || g.starts_with(&format!("{tag}/")));
            let open: Vec<_> = tasks.iter().filter(|t| !t.done).collect();
            let waiting = open.iter().filter(|t| has(t, "waiting-for")).count();
            let someday = open.iter().filter(|t| !has(t, "waiting-for") && has(t, "someday-maybe")).count();
            let links =
                backlinks.query_map(params![id], |r| r.get(0)).map_err(e)?.collect::<std::result::Result<Vec<String>, _>>().map_err(e)?;
            out.push(ProjectRow {
                name: project_name(&path),
                status: prop("status").and_then(|s| parse_status(&s)).unwrap_or("active").to_string(),
                area: prop("area"),
                outcome: prop("outcome"),
                next: open.len() - waiting - someday,
                waiting,
                someday,
                done: tasks.iter().filter(|t| t.done && t.status != "-").count(),
                last_touched: tasks.iter().map(|t| t.mtime).fold(mtime, i64::max),
                links,
                path,
            });
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.path.cmp(&b.path)));
        Ok(out)
    }
}

/// A project name that makes a good file name.
fn valid_name(name: &str) -> std::result::Result<String, WriteError> {
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() || n.starts_with('.') || n.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '[', ']', '#', '^']) {
        return Err(WriteError::Invalid(format!("“{name}” can't be a project name: leave out / \\ : * ? \" < > | [ ] # ^")));
    }
    Ok(n)
}

/// The new note's text: its properties, then the three headings.
pub fn project_note(status: &str, area: Option<&str>, outcome: Option<&str>) -> String {
    let mut s = format!("---\nstatus: {status}\n");
    for (k, v) in [("area", area), ("outcome", outcome)] {
        if let Some(v) = v.map(str::trim).filter(|v| !v.is_empty()) {
            s.push_str(&format!("{k}: {}\n", write::yaml_scalar(v)));
        }
    }
    s.push_str(&format!("---\n\n## {NEXT_ACTIONS}\n\n## {WAITING_FOR}\n\n## {NOTES}\n"));
    s
}

/// Creates `Project. <name>.md` at the top of the vault, refusing when it's there already.
/// Returns its vault path and the change (to undo, by deleting it).
pub fn create_project(
    root: &Path,
    name: &str,
    status: &str,
    area: Option<&str>,
    outcome: Option<&str>,
) -> std::result::Result<(String, FileChange), WriteError> {
    let name = valid_name(name)?;
    let status = parse_status(status).ok_or_else(|| WriteError::Invalid(format!("Not a project status: {status}")))?;
    if area.into_iter().chain(outcome).any(|v| v.contains(['\n', '\r'])) {
        return Err(WriteError::Invalid("Area and outcome are one line each.".into()));
    }
    let rel = format!("{PREFIX}{name}.md");
    let path = root.join(&rel);
    let version = write::create_file(&path, &project_note(status, area, outcome))?;
    Ok((rel, FileChange { path, before: None, version }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault")
    }

    #[test]
    fn names_and_statuses() {
        assert!(is_project_path("Project. Orbit App launch.md"));
        assert!(is_project_path("Work/Project. X.md"));
        assert!(!is_project_path("Project. .md") && !is_project_path("Project. X/Note.md") && !is_project_path("Projects.md"));
        assert_eq!(project_name("Work/Project. Orbit App launch.md"), "Orbit App launch");
        for (s, want) in [("Active", "active"), ("on hold", "on-hold"), ("hold", "on-hold"), ("Completed", "done"), ("someday", "someday")]
        {
            assert_eq!(parse_status(s), Some(want), "{s}");
        }
        assert_eq!(parse_status("blocked"), None);
    }

    #[test]
    fn reads_the_fixture_projects() {
        let v = Vault::new(fixture(), vec![]);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&v).unwrap();
        let ps = ix.projects().unwrap();
        let names: Vec<&str> = ps.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Garden", "Hub migration", "Orbit App launch"]);
        let b = &ps[2];
        assert_eq!((b.status.as_str(), b.area.as_deref()), ("active", Some("Work")));
        assert_eq!(b.outcome.as_deref(), Some("Orbit App is live for all staff, with comms sent"));
        // Two next actions in the note and two linked from elsewhere (by alias, and the first of two
        // project links); one waiting for; one done, and the cancelled one not counted.
        assert_eq!((b.next, b.waiting, b.someday, b.done), (4, 1, 0, 1));
        assert_eq!(b.links, ["Idea. Launch prep.md"]);
        let prep = std::fs::metadata(fixture().join("Idea. Launch prep.md")).unwrap();
        assert!(b.last_touched >= crate::vault::mtime_ms(&prep));
        // An unknown status reads as active; "on hold" as on-hold.
        assert_eq!((ps[0].status.as_str(), ps[0].area.as_deref(), ps[0].next), ("active", Some("Personal"), 1));
        let g = &ps[1];
        assert_eq!((g.status.as_str(), g.next, g.waiting, g.outcome.as_deref()), ("on-hold", 0, 1, None));
        assert_eq!(g.links, ["Idea. Launch prep.md"]);
        // The task's project, contexts and effort.
        let tasks = ix.all_tasks().unwrap();
        let call = tasks.iter().find(|t| t.text.starts_with("Call Sam about the launch date")).unwrap();
        assert_eq!(call.project.as_deref(), Some("Project. Orbit App launch.md"));
        assert_eq!((call.contexts.clone(), call.effort.as_deref(), call.effort_min), (vec!["calls".to_string()], Some("15m"), Some(15)));
        let linked = tasks.iter().find(|t| t.text.starts_with("Book the launch room")).unwrap();
        assert_eq!(linked.project.as_deref(), Some("Project. Orbit App launch.md"));
        assert!(tasks.iter().filter(|t| t.path == "Me. To Do List.md").all(|t| t.project.is_none() && t.project_status.is_none()));
        // And that project's status, for Next actions to leave a parked project's tasks off.
        assert_eq!(call.project_status.as_deref(), Some("active"));
        let hub: Vec<_> = tasks.iter().filter(|t| t.project.as_deref() == Some("Project. Hub migration.md")).collect();
        assert!(!hub.is_empty() && hub.iter().all(|t| t.project_status.as_deref() == Some("on-hold")));
    }

    #[test]
    fn creates_a_project_note() {
        let d = tempfile::tempdir().unwrap();
        let (rel, ch) = create_project(d.path(), "  New  thing ", "on hold", Some("Personal"), Some("Done: it works")).unwrap();
        assert_eq!(rel, "Project. New thing.md");
        assert_eq!(ch.before, None);
        let text = std::fs::read_to_string(d.path().join(&rel)).unwrap();
        assert_eq!(
            text,
            "---\nstatus: on-hold\narea: Personal\noutcome: \"Done: it works\"\n---\n\n## Next actions\n\n## Waiting for\n\n## Notes\n"
        );
        assert_eq!(ch.version, write::version(text.as_bytes()));
        assert!(matches!(create_project(d.path(), "New thing", "active", None, None), Err(WriteError::Exists(_))));
        assert!(matches!(create_project(d.path(), "a/b", "active", None, None), Err(WriteError::Invalid(_))));
        assert!(matches!(create_project(d.path(), "x", "blocked", None, None), Err(WriteError::Invalid(_))));
        let (rel, _) = create_project(d.path(), "Bare", "active", None, Some(" ")).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.path().join(rel)).unwrap(),
            "---\nstatus: active\n---\n\n## Next actions\n\n## Waiting for\n\n## Notes\n"
        );
    }
}
