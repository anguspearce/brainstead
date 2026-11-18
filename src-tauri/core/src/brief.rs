// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daily brief (D-20261118-04, D-20261118-14): one note a day, `Briefs/Brief YYYY-MM-DD.md`,
//! saying what came in and what's waiting. The code gathers and lays out everything it can know
//! for certain (the user's open actions, what they're waiting on, what can be archived); the model
//! writes only the Themes, from what arrived since the last brief. Nothing here writes: the app
//! makes the note through Changes.

use std::path::Path;

use crate::initiatives::{self as ini, Initiative, Owner, Status};

pub const FOLDER: &str = "Briefs";
pub const WORKFLOW: &str = include_str!("../workflows/brief.md");
/// Most of a source the model is shown.
const EXCERPT_CHARS: usize = 1_500;
/// Most sources the model is shown, newest first.
const MOST_SOURCES: usize = 40;

/// The brief's note for `day`.
pub fn path(day: &str) -> String {
    format!("{FOLDER}/Brief {day}.md")
}

/// A source that came in since the last brief.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrived {
    pub rel: String,
    /// Its text's start, for a note or text file; empty for a PDF or Office file.
    pub excerpt: String,
    /// Read by an ingest and taken out of the Inbox, but nothing cites it: nothing came of it.
    pub quiet: bool,
}

#[derive(Debug, Clone)]
pub struct Inputs {
    /// `YYYY-MM-DD`.
    pub today: String,
    /// The initiatives not done.
    pub initiatives: Vec<Initiative>,
    /// Newest first.
    pub arrived: Vec<Arrived>,
}

/// What a brief for `today` is made from: the initiatives, and the sources changed at or after
/// `since_ms` (epoch ms). `quiet` says whether a source was read and nothing came of it.
pub fn gather(root: &Path, today: &str, since_ms: i64, quiet: impl Fn(&str) -> bool) -> Inputs {
    let mut initiatives: Vec<Initiative> = ini::all(root).into_iter().filter(|i| i.status != "done").collect();
    initiatives.sort_by_key(|i| (i.path == ini::UNFILED, i.name.to_lowercase()));
    let mut found: Vec<(i64, String)> = Vec::new();
    let mut stack = vec![root.join("sources")];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ms = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as i64);
            if ms >= since_ms {
                if let Ok(rel) = p.strip_prefix(root) {
                    found.push((ms, rel.to_string_lossy().replace('\\', "/")));
                }
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let arrived = found
        .into_iter()
        .map(|(_, rel)| {
            let excerpt = if rel.ends_with(".md") || rel.ends_with(".txt") {
                let t = std::fs::read_to_string(root.join(&rel)).unwrap_or_default();
                let body = &t[crate::frontmatter::split(&t).body_start..];
                let cut: String = body.trim().chars().take(EXCERPT_CHARS).collect();
                cut
            } else {
                String::new()
            };
            let quiet = quiet(&rel);
            Arrived { rel, excerpt, quiet }
        })
        .collect();
    Inputs { today: today.to_string(), initiatives, arrived }
}

/// A source's name as a link: without `.md`, with any other extension.
fn link(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.strip_suffix(".md").unwrap_or(name).to_string()
}

/// What the model is asked: the Themes, from what arrived. None when nothing did (no call needed).
pub fn prompt(inputs: &Inputs) -> Option<String> {
    if inputs.arrived.is_empty() {
        return None;
    }
    let mut out = format!("{}\n\nToday is {}.\n\n# What came in\n\n", WORKFLOW.trim(), inputs.today);
    for a in inputs.arrived.iter().take(MOST_SOURCES) {
        out.push_str(&format!("## [[{}]]\n\n", link(&a.rel)));
        if a.excerpt.is_empty() {
            out.push_str("(A document: only its name is known here.)\n\n");
        } else {
            out.push_str(&format!("{}\n\n", a.excerpt));
        }
    }
    if inputs.arrived.len() > MOST_SOURCES {
        out.push_str(&format!("And {} more, left out here.\n\n", inputs.arrived.len() - MOST_SOURCES));
    }
    out.push_str("# The user's initiatives\n\n");
    for i in &inputs.initiatives {
        let open = i.actions.iter().filter(|a| a.status == Status::Open).count();
        out.push_str(&format!("- [[{}]]: {open} open action(s)", ini_link(i)));
        if !i.description.is_empty() {
            out.push_str(&format!(" — {}", i.description));
        }
        out.push('\n');
    }
    Some(out)
}

