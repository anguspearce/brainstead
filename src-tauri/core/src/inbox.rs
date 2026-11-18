// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Inbox: the Scratchpad's `## YYYY-MM-DD HH:MM` blocks below its pinned header (newest
//! first), then the open tasks under the To Do list's `#### Other` (in file order), as capture
//! writes them (`write::capture_thought`, `write::capture_task`). Clarifying an item writes its
//! result elsewhere and removes it from here.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::markdown::{heading, lines};
use crate::tasks::parse_line;
use crate::write::{self, FileChange, Result, WriteError};

pub const SCRATCHPAD: &str = "Me. Scratchpad.md";
pub const TODO_LIST: &str = "Me. To Do List.md";

static STAMP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^##\s+(\d{4}-\d{2}-\d{2} \d{2}:\d{2})\s*$").unwrap());

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InboxItem {
    /// thought or task (or, in the app, capture: a file from the Outlook or Teams extension).
    pub kind: &'static str,
    pub path: String,
    /// 0-based: the block's heading, or the task's line.
    pub line: usize,
    /// That line as it is in the file.
    pub line_text: String,
    /// The thought's body, trimmed, or the task's text.
    pub text: String,
    /// A thought's `YYYY-MM-DD HH:MM`.
    pub stamp: Option<String>,
    /// A thought's whole block (heading to last non-blank line, LF endings), to check before
    /// removing it.
    pub block: Option<String>,
    /// An initiative's action (kind `action`, D-20261118-12): `path` is the initiative's page and
    /// `line`/`line_text` the action's line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<InboxAction>,
}

/// What the Inbox shows of an initiative's action.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InboxAction {
    pub id: String,
    /// The initiative's name.
    pub initiative: String,
    /// me or delegated.
    pub owner: String,
    pub person: Option<String>,
    pub due: Option<String>,
    /// Past its due date.
    pub overdue: bool,
    /// Its quotes as `“words” — [[source]]` lines.
    pub quotes: Vec<String>,
}

/// The initiatives' actions that wait in the Inbox (`initiatives::wants_inbox`): Unfiled's last,
/// otherwise by initiative, overdue first, then by due date.
pub fn actions(root: &std::path::Path, today: &str) -> Vec<InboxItem> {
    use crate::initiatives as ini;
    let Ok(rd) = std::fs::read_dir(root.join(ini::FOLDER)) else { return vec![] };
    let mut pages: Vec<ini::Initiative> = rd
        .flatten()
        .filter_map(|e| {
            let rel = format!("{}/{}", ini::FOLDER, e.file_name().to_string_lossy());
            let text = std::fs::read_to_string(e.path()).ok()?;
            ini::is_initiative(&rel).then(|| ini::parse(&rel, &text))
        })
        .filter(|i| i.status != "done")
        .collect();
    pages.sort_by_key(|i| (i.path == ini::UNFILED, i.name.to_lowercase()));
    let mut out = Vec::new();
    for i in &pages {
        let mut acts: Vec<&ini::Action> = i.actions.iter().filter(|a| ini::wants_inbox(a, today)).collect();
        acts.sort_by_key(|a| (a.due.is_none(), a.due.clone()));
        for a in acts {
            let owner = if a.owner == Some(ini::Owner::Me) { "me" } else { "delegated" };
            out.push(InboxItem {
                kind: "action",
                path: i.path.clone(),
                line: a.line,
                line_text: a.line_text.clone(),
                text: a.text.clone(),
                stamp: a.due.clone(),
                block: None,
                action: Some(InboxAction {
                    id: a.id.clone(),
                    initiative: i.name.clone(),
                    owner: owner.into(),
                    person: a.person.clone(),
                    due: a.due.clone(),
                    overdue: a.due.as_deref().is_some_and(|d| d < today),
                    quotes: a.quotes.iter().map(|q| format!("“{}” — [[{}]]", q.text, q.source)).collect(),
                }),
            });
        }
    }
    out
}

/// One Scratchpad block, by line: its heading, its last non-blank line, and where the next
/// block (or other level 1 or 2 heading) starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Block {
    start: usize,
    last: usize,
    end: usize,
}

fn blocks(src: &str) -> Vec<(Block, String)> {
    let first = src[..write::pinned_header_len(src)].matches('\n').count();
    let ls = lines(src, 0);
    let top = |i: usize| !ls[i].code && heading(ls[i].text).is_some_and(|(n, _)| n <= 2);
    let mut out = Vec::new();
    for i in first..ls.len() {
        if ls[i].code {
            continue;
        }
        let Some(c) = STAMP.captures(ls[i].text) else { continue };
        let end = (i + 1..ls.len()).find(|&j| top(j)).unwrap_or(ls.len());
        let last = (i..end).rev().find(|&j| !ls[j].text.trim().is_empty()).unwrap_or(i);
        out.push((Block { start: i, last, end }, c[1].to_string()));
    }
    out
}

fn block_text(ls: &[String], b: Block) -> String {
    ls[b.start..=b.last].join("\n")
}

