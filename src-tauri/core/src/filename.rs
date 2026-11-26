// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The note filename grammar `Type. Title - YYYY-MM-DD.md`, as the previous app's
//! `parseNoteFilename` (bff/src/vault/notes.ts) reads it: the type is whatever comes before the
//! first ". ", the date is a trailing " - YYYY-MM-DD" or " - YYYY-MM", the rest is the title.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteName {
    pub kind: Option<String>,
    pub title: String,
    pub date: Option<String>,
}

/// The file name without its folders and `.md` / `.txt`.
pub fn stem(rel: &str) -> &str {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    // Compared as bytes: the name may end in a character of several bytes ("–"), where slicing
    // the text would panic. An ASCII match means the cut is on a character boundary.
    let (b, l) = (base.as_bytes(), base.len());
    if l > 3 && b[l - 3..].eq_ignore_ascii_case(b".md") {
        &base[..l - 3]
    } else if l > 4 && b[l - 4..].eq_ignore_ascii_case(b".txt") {
        &base[..l - 4]
    } else {
        base
    }
}

fn is_date_suffix(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    match b.len() {
        7 => digits(0..4) && b[4] == b'-' && digits(5..7),
        10 => digits(0..4) && b[4] == b'-' && digits(5..7) && b[7] == b'-' && digits(8..10),
        _ => false,
    }
}

/// The last `YYYY-MM-DD` or `YYYY-MM` in `s` that stands on its own (not inside a longer run of
/// digits, such as `20260917-1`).
fn date_in(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let d = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let mut found = None;
    for i in 0..b.len() {
        if i > 0 && (d(i - 1) || b[i - 1] == b'-') {
            continue;
        }
        if !(d(i) && d(i + 1) && d(i + 2) && d(i + 3) && b.get(i + 4) == Some(&b'-') && d(i + 5) && d(i + 6)) {
            continue;
        }
        let long = b.get(i + 7) == Some(&b'-') && d(i + 8) && d(i + 9);
        let end = if long { i + 10 } else { i + 7 };
        if d(end) || (!long && b.get(end) == Some(&b'-') && d(end + 1)) {
            continue;
        }
        found = Some(s[i..end].to_string());
    }
    found
}

pub fn parse(rel: &str) -> NoteName {
    let stem = stem(rel);
    let (kind, mut rest) = match stem.find(". ") {
        Some(i) if i > 0 => (Some(stem[..i].to_string()), &stem[i + 2..]),
        _ => (None, stem),
    };
    let mut date = None;
    if let Some(i) = rest.rfind(" - ") {
        if is_date_suffix(&rest[i + 3..]) {
            date = Some(rest[i + 3..].to_string());
            rest = rest[..i].trim_end();
        }
    }
    // No date at the end: the last one elsewhere in the name ("Newsletter 2026-09 Issue 5"), and
    // the title keeps it.
    if date.is_none() {
        date = date_in(stem);
    }
    let title = rest.trim();
    NoteName { kind, title: if title.is_empty() { stem.to_string() } else { title.to_string() }, date }
}

/// Takes out what a file name can't hold, or a wikilink to it can't (`# ^ [ ] |`), and runs of
/// spaces, as the window's `clean` (src/notes/filename.ts).
fn clean(v: &str) -> String {
    let kept: String =
        v.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']')).collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `[Type. ]Title[ - Date].md`, as the window's `composeFilename`; None when there's no title left.
