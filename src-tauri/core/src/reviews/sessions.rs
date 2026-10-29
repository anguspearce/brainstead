// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Claude Code sessions with real activity in a window, as the previous app's
//! `scripts/session_logs.py day` selects them. A session counts when it holds at least one real
//! user message whose timestamp falls in the window. Files are never picked by modification time:
//! Claude Code re-touches old sessions, and one resumed after midnight carries the next day's.
//! App-generated sessions (an app calling Claude as a feature) are labelled `automated: …`, so the
//! review counts them instead of writing them up as work: which ones they are is the user's to say,
//! in `automated.json` (see [`Automation`]), besides a few built in.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Zone;

/// Openings that mark a session as app-generated wherever it runs: the previous app's document
/// summariser, and Brainstead's own jobs, which open with their workflow's first line. Ask's
/// `/wiki` chats are the user's own, so they aren't here.
fn built_in() -> Vec<(&'static str, String)> {
    let first = |w: &str| w.lines().next().unwrap_or("").trim().to_lowercase();
    vec![
        ("document summariser", "summarise the following document".into()),
        ("Brainstead daily summary", first(super::DAILY_REVIEW)),
        ("Brainstead weekly summary", first(super::WEEKLY_REVIEW)),
        // The same jobs' openings before they were renamed from reviews to summaries.
        ("Brainstead daily summary", "# daily review".into()),
        ("Brainstead weekly summary", "# weekly review".into()),
        ("Brainstead ingest", first(crate::ingest::WORKFLOW)),
        ("Brainstead meeting note", first(crate::meeting::WORKFLOW)),
        ("Brainstead contradictions check", first(crate::contradictions::EXTRACT)),
        ("Brainstead contradictions check", first(crate::contradictions::JUDGE)),
        // The chat title prompt in src-tauri/src/ask.rs.
        ("Brainstead chat title", "name this conversation in 3 to 6 words".into()),
    ]
}

/// One kind of app-generated session: a session is it when its working directory contains
/// `cwd_contains` and its first message opens with `opening` (ignoring case). An empty field
/// matches anything, but not both. Read from `automated.json`, a JSON array of these.
/// Its fields keep the file's names (`cwd_contains`), which files already written use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    pub label: String,
    #[serde(default)]
    pub cwd_contains: String,
    #[serde(default)]
    pub opening: String,
}

impl Automation {
    fn matches(&self, cwd: &str, opening: &str) -> bool {
        (!self.cwd_contains.is_empty() || !self.opening.is_empty())
            && cwd.contains(&self.cwd_contains)
            && opening.starts_with(&self.opening.to_lowercase())
    }
}

/// The user's signatures from `automated.json` (its text, if there is one), then the built-in
/// ones. A file that doesn't parse adds nothing.
pub fn automations(file: Option<&str>) -> Vec<Automation> {
    let mut out: Vec<Automation> = file.and_then(|t| serde_json::from_str(t).ok()).unwrap_or_default();
    out.extend(built_in().into_iter().map(|(label, opening)| Automation {
        label: label.to_string(),
        cwd_contains: String::new(),
        opening,
    }));
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub path: String,
    /// The project folder's name under the projects folder.
    pub project: String,
    pub cwd: String,
    #[serde(serialize_with = "super::ser_time")]
    pub first_in_window: NaiveDateTime,
    #[serde(serialize_with = "super::ser_time")]
    pub last_in_window: NaiveDateTime,
    #[serde(serialize_with = "super::ser_time")]
    pub session_start: NaiveDateTime,
    #[serde(serialize_with = "super::ser_time")]
    pub session_end: NaiveDateTime,
    pub user_msgs_in_window: usize,
    pub user_msgs_total: usize,
    /// `work`, or `automated: <what>`.
    pub kind: String,
    /// The real user messages in the window, trimmed.
    pub messages: Vec<String>,
    /// Started on an earlier day than its first message in the window: "continued", not "started".
    pub spans_days: bool,
}

impl Session {
    pub fn automated(&self) -> Option<&str> {
        self.kind.strip_prefix("automated: ")
    }

    /// The first eight characters of the session file's name, as the script printed it.
    pub fn short_id(&self) -> String {
        Path::new(&self.path).file_name().map(|n| n.to_string_lossy().chars().take(8).collect()).unwrap_or_default()
    }
}

