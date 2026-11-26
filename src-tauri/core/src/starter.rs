// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The starter vault: a few example notes (`src-tauri/starter-vault/`, embedded by build.rs) that
//! a new vault is made with, and that Settings › Vault can add back. `{{date}}`, `{{date+N}}`,
//! `{{date-N}}` and `{{now}}` in a name or the text become the day they're written, N days on or
//! back, or the date and time, so the example tasks are never long overdue.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use chrono::{Duration, NaiveDateTime};
use regex::Regex;

include!(concat!(env!("OUT_DIR"), "/starter_files.rs"));

static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{(now|date)(?:([+-])(\d+))?\}\}").unwrap());

/// `text` with its placeholders filled in for `now`.
fn fill(text: &str, now: NaiveDateTime) -> String {
    PLACEHOLDER
        .replace_all(text, |c: &regex::Captures| {
            if &c[1] == "now" {
                return now.format("%Y-%m-%d %H:%M").to_string();
            }
            let n: i64 = c.get(3).map_or(0, |m| m.as_str().parse().unwrap_or(0));
            let d = if c.get(2).is_some_and(|m| m.as_str() == "-") { -n } else { n };
            (now.date() + Duration::days(d)).format("%Y-%m-%d").to_string()
        })
        .into_owned()
}

/// The starter files as they'd be written at `now`: (vault-relative path, text).
pub fn files(now: NaiveDateTime) -> Vec<(String, String)> {
    STARTER_FILES.iter().map(|(rel, text)| (fill(rel, now), fill(text, now))).collect()
}

/// Whether `dir` has anything in it but Finder's `.DS_Store`.
fn has_files(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|mut rd| rd.any(|e| e.is_ok_and(|e| e.file_name() != ".DS_Store")))
}

/// Makes a new vault called `name` inside `parent`, filled with the starter notes. Refuses a name
/// that isn't a plain folder name and a folder that already has files in it. Returns its path.
pub fn create(parent: &Path, name: &str, now: NaiveDateTime) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the vault a name.".into());
    }
    if name.contains(['/', '\\', ':']) || name.starts_with('.') {
        return Err(format!("“{name}” can't be a folder name. Leave out / and :, and don't start it with a dot."));
    }
    let dir = parent.join(name);
    if dir.is_file() {
        return Err(format!("There's already a file called “{name}” there. Choose another name."));
    }
    if has_files(&dir) {
        return Err(format!("“{name}” already has files in it. Choose another name, or open it with Choose folder."));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't make the folder: {e}"))?;
    for (rel, text) in files(now) {
        crate::write::create_file(&dir.join(&rel), &text).map_err(|e| format!("Couldn't write {rel}: {e}"))?;
    }
    Ok(dir)
}

/// Adds the starter notes that aren't in the vault at `root`, leaving every file already there as
/// it is. Returns the paths written.
pub fn add_missing(root: &Path, now: NaiveDateTime) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for (raw, text) in STARTER_FILES.iter() {
        let (rel, text) = (fill(raw, now), fill(text, now));
        let abs = root.join(&rel);
        if abs.exists() || dated_twin(&abs, raw) {
            continue;
        }
        crate::write::create_file(&abs, &text).map_err(|e| format!("Couldn't write {rel}: {e}"))?;
        out.push(abs);
    }
    Ok(out)
}

