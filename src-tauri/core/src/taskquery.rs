// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The task queries behind `tasks` and `dataview` blocks, as the previous app's `runTaskQuery` and
//! `compareTasks` (bff/src/services/tasks-index.ts) answer them. The blocks themselves are parsed
//! in the window (src/md/taskQuery.ts). Tasks come from notes only, as there.

use serde::{Deserialize, Serialize};

use crate::index::{Index, Result};
use crate::projects::is_project_sql;

fn e<E: std::fmt::Display>(x: E) -> String {
    x.to_string()
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Done,
    NotDone,
    #[default]
    Any,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Period {
    ThisWeek,
    LastWeek,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Sort {
    DoneAsc,
    DoneDesc,
    DueAsc,
    DueDesc,
    CreatedAsc,
    CreatedDesc,
    Manual,
    #[default]
    Unsorted,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskQuery {
    pub status: Status,
    pub period: Option<Period>,
    /// Every one must match (`#a` matches `#a` and `#a/b`).
    pub tags_include: Vec<String>,
    /// Any one drops the task.
    pub tags_exclude: Vec<String>,
    pub sort: Sort,
    pub limit: Option<usize>,
    /// Today's local date, `YYYY-MM-DD`, for "this week": the window knows the user's time zone.
    pub today: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRow {
    pub path: String,
    pub title: String,
    pub line: i64,
    /// The whole line as in the file, to find it again before a write.
    pub line_text: String,
    pub text: String,
    /// The character between the brackets: ` `, `x`, `/`, `-`…
    pub status: String,
    /// Done or cancelled.
    pub done: bool,
    pub due: Option<String>,
    pub scheduled: Option<String>,
    pub start: Option<String>,
    pub done_on: Option<String>,
    pub created: Option<String>,
    pub cancelled: Option<String>,
    pub rank: Option<i64>,
    pub tags: Vec<String>,
    pub heading: Option<String>,
    /// The project note it belongs to: the one it's written in, else the first link on its line
    /// to one.
    pub project: Option<String>,
    /// That project's status (active, on-hold, someday, done), when it has one.
    pub project_status: Option<String>,
    /// From `#context/<name>` tags, without the prefix.
    pub contexts: Vec<String>,
    /// As written in `[effort:: …]`.
    pub effort: Option<String>,
    pub effort_min: Option<i64>,
    #[serde(skip)]
    pub(crate) mtime: i64,
}

fn rows_sql() -> String {
    let own = is_project_sql("f");
    let linked = is_project_sql("pf");
    format!(
        "SELECT f.path, f.title, t.line, t.text, t.done, t.due, t.scheduled, t.done_on, t.created, t.rank, t.tags, t.heading, f.mtime, t.line_text, t.start, t.status, t.cancelled,
           CASE WHEN {own} THEN f.path ELSE (SELECT pf.path FROM links l JOIN files pf ON pf.id = l.target_id
             WHERE l.src = f.id AND l.line = t.line AND l.kind = 'link' AND {linked} ORDER BY l.rowid LIMIT 1) END,
           t.contexts, t.effort, t.effort_min,
           CASE WHEN {own} THEN f.frontmatter ELSE (SELECT pf.frontmatter FROM links l JOIN files pf ON pf.id = l.target_id
             WHERE l.src = f.id AND l.line = t.line AND l.kind = 'link' AND {linked} ORDER BY l.rowid LIMIT 1) END
         FROM tasks t JOIN files f ON f.id = t.file_id"
    )
}

fn words(s: Option<String>) -> Vec<String> {
    s.map(|s| s.split(' ').filter(|t| !t.is_empty()).map(String::from).collect()).unwrap_or_default()
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<TaskRow> {
    Ok(TaskRow {
        path: r.get(0)?,
        title: r.get(1)?,
        line: r.get(2)?,
        text: r.get(3)?,
        done: r.get(4)?,
        due: r.get(5)?,
        scheduled: r.get(6)?,
        start: r.get(14)?,
        done_on: r.get(7)?,
        created: r.get(8)?,
        rank: r.get(9)?,
        tags: words(r.get(10)?),
        heading: r.get(11)?,
        mtime: r.get(12)?,
        line_text: r.get(13)?,
        status: r.get(15)?,
        cancelled: r.get(16)?,
        project: r.get(17)?,
        contexts: words(r.get(18)?),
        effort: r.get(19)?,
        effort_min: r.get(20)?,
        project_status: project_status(r.get(17)?, r.get(21)?),
    })
}

/// A task's project's status from the project note's properties: a missing or unknown one reads as
/// active, as on the Projects screen (`projects::parse_status`).
fn project_status(project: Option<String>, fm: Option<String>) -> Option<String> {
    project?;
    let fm: serde_json::Value = fm.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let s = crate::frontmatter::string(&fm, "status").and_then(|s| crate::projects::parse_status(&s));
    Some(s.unwrap_or("active").to_string())
}

fn tag_matches(tags: &[String], q: &str) -> bool {
    let q = if q.starts_with('#') { q.to_string() } else { format!("#{q}") };
    let q = &q[1..];
    tags.iter().any(|t| t == q || t.starts_with(&format!("{q}/")))
}

/// Days since 1970-01-01 for a `YYYY-MM-DD`, without a date library.
fn days(d: &str) -> Option<i64> {
    let y: i64 = d.get(0..4)?.parse().ok()?;
    let m: i64 = d.get(5..7)?.parse().ok()?;
    let dd: i64 = d.get(8..10)?.parse().ok()?;
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + dd - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

/// The Monday-start week containing `today` (`offset` -1 for last week), as [start, end) days.
fn week(today: &str, offset: i64) -> Option<(i64, i64)> {
    let t = days(today)?;
    // 1970-01-01 was a Thursday; Monday is 0 here.
    let wd = (t + 3).rem_euclid(7);
    let start = t - wd + offset * 7;
    Some((start, start + 7))
}

impl Index {
    /// Every task outside the templates, in file order, for the query language in the window
    /// (src/tasksq/), which searches the whole vault as the Tasks plugin does.
    pub fn all_tasks(&self) -> Result<Vec<TaskRow>> {
        let mut st =
            self.conn().prepare_cached(&format!("{} WHERE f.layer != 'template' ORDER BY f.path, t.line", rows_sql())).map_err(e)?;
        let rows = st.query_map([], row).map_err(e)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(e)
    }

    pub fn tasks(&self, q: &TaskQuery) -> Result<Vec<TaskRow>> {
        let mut st = self.conn().prepare_cached(&format!("{} WHERE f.layer = 'note' ORDER BY f.path, t.line", rows_sql())).map_err(e)?;
        let rows = st.query_map([], row).map_err(e)?;
        let window = match (q.period, q.today.as_deref()) {
            (Some(Period::ThisWeek), Some(t)) => week(t, 0),
            (Some(Period::LastWeek), Some(t)) => week(t, -1),
            _ => None,
        };
        let mut out = Vec::new();
        for r in rows {
            let t = r.map_err(e)?;
            match q.status {
                Status::Done if !t.done => continue,
                Status::NotDone if t.done => continue,
                _ => {}
            }
            if q.period.is_some() {
                let Some((a, b)) = window else { continue };
                match t.done_on.as_deref().and_then(days) {
                    Some(d) if d >= a && d < b => {}
                    _ => continue,
                }
            }
            if !q.tags_include.iter().all(|g| tag_matches(&t.tags, g)) || q.tags_exclude.iter().any(|g| tag_matches(&t.tags, g)) {
                continue;
            }
            out.push(t);
        }
        sort(&mut out, q.sort);
        if let Some(n) = q.limit {
            out.truncate(n);
        }
        Ok(out)
    }
}

/// Missing dates go last in ascending sorts and first in descending ones, as there.
fn by_date(a: &Option<String>, b: &Option<String>) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    match (a, b) {
        (None, None) => Equal,
        (None, _) => Greater,
        (_, None) => Less,
        (Some(x), Some(y)) => x.cmp(y),
    }
}

fn sort(rows: &mut [TaskRow], s: Sort) {
    // A created date, else the file's modification day (local, as a date: compared with dates).
    let created = |t: &TaskRow| {
        t.created.clone().or_else(|| {
            chrono::DateTime::from_timestamp_millis(t.mtime).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
        })
    };
    match s {
        Sort::Unsorted => {}
        Sort::DoneAsc => rows.sort_by(|a, b| by_date(&a.done_on, &b.done_on)),
        Sort::DoneDesc => rows.sort_by(|a, b| by_date(&a.done_on, &b.done_on).reverse()),
        Sort::DueAsc => rows.sort_by(|a, b| by_date(&a.due, &b.due)),
        Sort::DueDesc => rows.sort_by(|a, b| by_date(&a.due, &b.due).reverse()),
        Sort::CreatedAsc => rows.sort_by(|a, b| by_date(&created(a), &created(b))),
        Sort::CreatedDesc => rows.sort_by(|a, b| by_date(&created(a), &created(b)).reverse()),
        // Unranked first, then by rank, then file and line.
        Sort::Manual => rows.sort_by(|a, b| {
            let k = |t: &TaskRow| (t.rank.is_some(), t.rank.unwrap_or(0));
            k(a).cmp(&k(b)).then_with(|| a.path.cmp(&b.path)).then(a.line.cmp(&b.line))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;
    use std::path::PathBuf;

    fn ix() -> Index {
        let v = Vault::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault"), vec![]);
        let mut ix = Index::open_in_memory().unwrap();
        ix.sync(&v).unwrap();
        ix
    }

    fn texts(r: &[TaskRow]) -> Vec<&str> {
        r.iter().map(|t| t.text.split(" 📅").next().unwrap().split(" #").next().unwrap()).collect()
    }

    #[test]
    fn weeks() {
        // Friday 2 October 2026: that week is Mon 28 Sep – Sun 4 Oct.
        let (a, b) = week("2026-10-02", 0).unwrap();
        assert_eq!((a, b - a), (days("2026-09-28").unwrap(), 7));
        assert_eq!(week("2026-10-02", -1).unwrap().0, days("2026-09-21").unwrap());
        assert_eq!(week("2026-09-28", 0).unwrap().0, days("2026-09-28").unwrap());
    }

    #[test]
    fn filters_and_sorts() {
        let ix = ix();
        let follow =
            ix.tasks(&TaskQuery { status: Status::NotDone, tags_include: vec!["#followup".into()], ..Default::default() }).unwrap();
        assert_eq!(
            texts(&follow),
            [
                "Role-level layering for the tables",
                "Example follow-up — this row also appears in **Follow-ups** above",
                "Confirm the launch date with Sam"
            ]
        );
        let by_due = ix
            .tasks(&TaskQuery { status: Status::NotDone, tags_include: vec!["followup".into()], sort: Sort::DueAsc, ..Default::default() })
            .unwrap();
        assert_eq!(by_due[0].due.as_deref(), Some("2026-10-02"));
        // A sub-tag counts for its parent.
        let p = ix.tasks(&TaskQuery { tags_include: vec!["#project".into()], ..Default::default() }).unwrap();
        assert_eq!(texts(&p), ["Nested subtask ⏳ 2026-10-05 🛫 2026-10-01"]);
        let none_waiting = ix
            .tasks(&TaskQuery {
                status: Status::NotDone,
                tags_exclude: vec!["#waiting-for".into(), "#followup".into(), "#someday-maybe".into()],
                ..Default::default()
            })
            .unwrap();
        assert!(none_waiting.iter().all(|t| !t.tags.iter().any(|g| g == "waiting-for")));
        let done = ix
            .tasks(&TaskQuery {
                status: Status::Done,
                period: Some(Period::ThisWeek),
                today: Some("2026-10-02".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(texts(&done), ["Send the minutes ✅ 2026-09-30"]);
        assert!(ix
            .tasks(&TaskQuery { period: Some(Period::LastWeek), today: Some("2026-10-02".into()), ..Default::default() })
            .unwrap()
            .is_empty());
        let manual = ix.tasks(&TaskQuery { status: Status::NotDone, sort: Sort::Manual, ..Default::default() }).unwrap();
        let ranked: Vec<Option<i64>> = manual.iter().map(|t| t.rank).collect();
        assert_eq!(ranked.iter().rev().take(1).collect::<Vec<_>>(), [&Some(1024)]);
        assert!(ranked[0].is_none());
        // Templates and sources hold no tasks for queries.
        assert!(ix.tasks(&TaskQuery::default()).unwrap().iter().all(|t| !t.path.starts_with("Templates/")));
    }

    #[test]
    fn created_sort_dates_undated_tasks_by_their_files_day() {
        // Undated, in a file last changed in 2026-06: between the May and July tasks.
        let june = chrono::NaiveDate::from_ymd_opt(2026, 6, 15).unwrap().and_hms_opt(12, 0, 0).unwrap();
        let june_ms = june.and_local_timezone(chrono::Local).unwrap().timestamp_millis();
        let mut rows = vec![
            TaskRow { text: "july".into(), created: Some("2026-07-01".into()), ..Default::default() },
            TaskRow { text: "undated".into(), mtime: june_ms, ..Default::default() },
            TaskRow { text: "may".into(), created: Some("2026-05-01".into()), ..Default::default() },
        ];
        sort(&mut rows, Sort::CreatedAsc);
        assert_eq!(rows.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["may", "undated", "july"]);
    }
}
