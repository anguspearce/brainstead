// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Projects: a `Project. <name>.md` note whose properties hold `status` (active, on-hold, someday,
//! done), `area` and `outcome`. Its tasks are those written in it and any task elsewhere whose line
//! links it (see `taskquery`). A missing or unknown status reads as active. `folders` lists the
//! folders on the computer the project's work happens in (see `for_folder`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// Which of a project's lists an open task counts in: a `#waiting-for` task is waiting, else a
/// `#someday-maybe` one someday, else it's a next action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskList {
    Next,
    Waiting,
    Someday,
}

/// The list an open task with these tags (without `#`) counts in.
pub fn task_list(tags: &[String]) -> TaskList {
    let has = |tag: &str| tags.iter().any(|g| g == tag || g.starts_with(&format!("{tag}/")));
    if has("waiting-for") {
        TaskList::Waiting
    } else if has("someday-maybe") {
        TaskList::Someday
    } else {
        TaskList::Next
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
    /// The folders its work happens in, as written (`~/repos/orbit-app`, or a bare folder name).
    pub folders: Vec<String>,
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
            let open: Vec<_> = tasks.iter().filter(|t| !t.done).collect();
            let waiting = open.iter().filter(|t| task_list(&t.tags) == TaskList::Waiting).count();
            let someday = open.iter().filter(|t| task_list(&t.tags) == TaskList::Someday).count();
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
                folders: folders_of(&fm),
                path,
            });
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.path.cmp(&b.path)));
        Ok(out)
    }
}

/// The frontmatter key for a project's folders.
pub const FOLDERS: &str = "folders";

/// A project's folders as written: a list, or one string (`folders: ~/repos/orbit-app`).
pub fn folders_of(fm: &serde_json::Value) -> Vec<String> {
    match fm.get(FOLDERS) {
        Some(serde_json::Value::String(s)) => Some(s.trim().to_string()).filter(|s| !s.is_empty()).into_iter().collect(),
        _ => crate::frontmatter::list(fm, &[FOLDERS], false),
    }
}

/// A name compared letters and digits only, ignoring case: `orbit-app`, `Orbit_App` and
/// `Orbit App` are one name. The weekly summaries match a session's folder to a project by it too.
pub fn name_key(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The last part of a folder path, either separator, ignoring a trailing one.
pub fn folder_basename(folder: &str) -> &str {
    folder.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("")
}

/// A project a folder belongs to: `Project. Orbit App.md` and `Orbit App`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLink {
    pub path: String,
    pub name: String,
}

/// What `for_folder` needs of a project note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderClaim {
    pub path: String,
    pub done: bool,
    pub folders: Vec<String>,
}

/// Every project note in the vault with its status and folders, read from the files (no index):
/// the MCP server and the terminal hook use it with the app closed.
pub fn folder_claims(vault_root: &Path) -> Vec<FolderClaim> {
    crate::vault::Vault::new(vault_root, vec![])
        .walk()
        .into_iter()
        .filter(|f| f.layer == crate::vault::Layer::Note && is_project_path(&f.rel))
        .map(|f| {
            let fm = std::fs::read_to_string(&f.abs).map(|t| crate::frontmatter::split(&t).data).unwrap_or_default();
            let done = crate::frontmatter::string(&fm, "status").and_then(|s| parse_status(&s)) == Some("done");
            FolderClaim { folders: folders_of(&fm), done, path: f.rel }
        })
        .collect()
}

impl Index {
    /// `folder_claims` from the index: the project notes' status and folders, without reading files.
    pub fn folder_claims(&self) -> Result<Vec<FolderClaim>> {
        let mut st = self.conn().prepare(&format!("SELECT f.path, f.frontmatter FROM files f WHERE {}", is_project_sql("f"))).map_err(e)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))).map_err(e)?;
        let mut out = Vec::new();
        for row in rows {
            let (path, fm) = row.map_err(e)?;
            let fm: serde_json::Value = fm.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
            let done = crate::frontmatter::string(&fm, "status").and_then(|s| parse_status(&s)) == Some("done");
            out.push(FolderClaim { folders: folders_of(&fm), done, path });
        }
        Ok(out)
    }
}

