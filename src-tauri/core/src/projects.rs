// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Projects: a `Project. <name>.md` note whose properties hold `status` (active, on-hold, someday,
//! done), `area` and `outcome`. Its tasks are those written in it, those in its member notes, and
//! any task elsewhere whose line links it (see `taskquery`). A missing or unknown status reads as
//! active.
//!
//! A member note (a Tracker, Plan, Findings… note) says which project it belongs to with a
//! `Project: [[Project. <name>]]` line near its top (`project_line`). Its tasks count for that
//! project unless a task's own line links another one.

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

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

/// How many body lines (after the frontmatter) a `Project:` line may be down: further down it's
/// prose that mentions a project, not the note saying where it belongs.
pub const PROJECT_LINE_WITHIN: usize = 15;

static PROJECT_LINE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)^\s*project:\s*\\?\[\\?\[").unwrap());

/// The 0-based file line of a note's `Project: [[…]]` line: the first line among its first
/// `PROJECT_LINE_WITHIN` body lines (outside code and HTML comments) that starts, after any indent,
/// with `Project:` and a wikilink (`Project::`, a Dataview field, and `**Project:**` aren't one).
/// Which project it names is the first link on that line that resolves to a project note
/// (`member_project_sql`), so an alias (`[[Project. X|X]]`) or a renamed note works as any link
/// does.
pub fn project_line(body: &[crate::markdown::Line]) -> Option<usize> {
    let mut comment = false;
    for l in body.iter().take(PROJECT_LINE_WITHIN) {
        let text = l.text.trim_start_matches('\u{feff}');
        let skip = l.code || comment || text.contains("<!--");
        if !l.code {
            let mut rest = text;
            loop {
                let (mark, open) = if comment { ("-->", false) } else { ("<!--", true) };
                let Some(i) = rest.find(mark) else { break };
                comment = open;
                rest = &rest[i + mark.len()..];
            }
        }
        if !skip && PROJECT_LINE.is_match(text) {
            return Some(l.no);
        }
    }
    None
}

/// In SQL, the id of the project note the `files` row aliased `f` belongs to by its `Project:`
/// line, or NULL. Only notes belong to a project, not wiki pages, sources or templates. Correlated
/// on `f`; a note with no such line costs nothing.
pub(crate) fn member_project_sql(f: &str) -> String {
    let p = is_project_sql("mp");
    format!(
        "(CASE WHEN {f}.project_line IS NULL OR {f}.layer != 'note' THEN NULL ELSE (SELECT ml.target_id FROM links ml JOIN files mp ON mp.id = ml.target_id
           WHERE ml.src = {f}.id AND ml.line = {f}.project_line AND ml.kind = 'link' AND {p} ORDER BY ml.rowid LIMIT 1) END)"
    )
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
    /// Its member notes, those whose `Project:` line names it, by path.
    pub documents: Vec<ProjectDoc>,
    /// The text in its properties: each text value, and each text item of a list. The window finds
    /// reference keys in it (Settings › Notes › Reference links), so the patterns stay out of here.
    pub property_text: Vec<String>,
}

/// A note that belongs to a project by its `Project: [[…]]` line.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDoc {
    pub path: String,
    pub title: String,
    /// The type in its file name (Tracker, Plan, Findings…).
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Its `summary` property, when it has one.
    pub summary: Option<String>,
    pub mtime: i64,
    /// A tracker's Current state text, when it has one.
    pub current_state: Option<String>,
    /// The text in its properties, as `ProjectRow::property_text`.
    pub property_text: Vec<String>,
}

/// The text a note's properties hold: each string value and each string item of a list, in order,
/// except those of `tags` and `aliases`, which are labels and never references.
fn property_text(fm: &serde_json::Value) -> Vec<String> {
    use serde_json::Value;
    let mut out = Vec::new();
    let labels = ["tags", "tag", "aliases", "alias"];
    for (_, v) in fm.as_object().into_iter().flat_map(|o| o.iter()).filter(|(k, _)| !labels.iter().any(|l| k.eq_ignore_ascii_case(l))) {
        match v {
            Value::String(s) if !s.trim().is_empty() => out.push(s.trim().to_string()),
            Value::Array(items) => {
                out.extend(items.iter().filter_map(|i| i.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(String::from))
            }
            _ => {}
        }
    }
    out
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
        // The notes' tasks only, as the Tasks screen counts them: a task quoted in a wiki page or a
        // source isn't the project's to do.
        for t in self.note_tasks()? {
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
        let mut documents = self.documents()?;
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
                documents: documents.remove(&id).unwrap_or_default(),
                property_text: property_text(&fm),
                path,
            });
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.path.cmp(&b.path)));
        Ok(out)
    }
}

