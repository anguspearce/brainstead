// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The text of a review's writes, as pure functions: the block upserted into its monthly file as
//! the previous app's `scripts/upsert_review_block.py` does it, the `log.md` entry as
//! `scripts/log_append.py` writes it, and the check on what the model sent back. A file keeps its
//! line endings: CRLF in, CRLF out. Writing the result to disk is the caller's, through the safe write.

use std::sync::LazyLock;

use chrono::NaiveDateTime;
use regex::Regex;
use serde::Serialize;

const SEPARATOR: &str = "---";

static LOG_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^#\s+Log\s*$").unwrap());

/// What an upsert did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Upsert {
    /// The file was empty or new.
    Created,
    /// A block with the same heading was swapped in place.
    Replaced,
    /// The block went in above the newest one.
    Prepended,
}

impl Upsert {
    pub fn as_str(self) -> &'static str {
        match self {
            Upsert::Created => "created",
            Upsert::Replaced => "replaced",
            Upsert::Prepended => "prepended",
        }
    }
}

fn to_lf(s: &str) -> (String, bool) {
    if s.contains("\r\n") {
        (s.replace("\r\n", "\n"), true)
    } else {
        (s.to_string(), false)
    }
}

fn restore(s: String, crlf: bool) -> String {
    if crlf {
        s.replace('\n', "\r\n")
    } else {
        s
    }
}

/// From a block's heading up to the next `## ` heading or `---` rule, or the end: [start, end) lines.
pub(crate) fn block_bounds(lines: &[&str], heading: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| l.trim() == heading.trim())?;
    let end = (start + 1..lines.len()).find(|&j| {
        let s = lines[j].trim();
        s.starts_with("## ") || s == SEPARATOR
    });
    Some((start, end.unwrap_or(lines.len())))
}

/// A block with nothing under its heading line that would end it (`block_bounds`): a `## ` heading
/// inside it goes down a level and a `---` rule is left out. Otherwise the next run would replace
/// only the part above it and leave the rest behind, once more each time.
fn one_block(block: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut dropped = false;
    for (i, l) in block.split('\n').enumerate() {
        let t = l.trim();
        if i > 0 && t == SEPARATOR {
            dropped = true;
            continue;
        }
        // The blank line after a rule goes with it when one came before it too.
        let gap = dropped && t.is_empty() && out.last().is_some_and(|p| p.trim().is_empty());
        dropped = false;
        if gap {
            continue;
        }
        out.push(if i > 0 && t.starts_with("## ") { l.replacen("## ", "### ", 1) } else { l.to_string() });
    }
    out.join("\n").trim_end_matches('\n').to_string()
}

/// The part of a weekly block the user writes at the end of the guided weekly review.
pub const YOUR_REVIEW: &str = "### Your review";

/// The `### Your review` section in a block's lines, to the next `## `/`### ` heading.
fn your_review(lines: &[&str]) -> Option<String> {
    let i = lines.iter().position(|l| l.trim() == YOUR_REVIEW)?;
    let end = (i + 1..lines.len()).find(|&j| lines[j].starts_with("### ") || lines[j].starts_with("## ")).unwrap_or(lines.len());
    Some(lines[i..end].join("\n").trim_end().to_string())
}

fn system_note(header: &str) -> String {
    format!("> **This is a system note** - {}", header.trim())
}

/// The monthly file's new text with `block` in it under `heading`: replacing the block with that
/// heading if there is one, else newest first under the system note. An empty file is created
/// with `header` as its system note. A replaced block keeps the user's `### Your review` section
/// when the new one has none: a scheduled or re-run review never takes it away.
pub fn upsert(text: &str, heading: &str, block: &str, header: Option<&str>) -> (String, Upsert) {
    let (text, crlf) = to_lf(text);
    let (block, _) = to_lf(block);
    let mut block = block.trim_matches('\n').to_string();
    if !block.trim_start().starts_with(heading.trim()) {
        block = format!("{}\n\n{block}", heading.trim());
    }
    let block = one_block(&block);

    if text.trim().is_empty() {
        let mut parts: Vec<String> = header.map(system_note).into_iter().collect();
        parts.push(block);
        return (restore(parts.join("\n\n") + "\n", crlf), Upsert::Created);
    }

    let lines: Vec<&str> = text.split('\n').collect();
    if let Some((start, end)) = block_bounds(&lines, heading) {
        let mut block = block;
        // The old block's trailing blank lines go with it; carry the gap across (at least one line
        // when anything follows) so re-runs neither close it nor widen it.
        if let Some(mine) = your_review(&lines[start..end]) {
            if your_review(&block.split('\n').collect::<Vec<_>>()).is_none() {
                block = format!("{}\n\n{mine}", block.trim_end());
            }
        }
        let old = lines[start..end].join("\n");
        let mut trailing = (end - start) - old.trim_end_matches('\n').split('\n').count();
        if end < lines.len() && trailing == 0 {
            trailing = 1;
        }
        let mut out: Vec<&str> = lines[..start].to_vec();
        out.extend(block.split('\n'));
        out.extend(std::iter::repeat_n("", trailing));
        out.extend(&lines[end..]);
        return (restore(out.join("\n"), crlf), Upsert::Replaced);
    }

    let at = lines.iter().position(|l| l.trim().starts_with("## ")).unwrap_or(lines.len());
    let head = lines[..at].join("\n").trim_end_matches('\n').to_string();
    let tail = lines[at..].join("\n").trim_matches('\n').to_string();
    let mut pieces: Vec<String> = [head, block].into_iter().filter(|p| !p.is_empty()).collect();
    if !tail.is_empty() {
        pieces.push(format!("{SEPARATOR}\n\n{tail}"));
    }
    (restore(pieces.join("\n\n") + "\n", crlf), Upsert::Prepended)
}