/// An ISO-8601 UTC timestamp from a log line, in local time.
fn parse_ts(raw: &Value, zone: Zone) -> Option<NaiveDateTime> {
    let s: String = raw.as_str()?.chars().take(19).collect();
    NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S").ok().map(|t| zone.to_local(t))
}

/// The text of a user message: the string, or its text blocks joined with spaces.
pub(crate) fn message_text(record: &Value) -> String {
    match record.get("message").and_then(|m| m.get("content")) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .map(|b| b.get("text").and_then(Value::as_str).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// A message the user typed: not meta, a tool result, hook output, or a wrapper the harness adds.
pub(crate) fn is_real_user_message(record: &Value) -> bool {
    if record.get("type").and_then(Value::as_str) != Some("user") || truthy(record.get("isMeta")) {
        return false;
    }
    let text = message_text(record);
    let text = text.trim();
    !(text.is_empty() || text.starts_with('<') || text.starts_with("Caveat:") || text == "[Request interrupted by user]")
}

struct Read {
    cwd: String,
    stamps: Vec<NaiveDateTime>,
    user_msgs: Vec<(NaiveDateTime, String)>,
}

fn read_session(path: &Path, zone: Zone) -> Read {
    let mut r = Read { cwd: String::new(), stamps: Vec::new(), user_msgs: Vec::new() };
    let Ok(bytes) = std::fs::read(path) else { return r };
    for line in String::from_utf8_lossy(&bytes).lines() {
        let line = line.trim();
        // Most lines are long tool output: skip the parse once nothing more could come of one.
        if line.is_empty() || (!r.cwd.is_empty() && !line.contains("\"timestamp\"")) {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(line) else { continue };
        if r.cwd.is_empty() {
            r.cwd = record.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
        }
        let Some(stamp) = record.get("timestamp").and_then(|t| parse_ts(t, zone)) else { continue };
        r.stamps.push(stamp);
        if is_real_user_message(&record) {
            r.user_msgs.push((stamp, message_text(&record).trim().to_string()));
        }
    }
    r
}

/// The user's own signatures, as `automated.json` holds them: each needs a name and something to
/// match on. Err says which one isn't.
pub fn check_automations(list: &[Automation]) -> Result<(), String> {
    for (i, a) in list.iter().enumerate() {
        if a.label.trim().is_empty() {
            return Err(format!("Give tool {} a name.", i + 1));
        }
        if a.cwd_contains.trim().is_empty() && a.opening.trim().is_empty() {
            return Err(format!("“{}” needs a folder or a first message to recognise its sessions by.", a.label.trim()));
        }
    }
    Ok(())
}

/// A folder whose sessions look like a tool's, as a suggestion to list it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub cwd: String,
    /// Its sessions with one prompt and no more, as a tool's run.
    pub count: usize,
    /// All its sessions.
    pub total: usize,
    #[serde(serialize_with = "super::ser_time")]
    pub last: NaiveDateTime,
}

/// The fewest single-prompt sessions a folder needs to be suggested.
const MIN_RUNS: usize = 3;

/// Folders whose sessions look like a tool's, most runs first. A tool runs a prompt and is done
/// (`claude -p`), where the user goes back and forth: a folder qualifies with at least
/// `MIN_RUNS` single-prompt sessions making up nearly all (four in five) of its sessions, so a
/// folder the user works in isn't suggested. Sessions already counted as automated, or with no
/// folder, are left out.
pub fn suggestions(sessions: &[Session]) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();
    for s in sessions.iter().filter(|s| s.automated().is_none() && !s.cwd.is_empty()) {
        let one = usize::from(s.user_msgs_total == 1);
        match out.iter_mut().find(|g| g.cwd == s.cwd) {
            Some(g) => {
                g.count += one;
                g.total += 1;
                g.last = g.last.max(s.first_in_window);
            }
            None => out.push(Suggestion { cwd: s.cwd.clone(), count: one, total: 1, last: s.first_in_window }),
        }
    }
    out.retain(|g| g.count >= MIN_RUNS && g.count * 5 >= g.total * 4);
    out.sort_by(|a, b| b.count.cmp(&a.count).then(b.last.cmp(&a.last)));
    out
}

/// `work`, or `automated: <what>` for the first of `automations` the session matches.
pub fn classify(cwd: &str, first_message: &str, automations: &[Automation]) -> String {
    let opening = first_message.to_lowercase();
    let opening = opening.trim_start();
    match automations.iter().find(|a| a.matches(cwd, opening)) {
        Some(a) => format!("automated: {}", a.label),
        None => "work".into(),
    }
}

/// Every session log (`<project>/<id>.jsonl`), in path order. Subagent transcripts sit a level
/// deeper and duplicate their parents, so they're never included.
fn session_files(projects: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(projects)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|d| d.path().is_dir())
        .flat_map(|d| std::fs::read_dir(d.path()).into_iter().flatten().flatten())
        .map(|f| f.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "jsonl") && !p.components().any(|c| c.as_os_str() == "subagents"))
        .collect();
    out.sort();
    out
}