/// The project a folder on the computer belongs to: first one whose `folders` names the folder or
/// a folder it's in (the deepest wins), as a path (absolute or `~/…`) or a bare folder name; else
/// one named like the folder (`orbit-app` for Orbit App). Done projects only when nothing else matches.
pub fn for_folder(vault_root: &Path, folder: &Path) -> Option<ProjectLink> {
    pick_project(&folder_claims(vault_root), folder, dirs::home_dir().as_deref())
}

/// `for_folder` over claims already read; `home` expands `~/`.
pub fn pick_project(claims: &[FolderClaim], folder: &Path, home: Option<&Path>) -> Option<ProjectLink> {
    let folder = clean_path(&crate::paths::real_path(folder));
    let link = |c: &FolderClaim| ProjectLink { path: c.path.clone(), name: project_name(&c.path) };
    for done in [false, true] {
        let mut best: Option<(usize, &FolderClaim)> = None;
        for c in claims.iter().filter(|c| c.done == done) {
            for f in &c.folders {
                if let Some(depth) = claim_depth(f, &folder, home) {
                    if best.is_none_or(|(d, b)| depth > d || (depth == d && c.path < b.path)) {
                        best = Some((depth, c));
                    }
                }
            }
        }
        if let Some((_, c)) = best {
            return Some(link(c));
        }
        let key = name_key(&folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        if !key.is_empty() {
            let mut named: Vec<&FolderClaim> =
                claims.iter().filter(|c| c.done == done && name_key(&project_name(&c.path)) == key).collect();
            named.sort_by(|a, b| a.path.cmp(&b.path));
            if let Some(c) = named.first() {
                return Some(link(c));
            }
        }
    }
    None
}

/// How deep the folder a `folders` entry names is, when it's `folder` or a folder `folder` is in.
fn claim_depth(entry: &str, folder: &Path, home: Option<&Path>) -> Option<usize> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    let tilde = entry == "~" || entry.starts_with("~/") || entry.starts_with("~\\");
    if tilde || crate::paths::is_rooted(entry) {
        let p = match entry.strip_prefix('~') {
            Some(rest) if tilde => home?.join(rest.trim_start_matches(['/', '\\'])),
            _ => PathBuf::from(entry),
        };
        let p = clean_path(&crate::paths::real_path(&p));
        return folder.starts_with(&p).then(|| p.components().count());
    }
    if entry.contains(['/', '\\']) {
        return None;
    }
    let key = name_key(entry);
    let mut at: Option<&Path> = Some(folder);
    while let Some(p) = at {
        if p.file_name().is_some_and(|n| !key.is_empty() && name_key(&n.to_string_lossy()) == key) {
            return Some(p.components().count());
        }
        at = p.parent();
    }
    None
}

/// A path with `.` and `..` worked out and no trailing separator, for comparing.
fn clean_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
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
    project_note_in(status, area, outcome, &[])
}