/// Whether a starter note with a date in its name (`raw`, placeholders unfilled) is beside `abs`
/// under another day's date: the example added on an earlier day, which isn't added again.
fn dated_twin(abs: &Path, raw: &str) -> bool {
    let name = raw.rsplit('/').next().unwrap_or(raw);
    if !PLACEHOLDER.is_match(name) {
        return false;
    }
    let mut pattern = String::from("^");
    let mut last = 0;
    for c in PLACEHOLDER.captures_iter(name) {
        let m = c.get(0).unwrap();
        pattern.push_str(&regex::escape(&name[last..m.start()]));
        pattern.push_str(if &c[1] == "now" { r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}" } else { r"\d{4}-\d{2}-\d{2}" });
        last = m.end();
    }
    pattern.push_str(&regex::escape(&name[last..]));
    pattern.push('$');
    let Ok(same) = Regex::new(&pattern) else { return false };
    let Some(Ok(rd)) = abs.parent().map(std::fs::read_dir) else { return false };
    rd.flatten().any(|e| same.is_match(&e.file_name().to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 4).unwrap().and_hms_opt(9, 30, 0).unwrap()
    }

    #[test]
    fn fills_dates() {
        assert_eq!(
            fill("{{date}} {{date+2}} {{date-5}} [{{now}}] {{query.file.path}}", now()),
            "2026-10-04 2026-10-06 2026-09-29 [2026-10-04 09:30] {{query.file.path}}"
        );
    }

    #[test]
    fn creates_a_vault_with_the_examples() {
        let d = tempfile::tempdir().unwrap();
        let v = create(d.path(), " My notes ", now()).unwrap();
        assert_eq!(v, d.path().join("My notes"));
        for rel in [
            "Start here.md",
            "Me. To Do List.md",
            "Me. Scratchpad.md",
            "Project. Orbit App launch.md",
            "Meeting. Orbit App kick-off - 2026-10-03.md",
            "Templates/Meeting.md",
            "Templates/Daily note.md",
            "Templates/Snippets/End of day.md",
            "Templates/scripts/quarter.js",
            "wiki/entities/Orbit App.md",
            "sources/Orbit App brief.md",
            "index.md",
            "log.md",
        ] {
            assert!(v.join(rel).is_file(), "{rel}");
        }
        for (rel, _) in files(now()) {
            let text = std::fs::read_to_string(v.join(&rel)).unwrap();
            assert!(!PLACEHOLDER.is_match(&text), "{rel}");
            // Each example says it's one, apart from the files that describe the vault.
            if !["index.md", "log.md", "CLAUDE.md"].contains(&rel.as_str()) {
                assert!(text.contains("[!example]") || text.contains("Example template"), "{rel}");
            }
        }
        // A folder Finder has only looked at counts as empty.
        let e = d.path().join("Empty");
        std::fs::create_dir(&e).unwrap();
        std::fs::write(e.join(".DS_Store"), "").unwrap();
        assert!(create(d.path(), "Empty", now()).is_ok());
    }

    #[test]
    fn refuses_a_folder_with_files_and_bad_names() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("Notes")).unwrap();
        std::fs::write(d.path().join("Notes/mine.md"), "mine").unwrap();
        assert!(create(d.path(), "Notes", now()).unwrap_err().contains("already has files"));
        assert_eq!(std::fs::read_dir(d.path().join("Notes")).unwrap().count(), 1);
        for bad in ["", "  ", "a/b", ".hidden", "a:b"] {
            assert!(create(d.path(), bad, now()).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn links_point_at_starter_notes() {
        let fs = files(now());
        let names: Vec<&str> = fs.iter().map(|(rel, _)| rel.rsplit('/').next().unwrap().trim_end_matches(".md")).collect();
        let link = Regex::new(r"\[\[([^\]|#]*)").unwrap();
        for (rel, text) in &fs {
            for c in link.captures_iter(text) {
                // A link a template writes (`[[<% … %>]]`) is only made when it runs.
                if c[1].contains("<%") {
                    continue;
                }
                let t = c[1].rsplit('/').next().unwrap();
                assert!(t.is_empty() || t == "Launch retrospective" || names.contains(&t), "{rel}: [[{t}]]");
            }
        }
    }

    #[test]
    fn adds_back_only_what_is_missing() {
        let d = tempfile::tempdir().unwrap();
        let v = create(d.path(), "V", now()).unwrap();
        std::fs::write(v.join("Me. To Do List.md"), "my own list").unwrap();
        std::fs::remove_file(v.join("Start here.md")).unwrap();
        let added = add_missing(&v, now()).unwrap();
        assert_eq!(added, [v.join("Start here.md")]);
        assert_eq!(std::fs::read_to_string(v.join("Me. To Do List.md")).unwrap(), "my own list");
        // Another day: the dated examples are there under the first day's dates, and stay the only ones.
        let later = now() + Duration::days(3);
        assert_eq!(add_missing(&v, later).unwrap(), Vec::<PathBuf>::new());
        // One of them gone is added back, under the new day's date.
        std::fs::remove_file(v.join("Meeting. Orbit App kick-off - 2026-10-03.md")).unwrap();
        assert_eq!(add_missing(&v, later).unwrap(), [v.join("Meeting. Orbit App kick-off - 2026-10-06.md")]);
    }
}