/// Sessions with at least one real user message in [start, end), local time, by first message.
/// `projects` is Claude Code's projects folder (normally `~/.claude/projects`).
pub fn sessions_for_window(projects: &Path, start: NaiveDateTime, end: NaiveDateTime, automations: &[Automation]) -> Vec<Session> {
    sessions_in(projects, start, end, Zone::Local, automations)
}

pub(crate) fn sessions_in(
    projects: &Path,
    start: NaiveDateTime,
    end: NaiveDateTime,
    zone: Zone,
    automations: &[Automation],
) -> Vec<Session> {
    let files = session_files(projects);
    let reads: Vec<Read> = files.par_iter().map(|p| read_session(p, zone)).collect();
    let mut found = Vec::new();
    // By session id: renaming a project folder leaves the same session under both names.
    let mut seen: HashSet<std::ffi::OsString> = HashSet::new();
    for (path, s) in files.iter().zip(reads) {
        let name = path.file_name().unwrap_or_default().to_os_string();
        if seen.contains(&name) {
            continue;
        }
        let inside: Vec<&(NaiveDateTime, String)> = s.user_msgs.iter().filter(|(t, _)| *t >= start && *t < end).collect();
        let (Some(first), Some(last)) = (inside.first(), inside.last()) else { continue };
        seen.insert(name);
        let session_start = s.stamps.iter().min().copied().unwrap_or(first.0);
        found.push(Session {
            path: path.to_string_lossy().into_owned(),
            project: path.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            kind: classify(&s.cwd, &first.1, automations),
            cwd: s.cwd,
            first_in_window: first.0,
            last_in_window: last.0,
            session_start,
            session_end: s.stamps.iter().max().copied().unwrap_or(last.0),
            user_msgs_in_window: inside.len(),
            user_msgs_total: s.user_msgs.len(),
            messages: inside.iter().map(|(_, m)| m.clone()).collect(),
            spans_days: session_start.date() != first.0.date(),
        });
    }
    found.sort_by_key(|s| s.first_in_window);
    found
}

/// A Claude Code session that ran in one folder, for the Workspace's Resume
/// (src/workspaceApi.ts `PastSession`). Times are ms since the epoch.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSession {
    /// Its id, the log's file name: what `claude --resume` takes.
    pub id: String,
    pub folder: String,
    pub started: i64,
    pub last_active: i64,
    pub first_message: String,
    pub program: &'static str,
}