impl Index {
    /// Every member note, by the id of the project note it belongs to, by path.
    fn documents(&self) -> Result<HashMap<i64, Vec<ProjectDoc>>> {
        let sql = format!(
            "SELECT pid, path, title, type, frontmatter, mtime, current_state FROM
               (SELECT {} AS pid, f.path, f.title, f.type, f.frontmatter, f.mtime, f.current_state FROM files f
                WHERE f.project_line IS NOT NULL AND NOT {})
             WHERE pid IS NOT NULL ORDER BY path",
            member_project_sql("f"),
            is_project_sql("f")
        );
        let mut st = self.conn().prepare(&sql).map_err(e)?;
        let rows = st
            .query_map([], |r| {
                let fm: Option<String> = r.get(4)?;
                let fm: serde_json::Value = fm.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
                Ok((
                    r.get::<_, i64>(0)?,
                    ProjectDoc {
                        path: r.get(1)?,
                        title: r.get(2)?,
                        kind: r.get(3)?,
                        summary: crate::frontmatter::string(&fm, "summary"),
                        mtime: r.get(5)?,
                        current_state: r.get(6)?,
                        property_text: property_text(&fm),
                    },
                ))
            })
            .map_err(e)?;
        let mut out: HashMap<i64, Vec<ProjectDoc>> = HashMap::new();
        for r in rows {
            let (pid, d) = r.map_err(e)?;
            out.entry(pid).or_default().push(d);
        }
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

    fn body_lines(src: &str) -> Vec<crate::markdown::Line<'_>> {
        crate::markdown::lines(src, crate::frontmatter::split(src).body_start)
    }

    #[test]
    fn reads_the_project_line() {
        let at = |src: &str| project_line(&body_lines(src));
        assert_eq!(at("---\ntags: [x]\n---\n\nProject: [[Project. Orbit]]\n"), Some(4));
        // An alias, an indent, any case.
        assert_eq!(at("Project: [[Project. Orbit|Orbit]]\n"), Some(0));
        assert_eq!(at("\n   Project:   [[Project. Orbit]]\n"), Some(1));
        assert_eq!(at("project: [[Project. Orbit]]\n"), Some(0));
        // Only the first counts; a line without a link isn't one.
        assert_eq!(at("Project: none yet\nProject: [[Project. A]]\nProject: [[Project. B]]\n"), Some(1));
        // Not in a code block, and not past the first lines of the body.
        assert_eq!(at("```\nProject: [[Project. A]]\n```\n"), None);
        let far = format!("---\na: 1\n---\n{}Project: [[Project. A]]\n", "text\n".repeat(PROJECT_LINE_WITHIN));
        assert_eq!(at(&far), None);
        let near = format!("---\na: 1\n---\n{}Project: [[Project. A]]\n", "text\n".repeat(PROJECT_LINE_WITHIN - 1));
        assert_eq!(at(&near), Some(3 + PROJECT_LINE_WITHIN - 1));
        // Mentioned in a sentence, it isn't the note's project.
        assert_eq!(at("We moved this to Project: [[Project. A]]\n"), None);
    }