/// `## [YYYY-MM-DD HH:MM] <kind> | <detail>`, with an optional summary on the next line.
pub fn log_entry(kind: &str, detail: &str, summary: Option<&str>, now: NaiveDateTime) -> String {
    let mut line = format!("## [{}] {kind} | {detail}", now.format("%Y-%m-%d %H:%M"));
    if let Some(s) = summary.filter(|s| !s.is_empty()) {
        line.push('\n');
        line.push_str(s.trim());
    }
    line
}

/// `log.md`'s new text with `entry` directly below the `# Log` heading, newest first; failing
/// that below the frontmatter, then at the very top.
pub fn log_insert(text: &str, entry: &str) -> String {
    let (text, crlf) = to_lf(text);
    let (head, tail) = if let Some(m) = LOG_HEADING.find(&text) {
        (&text[..m.end()], &text[m.end()..])
    } else if let Some(rest) = text.strip_prefix("---") {
        let cut = match rest.find("\n---").map(|i| i + 3) {
            // With no newline after the closing `---`, the frontmatter is the whole file.
            Some(end) => text[end + 1..].find('\n').map_or(text.len(), |i| end + 1 + i + 1),
            None => 0,
        };
        (&text[..cut], &text[cut..])
    } else {
        ("", text.as_str())
    };
    restore(format!("{}\n\n{entry}\n{}", head.trim_end(), tail.trim_start_matches('\n')), crlf)
}