/// The Scratchpad's thoughts, newest first.
pub fn thoughts(src: &str) -> Vec<InboxItem> {
    let (ls, _, _) = write::split_lines(src);
    let mut out: Vec<InboxItem> = blocks(src)
        .into_iter()
        .map(|(b, stamp)| InboxItem {
            kind: "thought",
            path: SCRATCHPAD.into(),
            line: b.start,
            line_text: ls[b.start].clone(),
            text: ls[b.start + 1..=b.last.max(b.start)].join("\n").trim().to_string(),
            stamp: Some(stamp),
            block: Some(block_text(&ls, b)),
            action: None,
        })
        .collect();
    out.sort_by(|a, b| b.stamp.cmp(&a.stamp));
    out
}

/// The To Do list's open tasks under `#### Other`, in file order.
pub fn other_tasks(src: &str) -> Vec<InboxItem> {
    let mut out = Vec::new();
    let mut inside = false;
    for l in lines(src, 0).iter().filter(|l| !l.code) {
        if let Some((n, h)) = heading(l.text) {
            if n <= 4 {
                inside = n == 4 && h == "Other";
            }
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(t) = parse_line(l.text, l.no).filter(|t| !t.done) {
            out.push(InboxItem {
                kind: "task",
                path: TODO_LIST.into(),
                line: l.no,
                line_text: t.line_text,
                text: t.text,
                stamp: None,
                block: None,
                action: None,
            });
        }
    }
    out
}

/// Everything in the Inbox: thoughts newest first, then tasks. A list that isn't there adds nothing.
pub fn items(root: &Path) -> Vec<InboxItem> {
    let read = |n: &str| std::fs::read_to_string(root.join(n)).unwrap_or_default();
    let mut out = thoughts(&read(SCRATCHPAD));
    out.extend(other_tasks(&read(TODO_LIST)));
    out
}

/// Removes one Scratchpad block: its heading, its body and the blank lines after it (the last
/// block takes the blank lines before it too, so the file doesn't end in them). It's found at
/// `line`, or by its heading if it moved, and refused as stale unless it's exactly `expected`.
pub fn remove_block(path: &Path, line: usize, expected: &str) -> Result<FileChange> {
    let l = write::lock(path);
    let _g = l.lock().unwrap();
    write::check_conflict_copies(path)?;
    let src = std::fs::read_to_string(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => WriteError::NotFound("The Scratchpad isn't in the vault any more.".into()),
        _ => write::io(e),
    })?;
    let (mut ls, eol, trailing) = write::split_lines(&src);
    let expected = expected.replace("\r\n", "\n");
    let all = blocks(&src);
    let found = all.iter().find(|(b, _)| b.start == line && block_text(&ls, *b) == expected).or_else(|| {
        let head = expected.lines().next().unwrap_or("");
        let mut same = all.iter().filter(|(b, _)| ls[b.start] == head);
        match (same.next(), same.next()) {
            (Some(b), None) if block_text(&ls, b.0) == expected => Some(b),
            _ => None,
        }
    });
    let Some((b, _)) = found else {
        return Err(WriteError::Stale("This thought changed in the Scratchpad since it was shown. It's been reloaded; try again.".into()));
    };
    let mut start = b.start;
    if b.end == ls.len() {
        let floor = src[..write::pinned_header_len(&src)].matches('\n').count();
        while start > floor && ls[start - 1].trim().is_empty() {
            start -= 1;
        }
    }
    ls.drain(start..b.end);
    let text = write::join_lines(&ls, &eol, trailing);
    write::write_atomic(path, text.as_bytes(), false)?;
    Ok(FileChange { path: path.to_path_buf(), before: Some(src), version: write::version(text.as_bytes()) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vault")
    }

    #[test]
    fn reads_the_fixture_inbox() {
        let items = items(&fixture());
        let thoughts: Vec<_> = items.iter().filter(|i| i.kind == "thought").collect();
        assert_eq!(thoughts.len(), 2);
        assert_eq!(thoughts[0].stamp.as_deref(), Some("2026-10-02 07:45"));
        assert_eq!(thoughts[0].text, "A short video for the launch comms.\nTwo minutes, no more.");
        assert_eq!(thoughts[0].block.as_deref(), Some("## 2026-10-02 07:45\n\nA short video for the launch comms.\nTwo minutes, no more."));
        assert_eq!((thoughts[1].line, thoughts[1].line_text.as_str()), (2, "## 2026-10-01 08:30"));
        let tasks: Vec<_> = items.iter().filter(|i| i.kind == "task").collect();
        assert_eq!(tasks.len(), 8);
        assert!(tasks[0].text.starts_with("Walk through the Dashboard"));
        assert!(tasks[0].line_text.starts_with("* [ ] Walk through"));
        // Thoughts come first.
        assert_eq!(items[2].kind, "task");
    }

    #[test]
    fn blocks_below_the_header_only() {
        let src = "# Me. Scratchpad\n\n> **This is a system note** — x\n\n## 2026-09-01 10:00\nno blank line\n\n```\n## 2026-09-03 10:00\n```\n\n## Not a stamp\n\nloose\n\n## 2026-09-02 10:00\n\n\n";
        let t = thoughts(src);
        assert_eq!(t.iter().map(|i| i.stamp.as_deref().unwrap()).collect::<Vec<_>>(), ["2026-09-02 10:00", "2026-09-01 10:00"]);
        assert_eq!(t[1].text, "no blank line\n\n```\n## 2026-09-03 10:00\n```");
        assert_eq!(t[0].text, "");
        let todo = "#### Other\n- [ ] one\n- [x] done\n##### Sub\n- [ ] two\n#### Done\n- [ ] not inbox\n";
        let o = other_tasks(todo);
        assert_eq!(o.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), ["one", "two"]);
    }

    fn temp(text: &str) -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(SCRATCHPAD);
        fs::write(&p, text).unwrap();
        (d, p)
    }

    #[test]
    fn removes_one_block() {
        let src = "> **This is a system note** — x\r\n\r\n## 2026-10-02 10:00\r\n\r\nnew\r\n\r\n## 2026-10-01 09:00\r\n\r\nold\r\nmore\r\n";
        let (_d, p) = temp(src);
        let first = "## 2026-10-02 10:00\n\nnew";
        let ch = remove_block(&p, 2, first).unwrap();
        let after = "> **This is a system note** — x\r\n\r\n## 2026-10-01 09:00\r\n\r\nold\r\nmore\r\n";
        assert_eq!(fs::read_to_string(&p).unwrap(), after);
        assert_eq!(ch.before.as_deref(), Some(src));
        // The last block, found by its heading though it moved up.
        remove_block(&p, 6, "## 2026-10-01 09:00\n\nold\nmore").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "> **This is a system note** — x\r\n\r\n");
        // Undo is refused once the file has moved on.
        assert!(matches!(write::restore_files(std::slice::from_ref(&ch)), Err(WriteError::Stale(_))));
    }

    #[test]
    fn refuses_a_changed_block() {
        let src = "## 2026-10-02 10:00\n\nnew\n\n## 2026-10-01 09:00\n\nold\n";
        let (d, p) = temp(src);
        assert!(matches!(remove_block(&p, 0, "## 2026-10-02 10:00\n\nnew, edited"), Err(WriteError::Stale(_))));
        assert!(matches!(remove_block(&p, 0, "## 2026-10-03 10:00\n\nnew"), Err(WriteError::Stale(_))));
        assert_eq!(fs::read_to_string(&p).unwrap(), src);
        fs::write(d.path().join("Me. Scratchpad 2.md"), "").unwrap();
        assert!(matches!(remove_block(&p, 0, "## 2026-10-02 10:00\n\nnew"), Err(WriteError::Conflict(_))));
    }

    #[test]
    fn undoes_a_removal() {
        let src = "## 2026-10-02 10:00\n\nnew\n\n## 2026-10-01 09:00\n\nold\n";
        let (_d, p) = temp(src);
        let ch = remove_block(&p, 0, "## 2026-10-02 10:00\n\nnew").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "## 2026-10-01 09:00\n\nold\n");
        write::restore_files(&[ch]).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), src);
    }

    #[test]
    fn initiatives_actions_wait_in_the_inbox() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("wiki/initiatives");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Orbit launch.md"),
            "---\ntype: initiative\n---\n## Actions\n\n- Brief support [owner:: me] [status:: open] ^a-1\n  - \"Can you brief support?\" — [[Mail]]\n- Book room [delegated:: Maya] [status:: open] [due:: 2026-11-01] ^a-2\n- Sign off [delegated:: Sam] [status:: open] [due:: 2026-12-01] ^a-3\n",
        )
        .unwrap();
        std::fs::write(dir.join("Unfiled.md"), "---\ntype: initiative\n---\n## Actions\n\n- Water the plants [owner:: me] ^a-4\n").unwrap();
        std::fs::write(dir.join("Old.md"), "---\ntype: initiative\nstatus: done\n---\n## Actions\n\n- Old thing [owner:: me] ^a-5\n")
            .unwrap();
        let got = actions(t.path(), "2026-11-18");
        let ids: Vec<&str> = got.iter().map(|i| i.action.as_ref().unwrap().id.as_str()).collect();
        assert_eq!(ids, ["a-2", "a-1", "a-4"], "by initiative, Unfiled last; overdue (dated) first; done initiatives left out");
        let a = &got[1];
        assert_eq!((a.kind, a.path.as_str(), a.line, a.text.as_str()), ("action", "wiki/initiatives/Orbit launch.md", 5, "Brief support"));
        assert_eq!(a.action.as_ref().unwrap().quotes, ["“Can you brief support?” — [[Mail]]"]);
        let m = got[0].action.as_ref().unwrap();
        assert_eq!((m.owner.as_str(), m.person.as_deref(), m.overdue), ("delegated", Some("Maya"), true));
    }
}