/// Claude Code's name for a folder under its projects folder: each character other than a letter
/// or digit as `-`.
fn project_dir_name(folder: &str) -> String {
    folder.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// The sessions whose working directory is `folder` and that have something the user typed,
/// most recently active first, at most `limit`. Only the folder's own project folder is read
/// (Claude Code cuts a long name short, so a long one matches by its start).
pub fn sessions_for_folder(projects: &Path, folder: &str, limit: usize) -> Vec<FolderSession> {
    let folder = folder.trim_end_matches(['/', '\\']);
    let want = project_dir_name(folder);
    let fits = |name: &str| name == want || (want.len() > 200 && name.starts_with(&want[..200]));
    let files: Vec<PathBuf> = session_files(projects)
        .into_iter()
        .filter(|p| p.parent().and_then(Path::file_name).is_some_and(|n| fits(&n.to_string_lossy())))
        .collect();
    let utc = Zone::Fixed(chrono::FixedOffset::east_opt(0).expect("a valid offset"));
    let ms = |t: NaiveDateTime| t.and_utc().timestamp_millis();
    let mut out: Vec<FolderSession> = files
        .par_iter()
        .filter_map(|p| {
            let r = read_session(p, utc);
            let (first, _) = r.user_msgs.first()?;
            (r.cwd.trim_end_matches(['/', '\\']) == folder).then(|| FolderSession {
                id: p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                folder: folder.to_string(),
                started: ms(r.stamps.iter().min().copied().unwrap_or(*first)),
                last_active: ms(r.stamps.iter().max().copied().unwrap_or(*first)),
                first_message: r.user_msgs[0].1.chars().take(200).collect(),
                program: "claude",
            })
        })
        .collect();
    out.sort_by(|a, b| b.last_active.cmp(&a.last_active).then(a.id.cmp(&b.id)));
    out.dedup_by(|a, b| a.id == b.id);
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folders_sessions_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = fixture::projects(tmp.path());
        let got = sessions_for_folder(&projects, "/home/me/Projects/alpha/", 50);
        assert_eq!(got.len(), 1, "{got:?}");
        let s = &got[0];
        assert_eq!(
            (s.id.as_str(), s.folder.as_str(), s.program),
            ("a1111111-0000-4000-8000-000000000001", "/home/me/Projects/alpha", "claude")
        );
        assert_eq!(s.first_message, "Start the alpha importer");
        let at = |x: &str| chrono::DateTime::parse_from_rfc3339(x).unwrap().timestamp_millis();
        assert_eq!(s.started, at("2026-09-29T20:00:00Z"));
        assert!(s.last_active > s.started);
        assert!(sessions_for_folder(&projects, "/home/me/Projects/nowhere", 50).is_empty());
        assert!(sessions_for_folder(&projects, "/home/me/Projects/alpha", 0).is_empty());
    }

    fn session(cwd: &str, first: &str, at: &str, kind: &str) -> Session {
        let t = NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M").unwrap();
        Session {
            path: String::new(),
            project: String::new(),
            cwd: cwd.into(),
            first_in_window: t,
            last_in_window: t,
            session_start: t,
            session_end: t,
            user_msgs_in_window: 1,
            user_msgs_total: 1,
            kind: kind.into(),
            messages: vec![first.into()],
            spans_days: false,
        }
    }

    #[test]
    fn alike_sessions_are_suggested_as_a_tool() {
        let one = |cwd: &str, at: &str| session(cwd, "Summarise this", at, "work");
        let mut talk = session("/work/orbit", "Why does the build fail?", "2026-10-02T10:00", "work");
        talk.user_msgs_total = 6;
        let s = [
            one("/work/digest", "2026-10-01T09:00"),
            one("/work/digest", "2026-10-02T09:00"),
            one("/work/digest", "2026-10-03T09:00"),
            // A folder the user works in: one-prompt sessions there too, but mostly conversations.
            one("/work/orbit", "2026-10-01T09:00"),
            one("/work/orbit", "2026-10-02T09:00"),
            one("/work/orbit", "2026-10-03T09:00"),
            talk.clone(),
            talk,
            // Already automated.
            session("/x", "name this", "2026-10-02T11:00", "automated: Brainstead chat title"),
            session("/x", "name this", "2026-10-02T12:00", "automated: Brainstead chat title"),
            session("/x", "name this", "2026-10-02T13:00", "automated: Brainstead chat title"),
        ];
        let g = suggestions(&s);
        assert_eq!(g.iter().map(|g| (g.cwd.as_str(), g.count, g.total)).collect::<Vec<_>>(), [("/work/digest", 3, 3)]);
        // A listed tool needs a name and something to match on.
        let ok = Automation { label: "Digest".into(), cwd_contains: "/work/digest".into(), opening: String::new() };
        assert!(check_automations(std::slice::from_ref(&ok)).is_ok());
        assert!(check_automations(&[Automation { label: " ".into(), ..ok.clone() }]).unwrap_err().contains("name"));
        assert!(check_automations(&[Automation { cwd_contains: String::new(), ..ok }]).unwrap_err().contains("folder"));
    }
    use crate::reviews::fixture;

    fn local(s: &str) -> NaiveDateTime {
        // The script's isoformat: `2026-09-30T00:30:00+02:00`.
        NaiveDateTime::parse_from_str(&s[..19], "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    fn compare(got: &[Session], want: &Value) {
        let want = want.as_array().unwrap();
        assert_eq!(got.len(), want.len(), "{:?}", got.iter().map(|s| &s.project).collect::<Vec<_>>());
        for (g, w) in got.iter().zip(want) {
            let name = |p: &str| Path::new(p).file_name().unwrap().to_string_lossy().into_owned();
            assert_eq!(name(&g.path), name(w["path"].as_str().unwrap()));
            assert_eq!(g.project, w["project"]);
            assert_eq!(g.cwd, w["cwd"]);
            assert_eq!(g.kind, w["kind"]);
            assert_eq!(g.first_in_window, local(w["first_in_window"].as_str().unwrap()));
            assert_eq!(g.last_in_window, local(w["last_in_window"].as_str().unwrap()));
            assert_eq!(g.session_start, local(w["session_start"].as_str().unwrap()));
            assert_eq!(g.session_end, local(w["session_end"].as_str().unwrap()));
            assert_eq!(g.user_msgs_in_window as u64, w["user_msgs_in_window"].as_u64().unwrap());
            assert_eq!(g.user_msgs_total as u64, w["user_msgs_total"].as_u64().unwrap());
            assert_eq!(serde_json::to_value(&g.messages).unwrap(), w["messages"]);
        }
    }

    #[test]
    fn matches_session_logs_py() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = fixture::projects(tmp.path());
        let day = |d: &str| NaiveDateTime::parse_from_str(&format!("{d} 00:00"), "%Y-%m-%d %H:%M").unwrap();
        let got = sessions_in(&projects, day("2026-09-30"), day("2026-10-01"), fixture::zone(), &fixture::automations());
        compare(&got, &fixture::expected("sessions-day.json"));
        // Started the evening before, so it reads as continued.
        let alpha = got.iter().find(|s| s.project == "-home-me-Projects-alpha").unwrap();
        assert!(alpha.spans_days);
        assert_eq!(alpha.messages, ["Now add the retry logic", "Ship it and write the changelog"]);
        assert_eq!(alpha.short_id(), "a1111111");
        assert_eq!(got.iter().filter(|s| s.automated().is_some()).count(), 5);
        let week = sessions_in(&projects, day("2026-09-28"), day("2026-10-05"), fixture::zone(), &fixture::automations());
        compare(&week, &fixture::expected("sessions-week.json"));
    }

    #[test]
    fn labels() {
        let a = fixture::automations();
        let c = |cwd: &str, first: &str| classify(cwd, first, &a);
        assert_eq!(c("/x/.batchbot/workdir", "You are writing a one-line summary"), "automated: batchbot pending summary");
        assert_eq!(c("/x/.batchbot/workdir", "Categorise"), "automated: batchbot record categorisation");
        assert_eq!(c("/x/Lexicon/y", "define: serendipity"), "automated: Lexicon word lookup");
        assert_eq!(c("/x/Lexicon/y", "What does this mean?"), "automated: Lexicon assistant");
        assert_eq!(c("/", "Read docs/drift-reports/latest.md"), "automated: drift report");
        assert_eq!(c("/x/Projects/lexicon", "Define: the word list format"), "work");
        // Built in, wherever it runs; a signature with nothing to match matches nothing.
        assert_eq!(c("/x", "Summarise the following document"), "automated: document summariser");
        // Brainstead's own jobs, by their workflow's first line; a /wiki chat in Ask is work.
        assert_eq!(c("/v", "# Ingest a source into the wiki\n\nYou are"), "automated: Brainstead ingest");
        assert_eq!(c("/v", "# Daily summary\n\nYou are"), "automated: Brainstead daily summary");
        assert_eq!(c("/v", "# Weekly summary\n\nYou are"), "automated: Brainstead weekly summary");
        // And as they opened before the rename.
        assert_eq!(c("/v", "# Daily review\nYou are"), "automated: Brainstead daily summary");
        assert_eq!(c("/v", "# Weekly review\nYou are"), "automated: Brainstead weekly summary");
        assert_eq!(c("/v", "# Extract checkable facts from wiki pages"), "automated: Brainstead contradictions check");
        assert_eq!(c("/d", "Name this conversation in 3 to 6 words, plain title case"), "automated: Brainstead chat title");
        assert_eq!(c("/v", "# Answer from the vault"), "work");
        let empty = automations(Some(r#"[{"label": "everything"}]"#));
        assert_eq!(classify("/x", "Hello", &empty), "work");
        assert_eq!(automations(Some("not json")), automations(None));
    }
}