pub fn compose(kind: Option<&str>, title: &str, date: Option<&str>) -> Option<String> {
    // A name can't start with a dot: the vault skips hidden files, so ".NET migration.md" would
    // be made and never listed (as the window's `composeFilename`).
    let undotted = |v: String| v.trim_start_matches(['.', ' ']).to_string();
    let kind = kind.map(clean).map(undotted).filter(|k| !k.is_empty());
    let title = clean(title.trim_end_matches(".md"));
    let title = if kind.is_some() { title } else { undotted(title) };
    if title.is_empty() {
        return None;
    }
    let mut s = match kind {
        Some(k) => format!("{k}. {title}"),
        None => title,
    };
    if let Some(d) = date.map(str::trim).filter(|d| !d.is_empty()) {
        s.push_str(&format!(" - {d}"));
    }
    Some(format!("{s}.md"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_stem_of_a_name_ending_in_a_wide_character() {
        assert_eq!(stem("Meeting. Q4 – plan"), "Meeting. Q4 – plan");
        assert_eq!(stem("a/Notes – 2026–"), "Notes – 2026–");
        assert_eq!(stem("Ünïcödé.MD"), "Ünïcödé");
        assert_eq!(stem("Plan –ab"), "Plan –ab");
        assert_eq!(stem("Plan –abc"), "Plan –abc");
    }

    use super::*;

    fn p(s: &str) -> (Option<String>, String, Option<String>) {
        let n = parse(s);
        (n.kind, n.title, n.date)
    }

    fn n(kind: Option<&str>, title: &str, date: Option<&str>) -> (Option<String>, String, Option<String>) {
        (kind.map(String::from), title.into(), date.map(String::from))
    }

    #[test]
    fn grammar() {
        assert_eq!(p("Meeting. Northwind Q4 prep - 2026-10-02.md"), n(Some("Meeting"), "Northwind Q4 prep", Some("2026-10-02")));
        assert_eq!(p("1-1. Theo - 2024-05-08.md"), n(Some("1-1"), "Theo", Some("2024-05-08")));
        assert_eq!(p("Me. To Do List.md"), n(Some("Me"), "To Do List", None));
        assert_eq!(p("Me. Daily Reviews - 2026-10.md"), n(Some("Me"), "Daily Reviews", Some("2026-10")));
        assert_eq!(
            p("1-1. Omar Hassan - Data sync and dashboards - 2024-09-30.md"),
            n(Some("1-1"), "Omar Hassan - Data sync and dashboards", Some("2024-09-30"))
        );
        assert_eq!(p("Untyped note.txt"), n(None, "Untyped note", None));
        assert_eq!(p("Victor/Me. Foo.md"), n(Some("Me"), "Foo", None));
        // Version-like suffixes stay in the title.
        assert_eq!(p("Spec. Hub v2.5.md"), n(Some("Spec"), "Hub v2.5", None));
        assert_eq!(p("Mr. Smith.md"), n(Some("Mr"), "Smith", None));
        assert_eq!(p(". odd.md").1, ". odd");
        // A date inside the name, when there's none at the end; the title keeps it.
        assert_eq!(p("Me. Team Newsletter 2026-09 Issue 5.md"), n(Some("Me"), "Team Newsletter 2026-09 Issue 5", Some("2026-09")));
        assert_eq!(p("Notes 2026-09-14 and 2026-10.md").2.as_deref(), Some("2026-10"));
        assert_eq!(p("Build 20260917-1.md").2, None);
        assert_eq!(p("Ticket 12026-09.md").2, None);
    }

    #[test]
    fn a_composed_name_never_starts_with_a_dot() {
        assert_eq!(compose(None, ".NET migration", None).as_deref(), Some("NET migration.md"));
        assert_eq!(compose(Some("Idea"), ".NET migration", None).as_deref(), Some("Idea. .NET migration.md"));
        assert_eq!(compose(Some(". Idea"), "Plan", Some("2026-10-02")).as_deref(), Some("Idea. Plan - 2026-10-02.md"));
        assert_eq!(compose(Some("..."), ".NET migration", None).as_deref(), Some("NET migration.md"));
        assert_eq!(compose(None, "...", None), None);
    }

    #[test]
    fn stems() {
        assert_eq!(stem("a/b/Foo.MD"), "Foo");
        assert_eq!(stem("x.txt"), "x");
        assert_eq!(stem("pic.png"), "pic.png");
    }
}
