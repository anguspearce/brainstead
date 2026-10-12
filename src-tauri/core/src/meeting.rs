// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! A meeting or 1-1 note from a Teams transcript (the previous app's `/new-meeting-note`,
//! `infer_meeting.py` and `new_meeting.py`): the note's type, name and date read from the
//! transcript's file name by the script's rules, the note started from `Templates/<Type>.md`
//! (its headings, Templater's tags taken out), and the model asked to fill it in from the
//! transcript. The note is a proposal; ingesting it and trashing the transcript follow.
//!
//! The script knew whose vault it was by a name in its code. Here it's the user's name in
//! Settings › General, how Teams writes it.

use serde::Serialize;
use std::sync::LazyLock;

use regex::Regex;

pub const WORKFLOW: &str = include_str!("../workflows/meeting-note.md");

static DATE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*-\s*(\d{4}-\d{2}-\d{2})$").unwrap());
static PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^Teams\.\s*Transcript\.\s*").unwrap());
static TOPIC_SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+-\s+").unwrap());

/// What the file name says.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inference {
    pub filename: String,
    /// 1-1, meeting or not-transcript.
    pub kind: String,
    /// The note's type: `1-1` or `Meeting`.
    #[serde(rename = "type")]
    pub note_type: Option<String>,
    pub name: Option<String>,
    pub date: Option<String>,
    pub topic: Option<String>,
    /// False when the user must say: no date, an unresolved 1-1, name forms that disagree, a
    /// note that exists already, or not a transcript.
    pub confident: bool,
    pub ask: Vec<String>,
    pub suggested_filename: Option<String>,
    pub existing_notes: Vec<String>,
    /// The names the existing 1-1 notes for this first name use, when they differ and the
    /// transcript's name is none of them: the screen offers them to pick from.
    pub name_forms: Vec<String>,
    pub exists: bool,
    /// The date is only the day the transcript was captured (the name's date, with no
    /// `**Meeting:**` line), so the user confirms or changes it before the note is drafted.
    pub date_check: bool,
}

/// What the screen says when [`Inference::date_check`] is set.
pub const DATE_CHECK: &str = "This is the day it was captured; check the meeting date.";

static MEETING_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\*\*Meeting:\*\*\s*(\d{4}-\d{2}-\d{2})\b").unwrap());
static CAPTURED_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\*\*Captured:\*\*\s*(\S+)").unwrap());

/// The lines above a capture's first `---`: its title, source, meeting and capture lines.
fn header_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().take_while(|l| l.trim() != "---").take(40)
}

/// The meeting's day from the transcript's `**Meeting:**` line (the Teams extension writes it).
pub fn meeting_day(text: &str) -> Option<String> {
    header_lines(text)
        .find_map(|l| MEETING_LINE.captures(l.trim()).map(|c| c[1].to_string()))
        .filter(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
}

/// The local day of the transcript's `**Captured:**` time.
pub fn captured_day(text: &str) -> Option<String> {
    header_lines(text)
        .find_map(|l| CAPTURED_LINE.captures(l.trim()).map(|c| c[1].to_string()))
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok().map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string()))
}

pub fn is_transcript(filename: &str) -> bool {
    PREFIX.is_match(filename)
}

/// `11`, `1-1` or `1:1` standing alone (Teams strips the colon from "1:1").
fn one_on_one_spans(t: &str) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    for tok in ["1-1", "1:1", "11"] {
        for (i, _) in t.match_indices(tok) {
            let j = i + tok.len();
            let before = t[..i].chars().next_back();
            let after = t[j..].chars().next();
            let bad = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit() || c == '-');
            if !bad(before) && !bad(after) && !out.iter().any(|r: &std::ops::Range<usize>| r.start < j && i < r.end) {
                out.push(i..j);
            }
        }
    }
    out.sort_by_key(|r| r.start);
    out
}

/// Comma-separated person names, when that's what the title is: each one or two words,
/// capitalised.
fn candidate_names(title: &str) -> Vec<String> {
    let parts: Vec<String> = title.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return vec![];
    }
    for p in &parts {
        let words: Vec<&str> = p.split_whitespace().collect();
        if !(1..=2).contains(&words.len()) || !words[0].chars().next().is_some_and(|c| c.is_uppercase()) {
            return vec![];
        }
    }
    parts
}

