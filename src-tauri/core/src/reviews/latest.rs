// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The latest daily or weekly summary block on file, for Today's summary cards: today's (or this
//! week's), else the most recent within a short look-back. Found through [`Window::locate`], so
//! files and headings from before the rename are read too.

use std::cell::RefCell;
use std::collections::HashMap;

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use super::target::{window, Target};
use super::ReviewKind;

/// How many days before today a daily block is looked for.
pub const DAILY_LOOK_BACK: i64 = 31;
/// How many weeks before this one a weekly block is looked for: Today shows the most recent.
pub const WEEKLY_LOOK_BACK: i64 = 8;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Latest {
    /// The monthly file, vault-relative.
    pub file: String,
    /// The block's heading line, `## Daily summary 2026-10-04` (or its old form).
    pub heading: String,
    /// `2026-10-04` or `2026-W40`.
    pub label: String,
    /// The block, heading included.
    pub text: String,
}

/// The block under `heading` in a summaries file: to the next `## ` heading or `---` rule.
pub fn block_of(text: &str, heading: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| l.trim() == heading.trim())?;
    let end = (start + 1..lines.len()).find(|&i| lines[i].starts_with("## ") || lines[i].trim() == "---").unwrap_or(lines.len());
    Some(lines[start..end].join("\n").trim_end().to_string())
}

/// The newest `kind` block on or before `today`; `read` takes a vault-relative path.
pub fn latest(kind: ReviewKind, today: NaiveDate, read: impl Fn(&str) -> Option<String>) -> Option<Latest> {
    // Each monthly file is read once, however many days point at it.
    let cache: RefCell<HashMap<String, Option<String>>> = RefCell::new(HashMap::new());
    let cached = |p: &str| -> Option<String> { cache.borrow_mut().entry(p.to_string()).or_insert_with(|| read(p)).clone() };
    let targets: Vec<Target> = match kind {
        ReviewKind::Daily => (0..=DAILY_LOOK_BACK).map(|i| Target::Day(today - Duration::days(i))).collect(),
        ReviewKind::Weekly => (0..=WEEKLY_LOOK_BACK).map(|i| Target::week_of(today - Duration::weeks(i))).collect(),
        // A brief is a note of its own, not a block in a monthly file.
        ReviewKind::Brief => {
            return (0..=DAILY_LOOK_BACK).find_map(|i| {
                let day = (today - Duration::days(i)).to_string();
                let file = crate::brief::path(&day);
                let text = cached(&file)?;
                let body = text[crate::frontmatter::split(&text).body_start..].trim().to_string();
                Some(Latest { heading: format!("# Brief {day}"), label: day, text: body, file })
            })
        }
    };
    targets.into_iter().find_map(|t| {
        let w = window(t);
        let (file, heading) = w.locate(cached);
        let text = block_of(&cached(&file)?, &heading)?;
        Some(Latest { file, heading, label: w.label().to_string(), text })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reviews::fixture;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn finds_old_name_blocks_in_the_fixture_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let root = fixture::vault(tmp.path());
        let read = |p: &str| std::fs::read_to_string(root.join(p)).ok();
        // The fixture has only the old names: `Me. Daily Reviews - …` with `## Daily review …`.
        let d = latest(ReviewKind::Daily, day("2026-10-03"), read).unwrap();
        assert_eq!(d.file, "Me. Daily Reviews - 2026-10.md");
        assert_eq!(d.heading, "## Daily review 2026-10-01");
        assert_eq!(d.label, "2026-10-01");
        assert!(d.text.starts_with("## Daily review 2026-10-01\n"));
        // Across the month boundary, and stopping at the `---` before the next block.
        let s = latest(ReviewKind::Daily, day("2026-09-30"), read).unwrap();
        assert_eq!((s.file.as_str(), s.label.as_str()), ("Me. Daily Reviews - 2026-09.md", "2026-09-29"));
        assert!(!s.text.contains("---") && !s.text.contains("2026-09-28"));
        // This week (W40) has no block: last week's.
        let w = latest(ReviewKind::Weekly, day("2026-09-30"), read).unwrap();
        assert_eq!((w.file.as_str(), w.heading.as_str()), ("Me. Weekly Reviews - 2026-09.md", "## Weekly review 2026-W39"));
        // The most recent, a couple of weeks back; nothing past the look-back.
        assert_eq!(latest(ReviewKind::Weekly, day("2026-10-07"), read).unwrap().label, "2026-W39");
        assert_eq!(latest(ReviewKind::Weekly, day("2026-12-30"), read), None);
        // Nothing before the first block.
        assert_eq!(latest(ReviewKind::Daily, day("2026-08-01"), read), None);
    }

    #[test]
    fn todays_new_block_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let root = fixture::vault(tmp.path());
        std::fs::write(
            root.join("Me. Daily Summaries - 2026-10.md"),
            "> note\n\n## Daily summary 2026-10-02\n\n- Lena's plan.\n\n---\n\n## Daily summary 2026-09-01\n\nOld.\n",
        )
        .unwrap();
        std::fs::write(root.join("Me. Weekly Summaries - 2026-10.md"), "## Weekly summary 2026-W40\n\nThe week.\n").unwrap();
        let read = |p: &str| std::fs::read_to_string(root.join(p)).ok();
        let d = latest(ReviewKind::Daily, day("2026-10-02"), read).unwrap();
        assert_eq!(d.file, "Me. Daily Summaries - 2026-10.md");
        assert_eq!(d.text, "## Daily summary 2026-10-02\n\n- Lena's plan.");
        // The day after: still the 2nd, newer than the old file's 1st.
        assert_eq!(latest(ReviewKind::Daily, day("2026-10-03"), read).unwrap().label, "2026-10-02");
        let w = latest(ReviewKind::Weekly, day("2026-10-03"), read).unwrap();
        assert_eq!((w.label.as_str(), w.text.as_str()), ("2026-W40", "## Weekly summary 2026-W40\n\nThe week."));
    }

    #[test]
    fn reads_each_file_once() {
        let n = RefCell::new(0);
        let none = latest(ReviewKind::Daily, day("2026-10-15"), |_| {
            *n.borrow_mut() += 1;
            None
        });
        assert_eq!(none, None);
        // Two months, new and old names.
        assert_eq!(*n.borrow(), 4);
    }
}