fn ini_link(i: &Initiative) -> String {
    link(&i.path)
}

/// One action as a line of the brief: its words, whose it is, its initiative and when.
fn action_line(i: &Initiative, a: &ini::Action, today: &str) -> String {
    let who = match (a.owner, a.person.as_deref()) {
        (Some(Owner::Delegated), Some(p)) => format!("{p}: "),
        _ => String::new(),
    };
    let mut when = Vec::new();
    match a.owner {
        Some(Owner::Delegated) => {
            if let Some(d) = &a.due {
                when.push(format!("due {d}"));
            } else if let Some(s) = &a.since {
                when.push(format!("since {s}"));
            }
            if ini::chase_from(a).is_some_and(|d| d.as_str() < today) {
                when.push("chase it".into());
            }
        }
        _ => {
            if let Some(d) = &a.due {
                when.push(if d.as_str() < today { format!("overdue, due {d}") } else { format!("due {d}") });
            }
        }
    }
    if a.task.is_none() {
        when.push("in the Inbox".into());
    }
    let when = if when.is_empty() { String::new() } else { format!(" · {}", when.join(", ")) };
    format!("- {who}{} — [[{}]]{when}", a.text, ini_link(i))
}

/// The brief, whole: its properties, then Themes (the model's, or a line saying nothing came in),
/// Your actions, Waiting for and You can archive.
pub fn render(inputs: &Inputs, themes: Option<&str>) -> String {
    let today = inputs.today.as_str();
    let day = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d").map(|d| d.format("%A %-d %B %Y").to_string()).unwrap_or(today.into());
    let mut out = format!("---\ntype: brief\ndate: {today}\n---\n\n# Brief, {day}\n\n## Themes\n\n");
    match themes.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => out.push_str(&format!("{t}\n\n")),
        None if inputs.arrived.is_empty() => out.push_str("Nothing new came in since the last brief.\n\n"),
        None => out.push_str(&format!("{} source(s) came in; their themes weren't written.\n\n", inputs.arrived.len())),
    }

    // The user's own, overdue first, then by due date, then the rest.
    let key = |a: &ini::Action| (a.due.is_none(), a.due.clone());
    let mut mine: Vec<(&Initiative, &ini::Action)> = inputs
        .initiatives
        .iter()
        .flat_map(|i| i.actions.iter().filter(|a| a.status == Status::Open && a.owner == Some(Owner::Me)).map(move |a| (i, a)))
        .collect();
    mine.sort_by_key(|(_, a)| key(a));
    out.push_str("## Your actions\n\n");
    if mine.is_empty() {
        out.push_str("None open.\n\n");
    } else {
        out.push_str(&mine.iter().map(|(i, a)| action_line(i, a, today)).collect::<Vec<_>>().join("\n"));
        out.push_str("\n\n");
    }

    // Delegated, the ones to chase first.
    let mut waiting: Vec<(&Initiative, &ini::Action)> = inputs
        .initiatives
        .iter()
        .flat_map(|i| i.actions.iter().filter(|a| a.status == Status::Open && a.owner == Some(Owner::Delegated)).map(move |a| (i, a)))
        .collect();
    waiting.sort_by_key(|(_, a)| {
        let c = ini::chase_from(a);
        (c.is_none(), c)
    });
    out.push_str("## Waiting for\n\n");
    if waiting.is_empty() {
        out.push_str("Nothing.\n\n");
    } else {
        out.push_str(&waiting.iter().map(|(i, a)| action_line(i, a, today)).collect::<Vec<_>>().join("\n"));
        out.push_str("\n\n");
    }

    // What can go: an initiative whose every action is closed, and a source nothing came of.
    let mut archive: Vec<String> = Vec::new();
    for i in inputs.initiatives.iter().filter(|i| i.path != ini::UNFILED && i.status == "active") {
        if !i.actions.is_empty() && i.actions.iter().all(|a| a.status != Status::Open) {
            archive.push(format!("- [[{}]]: every action is done or dropped; set its status to done", ini_link(i)));
        }
    }
    for a in inputs.arrived.iter().filter(|a| a.quiet) {
        archive.push(format!("- [[{}]]: read, and nothing in it was an action, a decision or news for the wiki", link(&a.rel)));
    }
    out.push_str("## You can archive\n\n");
    if archive.is_empty() {
        out.push_str("Nothing.\n");
    } else {
        out.push_str(&archive.join("\n"));
        out.push('\n');
    }
    out
}