    /// A vault with two projects and member notes of each kind the rule has to tell apart.
    fn member_vault() -> (tempfile::TempDir, Index) {
        let d = tempfile::tempdir().unwrap();
        let w = |name: &str, text: &str| std::fs::write(d.path().join(name), text).unwrap();
        w("Project. Orbit.md", "---\nstatus: active\ntickets: ORBIT-9\n---\n\n## Next actions\n\n- [ ] In the project note\n");
        w("Project. Hub.md", "---\nstatus: on hold\n---\n\n## Next actions\n");
        w(
            "Tracker. Orbit rollout.md",
            "---\nsummary: Where the rollout stands.\ntickets: [ORBIT-12, ORBIT-14]\nstage: 3\ntags: [ORBIT-99]\naliases: [ORBIT-98]\n---\n\nProject: [[Project. Orbit|Orbit]]\n\n## Next\n\n- [ ] Inherited from the tracker\n- [ ] Belongs to Hub instead [[Project. Hub]]\n- [x] Done in the tracker\n",
        );
        w("Plan. Orbit comms.md", "  Project: [[Project. Orbit]]\n\n- [ ] Write the comms\n");
        w("Findings. Orbit outage.md", "Project: [[Project. Orbit]]\n\nWhat we found.\n");
        w("Plan. Far down.md", &format!("{}Project: [[Project. Orbit]]\n\n- [ ] Not inherited\n", "line\n".repeat(PROJECT_LINE_WITHIN)));
        w("Ticket. Ghost.md", "Project: [[Project. Nowhere]]\n\n- [ ] No project\n");
        w("Idea. Mentions it.md", "Some idea about [[Project. Orbit]].\n\n- [ ] Not inherited either\n");
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&Vault::new(d.path().to_path_buf(), vec![])).unwrap();
        (d, ix)
    }

    #[test]
    fn member_notes_lend_their_tasks_their_project() {
        let (_d, ix) = member_vault();
        let tasks = ix.all_tasks().unwrap();
        let project = |text: &str| tasks.iter().find(|t| t.text.starts_with(text)).unwrap().project.clone();
        for (text, want) in [
            ("In the project note", Some("Project. Orbit.md")),
            ("Inherited from the tracker", Some("Project. Orbit.md")),
            ("Belongs to Hub instead", Some("Project. Hub.md")),
            ("Write the comms", Some("Project. Orbit.md")),
            ("Not inherited", None),
            ("No project", None),
            ("Not inherited either", None),
        ] {
            assert_eq!(project(text).as_deref(), want, "{text}");
        }
        let hub = tasks.iter().find(|t| t.text.starts_with("Belongs to Hub")).unwrap();
        assert_eq!(hub.project_status.as_deref(), Some("on-hold"));
        // The Tasks screen's query agrees with the whole list.
        let q = ix.tasks(&Default::default()).unwrap();
        assert!(q.iter().all(|t| tasks.iter().any(|a| a.path == t.path && a.line == t.line && a.project == t.project)));

        // The Projects screen counts them, and lists the member notes.
        let ps = ix.projects().unwrap();
        let orbit = ps.iter().find(|p| p.name == "Orbit").unwrap();
        assert_eq!((orbit.next, orbit.done), (3, 1));
        let docs: Vec<(&str, Option<&str>)> = orbit.documents.iter().map(|d| (d.path.as_str(), d.kind.as_deref())).collect();
        assert_eq!(
            docs,
            [
                ("Findings. Orbit outage.md", Some("Findings")),
                ("Plan. Orbit comms.md", Some("Plan")),
                ("Tracker. Orbit rollout.md", Some("Tracker"))
            ]
        );
        let tracker = orbit.documents.iter().find(|d| d.kind.as_deref() == Some("Tracker")).unwrap();
        assert_eq!(tracker.summary.as_deref(), Some("Where the rollout stands."));
        let hub = ps.iter().find(|p| p.name == "Hub").unwrap();
        assert_eq!(hub.next, 1);
        assert!(hub.documents.is_empty());
    }

    #[test]
    fn projects_and_member_notes_carry_their_property_text() {
        let (_d, ix) = member_vault();
        let ps = ix.projects().unwrap();
        let orbit = ps.iter().find(|p| p.name == "Orbit").unwrap();
        let sorted = |v: &[String]| {
            let mut v = v.to_vec();
            v.sort();
            v
        };
        assert_eq!(sorted(&orbit.property_text), ["ORBIT-9", "active"]);
        let tracker = orbit.documents.iter().find(|d| d.kind.as_deref() == Some("Tracker")).unwrap();
        // Text values and the items of a list, not a number.
        assert_eq!(sorted(&tracker.property_text), ["ORBIT-12", "ORBIT-14", "Where the rollout stands."]);
        let plan = orbit.documents.iter().find(|d| d.kind.as_deref() == Some("Plan")).unwrap();
        assert!(plan.property_text.is_empty());
    }

    #[test]
    fn file_summaries_carry_their_project() {
        let (_d, ix) = member_vault();
        let notes = ix.list(Some("note")).unwrap();
        let project = |path: &str| notes.iter().find(|n| n.path == path).unwrap().project.clone();
        for (path, want) in [
            ("Project. Orbit.md", Some("Project. Orbit.md")),
            ("Tracker. Orbit rollout.md", Some("Project. Orbit.md")),
            ("Plan. Orbit comms.md", Some("Project. Orbit.md")),
            ("Findings. Orbit outage.md", Some("Project. Orbit.md")),
            ("Plan. Far down.md", None),
            ("Ticket. Ghost.md", None),
            ("Idea. Mentions it.md", None),
        ] {
            assert_eq!(project(path).as_deref(), want, "{path}");
        }
        assert_eq!(ix.summary("Plan. Orbit comms.md").unwrap().unwrap().project.as_deref(), Some("Project. Orbit.md"));
    }

    #[test]
    fn a_vault_without_project_lines_is_unchanged() {
        let v = Vault::new(fixture(), vec![]);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&v).unwrap();
        assert!(ix.projects().unwrap().iter().all(|p| p.documents.is_empty()));
        assert!(ix.list(Some("note")).unwrap().iter().all(|n| n.project.is_none() || is_project_path(&n.path)));
    }

    #[test]
    fn reads_the_project_line_past_a_bom_and_not_in_comments() {
        let at = |src: &str| project_line(&body_lines(src));
        assert_eq!(at("\u{feff}Project: [[Project. Orbit]]\n"), Some(0));
        // Inside an HTML comment, on its opening line or within it, it isn't one.
        assert_eq!(at("<!--\nProject: [[Project. A]]\n-->\nProject: [[Project. B]]\n"), Some(3));
        assert_eq!(at("<!-- Project: [[Project. A]] -->\n"), None);
        assert_eq!(at("<!-- note -->\nProject: [[Project. B]]\n"), Some(1));
        // A Dataview field and a bold label aren't the line.
        assert_eq!(at("Project:: [[Project. A]]\n"), None);
        assert_eq!(at("**Project:** [[Project. A]]\n"), None);
    }

    #[test]
    fn membership_is_for_notes_and_a_missing_project_link_is_the_tasks_own() {
        let (d, mut ix) = member_vault();
        std::fs::create_dir_all(d.path().join("sources")).unwrap();
        std::fs::create_dir_all(d.path().join("wiki/concepts")).unwrap();
        std::fs::write(d.path().join("sources/Capture. Orbit.md"), "Project: [[Project. Orbit]]\n\n- [ ] In a source\n").unwrap();
        std::fs::write(d.path().join("wiki/concepts/Orbit.md"), "Project: [[Project. Orbit]]\n\n- [ ] In the wiki\n").unwrap();
        std::fs::write(
            d.path().join("Tracker. Orbit rollout.md"),
            "Project: [[Project. Orbit]]\n\n## Current state\n\nStaged.\n\n## Next\n\n- [ ] Inherited\n- [ ] Links a missing one [[Project. Gone]]\n- [ ] Links Orbit too [[Project. Orbit]]\n- [ ] Links Hub [[Project. Hub]]\n",
        )
        .unwrap();
        ix.sync(&Vault::new(d.path().to_path_buf(), vec![])).unwrap();
        let tasks = ix.all_tasks().unwrap();
        let of = |text: &str| {
            let t = tasks.iter().find(|t| t.text.starts_with(text)).unwrap();
            (t.project.clone(), t.project_from.clone())
        };
        let orbit = Some("Project. Orbit.md".to_string());
        assert_eq!(of("In the project note"), (orbit.clone(), Some("note".into())));
        assert_eq!(of("Inherited"), (orbit.clone(), Some("member".into())));
        // Linking its note's own project changes nothing when unlinked: still inherited.
        assert_eq!(of("Links Orbit too"), (orbit.clone(), Some("member".into())));
        assert_eq!(of("Links Hub"), (Some("Project. Hub.md".into()), Some("line".into())));
        assert_eq!(of("Links a missing one"), (None, None));
        assert_eq!(of("In a source"), (None, None));
        assert_eq!(of("In the wiki"), (None, None));
        let ps = ix.projects().unwrap();
        let o = ps.iter().find(|p| p.name == "Orbit").unwrap();
        assert!(o.documents.iter().all(|x| !x.path.starts_with("sources/") && !x.path.starts_with("wiki/")));
        // The tracker's Current state comes with it, for the project page.
        let t = o.documents.iter().find(|x| x.kind.as_deref() == Some("Tracker")).unwrap();
        assert_eq!(t.current_state.as_deref(), Some("Staged."));
        assert!(o.documents.iter().filter(|x| x.kind.as_deref() != Some("Tracker")).all(|x| x.current_state.is_none()));
        let all = ix.list(None).unwrap();
        assert!(all.iter().filter(|f| f.layer != "note").all(|f| f.project.is_none()));
    }
}