/// As `project_note`, with the folders its work happens in (the Workspace's link).
pub fn project_note_in(status: &str, area: Option<&str>, outcome: Option<&str>, folders: &[String]) -> String {
    let mut s = format!("---\nstatus: {status}\n");
    for (k, v) in [("area", area), ("outcome", outcome)] {
        if let Some(v) = v.map(str::trim).filter(|v| !v.is_empty()) {
            s.push_str(&format!("{k}: {}\n", write::yaml_scalar(v)));
        }
    }
    let folders: Vec<&str> = folders.iter().map(|f| f.trim()).filter(|f| !f.is_empty()).collect();
    if !folders.is_empty() {
        s.push_str("folders:\n");
        for f in folders {
            s.push_str(&format!("  - {}\n", write::yaml_scalar(f)));
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
        assert!(tasks.iter().filter(|t| t.path == "Me. To Do List.md").all(|t| t.project.is_none()));
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

    #[test]
    fn reads_folders_as_a_list_or_one_string() {
        let fm = |y: &str| crate::frontmatter::split(&format!("---\n{y}\n---\n")).data;
        assert_eq!(folders_of(&fm("folders:\n  - ~/repos/orbit-app\n  - orbit-docs")), ["~/repos/orbit-app", "orbit-docs"]);
        assert_eq!(folders_of(&fm("folders: ~/repos/orbit app")), ["~/repos/orbit app"]);
        assert!(folders_of(&fm("status: active")).is_empty());
    }

    #[test]
    fn finds_the_project_a_folder_belongs_to() {
        let d = tempfile::tempdir().unwrap();
        let (vault, home) = (d.path().join("vault"), d.path().join("home"));
        let repos = home.join("repos");
        for p in ["orbit-app/src", "orbit-app/docs/site", "acme_tools", "garden", "old-thing", "elsewhere"] {
            std::fs::create_dir_all(repos.join(p)).unwrap();
        }
        std::fs::create_dir_all(vault.join("Work")).unwrap();
        std::fs::create_dir_all(vault.join("Templates")).unwrap();
        let note = |rel: &str, fm: &str| std::fs::write(vault.join(rel), format!("---\n{fm}\n---\n\n## Notes\n")).unwrap();
        note("Work/Project. Orbit App.md", "status: active\nfolders:\n  - ~/repos/orbit-app");
        note("Project. Orbit docs.md", &format!("folders: {}", repos.join("orbit-app/docs").display()));
        note("Project. Tools.md", "folders: [Acme Tools]");
        note("Project. Garden.md", "status: on hold");
        note("Project. Old thing.md", "status: done");
        note("Project. Finished.md", "status: done\nfolders: elsewhere");
        note("Project. Elsewhere.md", "status: someday");
        note("Templates/Project. Template.md", "folders: [garden]");
        let claims = folder_claims(&vault);
        assert_eq!(claims.len(), 7, "{claims:?}");
        let at = |p: &str| pick_project(&claims, &repos.join(p), Some(&home)).map(|l| l.name);
        // `~/…` matches the folder and what's in it; a deeper path wins.
        assert_eq!(at("orbit-app").as_deref(), Some("Orbit App"));
        assert_eq!(at("orbit-app/src").as_deref(), Some("Orbit App"));
        assert_eq!(at("orbit-app/docs/site").as_deref(), Some("Orbit docs"));
        // A bare name, ignoring case and `-`/`_`/spaces.
        assert_eq!(at("acme_tools").as_deref(), Some("Tools"));
        // Named like the folder; a done project when nothing else matches, but not over an open one.
        assert_eq!(at("garden").as_deref(), Some("Garden"));
        assert_eq!(at("old-thing").as_deref(), Some("Old thing"));
        assert_eq!(at("elsewhere").as_deref(), Some("Elsewhere"));
        assert_eq!(at(".."), None);
        let l = pick_project(&claims, &repos.join("orbit-app/../orbit-app"), Some(&home)).unwrap();
        assert_eq!(l, ProjectLink { path: "Work/Project. Orbit App.md".into(), name: "Orbit App".into() });
        assert_eq!(serde_json::to_value(&l).unwrap(), serde_json::json!({"path": "Work/Project. Orbit App.md", "name": "Orbit App"}));
        // The index holds the same.
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&Vault::new(vault.clone(), vec![])).unwrap();
        let mut from_index = ix.folder_claims().unwrap();
        from_index.sort_by(|a, b| a.path.cmp(&b.path));
        let mut from_files = claims.clone();
        from_files.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(from_index, from_files);
        // From the files alone.
        assert_eq!(for_folder(&vault, &repos.join("acme_tools")).map(|l| l.name).as_deref(), Some("Tools"));
        assert_eq!(for_folder(&vault, &d.path().join("nowhere")), None);
    }
}