/// Reads a transcript's file name, and its first lines (`head`; empty when not read) for the
/// meeting's day, which wins over the name's. `owner` is the user's first name as Teams writes
/// it; `forms` gives the existing `1-1. <First>…` notes' file names for a first name; `exists`
/// whether a file of that name is at the vault's top level.
pub fn infer(
    filename: &str,
    head: &str,
    owner: Option<&str>,
    forms: impl Fn(&str) -> Vec<String>,
    exists: impl Fn(&str) -> bool,
) -> Inference {
    let stem = if filename.to_lowercase().ends_with(".md") { &filename[..filename.len() - 3] } else { filename };
    let mut out = Inference { filename: filename.into(), confident: true, ..Default::default() };
    if !is_transcript(filename) {
        out.kind = "not-transcript".into();
        out.confident = false;
        out.ask.push("Is this a transcript? It does not look like one.".into());
        return out;
    }
    let title = PREFIX.replace(stem, "").to_string();
    let (title, date) = match DATE_SUFFIX.captures(&title) {
        Some(c) => (title[..c.get(0).unwrap().start()].to_string(), Some(c[1].to_string())),
        None => (title.clone(), None),
    };
    let meeting = meeting_day(head);
    if meeting.is_none() && date.is_some() && date == captured_day(head) {
        out.date_check = true;
        out.confident = false;
        out.ask.push(DATE_CHECK.into());
    }
    let date = meeting.or(date);
    out.date = date.clone();
    if date.is_none() {
        out.confident = false;
        out.ask.push(
            "No date in the filename - take it from the transcript body, then the file's modified date. Never stamp it with today.".into(),
        );
    }
    let spans = one_on_one_spans(&title);
    let token = !spans.is_empty();
    let mut cleaned = title.clone();
    for r in spans.iter().rev() {
        cleaned.replace_range(r.clone(), "");
    }
    let cleaned = cleaned.trim_matches(|c| c == ' ' || c == '-').to_string();
    let mut pieces = TOPIC_SPLIT.splitn(&cleaned, 2);
    let head = pieces.next().unwrap_or("").trim().to_string();
    let topic = pieces.next().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let names = candidate_names(&head);
    let is_owner = |n: &str| owner.is_some_and(|o| n.split_whitespace().next().is_some_and(|f| f.eq_ignore_ascii_case(o.trim())));
    let others: Vec<String> = names.iter().filter(|n| !is_owner(n)).cloned().collect();

    // Two names, one the user's; or a 1:1 marker and one other name.
    if (names.len() == 2 || token) && others.len() == 1 {
        out.kind = "1-1".into();
        out.note_type = Some("1-1".into());
        out.name = Some(others[0].clone());
        out.topic = topic;
    } else if names.len() >= 3 {
        out.kind = "meeting".into();
        out.note_type = Some("Meeting".into());
        out.name = Some(head.clone());
    } else {
        out.kind = "meeting".into();
        out.note_type = Some("Meeting".into());
        out.name = Some(if cleaned.is_empty() { title.clone() } else { cleaned.clone() });
        if token {
            out.confident = false;
            out.ask.push(format!("Title looks like a 1:1 but no counterpart name resolved from {title:?}."));
        } else if names.len() == 2 && owner.is_none() {
            // Two people and no idea which is the user: a 1-1, but with whom?
            out.confident = false;
            out.ask.push("Two names: set your name in Settings › General so Brainstead knows which one is you.".into());
        }
    }

    if out.note_type.as_deref() == Some("1-1") {
        if let Some(name) = out.name.clone() {
            let first = name.split_whitespace().next().unwrap_or("").to_string();
            let f = forms(&first);
            // Each note's name: what's between `1-1. ` and its first ` - ` (a topic or the date
            // follows), for this first name only (not a Theodora for a Theo).
            let mut distinct: Vec<String> = f
                .iter()
                .filter_map(|x| {
                    let rest = x.strip_prefix("1-1. ")?.trim_end_matches(".md");
                    let n = rest.split(" - ").next().unwrap_or(rest).trim();
                    let n = Regex::new(r"\s*-?\s*\d{4}-\d{2}-\d{2}$").unwrap().replace(n, "").trim().to_string();
                    (n.split_whitespace().next() == Some(first.as_str())).then_some(n)
                })
                .collect();
            distinct.sort();
            distinct.dedup();
            out.existing_notes = f;
            if let Some(same) = distinct.iter().find(|d| d.eq_ignore_ascii_case(&name)) {
                // The transcript names one the notes use already: that one, no question.
                out.name = Some(same.clone());
            } else if distinct.len() > 1 {
                out.confident = false;
                out.ask.push(format!(
                    "Existing 1-1 notes for {first} use {}: pick the one for this note, or keep {name}.",
                    distinct.join(", ")
                ));
                out.name_forms = distinct;
            } else if let Some(one) = distinct.into_iter().next() {
                out.name = Some(one);
            }
        }
    }
    if let (Some(t), Some(n), Some(d)) = (&out.note_type, &out.name, &out.date) {
        let f = format!("{t}. {n} - {d}.md");
        out.exists = exists(&f);
        if out.exists {
            out.confident = false;
            out.ask.push(format!("{f} exists already: open it, ingest it as it is, or change the name or date for a new note."));
        }
        out.suggested_filename = Some(f);
    }
    out
}

