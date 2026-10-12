// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Inbox's suggestions and the guided weekly review (stage 6, src/Inbox.tsx, src/Weekly.tsx).
//!
//! A suggestion is the model's answer, read-only, as JSON checked here: nothing is written until
//! you accept one in the window. The weekly review's side pane gets the week's notes, meetings and
//! ghost links (gathered as the scheduled summary gathers them), that week's summary block and its
//! review note; its progress is kept in the app data folder so it can be paused; finishing writes
//! the week's own note, `Me. Weekly Review - YYYY-Www.md` (brainstead_core::reviews::note), and
//! leaves the weekly summaries alone.

use std::path::{Path, PathBuf};

use brainstead_core::reviews::latest::block_of;
use brainstead_core::reviews::{self, note, target, Target};
use brainstead_core::write;
use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::edits::{writable, written, FileUndo, Res, UndoEntry, UndoStack};
use crate::vault::VaultService;
use crate::{ask, platform, AppState};

#[derive(Deserialize)]
pub struct ClarifyItem {
    id: String,
    kind: String,
    text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Suggestion {
    id: String,
    becomes: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    due: Option<String>,
    #[serde(default)]
    why: String,
}

const BECOMES: [&str; 9] = ["next", "project", "waiting", "done", "someday", "reference", "delete", "ingest", "reply"];

/// Whether an item of `kind` can become `becomes`: ingest is for a capture, reply for a captured
/// email or chat thread (kind "thread", which is a capture too). A transcript is never asked about.
fn fits(kind: &str, becomes: &str) -> bool {
    match becomes {
        "ingest" => kind == "capture" || kind == "thread",
        "reply" => kind == "thread",
        _ => true,
    }
}

/// The suggestions in a model's answer: the JSON array in it, each checked; unknown ids and
/// outcomes are dropped. The model is given the items numbered from 1 (`ids` in that order), not
/// their ids, which are long (a capture's path, a task's line and words) and came back changed
/// often enough that every suggestion was dropped; a number, or an id given back exactly, is taken.
pub fn parse_suggestions(answer: &str, ids: &[String]) -> Result<Vec<Suggestion>, String> {
    let (raw, _) = brainstead_core::reviews::prep::array_items(answer)?;
    let id_of = |v: &serde_json::Value| -> Option<String> {
        let n = v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().trim_start_matches('#').parse().ok()));
        match n {
            Some(n) if n >= 1 => ids.get(n as usize - 1).cloned(),
            _ => v.as_str().filter(|s| ids.iter().any(|i| i == s)).map(str::to_string),
        }
    };
    Ok(raw
        .into_iter()
        .filter_map(|mut v| {
            let id = id_of(v.get("id")?)?;
            v["id"] = serde_json::Value::String(id);
            serde_json::from_value::<Suggestion>(v).ok()
        })
        .filter(|s| ids.contains(&s.id) && BECOMES.contains(&s.becomes.as_str()))
        .map(|mut s| {
            let clean = |o: Option<String>| o.map(|x| x.trim().to_string()).filter(|x| !x.is_empty() && x != "null");
            s.project = clean(s.project);
            s.context = clean(s.context).map(|c| c.trim_start_matches('@').trim_start_matches("#context/").to_lowercase());
            s.effort = clean(s.effort);
            s.due = clean(s.due).filter(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok());
            s
        })
        .collect())
}

fn model(app: &AppHandle) -> String {
    crate::ask::job_model(app, "clarify")
}