/// The model's Themes: its answer as markdown, without a heading of its own or a code fence.
pub fn themes_of(answer: &str) -> String {
    let t = answer.trim();
    let t = t.strip_prefix("```markdown").or_else(|| t.strip_prefix("```md")).or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    let t = t.strip_prefix("## Themes").or_else(|| t.strip_prefix("# Themes")).unwrap_or(t);
    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAUNCH: &str = "---\ntype: initiative\nname: Orbit launch\n---\n## Actions\n\n- Confirm the date [owner:: me] [status:: open] [due:: 2026-11-10] ^a-1\n- Draft FAQ [owner:: me] [status:: open] [task:: [[Me. To Do List]]] ^a-2\n- Brief support [delegated:: Lena] [since:: 2026-11-01] [status:: open] ^a-3\n- Book room [delegated:: Maya] [status:: open] [due:: 2026-12-01] ^a-4\n- Sign off [owner:: Sam] [status:: open] ^a-5\n";
    const OLD: &str = "---\ntype: initiative\nname: Pilot\n---\n## Actions\n\n- Run the pilot [owner:: me] [status:: done] ^a-9\n";

    fn vault() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let w = |rel: &str, text: &str| {
            let p = t.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        w("wiki/initiatives/Orbit launch.md", LAUNCH);
        w("wiki/initiatives/Pilot.md", OLD);
        w("wiki/initiatives/Done one.md", "---\ntype: initiative\nstatus: done\n---\n## Actions\n\n- x [owner:: me] ^a-8\n");
        w("sources/emails/Email. Thread. Launch - 2026-11-17.md", "---\nfrom: Maya\n---\n# Launch\n\nThe launch moves to 28 November.");
        w("sources/Email. Thread. Newsletter - 2026-11-17.md", "Our newsletter.");
        t
    }

    #[test]
    fn a_brief_lays_out_actions_waiting_fors_and_what_can_go() {
        let t = vault();
        let i = gather(t.path(), "2026-11-18", 0, |p| p.contains("Newsletter"));
        assert_eq!(i.initiatives.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Orbit launch", "Pilot"]);
        assert_eq!(i.arrived.len(), 2);
        let text = render(&i, Some("**Launch date moves.** It's now 28 November ([[Email. Thread. Launch - 2026-11-17]])."));
        assert!(
            text.starts_with(
                "---\ntype: brief\ndate: 2026-11-18\n---\n\n# Brief, Wednesday 18 November 2026\n\n## Themes\n\n**Launch date"
            ),
            "{text}"
        );
        let section = |h: &str| text.split(&format!("## {h}\n\n")).nth(1).unwrap().split("\n\n## ").next().unwrap().trim().to_string();
        assert_eq!(
            section("Your actions"),
            "- Confirm the date — [[Orbit launch]] · overdue, due 2026-11-10, in the Inbox\n- Draft FAQ — [[Orbit launch]]"
        );
        assert_eq!(
            section("Waiting for"),
            "- Lena: Brief support — [[Orbit launch]] · since 2026-11-01, chase it, in the Inbox\n- Maya: Book room — [[Orbit launch]] · due 2026-12-01, in the Inbox"
        );
        assert_eq!(
            section("You can archive"),
            "- [[Pilot]]: every action is done or dropped; set its status to done\n- [[Email. Thread. Newsletter - 2026-11-17]]: read, and nothing in it was an action, a decision or news for the wiki"
        );
        // No tasks: nothing in it is a checkbox.
        assert!(!text.contains("- [ ]"));
    }

    #[test]
    fn with_nothing_new_there_is_no_model_call() {
        let t = vault();
        let far = i64::MAX;
        let i = gather(t.path(), "2026-11-18", far, |_| false);
        assert!(i.arrived.is_empty() && prompt(&i).is_none());
        assert!(render(&i, None).contains("## Themes\n\nNothing new came in since the last brief."));
        let i = gather(t.path(), "2026-11-18", 0, |_| false);
        let p = prompt(&i).unwrap();
        assert!(p.contains("## [[Email. Thread. Launch - 2026-11-17]]\n\n# Launch\n\nThe launch moves to 28 November."), "{p}");
        assert!(p.contains("- [[Orbit launch]]: 5 open action(s)"), "{p}");
    }

    #[test]
    fn the_models_themes_without_fence_or_heading() {
        assert_eq!(themes_of("```markdown\n## Themes\n\n**A.** b\n```"), "**A.** b");
        assert_eq!(themes_of("**A.** b"), "**A.** b");
    }
}