static TP_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*<%[\s\S]*?%>[ \t]*\r?\n?").unwrap());
static TP_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<%[\s\S]*?%>").unwrap());

static TP_PROMPT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*await\s+tp\.system\.(?:prompt|suggester)\(").unwrap());

/// A template's text with Templater's tags taken out: what the note starts from. A tag that only
/// prints what the template asked for (`<% name %>` after `const name = await tp.system.prompt(…)`,
/// or `suggester(…)`, as the starter vault's 1-1 template picks the person)
/// becomes the note's name, as Templater would have made it; inside a tag (`#followup/<% name %>`)
/// it's the first name, since a tag can't hold a space.
pub fn scaffold(template: &str, name: &str) -> String {
    let mut filled = template.to_string();
    for var in TP_PROMPT.captures_iter(template).map(|c| c[1].to_string()) {
        let first = name.split_whitespace().next().unwrap_or(name);
        let re = Regex::new(&format!(r"(#[\w/-]*)?<%[ \t]*{}[ \t]*%>", regex::escape(&var))).unwrap();
        filled = re
            .replace_all(&filled, |c: &regex::Captures| match c.get(1) {
                Some(tag) => format!("{}{first}", tag.as_str()),
                None => name.to_string(),
            })
            .into_owned();
    }
    let t = TP_LINE.replace_all(&filled, |c: &regex::Captures| {
        // A tag alone on its line goes with its line; one inside text just goes.
        let s = c.get(0).unwrap().as_str();
        if s.trim_start().starts_with("<%") && s.trim_end().ends_with("%>") {
            String::new()
        } else {
            s.to_string()
        }
    });
    let t = TP_TAG.replace_all(&t, "");
    let t = t.trim_start_matches('\n').to_string();
    if t.trim().is_empty() {
        format!("# {name}\n\n## Notes\n\n## Actions\n")
    } else {
        t
    }
}

/// The message to the model: the workflow, the note to fill in and the transcript.
pub fn prompt(note_filename: &str, scaffold: &str, transcript: &str, owner: Option<&str>) -> String {
    let who = owner
        .map(|o| format!("The user is {o}: their own actions are tasks."))
        .unwrap_or_else(|| "The user's name isn't known: write every action as a plain bullet.".into());
    format!(
        "{}\n\n{who}\n\n# The note: {note_filename}\n\nFill in this note, keeping its headings and their levels:\n\n```markdown\n{}\n```\n\n# The transcript\n\n{}",
        WORKFLOW.trim(),
        scaffold.trim_end(),
        transcript
    )
}

/// The note from the model's answer: inside a markdown fence or not.
pub fn note_from_answer(answer: &str) -> Result<String, String> {
    let t = answer.trim();
    let body = match (t.find("```"), t.rfind("```")) {
        // Two separate runs: in "````" the first and last "```" overlap.
        (Some(a), Some(b)) if b >= a + 3 => {
            let inner = &t[a + 3..b];
            inner
                .split_once('\n')
                .map(|(lang, rest)| if lang.trim().chars().all(char::is_alphanumeric) { rest } else { inner })
                .unwrap_or(inner)
        }
        _ => t,
    };
    let body = body.trim();
    if body.len() < 20 {
        return Err("The answer had no note in it.".into());
    }
    Ok(format!("{body}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `tests/fixtures/meeting/expected.json`: infer_meeting.py's answers for the names in
    /// `cases.json`, with its owner set to the cases' owner (`freeze.py`).
    #[test]
    fn matches_infer_meeting_py() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/meeting");
        let cases: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("cases.json")).unwrap()).unwrap();
        let want: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
        let owner = cases["owner"].as_str().unwrap();
        let notes: Vec<String> = cases["notes"].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_string()).collect();
        let forms = |first: &str| {
            let mut v: Vec<String> =
                notes.iter().filter(|n| n.starts_with(&format!("1-1. {first}")) && n.ends_with(".md")).cloned().collect();
            v.sort();
            v
        };
        let exists = |f: &str| notes.iter().any(|n| n == f);
        for (i, f) in cases["files"].as_array().unwrap().iter().enumerate() {
            let got = infer(f.as_str().unwrap(), "", Some(owner), forms, exists);
            let w = &want[i];
            assert_eq!(serde_json::json!(got.note_type), w["type"], "{f}");
            assert_eq!(serde_json::json!(got.name), w["name"], "{f}");
            assert_eq!(serde_json::json!(got.date), w["date"], "{f}");
            assert_eq!(got.confident, w["confident"], "{f}");
            assert_eq!(serde_json::json!(got.suggested_filename), w["suggested_filename"], "{f}");
        }
    }

    #[test]
    fn a_name_the_notes_use_already_needs_no_question() {
        let notes = [
            "1-1. Theo - 2026-09-01.md",
            "1-1. Theo - Roadmap review- 2024-09-04.md",
            "1-1. Theo - Next role - 2026-08-01.md",
            "1-1. Theo Ward - 2026-07-01.md",
            "1-1. Theodora - 2026-06-01.md",
        ];
        let forms = |first: &str| notes.iter().filter(|n| n.starts_with(&format!("1-1. {first}"))).map(|n| n.to_string()).collect();
        let got = infer("Teams. Transcript. 11 Theo, Maya - 2026-10-12.md", "", Some("Maya"), forms, |_| false);
        assert!(got.confident, "{:?}", got.ask);
        assert_eq!(got.name.as_deref(), Some("Theo"));
        assert!(got.name_forms.is_empty());
        // A name none of them use: the ones they do are offered.
        let got = infer("Teams. Transcript. 11 Theo W, Maya - 2026-10-12.md", "", Some("Maya"), forms, |_| false);
        assert!(!got.confident);
        assert_eq!(got.name_forms, vec!["Theo", "Theo Ward"]);
    }

    #[test]
    fn without_the_users_name() {
        let i = infer("Teams. Transcript. Lena, Maya - 2026-09-03.md", "", None, |_| vec![], |_| false);
        assert!(!i.confident && i.ask[0].contains("Settings"));
    }

    #[test]
    fn the_meeting_line_wins_over_the_names_date() {
        let name = "Teams. Transcript. Orbit App Steerco - 2026-10-03.md";
        let head = "# Orbit App Steerco\n\n**Source:** [Open meeting recap](https://teams.microsoft.com/v2/)\n**Meeting:** 2026-10-02 10:00\n**Captured:** 2026-10-03T08:40:00.000Z\n\n---\n\n## Maya — 0:14\n";
        let i = infer(name, head, None, |_| vec![], |f| f == "Meeting. Orbit App Steerco - 2026-10-02.md");
        assert_eq!(i.date.as_deref(), Some("2026-10-02"));
        assert!(!i.date_check);
        assert_eq!(i.suggested_filename.as_deref(), Some("Meeting. Orbit App Steerco - 2026-10-02.md"));
        assert!(i.exists, "the note is looked for under the meeting's day");
        // Without a name date, the meeting line still gives one.
        let i = infer("Teams. Transcript. Orbit App Steerco.md", head, None, |_| vec![], |_| false);
        assert_eq!(i.date.as_deref(), Some("2026-10-02"));
        assert!(i.confident, "{:?}", i.ask);
    }

    #[test]
    fn a_date_that_is_only_the_capture_day_needs_a_check() {
        let at = chrono::DateTime::parse_from_rfc3339("2026-10-03T12:00:00.000Z").unwrap().with_timezone(&chrono::Local);
        let day = at.format("%Y-%m-%d").to_string();
        let name = format!("Teams. Transcript. Orbit App Steerco - {day}.md");
        let head = "# Orbit App Steerco\n\n**Source:** [Open meeting recap](https://teams.microsoft.com/v2/)\n**Captured:** 2026-10-03T12:00:00.000Z\n\n---\n";
        let i = infer(&name, head, None, |_| vec![], |_| false);
        assert!(i.date_check && !i.confident);
        assert_eq!(i.date.as_deref(), Some(day.as_str()));
        assert_eq!(i.ask, vec![DATE_CHECK.to_string()]);
        // A name dated another day (renamed by hand, or an older capture) is taken as it is.
        let i = infer("Teams. Transcript. Orbit App Steerco - 2026-09-28.md", head, None, |_| vec![], |_| false);
        assert!(!i.date_check && i.confident);
        // A `**Captured:**` line in the transcript itself, below the header, doesn't count.
        let body = format!("# T\n\n---\n\n## Maya — 0:14\n\n**Captured:** {}\n", "2026-10-03T12:00:00.000Z");
        assert!(!infer(&name, &body, None, |_| vec![], |_| false).date_check);
        // Not read: no check.
        assert!(!infer(&name, "", None, |_| vec![], |_| false).date_check);
    }

    #[test]
    fn scaffolds() {
        let t = "<%* const name = await tp.system.prompt(\"Name?\") -%>\n\n## Attendees\n- \n\n## Notes\n<% tp.file.cursor(1) -%>\n";
        assert_eq!(scaffold(t, "Steerco"), "## Attendees\n- \n\n## Notes\n");
        assert_eq!(scaffold("", "Steerco"), "# Steerco\n\n## Notes\n\n## Actions\n");
        // The 1-1 template's follow-ups query gets the person's first name.
        let one = "<%* const name = await tp.system.prompt(\"Name?\", \"\") -%>\n### Followups\n\n```tasks\nnot done\n(tags include #followup/<% name %>)\n```\nWith <% name %>.\n";
        assert_eq!(
            scaffold(one, "Maya Chen"),
            "### Followups\n\n```tasks\nnot done\n(tags include #followup/Maya)\n```\nWith Maya Chen.\n"
        );
        // The starter vault's 1-1 template picks the person from a list.
        let picked = "<%* const name = await tp.system.suggester(people, people, true, \"Who?\") -%>\n# 1-1 with <% name %>\n\n- [ ] <% tp.file.cursor(2) %> #followup/<% name %>\n";
        assert_eq!(scaffold(picked, "Lena"), "# 1-1 with Lena\n\n- [ ]  #followup/Lena\n");
        let starter = include_str!("../../starter-vault/Templates/1-1.md");
        let sc = scaffold(starter, "Theo");
        assert!(sc.starts_with("# 1-1 with Theo\n") && sc.contains("#followup/Theo") && !sc.contains("<%"), "{sc}");
    }

    #[test]
    fn notes_from_answers() {
        assert_eq!(
            note_from_answer("Here:\n```markdown\n## Notes\n\nLaunch moved to November.\n```\nDone.").unwrap(),
            "## Notes\n\nLaunch moved to November.\n"
        );
        assert!(note_from_answer("ok").is_err());
    }

    #[test]
    fn a_lone_run_of_backticks_doesnt_panic() {
        for n in 3..=6 {
            let fence = "`".repeat(n);
            assert!(note_from_answer(&fence).is_err());
            let _ = note_from_answer(&format!("{fence}\n# Meeting notes for the steerco"));
        }
    }
}