/// The block in the model's answer: from the line that is exactly `heading` to the end, without
/// a code fence around it or anything said before it. Err when the heading isn't there or
/// nothing follows it.
pub fn extract_block(answer: &str, heading: &str) -> Result<String, String> {
    let (answer, _) = to_lf(answer);
    let lines: Vec<&str> = answer.split('\n').collect();
    let start = lines
        .iter()
        .position(|l| l.trim() == heading.trim())
        .ok_or_else(|| format!("The answer doesn't contain the heading “{}”.", heading.trim()))?;
    let fence = |l: &str| {
        let t = l.trim_start();
        t.starts_with("```") || t.starts_with("~~~")
    };
    // Inside a fence opened before the heading, the block ends where the fence closes.
    let fenced = lines[..start].iter().filter(|l| fence(l)).count() % 2 == 1;
    let end = if fenced { (start + 1..lines.len()).find(|&j| fence(lines[j])).unwrap_or(lines.len()) } else { lines.len() };
    let block = lines[start..end].join("\n").trim_end().to_string();
    if block.trim() == heading.trim() {
        return Err("The answer has the heading but nothing under it.".into());
    }
    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reviews::fixture;

    fn s(v: &serde_json::Value) -> &str {
        v.as_str().unwrap_or("")
    }

    #[test]
    fn upsert_matches_upsert_review_block_py() {
        let cases = fixture::json("upsert-cases.json");
        let want = fixture::expected("upsert.json");
        for c in cases.as_array().unwrap() {
            let name = s(&c["name"]);
            let w = &want[name];
            let (text, action) = upsert(s(&c["text"]), s(&c["heading"]), s(&c["block"]), c["header"].as_str());
            assert_eq!(text, s(&w["text"]), "{name}");
            assert_eq!(action.as_str(), s(&w["action"]), "{name}");
            // The same with CRLF line endings, which the script didn't keep.
            let crlf = |x: &str| x.replace('\n', "\r\n");
            let (text, _) = upsert(&crlf(s(&c["text"])), s(&c["heading"]), &crlf(s(&c["block"])), c["header"].as_str());
            if s(&c["text"]).contains('\n') {
                assert_eq!(text, crlf(s(&w["text"])), "{name} (CRLF)");
            }
        }
    }

    #[test]
    fn rerun_is_a_no_op() {
        let cases = fixture::json("upsert-cases.json");
        let c = &cases[3];
        let (once, _) = upsert(s(&c["text"]), s(&c["heading"]), s(&c["block"]), None);
        let (twice, action) = upsert(&once, s(&c["heading"]), s(&c["block"]), None);
        assert_eq!((once, Upsert::Replaced), (twice, action));
    }

    #[test]
    fn a_rerun_keeps_your_review() {
        let h = "## Weekly review 2026-W40";
        let file = "> note\n\n## Weekly review 2026-W40\n\nOld body.\n\n### Your review\n\nMine.\n- kept\n\n---\n\n## Weekly review 2026-W39\n\nPrev.\n";
        let (text, how) = upsert(file, h, "## Weekly review 2026-W40\n\nNew body.", None);
        assert_eq!(how, Upsert::Replaced);
        assert_eq!(
            text,
            "> note\n\n## Weekly review 2026-W40\n\nNew body.\n\n### Your review\n\nMine.\n- kept\n\n---\n\n## Weekly review 2026-W39\n\nPrev.\n"
        );
        // Run again: still once.
        assert_eq!(upsert(&text, h, "## Weekly review 2026-W40\n\nNew body.", None).0, text);
        // CRLF too.
        let (crlf, _) = upsert(&file.replace('\n', "\r\n"), h, "## Weekly review 2026-W40\n\nNew body.", None);
        assert_eq!(crlf, text.replace('\n', "\r\n"));
        // A block that brings its own (the weekly review's Finish) replaces it.
        let (t, _) = upsert(file, h, "## Weekly review 2026-W40\n\nOld body.\n\n### Your review\n\nNewer.", None);
        assert!(t.contains("Newer.") && !t.contains("Mine."));
    }

    #[test]
    fn a_block_with_a_rule_or_a_heading_in_it_is_replaced_whole() {
        let h = "## Daily summary 2026-10-01";
        let file = "> note\n\n## Daily summary 2026-09-30\n\nPrev.\n";
        let block = "## Daily summary 2026-10-01\n\nFirst.\n\n---\n\n## Orbit App\n\nSecond.\n---\nThird.";
        let (once, how) = upsert(file, h, block, None);
        assert_eq!(how, Upsert::Prepended);
        assert_eq!(
            once,
            "> note\n\n## Daily summary 2026-10-01\n\nFirst.\n\n### Orbit App\n\nSecond.\nThird.\n\n---\n\n## Daily summary 2026-09-30\n\nPrev.\n"
        );
        // Run again: the same file, with no tail left over from the first.
        assert_eq!(upsert(&once, h, block, None), (once.clone(), Upsert::Replaced));
        let (other, _) = upsert(&once, h, "## Daily summary 2026-10-01\n\nShort.", None);
        assert_eq!(other, "> note\n\n## Daily summary 2026-10-01\n\nShort.\n\n---\n\n## Daily summary 2026-09-30\n\nPrev.\n");
    }

    #[test]
    fn log_matches_log_append_py() {
        let now = NaiveDateTime::parse_from_str("2026-10-02 07:15", "%Y-%m-%d %H:%M").unwrap();
        let want = fixture::expected("log.json");
        let entry = log_entry("review", "daily-review 2026-10-01", None, now);
        assert_eq!(entry, s(&want["entry"]));
        assert_eq!(log_entry("ingest", "x", Some(" Created 2 pages. "), now), s(&want["entryWithSummary"]));
        for c in fixture::json("log-cases.json").as_array().unwrap() {
            let name = s(&c["name"]);
            assert_eq!(log_insert(s(&c["text"]), &entry), s(&want["inserts"][name]), "{name}");
        }
        assert_eq!(log_insert("# Log\r\n\r\nolder\r\n", "## e"), "# Log\r\n\r\n## e\r\nolder\r\n");
        // Frontmatter that ends the file with no newline: the entry goes below it.
        assert_eq!(log_insert("---\ntitle: Log\n---", "## e"), "---\ntitle: Log\n---\n\n## e\n");
    }

    #[test]
    fn extracts_the_block() {
        let h = "## Daily summary 2026-09-30";
        assert_eq!(
            extract_block("## Daily summary 2026-09-30\n\n- **Work done** — x\n\n", h).unwrap(),
            "## Daily summary 2026-09-30\n\n- **Work done** — x"
        );
        let chatty = "Here is the review:\n\n```markdown\n## Daily summary 2026-09-30\n\n- a\n```\n\nLet me know!";
        assert_eq!(extract_block(chatty, h).unwrap(), "## Daily summary 2026-09-30\n\n- a");
        // A fence inside the block is kept.
        let inner = "## Daily summary 2026-09-30\n\n```\ncode\n```\n- b";
        assert_eq!(extract_block(inner, h).unwrap(), inner);
        assert!(extract_block("## Daily summary 2026-09-29\n\n- a", h).is_err());
        assert!(extract_block("```\n## Daily summary 2026-09-30\n```", h).is_err());
        assert!(extract_block("Sure.\r\n## Daily summary 2026-09-30\r\n- a\r\n", h).unwrap().ends_with("- a"));
    }
}