/// What each Inbox item could become, by the default model. Slow; off the main thread.
#[tauri::command]
pub async fn clarify_suggest(app: AppHandle, items: Vec<ClarifyItem>, projects: Vec<String>) -> Result<Vec<Suggestion>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // A Teams transcript always becomes a meeting note (the Inbox's Make a meeting note): no
        // need to ask, and asked, the model made it a next action to write one.
        let (transcripts, items): (Vec<ClarifyItem>, Vec<ClarifyItem>) = items.into_iter().partition(|i| i.kind == "transcript");
        let mut out: Vec<Suggestion> = transcripts
            .into_iter()
            .map(|i| Suggestion {
                id: i.id,
                becomes: "meeting".into(),
                text: String::new(),
                project: None,
                context: None,
                effort: None,
                due: None,
                why: "A meeting transcript becomes a meeting note, written from your template and then ingested.".into(),
            })
            .collect();
        if items.is_empty() {
            return Ok(out);
        }
        let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
        // Numbered, not by their ids (parse_suggestions maps the numbers back).
        let list: String = items.iter().enumerate().map(|(n, i)| format!("- id {}: ({}) {}\n", n + 1, i.kind, i.text.replace('\n', " / "))).collect();
        let today = Local::now().format("%Y-%m-%d (%A)");
        let prompt = format!(
            "Clarify these GTD inbox items, as David Allen's clarify step does: for each, decide what it becomes.\n\
Today is {today}. The active projects are: {}.\n\n\
Outcomes: \"next\" (a next action: write it as a clear task starting with a verb), \"project\" (needs more than one step: give the project's name), \"waiting\" (someone else must act: the task names who and what), \"done\" (takes under two minutes: do it now), \"someday\" (maybe later), \"reference\" (no action, worth keeping: name the note to file it in), \"delete\" (no longer needed).\n\
A capture (an email, chat or document captured into the vault's sources) can also be \"ingest\" (worth reading into the wiki: facts, decisions, people) and a thread (an email or chat thread) \"reply\" (it asks the user something to answer); \"ingest\" and \"reply\" are only for those.\n\
You may look up related notes in the vault to decide (who a person is, which project it belongs to), but keep it quick.\n\
Answer with only a JSON array, one object per item: {{\"id\": the item's number, \"becomes\": \"next|project|waiting|done|someday|reference|delete|ingest|reply\", \"text\": \"…\", \"project\": \"an active project's name or null\", \"context\": \"calls|computer|errands|office|home|… or null\", \"effort\": \"15m|30m|1h|… or null\", \"due\": \"YYYY-MM-DD or null\", \"why\": \"one short sentence\"}}.\n\n\
Items:\n{list}",
            if projects.is_empty() { "none".to_string() } else { projects.join("; ") }
        );
        let system = brainstead_core::ask::system_prompt(&ask::today(), false);
        let id = format!("clarify-{}", Local::now().format("%H%M%S%3f"));
        let answer = ask::run(&app, &id, &prompt, &model(&app), None, &system, None, &mut |_| {});
        if let Some(e) = answer.error {
            return Err(e);
        }
        let kind = |id: &str| items.iter().find(|i| i.id == id).map(|i| i.kind.as_str()).unwrap_or("");
        out.extend(parse_suggestions(&answer.text, &ids)?.into_iter().filter(|s| fits(kind(&s.id), &s.becomes)));
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct Note {
    path: String,
    title: String,
    mtime: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostLink {
    target: String,
    refs: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyContext {
    week: String,
    range: String,
    start: String,
    end: String,
    block: Option<String>,
    file: String,
    /// The week's own review note, once the review has been finished.
    review: Option<String>,
    notes: Vec<Note>,
    meetings: Vec<Note>,
    ghost_links: Vec<GhostLink>,
}

fn week_window(week: &str) -> Result<reviews::Window, String> {
    let (year, w) = target::parse_iso_week(week)?;
    Ok(target::window(Target::Week { year, week: w }))
}

/// Whether the note's name carries a day (`Meeting. Plan - 2026-10-01`) from `start` to `end`.
fn dated_in(file: &str, start: &str, end: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    brainstead_core::filename::parse(name).date.is_some_and(|d| d.len() == 10 && start <= d.as_str() && d.as_str() <= end)
}

fn day_label(d: &str) -> String {
    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").map(|d| d.format("%-d %b").to_string()).unwrap_or_else(|_| d.to_string())
}

#[tauri::command]
pub async fn weekly_context(app: AppHandle, week: String) -> Result<WeeklyContext, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let w = week_window(&week)?;
        let now = Local::now().naive_local();
        let inputs = app.state::<VaultService>().with_index(|ix, v| reviews::gather::gather_vault(&v.root, ix, &w, now))?;
        let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
        let (file, heading) = w.locate(|f| std::fs::read_to_string(root.join(f)).ok());
        let block = std::fs::read_to_string(root.join(&file)).ok().and_then(|t| block_of(&t, &heading));
        let review = review_note(&app, w.label());
        let title = |f: &str| f.trim_end_matches(".md").rsplit('/').next().unwrap_or(f).to_string();
        let mut notes: Vec<Note> = Vec::new();
        let mut meetings: Vec<Note> = Vec::new();
        for (kind, rows) in &inputs.journals {
            // Only notes dated in the week (`… - YYYY-MM-DD`), not older notes that were edited.
            for r in rows.iter().filter(|r| dated_in(&r.file, &w.start, &w.end)) {
                let mtime = chrono::NaiveDateTime::parse_from_str(&r.modified, "%Y-%m-%dT%H:%M:%S")
                    .map(|t| t.and_utc().timestamp_millis())
                    .unwrap_or(0);
                let n = Note { path: r.file.clone(), title: title(&r.file), mtime };
                // Meetings are looked back on in their own step, so the notes step leaves them out.
                if ["Meeting", "1-1", "Interview"].contains(&kind.as_str()) {
                    meetings.push(n);
                } else {
                    notes.push(n);
                }
            }
        }
        notes.sort_by_key(|n| std::cmp::Reverse(n.mtime));
        meetings.sort_by(|a, b| a.title.cmp(&b.title));
        let ghost_links = inputs
            .weekly
            .as_ref()
            .map(|wk| wk.ghost_links.iter().take(10).map(|g| GhostLink { target: g.target.clone(), refs: g.pages }).collect())
            .unwrap_or_default();
        Ok(WeeklyContext {
            range: format!("{} – {}", day_label(&w.start), day_label(&w.end)),
            week: w.label().to_string(),
            start: w.start.clone(),
            end: w.end.clone(),
            block,
            file,
            review: Some(review).filter(|p| root.join(p).is_file()),
            notes,
            meetings,
            ghost_links,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn state_file() -> PathBuf {
    platform::data_dir().join("weekly.json")
}

/// The weekly review in progress, as the window last saved it.
#[tauri::command]
pub fn weekly_state_read() -> Option<serde_json::Value> {
    std::fs::read(state_file()).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

#[tauri::command]
pub fn weekly_state_write(state: Option<serde_json::Value>) -> Result<(), String> {
    match state {
        Some(s) => {
            let _ = std::fs::create_dir_all(platform::data_dir());
            write::write_atomic(&state_file(), &serde_json::to_vec_pretty(&s).map_err(|e| e.to_string())?, false).map_err(|e| e.to_string())
        }
        None => {
            let _ = std::fs::remove_file(state_file());
            Ok(())
        }
    }
}

/// Where the week's review note is: by its name, wherever it was moved to, else in the vault root,
/// where Finish writes a new one. Vault-relative.
fn review_note(app: &AppHandle, week: &str) -> String {
    let name = note::file_name(week);
    let stem = brainstead_core::filename::stem(&name).to_string();
    app.state::<VaultService>()
        .with_index(|ix, _| ix.resolve(std::slice::from_ref(&stem)))
        .ok()
        .and_then(|r| r.into_iter().next().flatten())
        .unwrap_or(name)
}

/// The week's review note, if the review has been finished (for weekly_review over MCP).
#[tauri::command]
pub fn weekly_review_note(app: AppHandle, week: String) -> Result<Option<String>, String> {
    let w = week_window(&week)?;
    let root = app.state::<VaultService>().root().ok_or("No vault is open.")?;
    let path = review_note(&app, w.label());
    Ok(root.join(&path).is_file().then_some(path))
}

/// The review note at `file` written with `body`, refusing if it changed since it was read.
fn save_note(root: &Path, file: &str, w: &reviews::Window, body: &str, now: NaiveDateTime) -> Res<FileUndo> {
    let abs = root.join(file);
    let before = std::fs::read_to_string(&abs).ok();
    let text = note::compose(w, now, body, before.as_deref());
    let version = match &before {
        Some(b) => write::save_file(&abs, &text, &write::version(b.as_bytes()))?,
        None => write::create_file(&abs, &text)?,
    };
    Ok(FileUndo { path: file.to_string(), before, version })
}

/// Writes the week's review note with `body` (what the review did and your notes): created, or
/// rewritten keeping what you added under its `## Added later` heading (reviews::note). Logged in
/// `log.md` and undoable with ⌘Z. The weekly summaries aren't touched. Returns the note's path.
#[tauri::command]
pub fn weekly_finish(app: AppHandle, week: String, body: String) -> Res<String> {
    writable(&app.state::<AppState>())?;
    let w = week_window(&week)?;
    let root = app.state::<VaultService>().root().ok_or("No vault is open.".to_string())?;
    let file = review_note(&app, w.label());
    let now = Local::now().naive_local();
    let saved = save_note(&root, &file, &w, &body, now)?;
    let title = file.trim_end_matches(".md").rsplit('/').next().unwrap_or(&file).to_string();
    let what = if saved.before.is_some() { "rewritten" } else { "written" };
    let abs = root.join(&file);
    let mut files = vec![saved];
    let entry = reviews::log_entry("weekly-review", w.label(), Some(&format!("[[{title}]] {what}")), now);
    match crate::edits::add_log(&root, &entry) {
        Ok(u) => files.push(u),
        Err(e) => crate::applog!("weekly log: {e}"),
    }
    app.state::<UndoStack>().push(UndoEntry::files(format!("Weekly review {} saved", w.label()), files));
    written(&app, &[abs, root.join("log.md")]);
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_notes_dated_in_the_week() {
        let (s, e) = ("2026-09-28", "2026-10-04");
        assert!(dated_in("Meeting. Orbit App launch - 2026-10-01.md", s, e));
        assert!(dated_in("notes/1-1. Maya - 2026-09-28.md", s, e));
        assert!(!dated_in("Meeting. Orbit App kickoff - 2026-06-02.md", s, e));
        assert!(!dated_in("Project. Orbit App launch.md", s, e));
        assert!(!dated_in("Me. Daily Summaries - 2026-10.md", s, e));
    }

    #[test]
    fn reads_suggestions() {
        let ids = vec!["a".to_string(), "b".to_string()];
        let s = parse_suggestions(
            "Here you go:\n```json\n[{\"id\":\"a\",\"becomes\":\"next\",\"text\":\"Call Lena\",\"context\":\"@calls\",\"effort\":\"15m\",\"due\":\"2026-10-05\",\"project\":null,\"why\":\"x\"},{\"id\":\"zz\",\"becomes\":\"next\"},{\"id\":\"b\",\"becomes\":\"nonsense\"},{\"id\":\"b\",\"becomes\":\"someday\",\"due\":\"soon\"}]\n```",
            &ids,
        )
        .unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].context.as_deref(), Some("calls"));
        assert_eq!(s[0].due.as_deref(), Some("2026-10-05"));
        assert_eq!(s[1].becomes, "someday");
        assert_eq!(s[1].due, None);
        assert!(parse_suggestions("no json", &ids).is_err());
    }

    #[test]
    fn capture_outcomes_only_for_captures() {
        assert!(fits("capture", "ingest") && fits("thread", "ingest") && fits("thread", "reply"));
        assert!(!fits("capture", "reply") && !fits("task", "ingest") && !fits("thought", "reply"));
        assert!(fits("task", "next") && fits("thread", "delete"));
    }

    #[test]
    fn suggestions_by_the_items_numbers() {
        // The ids the Inbox has: long, with paths, colons and a task's words.
        let ids = vec![
            "capture:sources/Teams. Transcript. Steerco - 2026-10-02.md".to_string(),
            "task:14:- [ ] Ask Lena about \"the\" deck".to_string(),
        ];
        let s = parse_suggestions(
            r##"[{"id":2,"becomes":"next"},{"id":"1","becomes":"reference"},{"id":"#3","becomes":"done"},{"id":"0","becomes":"done"}]"##,
            &ids,
        )
        .unwrap();
        assert_eq!(
            s.iter().map(|x| (x.id.as_str(), x.becomes.as_str())).collect::<Vec<_>>(),
            vec![(ids[1].as_str(), "next"), (ids[0].as_str(), "reference")]
        );
    }

    #[test]
    fn finishing_writes_its_own_note_and_not_the_summaries() {
        let dir = tempfile::tempdir().unwrap();
        let summaries = "> note\n\n## Weekly summary 2026-W40\n\nBody.\n\n### Your review\n\nOld.\n";
        std::fs::write(dir.path().join("Me. Weekly Summaries - 2026-10.md"), summaries).unwrap();
        let w = week_window("2026-W40").unwrap();
        let now = NaiveDateTime::parse_from_str("2026-10-04 17:32", "%Y-%m-%d %H:%M").unwrap();
        let file = note::file_name(w.label());
        let first = save_note(dir.path(), &file, &w, "Mine.", now).unwrap();
        assert_eq!((first.path.as_str(), first.before.as_deref()), ("Me. Weekly Review - 2026-W40.md", None));
        let text = std::fs::read_to_string(dir.path().join(&file)).unwrap();
        assert!(text.starts_with("# Weekly review 2026-W40\n\n28 Sep – 4 Oct 2026 · finished 2026-10-04 17:32\n\nMine.\n"));
        // Finished again: rewritten, what was added kept, the old text there for undo.
        std::fs::write(dir.path().join(&file), format!("{text}\n- Call Maya\n")).unwrap();
        let again = save_note(dir.path(), &file, &w, "Mine again.", now).unwrap();
        assert!(again.before.as_deref().unwrap().contains("Mine."));
        let text = std::fs::read_to_string(dir.path().join(&file)).unwrap();
        assert!(text.contains("Mine again.") && text.ends_with("- Call Maya\n") && !text.contains("\nMine.\n"));
        // The summaries, with their old `### Your review`, are as they were.
        assert_eq!(std::fs::read_to_string(dir.path().join("Me. Weekly Summaries - 2026-10.md")).unwrap(), summaries);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
