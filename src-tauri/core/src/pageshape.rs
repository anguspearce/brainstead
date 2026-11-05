// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The one layout every wiki page has (D-20261006-06): opening text, Current state, topical
//! sections, Timeline, See also. Timeline entries are `###` headings that start with an ISO date
//! (`YYYY-MM-DD`, `YYYY-MM`, `YYYY-Q3`, `YYYY-H1`), newest first (D-20261006-07), each with a
//! `Source: [[…]]` line under it naming the note or source it came from (D-20261006-11): one
//! source, one entry per page.
//!
//! `parse` cuts a page into its frontmatter, opening text and level-2 sections; `classify` says
//! what a section is, reading a date from a dated section's heading; `reshape` puts a page in the
//! shape by moving whole sections and rewriting dated headings, and checks the result (`check`)
//! before handing it back. It never asks a model and never changes a line it doesn't have to.
//!
//! Tracker notes (`Tracker. <title>.md`) have a shape of their own: the opening (with the note's
//! `Project: [[…]]` line), then Current state, Blockers, Next and Log, the Log's dated entries
//! newest first, and any other sections below those (`reshape_tracker`).

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::markdown;

/// Sections that sum a page up; one of them becomes Current state.
const SUMMING: &[&str] = &["current state", "current status", "summary", "overview", "status", "executive summary"];
/// Sections that close a page.
const CLOSING: &[&str] = &["see also", "related", "related pages", "related notes", "references"];
pub const CURRENT_STATE: &str = "Current state";
pub const TIMELINE: &str = "Timeline";

/// A page cut into its parts. Every byte of the page is in exactly one of them, in order.
#[derive(Debug)]
pub struct Page<'a> {
    /// The frontmatter, `---` lines and all ("" when there is none).
    pub front: &'a str,
    /// What's before the first level-2 heading: the H1 and the opening text.
    pub opening: &'a str,
    pub sections: Vec<Section<'a>>,
}

/// A level-2 section: its heading line and everything up to the next level-2 heading.
#[derive(Debug)]
pub struct Section<'a> {
    /// The heading's text, without `## `.
    pub heading: &'a str,
    pub text: &'a str,
}

impl Section<'_> {
    /// The heading line (without its line ending).
    pub fn heading_line(&self) -> &str {
        self.text.lines().next().unwrap_or("")
    }
    /// What's under the heading line.
    pub fn body(&self) -> &str {
        self.text.split_once('\n').map_or("", |(_, b)| b)
    }
}

/// The page cut at its level-2 headings (outside code fences).
pub fn parse(page: &str) -> Page<'_> {
    let start = crate::frontmatter::split(page).body_start;
    let mut cuts = Vec::new();
    for l in markdown::lines(page, start) {
        if l.code {
            continue;
        }
        if let Some((2, t)) = markdown::heading(l.text) {
            cuts.push((l.start, t));
        }
    }
    let first = cuts.first().map_or(page.len(), |c| c.0);
    let sections = cuts
        .iter()
        .enumerate()
        .map(|(i, &(at, heading))| Section { heading, text: &page[at..cuts.get(i + 1).map_or(page.len(), |c| c.0)] })
        .collect();
    Page { front: &page[..start], opening: &page[start..first], sections }
}

/// When something happened, as precisely as the heading says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Date {
    Day(i32, u32, u32),
    Month(i32, u32),
    Quarter(i32, u32),
    Half(i32, u32),
}

impl Date {
    /// `2026-10-02`, `2026-10`, `2026-Q3`, `2026-H1`.
    pub fn iso(self) -> String {
        match self {
            Date::Day(y, m, d) => format!("{y:04}-{m:02}-{d:02}"),
            Date::Month(y, m) => format!("{y:04}-{m:02}"),
            Date::Quarter(y, q) => format!("{y:04}-Q{q}"),
            Date::Half(y, h) => format!("{y:04}-H{h}"),
        }
    }

    /// Sorts newest last; a month, quarter or half sorts before (so, newest first, after) the
    /// days and months in it.
    pub fn key(self) -> (i32, u32, i32) {
        match self {
            Date::Day(y, m, d) => (y, m, d as i32),
            Date::Month(y, m) => (y, m, 0),
            Date::Quarter(y, q) => (y, 3 * q - 2, -1),
            Date::Half(y, h) => (y, 6 * h - 5, -2),
        }
    }

    pub fn year(self) -> i32 {
        match self {
            Date::Day(y, ..) | Date::Month(y, _) | Date::Quarter(y, _) | Date::Half(y, _) => y,
        }
    }

    /// The ISO forms a Timeline entry's heading starts with.
    pub fn parse_iso(s: &str) -> Option<Date> {
        let c = ISO_ENTRY.captures(s)?;
        let y: i32 = c[1].parse().ok()?;
        if let Some(q) = c.get(4) {
            let n: u32 = q.as_str()[1..].parse().ok()?;
            return Some(if q.as_str().starts_with('Q') { Date::Quarter(y, n) } else { Date::Half(y, n) });
        }
        let m: u32 = c.get(2)?.as_str().parse().ok()?;
        match c.get(3) {
            Some(d) => valid_day(y, m, d.as_str().parse().ok()?),
            None => (1..=12).contains(&m).then_some(Date::Month(y, m)),
        }
    }
}

fn valid_day(y: i32, m: u32, d: u32) -> Option<Date> {
    chrono::NaiveDate::from_ymd_opt(y, m, d).map(|_| Date::Day(y, m, d))
}

/// `2026-10-02`, `2026-10`, `2026-Q3` or `2026-H1`, alone or followed by ` — ` and a title.
static ISO_ENTRY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d{4})-(?:(\d{2})(?:-(\d{2}))?|([QH][1-4]))(?:$| — )").unwrap());

/// A date read from a heading, its year possibly still to be found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Day(Option<i32>, u32, u32),
    Month(Option<i32>, u32),
    Quarter(i32, u32),
    Half(i32, u32),
}

impl When {
    pub fn year(self) -> Option<i32> {
        match self {
            When::Day(y, ..) | When::Month(y, _) => y,
            When::Quarter(y, _) | When::Half(y, _) => Some(y),
        }
    }
    fn month(self) -> u32 {
        match self {
            When::Day(_, m, _) | When::Month(_, m) => m,
            When::Quarter(_, q) => 3 * q - 2,
            When::Half(_, h) => 6 * h - 5,
        }
    }
    /// With this year, if it has none; None if the day doesn't exist in it.
    pub fn in_year(self, year: i32) -> Option<Date> {
        match self {
            When::Day(y, m, d) => valid_day(y.unwrap_or(year), m, d),
            When::Month(y, m) => Some(Date::Month(y.unwrap_or(year), m)),
            When::Quarter(y, q) => Some(Date::Quarter(y, q)),
            When::Half(y, h) => Some(Date::Half(y, h)),
        }
    }
}

/// A dated heading: when, what's left of its words, and anything worth noting about how it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingDate {
    pub when: When,
    pub title: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// See also, Related, References.
    Closing,
    /// Current state, Summary, Overview, Status…
    Summing,
    /// The Timeline itself.
    Timeline,
    Dated(HeadingDate),
    Topical,
}

/// The heading's words, lower case, without trailing punctuation.
fn name(heading: &str) -> String {
    heading.trim().trim_end_matches([':', '.']).trim().to_lowercase()
}

/// Whether the heading is one of `names`, or starts with one and then a bracket or separator
/// (`Status (Apr 2026)`, `Status — Apr 2026`; not `Status of teams`): Some(true) for exactly,
/// Some(false) for with more.
fn named(heading: &str, names: &[&str]) -> Option<bool> {
    let n = name(heading);
    if names.contains(&n.as_str()) {
        return Some(true);
    }
    names.iter().any(|s| n.strip_prefix(s).is_some_and(|r| r.trim_start().starts_with(['(', '—', '–', '-', ':', ',']))).then_some(false)
}

/// A summing-up heading's words after its name, as an "As of …" line (`Status (Apr 2026)`:
/// `As of Apr 2026.`), when they say when.
fn as_of(heading: &str) -> Option<String> {
    let h = heading.trim();
    let n = SUMMING.iter().filter(|s| h.to_lowercase().starts_with(*s)).map(|s| s.len()).max()?;
    let rest = tidy(&h[n..].replace(['(', ')'], " "));
    let rest = rest.strip_prefix("as of ").or_else(|| rest.strip_prefix("As of ")).unwrap_or(&rest);
    (!rest.is_empty() && !dates_in(rest).is_empty()).then(|| format!("As of {}.", rest.trim_end_matches('.')))
}

pub fn classify(heading: &str) -> Kind {
    if named(heading, CLOSING) == Some(true) {
        return Kind::Closing;
    }
    if named(heading, SUMMING).is_some() {
        return Kind::Summing;
    }
    if name(heading) == "timeline" {
        return Kind::Timeline;
    }
    match read_date(heading) {
        Some(d) => Kind::Dated(d),
        None => Kind::Topical,
    }
}

const MON: &str = r"(jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?|tember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\b\.?";
const DAY: &str = r"\b(\d{1,2})(?:st|nd|rd|th)?";
const DASH: &str = r"\s*(?:[-–—]|to)\s*";
const PRE: &str = r"(?:\b(?:week of|week commencing|week beginning|w/c)\s+)?";

fn month_no(s: &str) -> u32 {
    let s = s.to_lowercase();
    ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
        .iter()
        .position(|m| s.starts_with(m))
        .map_or(0, |i| i as u32 + 1)
}

type Reader = fn(&regex::Captures) -> Option<When>;

/// Date forms, most specific first; a later form never matches inside an earlier one's match.
static FORMS: LazyLock<Vec<(Regex, Reader)>> = LazyLock::new(|| {
    let r = |s: String| Regex::new(&format!("(?i){s}")).unwrap();
    vec![
        // 2026-10-02
        (r(r"\b(\d{4})-(\d{2})-(\d{2})\b".into()), |c| {
            let (y, m, d) = (c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?);
            chrono::NaiveDate::from_ymd_opt(y, m, d).map(|_| When::Day(Some(y), m, d))
        }),
        // 2026-Q3, 2026 H1
        (r(r"\b(\d{4})[- ]?([qh])([1-4])\b".into()), |c| qh(&c[2], c[3].parse().ok()?, c[1].parse().ok()?)),
        // Q3 2026, H2 2025
        (r(r"\b([qh])([1-4])[\s-]*(\d{4})\b".into()), |c| qh(&c[1], c[2].parse().ok()?, c[3].parse().ok()?)),
        // 2026-10
        (r(r"\b(\d{4})-(\d{2})\b".into()), |c| {
            let m: u32 = c[2].parse().ok()?;
            let y = c[1].parse().ok()?;
            (1..=12).contains(&m).then_some(When::Month(Some(y), m))
        }),
        // Sep–Oct 2026: the first month, a year earlier when the range crosses one
        (r(format!(r"\b{MON}{DASH}{MON},?\s+(\d{{4}})\b")), |c| {
            let (m1, m2) = (month_no(&c[1]), month_no(&c[2]));
            let y: i32 = c[3].parse().ok()?;
            Some(When::Month(Some(if m1 > m2 { y - 1 } else { y }), m1))
        }),
        // Week of 29 Sep – 3 Oct 2026: the first day, a year earlier when the range crosses one
        (r(format!(r"{PRE}{DAY}\s+{MON}{DASH}{DAY}\s+{MON},?\s+(\d{{4}})\b")), |c| {
            let (m1, m2): (u32, u32) = (month_no(&c[2]), month_no(&c[4]));
            let y: i32 = c[5].parse().ok()?;
            when_day(Some(if m1 > m2 { y - 1 } else { y }), m1, c[1].parse().ok()?)
        }),
        // Week of 8–12 Jun 2026: the first day
        (r(format!(r"{PRE}{DAY}{DASH}(\d{{1,2}})\s+{MON},?\s+(\d{{4}})\b")), |c| {
            when_day(Some(c[4].parse().ok()?), month_no(&c[3]), c[1].parse().ok()?)
        }),
        // 2 Oct 2026, 02 October 2026
        (r(format!(r"{PRE}{DAY}\s+{MON},?\s+(\d{{4}})\b")), |c| when_day(Some(c[3].parse().ok()?), month_no(&c[2]), c[1].parse().ok()?)),
        // October 2, 2026
        (r(format!(r"{PRE}\b{MON}\s+(\d{{1,2}})(?:st|nd|rd|th)?,?\s+(\d{{4}})\b")), |c| {
            when_day(Some(c[3].parse().ok()?), month_no(&c[1]), c[2].parse().ok()?)
        }),
        // Oct 2026
        (r(format!(r"\b{MON},?\s+(\d{{4}})\b")), |c| Some(When::Month(Some(c[2].parse().ok()?), month_no(&c[1])))),
        // 3 Oct (a year to find)
        (r(format!(r"{PRE}{DAY}\s+{MON}")), |c| when_day(None, month_no(&c[2]), c[1].parse().ok()?)),
        // Oct — … at the start (a year to find)
        (r(format!(r"^{MON}\s*(?:[—–:]|-\s)")), |c| Some(When::Month(None, month_no(&c[1])))),
    ]
});

fn qh(letter: &str, n: u32, y: i32) -> Option<When> {
    match letter.to_ascii_lowercase().as_str() {
        "q" => Some(When::Quarter(y, n)),
        _ => (n <= 2).then_some(When::Half(y, n)),
    }
}

fn when_day(y: Option<i32>, m: u32, d: u32) -> Option<When> {
    (m > 0 && chrono::NaiveDate::from_ymd_opt(y.unwrap_or(2000), m, d).is_some()).then_some(When::Day(y, m, d))
}

#[derive(Debug, Clone, Copy)]
struct Found {
    start: usize,
    end: usize,
    when: When,
}

/// Every date in the heading, by position; none overlapping.
fn dates_in(h: &str) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for (re, read) in FORMS.iter() {
        for c in re.captures_iter(h) {
            let m = c.get(0).unwrap();
            if out.iter().any(|f| m.start() < f.end && f.start < m.end()) {
                continue;
            }
            if let Some(when) = read(&c) {
                out.push(Found { start: m.start(), end: m.end(), when });
            }
        }
    }
    out.sort_by_key(|f| f.start);
    out
}

const SEP: &[char] = &[' ', '—', '–', '-', ':', ',', '|', '·', ';'];

/// The date a section's heading gives, and the heading's other words. A date counts at the
/// start of the heading, at its end after a separator, or in a parenthesis that ends it; one in
/// the middle of the words (`Plans for Oct 2026 launch`) doesn't make the section dated.
pub fn read_date(heading: &str) -> Option<HeadingDate> {
    let h = heading.trim();
    let found = dates_in(h);
    let at_start = found.iter().copied().find(|f| f.start == 0);
    // A parenthesis that ends the heading and holds the date alone, or first before a comma:
    // `(3 Oct)`, `(3 Oct, via Lena)`; not `(25 Sep workstream update)`.
    let in_paren = found.iter().copied().find(|f| {
        let after = h[f.end..].trim_start();
        h[..f.start].trim_end().ends_with('(')
            && (after.starts_with(')') || after.starts_with([',', ';']))
            && after.trim_end().ends_with(')')
            && !after.contains('(')
    });
    let at_end =
        found.iter().copied().find(|f| h[f.end..].trim().is_empty() && f.start > 0 && h[..f.start].ends_with(|c: char| SEP.contains(&c)));
    let first = at_start.or(at_end).or(in_paren)?;
    let mut used = vec![first];
    let mut when = first.when;
    let mut note = None;
    if let Some(p) = in_paren.filter(|p| p.start != first.start) {
        match (when, p.when) {
            // Oct 2026 — … (30 Sep): the day, in the heading's year (or the next or last, across one).
            (When::Month(y, m), When::Day(py, pm, pd)) if py.is_none() || py == y => {
                let year = y.map(|y| roll(y, m, pm));
                if let Some(d) = when_day(py.or(year), pm, pd) {
                    if pm != m {
                        note = Some(format!("the heading says {} but the day is in another month; took the day", heading.trim()));
                    }
                    when = d;
                    used.push(p);
                }
            }
            // 3 Oct 2026 — … (3 Oct): the same day twice.
            (When::Day(_, m, d), When::Day(None, pm, pd)) if (pm, pd) == (m, d) => used.push(p),
            (When::Day(..), When::Day(..)) => note = Some("two days in the heading; took the first".into()),
            _ => {}
        }
    } else if first.when.year().is_none() {
        // (3 Oct) with an Oct 2026 elsewhere in the heading.
        if let Some(y) = found.iter().filter(|f| f.start != first.start).find_map(|f| f.when.year().map(|y| (y, f.when.month()))) {
            when = with_year(when, roll(y.0, y.1, when.month()));
        }
    }
    used.sort_by_key(|f| std::cmp::Reverse(f.start));
    let mut title = h.to_string();
    for f in used {
        title.replace_range(f.start..f.end, "");
    }
    Some(HeadingDate { when, title: tidy(&title), note })
}

/// The year of a day in month `day_m` written under a heading in month `m` of year `y`: the
/// year before or after when the months are more than half a year apart.
fn roll(y: i32, m: u32, day_m: u32) -> i32 {
    match day_m as i32 - m as i32 {
        n if n > 6 => y - 1,
        n if n < -6 => y + 1,
        _ => y,
    }
}

fn with_year(w: When, y: i32) -> When {
    match w {
        When::Day(None, m, d) => When::Day(Some(y), m, d),
        When::Month(None, m) => When::Month(Some(y), m),
        w => w,
    }
}

static EMPTY_PAREN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(\s*[,;:–—-]?\s*\)").unwrap());
static PAREN_OPEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(\s*[,;:–—-]\s*").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());

/// What's left of a heading once its date is out: no empty brackets, no separators at the ends.
fn tidy(s: &str) -> String {
    let s = EMPTY_PAREN.replace_all(s, "");
    let s = PAREN_OPEN.replace_all(&s, "(");
    let s = SPACES.replace_all(&s, " ");
    s.trim_matches(|c: char| SEP.contains(&c) || c.is_whitespace()).to_string()
}

/// What `reshape` did to a page, and whether it can be applied without the user.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    /// Why the page needs the user (empty: it can be reshaped by itself).
    pub reasons: Vec<String>,
    /// How something was read, worth a look but not stopping it.
    pub notes: Vec<String>,
    /// Heading lines rewritten, old and new.
    pub headings: Vec<(String, String)>,
    /// Lines added: the Timeline heading, entries' Source lines.
    pub added: Vec<String>,
    /// Lines left out: a merged See also's heading and links it repeated.
    pub removed: Vec<String>,
    /// Sources more than one entry cites (listed, not merged).
    pub repeated: Vec<String>,
    /// Timeline entries on the reshaped page.
    pub entries: usize,
    /// The checks the result failed; when there are any, the page is left as it was.
    pub broken: Vec<String>,
}

impl Report {
    /// Every section placed, every date sure, nothing for the user to decide.
    pub fn auto(&self) -> bool {
        self.reasons.is_empty() && self.broken.is_empty()
    }
}

/// One piece of the page as it will be written, with its place on the old page (None: new).
struct Unit {
    text: String,
    ord: Option<usize>,
}

type Closing<'a> = (String, Vec<&'a Section<'a>>, Option<usize>);

/// What an entry's heading says when it is read.
enum Head {
    /// Already `YYYY-MM-DD — …`.
    Iso(Date),
    Read(HeadingDate),
    /// A `###` in the Timeline with no date.
    Undated,
}

/// A Timeline entry on its way to the new page.
struct Entry {
    date: Option<Date>,
    unit: Unit,
    source: Option<String>,
}

static SOURCE_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Source: \[\[([^\[\]\n|#]+)(?:[|#][^\[\]\n]*)?\]\]\s*$").unwrap());

/// The page in the shape, and what was done. `is_source` says whether a link's target is a
/// vault note or source (not a wiki page), for an entry's Source line. The result is checked
/// (`check`); if it fails, the page comes back as it was with the failures in `broken`.
pub fn reshape(page: &str, is_source: &dyn Fn(&str) -> bool) -> (String, Report) {
    let (new, mut report) = reshape_unchecked(page, is_source);
    report.broken = check(page, &new, &report, is_source);
    if report.broken.is_empty() {
        (new, report)
    } else {
        (page.to_string(), report)
    }
}

/// Whether the page is in the shape: reshaping it would change nothing.
pub fn in_shape(page: &str, is_source: &dyn Fn(&str) -> bool) -> bool {
    reshape_unchecked(page, is_source).0 == page
}

fn reshape_unchecked(page: &str, is_source: &dyn Fn(&str) -> bool) -> (String, Report) {
    let p = parse(page);
    let mut r = Report::default();
    let mut ord = 0;
    let mut next = || {
        ord += 1;
        Some(ord)
    };
    let opening = Unit { text: p.opening.to_string(), ord: Some(0) };
    let mut summing: Vec<(&str, Unit)> = Vec::new();
    let mut topical: Vec<Unit> = Vec::new();
    // Closing sections by name: the sections, and the first one's place.
    let mut closing: Vec<Closing> = Vec::new();
    let mut timeline: Option<Unit> = None;
    // Dated sections (true) and Timeline entries (false), in the order they were on the page.
    let mut dated: Vec<(Head, Unit, bool)> = Vec::new();
    for s in &p.sections {
        match classify(s.heading) {
            Kind::Summing => summing.push((s.heading, Unit { text: s.text.to_string(), ord: next() })),
            Kind::Topical => topical.push(Unit { text: s.text.to_string(), ord: next() }),
            Kind::Closing => {
                let o = next();
                match closing.iter_mut().find(|c| c.0 == name(s.heading)) {
                    Some(c) => c.1.push(s),
                    None => closing.push((name(s.heading), vec![s], o)),
                }
            }
            Kind::Dated(d) => {
                let o = next();
                dated.push((Head::Read(d), Unit { text: s.text.to_string(), ord: o }, true));
            }
            Kind::Timeline => {
                let (head, entries) = timeline_parts(s.text);
                match timeline.as_mut() {
                    Some(t) => {
                        r.reasons.push("more than one Timeline section".into());
                        r.removed.push(s.heading_line().to_string());
                        t.text.push_str(head.split_once('\n').map_or("", |x| x.1));
                    }
                    None => timeline = Some(Unit { text: head.to_string(), ord: next() }),
                }
                if head.lines().skip(1).any(|l| !l.trim().is_empty()) {
                    r.reasons.push("the Timeline has text outside its entries".into());
                }
                for e in entries {
                    let o = next();
                    let h = e.lines().next().unwrap_or("");
                    let words = markdown::heading(h).map_or("", |x| x.1);
                    let unit = Unit { text: e.to_string(), ord: o };
                    let head = match (Date::parse_iso(words), read_date(words)) {
                        (Some(d), _) => Head::Iso(d),
                        (None, Some(d)) => Head::Read(d),
                        (None, None) => {
                            r.reasons.push(format!("a Timeline entry with no date: {h}"));
                            Head::Undated
                        }
                    };
                    dated.push((head, unit, false));
                }
            }
        }
    }

    // Two or more summing sections, each dated (Status (Mar 2026), Status (Apr 2026)): the newest
    // is the Current state, the others Timeline entries (D-20261006-14).
    if summing.len() > 1 {
        let dates: Vec<Option<HeadingDate>> = summing.iter().map(|(h, _)| as_of(h).and(read_date(h))).collect();
        let keys: Vec<Option<(i32, u32, i32)>> =
            dates.iter().map(|d| d.as_ref().and_then(|d| d.when.year().and_then(|y| d.when.in_year(y))).map(Date::key)).collect();
        if keys.iter().all(Option::is_some) {
            let newest = keys.iter().max().copied().flatten();
            if keys.iter().filter(|k| **k == newest).count() == 1 {
                let keep = keys.iter().position(|k| *k == newest).unwrap();
                let mut rest = Vec::new();
                for (i, (s, d)) in summing.drain(..).zip(dates).enumerate() {
                    if i == keep {
                        rest.push(s);
                    } else {
                        dated.push((Head::Read(d.unwrap()), s.1, true));
                    }
                }
                summing = rest;
            }
        }
    }

    // Settle each date, rewrite its heading and add its Source line. A missing year comes from
    // the sections around it only for a level-2 section, whose order is the author's; inside the
    // Timeline the order is the app's, so only the note it cites can give one.
    let known: Vec<Option<i32>> = dated
        .iter()
        .map(|d| match &d.0 {
            Head::Iso(date) => Some(date.year()),
            Head::Read(h) => h.when.year(),
            Head::Undated => None,
        })
        .collect();
    let mut entries = Vec::new();
    for (i, (head, unit, h2)) in dated.into_iter().enumerate() {
        let line = unit.text.lines().next().unwrap_or("").to_string();
        let (date, heading) = match head {
            Head::Iso(d) => (Some(d), None),
            Head::Undated => (None, None),
            Head::Read(h) => {
                r.notes.extend(h.note);
                let around = if h2 { &known[..] } else { &[] };
                match settle(h.when, i, around, &unit.text, is_source) {
                    Ok(d) if h.title.is_empty() => (Some(d), Some(format!("### {}", d.iso()))),
                    Ok(d) => (Some(d), Some(format!("### {} — {}", d.iso(), h.title))),
                    Err(why) => {
                        r.reasons.push(format!("{why}: {line}"));
                        (None, h2.then(|| format!("#{line}")))
                    }
                }
            }
        };
        let (text, source) = entry_text(&unit.text, heading, h2, is_source, &mut r);
        entries.push(Entry { date, unit: Unit { text, ord: unit.ord }, source });
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.date.map_or((i32::MIN, 0, 0), Date::key)));
    let entries = merge_same_source(entries, &mut r);
    let mut cited: HashMap<String, usize> = HashMap::new();
    for e in &entries {
        if let Some(s) = &e.source {
            *cited.entry(crate::links::key(s)).or_default() += 1;
        }
    }
    for e in &entries {
        if let Some(s) = e.source.as_ref().filter(|s| cited.get(&crate::links::key(s)).is_some_and(|&n| n > 1)) {
            if !r.repeated.contains(s) {
                r.repeated.push(s.clone());
                r.reasons.push(format!("more than one entry cites [[{s}]]"));
            }
        }
    }
    r.entries = entries.len();

    // One summing section becomes Current state.
    if summing.len() == 1 {
        let (h, u) = &mut summing[0];
        let old = u.text.lines().next().unwrap_or("").to_string();
        if named(h, SUMMING) == Some(false) {
            // Status (Apr 2026): Current state, its date the first line under it.
            match as_of(h) {
                Some(line) => {
                    let new = format!("## {CURRENT_STATE}");
                    u.text = format!("{new}\n\n{line}\n{}", u.text[old.len()..].strip_prefix('\n').unwrap_or(&u.text[old.len()..]));
                    r.headings.push((old, new));
                    r.added.push(line);
                }
                None => r.reasons.push(format!("the summing-up heading has more words than its name: {old}")),
            }
        } else if old != format!("## {CURRENT_STATE}") {
            let new = format!("## {CURRENT_STATE}");
            u.text = format!("{new}{}", &u.text[old.len()..]);
            r.headings.push((old, new));
        }
    } else if summing.len() > 1 {
        r.reasons.push(format!("{} summing-up sections: {}", summing.len(), summing.iter().map(|s| s.0).collect::<Vec<_>>().join(", ")));
    }

    // Closing sections: one of each name, links not repeated.
    if closing.len() > 1 {
        r.reasons.push(format!(
            "closing sections with different names: {}",
            closing.iter().map(|c| c.1[0].heading).collect::<Vec<_>>().join(", ")
        ));
    }
    let closing: Vec<Unit> = closing
        .into_iter()
        .map(|(_, ss, o)| Unit { text: merge_closing(&ss, &mut r), ord: if ss.len() == 1 { o } else { None } })
        .collect();

    let mut units = vec![opening];
    units.extend(summing.into_iter().map(|s| s.1));
    units.extend(topical);
    if timeline.is_some() || !entries.is_empty() {
        units.push(timeline.unwrap_or_else(|| {
            r.added.push(format!("## {TIMELINE}"));
            Unit { text: format!("## {TIMELINE}\n\n"), ord: None }
        }));
        units.extend(entries.into_iter().map(|e| e.unit));
    }
    units.extend(closing);
    (with_tail(&assemble(p.front, &units), page), r)
}

/// The page's units after its frontmatter, in order: each as it was, with a blank line put
/// before one that didn't follow the one before it on the old page.
fn assemble(front: &str, units: &[Unit]) -> String {
    let mut out = front.to_string();
    let mut last: Option<usize> = None;
    for (k, u) in units.iter().enumerate() {
        if u.text.is_empty() {
            continue;
        }
        let follows = k > 0 && last.is_some() && u.ord.is_some() && u.ord == last.map(|l| l + 1);
        if out.len() > front.len() {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            if !follows && !ends_blank(&out) {
                out.push('\n');
            }
        }
        out.push_str(&u.text);
        last = u.ord;
    }
    out
}

/// Entries from one source on the same date as one (D-20261006-14): the first keeps its heading,
/// each later one follows it under its own title as a `####` heading (its headings a level down),
/// without its Source line. Entries come sorted, so those of one date are next to each other.
fn merge_same_source(entries: Vec<Entry>, r: &mut Report) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for e in entries {
        let key = e.source.as_deref().map(crate::links::key);
        let into = out
            .iter_mut()
            .rev()
            .take_while(|o| o.date == e.date)
            .find(|o| key.is_some() && o.source.as_deref().map(crate::links::key) == key);
        let (Some(into), Some(_)) = (into, e.date) else {
            out.push(e);
            continue;
        };
        let lines = markdown::lines(&e.unit.text, 0);
        let mut text = String::new();
        let mut source_dropped = false;
        for (k, raw) in e.unit.text.split_inclusive('\n').enumerate() {
            let l = &lines[k];
            let ending = &raw[l.text.len()..];
            let ending = if ending.is_empty() { "\n" } else { ending };
            let new = if k == 0 {
                let title = markdown::heading(l.text).map_or("", |h| h.1);
                let title = title.split_once(" — ").map_or(title, |x| x.1).to_string();
                let title = if Date::parse_iso(&title).is_some() || title.is_empty() { "More from the same source".into() } else { title };
                Some(format!("#### {title}"))
            } else if !source_dropped && SOURCE_LINE.is_match(l.text) {
                source_dropped = true;
                // A Source line this reshape added isn't added; one already there is left out.
                match r.added.iter().position(|a| a == l.text) {
                    Some(i) => {
                        r.added.remove(i);
                    }
                    None => r.removed.push(l.text.to_string()),
                }
                continue;
            } else {
                markdown::heading(l.text).filter(|(n, _)| !l.code && *n >= 4 && *n < 6).map(|_| format!("#{}", l.text.trim_start()))
            };
            match new {
                Some(n) => {
                    // The heading as it was on the page, not as this reshape first rewrote it.
                    match r.headings.iter().position(|(_, w)| w == l.text) {
                        Some(i) => r.headings[i].1 = n.clone(),
                        None => r.headings.push((l.text.to_string(), n.clone())),
                    }
                    text.push_str(&n);
                    text.push_str(ending);
                }
                None => text.push_str(raw),
            }
        }
        if !into.unit.text.ends_with('\n') {
            into.unit.text.push('\n');
        }
        if !ends_blank(&into.unit.text) {
            into.unit.text.push('\n');
        }
        into.unit.text.push_str(&text);
        into.unit.ord = None;
    }
    out
}

/// The Timeline section cut into its heading (with any text before the first entry) and its `###` entries.
fn timeline_parts(text: &str) -> (&str, Vec<&str>) {
    let mut cuts = Vec::new();
    for l in markdown::lines(text, 0).into_iter().skip(1) {
        if !l.code && matches!(markdown::heading(l.text), Some((3, _))) {
            cuts.push(l.start);
        }
    }
    let first = cuts.first().copied().unwrap_or(text.len());
    let entries = cuts.iter().enumerate().map(|(i, &a)| &text[a..cuts.get(i + 1).copied().unwrap_or(text.len())]).collect();
    (&text[..first], entries)
}

/// The year for a date whose heading has none: from the dated sections around it when they
/// agree, or from the note it cites when that is dated in the same month.
fn settle(when: When, i: usize, known: &[Option<i32>], text: &str, is_source: &dyn Fn(&str) -> bool) -> Result<Date, String> {
    if let Some(y) = when.year() {
        return when.in_year(y).ok_or_else(|| "a day that doesn't exist".to_string());
    }
    let prev = known.get(..i).unwrap_or_default().iter().rev().find_map(|y| *y);
    let next = known.get(i + 1..).unwrap_or_default().iter().find_map(|y| *y);
    let around: Vec<i32> = [prev, next].into_iter().flatten().collect();
    let cited = first_source(text, is_source)
        .and_then(|s| crate::ingest::source_day(&s))
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .filter(|d| chrono::Datelike::month(d) == when.month())
        .map(|d| chrono::Datelike::year(&d));
    let year = match (cited, around.as_slice()) {
        (Some(y), a) if a.is_empty() || a.contains(&y) => y,
        (Some(_), _) => return Err("no year, and the note it cites and the sections around it disagree".into()),
        (None, [a]) => *a,
        (None, [a, b]) if a == b => *a,
        (None, []) => return Err("no year in the heading".into()),
        (None, _) => return Err("no year, and the sections around it are in different years".into()),
    };
    when.in_year(year).ok_or_else(|| "a day that doesn't exist".to_string())
}

/// The first link in the text to a vault note or source.
fn first_source(text: &str, is_source: &dyn Fn(&str) -> bool) -> Option<String> {
    let lines = markdown::lines(text, 0);
    crate::links::parse_links(&lines[1.min(lines.len())..])
        .into_iter()
        .find(|l| l.kind == crate::links::LinkKind::Link && !l.target.is_empty() && is_source(&l.target))
        .map(|l| l.target)
}

/// An entry's text: its heading rewritten (`heading`), headings under it a level down when it
/// was a level-2 section, and a Source line under the heading when it has none and cites one.
fn entry_text(
    text: &str,
    heading: Option<String>,
    demote: bool,
    is_source: &dyn Fn(&str) -> bool,
    r: &mut Report,
) -> (String, Option<String>) {
    let lines = markdown::lines(text, 0);
    let first_text = lines.iter().skip(1).find(|l| !l.text.trim().is_empty()).map(|l| l.text);
    let existing = first_text.and_then(|t| SOURCE_LINE.captures(t)).map(|c| c[1].trim().to_string());
    let source = existing.clone().or_else(|| first_source(text, is_source));
    let mut out = String::new();
    for (k, raw) in text.split_inclusive('\n').enumerate() {
        let l = &lines[k];
        let ending = &raw[l.text.len()..];
        if k == 0 {
            match &heading {
                Some(h) if h != l.text => {
                    r.headings.push((l.text.to_string(), h.clone()));
                    out.push_str(h);
                }
                _ => out.push_str(l.text),
            }
            out.push_str(if ending.is_empty() { "\n" } else { ending });
            if existing.is_none() {
                if let Some(s) = &source {
                    let line = format!("Source: [[{s}]]");
                    out.push_str(&line);
                    out.push('\n');
                    r.added.push(line);
                    if lines.get(1).is_some_and(|n| !n.text.trim().is_empty()) {
                        out.push('\n');
                    }
                }
            }
            continue;
        }
        match markdown::heading(l.text).filter(|(n, _)| demote && !l.code && *n >= 3 && *n < 6) {
            Some(_) => {
                let new = format!("#{}", l.text.trim_start());
                r.headings.push((l.text.to_string(), new.clone()));
                out.push_str(&new);
                out.push_str(ending);
            }
            None => out.push_str(raw),
        }
    }
    (out, source)
}

/// Closing sections of one name as one: the first, then the others' lines, without the list
/// items (by link, else by text) the ones before already have.
fn merge_closing(ss: &[&Section], r: &mut Report) -> String {
    if ss.len() == 1 {
        return ss[0].text.to_string();
    }
    let item_key = |l: &str| {
        let t = l.trim();
        let item = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ "))?;
        let links = crate::links::parse_links(&markdown::lines(item, 0));
        Some(links.first().map_or(item.to_lowercase(), |k| crate::links::key(&k.target)))
    };
    let base = ss[0].text;
    let keep = base.trim_end_matches(|c: char| c.is_whitespace());
    let mut seen: Vec<String> = base.lines().filter_map(item_key).collect();
    let mut out = format!("{keep}\n");
    for s in &ss[1..] {
        r.removed.push(s.heading_line().to_string());
        for l in s.body().lines().skip_while(|l| l.trim().is_empty()) {
            match item_key(l) {
                Some(k) if seen.contains(&k) => r.removed.push(l.to_string()),
                Some(k) => {
                    seen.push(k);
                    out.push_str(l);
                    out.push('\n');
                }
                None => {
                    if !l.trim().is_empty() {
                        r.reasons.push(format!("a second {} has text that isn't a list", ss[0].heading));
                    }
                    out.push_str(l);
                    out.push('\n');
                }
            }
        }
        while out.ends_with("\n\n") {
            out.pop();
        }
    }
    out.push_str(base[keep.len()..].trim_start_matches(|c: char| c != '\n').get(1..).unwrap_or(""));
    out
}

fn ends_blank(s: &str) -> bool {
    s.strip_suffix('\n').is_some_and(|s| s.rsplit('\n').next().is_some_and(|l| l.trim().is_empty()))
}

/// `out` ending the way `page` ends: the same blank lines (or none) after its last line.
fn with_tail(out: &str, page: &str) -> String {
    let eol = |s: &str| {
        let end = s.trim_end().len();
        s[end..].find('\n').map_or(s.len(), |i| end + i)
    };
    format!("{}{}", &out[..eol(out)], &page[eol(page)..])
}

/// The invariants a reshaped page must keep (what was checked by hand on 6 Oct): every
/// non-blank line still there as many times, except what `report` says was rewritten, added or
/// left out; no longer run of blank lines; reshaping it again changes nothing; the frontmatter
/// byte for byte the same. The failures, if any.
pub fn check(old: &str, new: &str, report: &Report, is_source: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut out = kept_lines(old, new, report);
    if reshape_unchecked(new, is_source).0 != new {
        out.push("reshaping it again changes it".into());
    }
    out
}

/// `check` without the second reshape: the frontmatter, every non-blank line and the longest run
/// of blank lines kept.
fn kept_lines(old: &str, new: &str, report: &Report) -> Vec<String> {
    let mut out = Vec::new();
    let (fo, fnew) = (crate::frontmatter::split(old).body_start, crate::frontmatter::split(new).body_start);
    if old[..fo] != new[..fnew] {
        out.push("the frontmatter changed".into());
    }
    let mut count: HashMap<&str, i64> = HashMap::new();
    for l in old[fo..].lines().filter(|l| !l.trim().is_empty()) {
        *count.entry(l).or_default() += 1;
    }
    for (o, n) in &report.headings {
        *count.entry(o).or_default() -= 1;
        *count.entry(n).or_default() += 1;
    }
    for l in &report.removed {
        *count.entry(l).or_default() -= 1;
    }
    for l in &report.added {
        *count.entry(l).or_default() += 1;
    }
    for l in new[fnew..].lines().filter(|l| !l.trim().is_empty()) {
        *count.entry(l).or_default() -= 1;
    }
    let mut off: Vec<_> = count.into_iter().filter(|(_, n)| *n != 0).collect();
    off.sort();
    for (l, n) in off.into_iter().take(5) {
        out.push(if n > 0 { format!("a line went missing: {l}") } else { format!("a line appeared: {l}") });
    }
    let run =
        |s: &str| s.lines().fold((0, 0), |(best, cur), l| if l.trim().is_empty() { (best.max(cur + 1), cur + 1) } else { (best, 0) }).0;
    if run(&new[fnew..]) > run(&old[fo..]).max(1) {
        out.push("a longer run of blank lines".into());
    }
    out
}

/// The wiki pages that have the shape: entities and concepts (summaries are left as they are).
pub fn shaped(rel: &str) -> bool {
    (rel.starts_with("wiki/entities/") || rel.starts_with("wiki/concepts/")) && rel.ends_with(".md")
}

/// One page's dry run: what reshaping it would do.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResult {
    pub path: String,
    /// Already in the shape: nothing to do.
    pub in_shape: bool,
    pub auto: bool,
    #[serde(flatten)]
    pub report: Report,
}

/// Every entity and concept page of the vault at `root`, reshaped in memory: the result for
/// each, and the reshaped text of those it would change. Nothing is written.
pub fn survey(root: &std::path::Path) -> Vec<(PageResult, Option<String>)> {
    let files = vault_files(root);
    let sources = Sources::from_paths(files.iter().map(String::as_str));
    let mut pages: Vec<&String> = files.iter().filter(|f| shaped(f)).collect();
    pages.sort();
    pages
        .into_iter()
        .filter_map(|p| {
            let text = std::fs::read(root.join(p)).ok().map(|b| String::from_utf8_lossy(&b).into_owned())?;
            let (result, new) = one(p, &text, &sources);
            Some((result, new))
        })
        .collect()
}

/// Every Tracker note of the vault at `root`, reshaped in memory to the tracker shape, as
/// `survey` does the wiki's pages. Kept apart from `survey`, so Knowledge health and Reshape
/// pages' run over every page leave notes alone; they're reshaped when named.
pub fn survey_trackers(root: &std::path::Path) -> Vec<(PageResult, Option<String>)> {
    let mut pages: Vec<String> = vault_files(root).into_iter().filter(|f| is_tracker(f)).collect();
    pages.sort();
    let none = Sources::from_paths(std::iter::empty());
    pages
        .iter()
        .filter_map(|p| {
            let text = std::fs::read(root.join(p)).ok().map(|b| String::from_utf8_lossy(&b).into_owned())?;
            Some(one(p, &text, &none))
        })
        .collect()
}

/// The vault's files, vault-relative, outside dot folders.
pub fn vault_files(root: &std::path::Path) -> Vec<String> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| crate::vault::rel_of(root, e.path()))
        .collect()
}

/// One page reshaped: its result, and its new text when it changes.
pub fn one(path: &str, text: &str, sources: &Sources) -> (PageResult, Option<String>) {
    if is_tracker(path) {
        let (new, report) = reshape_tracker(text);
        // A Log it can't sort leaves the text as it was, but the note isn't in the shape.
        let in_shape = new == text && report.broken.is_empty() && report.reasons.is_empty();
        let auto = report.auto();
        return (PageResult { path: path.to_string(), in_shape, auto, report }, (new != text).then_some(new));
    }
    let (new, report) = reshape(text, &|t| sources.is_source(t));
    let in_shape = new == text && report.broken.is_empty();
    let auto = report.auto();
    (PageResult { path: path.to_string(), in_shape, auto, report }, (new != text).then_some(new))
}

/// The Knowledge health item for a page not in the shape: safe when Reshape pages can do it by
/// itself; else why it needs the user.
pub fn item(path: &str, text: &str, sources: &Sources) -> Option<crate::lint::Item> {
    let (r, _) = one(path, text, sources);
    if r.in_shape {
        return None;
    }
    let detail = if !r.report.broken.is_empty() {
        format!("Can't be reshaped: {}", r.report.broken.join("; "))
    } else if r.auto {
        "Reshapes by itself".to_string()
    } else {
        r.report.reasons.join("; ")
    };
    Some(crate::lint::Item {
        text: format!("{path}: {detail}"),
        page: Some(path.to_string()),
        count: Some(r.report.entries as i64),
        detail: Some(detail),
        safe: r.auto,
        ..Default::default()
    })
}

/// Whether the page has a Current state section.
pub fn has_current_state(page: &str) -> bool {
    parse(page).sections.iter().any(|s| name(s.heading) == "current state")
}

/// A page with a Timeline (something to say how things stand) but no Current state.
pub fn wants_current_state(page: &str) -> bool {
    let p = parse(page);
    !p.sections.iter().any(|s| name(s.heading) == "current state") && p.sections.iter().any(|s| classify(s.heading) == Kind::Timeline)
}

/// What a model is shown to write a page's Current state from (D-20261006-10): the page's
/// opening text and its newest Timeline entries, within `budget` characters, and nothing else.
pub fn current_state_prompt(path: &str, page: &str, budget: usize) -> String {
    let p = parse(page);
    let mut text = p.opening.trim().to_string();
    if let Some(t) = p.sections.iter().find(|s| classify(s.heading) == Kind::Timeline) {
        for e in timeline_parts(t.text).1 {
            if text.chars().count() + e.chars().count() > budget {
                break;
            }
            text.push_str("\n\n");
            text.push_str(e.trim());
        }
    }
    let title = crate::filename::stem(path);
    format!(
        "Write the Current state section of the wiki page \"{title}\": what is true now, from the page's opening and its newest timeline entries below (newest first).\n\n\
         - At most 120 words: two to six short bullet points, or a short paragraph.\n\
         - Only facts in the text below. Say when for dated facts (\"From 28 September, …\", \"As of the 2 October steerco, …\").\n\
         - Cite with [[links]] copied exactly from the text below; add no other links.\n\
         - Answer with the section's text only: no heading, nothing before or after it.\n\
         - If the text says nothing about how things stand now, answer NONE.\n\n\
         ---\n{text}\n---\n"
    )
}

/// A model's Current state, checked against the page: its text, or why it can't be used. A link
/// to a page neither the page links nor `exists` knows (the vault's notes and pages) is made
/// plain text, its words kept, so the run makes no links to pages that don't exist.
pub fn current_state_answer(page: &str, answer: &str, exists: &dyn Fn(&str) -> bool) -> Result<String, String> {
    let mut t = answer.trim();
    if let Some(inner) = t.strip_prefix("```").and_then(|x| x.rsplit_once("```")).map(|x| x.0) {
        t = inner.split_once('\n').map_or("", |x| x.1).trim();
    }
    let t = t.strip_prefix("## Current state").unwrap_or(t).trim();
    if t.is_empty() || t.eq_ignore_ascii_case("none") {
        return Err("the model found nothing about how things stand now".into());
    }
    if t.chars().count() > 1500 {
        return Err("the answer was too long for a Current state".into());
    }
    if t.lines().any(|l| markdown::heading(l).is_some()) {
        return Err("the answer had headings in it".into());
    }
    let have: std::collections::HashSet<String> =
        crate::links::parse_links(&markdown::lines(page, 0)).into_iter().map(|l| crate::links::key(&l.target)).collect();
    static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\[\]\n|#]+)(#[^\[\]\n|]*)?(?:\|([^\[\]\n]+))?\]\]").unwrap());
    let t = LINK.replace_all(t, |c: &regex::Captures| {
        let target = c[1].trim();
        if target.is_empty() || have.contains(&crate::links::key(target)) || exists(target) {
            c[0].to_string()
        } else {
            c.get(3).map_or(target, |a| a.as_str().trim()).to_string()
        }
    });
    Ok(t.into_owned())
}

/// The page with a Current state section of `text` where the shape puts it: after the opening.
pub fn with_current_state(page: &str, text: &str) -> String {
    let p = parse(page);
    let at = p.front.len() + p.opening.len();
    let mut out = page[..at].to_string();
    if !p.opening.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if !ends_blank(&out) {
            out.push('\n');
        }
    }
    out.push_str(&format!("## {CURRENT_STATE}\n\n{}\n", text.trim()));
    if at < page.len() {
        out.push('\n');
        out.push_str(&page[at..]);
    }
    out
}

/// A Timeline entry: its heading, its Source line and its text, any headings in the text put
/// below the entry's (`####` or deeper), and Source lines in the text left out (they're the app's).
pub fn entry(date: Date, title: &str, source: &str, body: &str) -> String {
    let title = title.trim();
    let head = if title.is_empty() { format!("### {}", date.iso()) } else { format!("### {} — {title}", date.iso()) };
    let lines = markdown::lines(body.trim(), 0);
    let mut text = Vec::new();
    for l in lines.iter().filter(|l| l.code || !SOURCE_LINE.is_match(l.text.trim())).skip_while(|l| l.text.trim().is_empty()) {
        match markdown::heading(l.text).filter(|(n, _)| !l.code && *n < 4) {
            Some((_, t)) => text.push(format!("#### {t}")),
            None => text.push(l.text.to_string()),
        }
    }
    let body = text.join("\n");
    format!("{head}\nSource: [[{source}]]\n\n{}\n", body.trim_end())
}

/// Where a section's lines end on the page: after its last line that isn't blank.
fn content_end(page: &str, start: usize, end: usize) -> usize {
    let t = page[start..end].trim_end();
    start + t.len() + page[start + t.len()..end].find('\n').map_or(page[start + t.len()..end].len(), |i| i + 1)
}

/// `text` (whole sections, ending in a newline) put in the page above its first section of the
/// kinds in `before`, else at the end.
fn insert_sections(page: &str, text: &str, before: &[Kind]) -> String {
    let p = parse(page);
    let at = p.sections.iter().find(|s| before.contains(&classify(s.heading))).map(|s| s.text.as_ptr() as usize - page.as_ptr() as usize);
    match at {
        Some(at) => format!("{}{text}\n{}", &page[..at], &page[at..]),
        None => {
            let end = content_end(page, 0, page.len());
            let sep = if page[..end].ends_with('\n') { "\n" } else { "\n\n" };
            format!("{}{sep}{text}{}", &page[..end], &page[end..])
        }
    }
}

/// The page with a Timeline entry for `source` (D-20261006-06, -11): the entry that cites it
/// already is taken out and the new one put in by date, newest first, after any of the same date;
/// a page with no Timeline gets one, above See also. True when it replaced an entry.
pub fn with_entry(page: &str, date: Date, title: &str, source: &str, body: &str) -> (String, bool) {
    let new = entry(date, title, source, body);
    let key = crate::links::key(source);
    let p = parse(page);
    let Some(t) = p.sections.iter().find(|s| classify(s.heading) == Kind::Timeline) else {
        return (insert_sections(page, &format!("## {TIMELINE}\n\n{new}"), &[Kind::Closing]), false);
    };
    let (_, entries) = timeline_parts(t.text);
    let at = |e: &str| e.as_ptr() as usize - page.as_ptr() as usize;
    let cites = |e: &str| {
        e.lines()
            .skip(1)
            .find(|l| !l.trim().is_empty())
            .and_then(|l| SOURCE_LINE.captures(l.trim()))
            .is_some_and(|c| crate::links::key(&c[1]) == key)
    };
    // Take out the entry it replaces, lines and the blank lines after it.
    let (mut page, mut replaced) = (page.to_string(), false);
    if let Some(old) = entries.iter().find(|e| cites(e)) {
        let a = at(old);
        page.replace_range(a..a + old.len(), "");
        replaced = true;
    }
    let p2 = parse(&page);
    let t2 = p2.sections.iter().find(|s| classify(s.heading) == Kind::Timeline).unwrap();
    let (_, entries) = timeline_parts(t2.text);
    let at2 = |e: &str| e.as_ptr() as usize - page.as_ptr() as usize;
    let place = entries.iter().find(|e| {
        let h = e.lines().next().and_then(markdown::heading).map_or("", |h| h.1);
        Date::parse_iso(h).is_some_and(|d| d.key() < date.key())
    });
    let out = match place {
        Some(e) => {
            let a = at2(e);
            format!("{}{new}\n{}", &page[..a], &page[a..])
        }
        None => {
            let start = t2.text.as_ptr() as usize - page.as_ptr() as usize;
            let end = content_end(&page, start, start + t2.text.len());
            let rest = &page[end..];
            let after = if rest.is_empty() || rest.starts_with('\n') { "" } else { "\n" };
            format!("{}\n{new}{after}{rest}", &page[..end])
        }
    };
    (out, replaced)
}

/// The page with a topical section `heading` of `content`: the section's text replaced when the
/// page has it, else a new section above the Timeline (or See also). Err for a heading the shape
/// keeps for itself.
pub fn with_topic(page: &str, heading: &str, content: &str) -> Result<String, String> {
    let h = heading.trim().trim_start_matches('#').trim();
    match classify(h) {
        Kind::Topical => {}
        _ => return Err(format!("“{h}” isn't a topical section: use current_state or entry for it.")),
    }
    if parse(page).sections.iter().any(|s| name(s.heading) == name(h)) {
        return crate::proposals::patched(page, &crate::proposals::Patch::Section { section: h.to_string(), content: content.to_string() });
    }
    Ok(insert_sections(page, &format!("## {h}\n\n{}\n", content.trim()), &[Kind::Timeline, Kind::Closing]))
}

/// A section's new text, as an edit gives it (`edit_page`, a change's section): on an entity or
/// concept page a heading it doesn't have goes where the shape puts it, a topical one above the
/// Timeline and a summing one (Summary, Status…) as Current state after the opening; on a
/// Tracker note one of the tracker shape's sections goes where that shape puts it; anything else
/// as `proposals::patched` does it.
pub fn with_section(rel: &str, page: &str, heading: &str, content: &str) -> Result<String, String> {
    let page = page.replace("\r\n", "\n");
    let h = heading.trim().trim_start_matches('#').trim();
    let has =
        markdown::lines(&page, 0).iter().any(|l| !l.code && markdown::heading(l.text).is_some_and(|(_, t)| t.eq_ignore_ascii_case(h)));
    if is_tracker(rel) && !has {
        if let Some(k) = tracker_slot(h) {
            return Ok(tracker_insert(&page, k, content));
        }
    }
    if shaped(rel) && !has {
        match classify(h) {
            Kind::Topical => return with_topic(&page, h, content),
            Kind::Summing => return with_current_state_text(&page, content),
            _ => {}
        }
    }
    crate::proposals::patched(&page, &crate::proposals::Patch::Section { section: h.to_string(), content: content.to_string() })
}

/// The daily check's line on the page shape: how many pages aren't in it, and how many of those
/// Reshape pages can do by itself.
pub fn drift_line(results: &[PageResult]) -> String {
    let out: Vec<&PageResult> = results.iter().filter(|r| !r.in_shape).collect();
    if out.is_empty() {
        return "every wiki page in the page shape".into();
    }
    let auto = out.iter().filter(|r| r.auto && r.report.broken.is_empty()).count();
    let n = out.len();
    format!(
        "{n} wiki page{} not in the page shape ({auto} Reshape pages can do by itself, in Knowledge health)",
        if n == 1 { "" } else { "s" }
    )
}

/// The page with its Current state's text `text`: replaced when it has one, else put after the opening.
pub fn with_current_state_text(page: &str, text: &str) -> Result<String, String> {
    if has_current_state(page) {
        crate::proposals::patched(page, &crate::proposals::Patch::Section { section: CURRENT_STATE.into(), content: text.to_string() })
    } else {
        Ok(with_current_state(page, text))
    }
}

/// The sections a Tracker note has, in this order, after its opening (the H1, the
/// `Project: [[…]]` line and any text before the first section). Sections not in the list stay,
/// in their order, below these.
pub const TRACKER_SECTIONS: [&str; 4] = [CURRENT_STATE, "Blockers", "Next", "Log"];
/// Where the Log is in `TRACKER_SECTIONS`.
const LOG_SLOT: usize = 3;

/// Whether a vault path is a Tracker note: `Tracker. <title>.md` among the notes (not in the
/// wiki, the sources or the templates).
pub fn is_tracker(rel: &str) -> bool {
    let top = if rel.contains('/') { rel.split('/').next().unwrap_or("") } else { "" };
    !matches!(top, "wiki" | "sources" | "Templates" | "templates")
        && rel.ends_with(".md")
        && crate::filename::parse(rel).kind.as_deref() == Some("Tracker")
}

/// The pages with a shape: wiki entity and concept pages, and Tracker notes.
pub fn has_shape(rel: &str) -> bool {
    shaped(rel) || is_tracker(rel)
}

/// Which of `TRACKER_SECTIONS` a heading is, in any case.
fn tracker_slot(heading: &str) -> Option<usize> {
    let n = name(heading);
    TRACKER_SECTIONS.iter().position(|s| s.to_lowercase() == n)
}

/// A Log entry's date: a top-level bullet that starts with `YYYY-MM-DD` (bold or not).
static LOG_DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[-*+][ \t]+(?:\*\*|__)?(\d{4}-\d{2}-\d{2})\b").unwrap());

/// A top-level list item's first line: `- `, `* ` or `+ ` with no indent.
fn top_bullet(l: &str) -> bool {
    l.len() > 1 && l.starts_with(['-', '*', '+']) && l[1..].starts_with([' ', '\t'])
}

/// How many blank lines end a run of lines.
fn blank_after(ls: &[&str]) -> usize {
    ls.iter().rev().take_while(|l| l.trim().is_empty()).count()
}

/// The Log section's text with its entries newest first. An entry is a top-level bullet and the
/// indented lines under it; text above the first entry stays above. A Log with text between its
/// entries, or with an entry that has no date, is left as it is and the reason given. Entries of
/// one date keep their order, and the entries keep the spacing the first two had (a blank line
/// between them or none).
fn sorted_log(text: &str, r: &mut Report) -> String {
    let Some((head, body)) = text.split_once('\n') else { return text.to_string() };
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let Some(first) = lines.iter().position(|l| top_bullet(l)) else { return text.to_string() };
    let mut items: Vec<Vec<&str>> = Vec::new();
    for &l in &lines[first..] {
        if top_bullet(l) {
            items.push(vec![l]);
        } else if l.trim().is_empty() || l.starts_with([' ', '\t']) {
            if let Some(i) = items.last_mut() {
                i.push(l);
            }
        } else {
            r.reasons.push("the Log has text between its entries".into());
            return text.to_string();
        }
    }
    r.entries = items.len();
    let dates: Vec<Option<&str>> = items.iter().map(|i| LOG_DATE.captures(i[0]).and_then(|c| c.get(1)).map(|m| m.as_str())).collect();
    let undated = dates.iter().filter(|d| d.is_none()).count();
    if undated > 0 {
        r.reasons.push(format!("{undated} Log entr{} with no date (- YYYY-MM-DD …)", if undated == 1 { "y" } else { "ies" }));
        return text.to_string();
    }
    let dates: Vec<&str> = dates.into_iter().flatten().collect();
    if dates.windows(2).all(|w| w[0] >= w[1]) {
        return text.to_string();
    }
    // Each entry without the blank lines after it, then the first two entries' spacing between
    // every two.
    let loose = blank_after(&items[0]) > 0;
    let tail: String = items.last().map(|i| i[i.len() - blank_after(i)..].concat()).unwrap_or_default();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| dates[b].cmp(dates[a]));
    let mut out = format!("{head}\n{}", lines[..first].concat());
    for (k, &i) in order.iter().enumerate() {
        let item = &items[i];
        let mut t = item[..item.len() - blank_after(item)].concat();
        if !t.ends_with('\n') {
            t.push('\n');
        }
        out.push_str(&t);
        if loose && k + 1 < order.len() {
            out.push('\n');
        }
    }
    out.push_str(&tail);
    out
}

/// A Tracker note in the tracker shape, and what was done: Current state, Blockers, Next and Log
/// after the opening (each added, empty, when missing), the Log's entries newest first, and every
/// other section kept below them in its order. The result is checked (`check`'s line counts, and
/// reshaping it again changes nothing); if it fails, the note comes back as it was with the
/// failures in `broken`. A note with Windows line endings keeps them.
pub fn reshape_tracker(page: &str) -> (String, Report) {
    if page.contains("\r\n") {
        let lf = page.replace("\r\n", "\n");
        let (new, report) = reshape_tracker(&lf);
        return (if new == lf { page.to_string() } else { new.replace('\n', "\r\n") }, report);
    }
    let (new, mut report) = reshape_tracker_unchecked(page);
    report.broken = kept_lines(page, &new, &report);
    if reshape_tracker_unchecked(&new).0 != new {
        report.broken.push("reshaping it again changes it".into());
    }
    if report.broken.is_empty() {
        (new, report)
    } else {
        (page.to_string(), report)
    }
}

fn reshape_tracker_unchecked(page: &str) -> (String, Report) {
    let p = parse(page);
    let mut r = Report::default();
    // The `Project:` line belongs above the sections, where moving them can't push it down.
    let opening_end = p.front.len() + p.opening.len();
    let body = markdown::lines(page, p.front.len());
    if let Some(no) = crate::projects::project_line(&body) {
        if body.iter().find(|l| l.no == no).is_some_and(|l| l.start >= opening_end) {
            r.reasons.push("the Project: line is inside a section, not above the first one".into());
        }
    }
    let mut slots: [Option<Unit>; 4] = Default::default();
    let mut rest: Vec<Unit> = Vec::new();
    for (i, s) in p.sections.iter().enumerate() {
        let ord = Some(i + 1);
        match tracker_slot(s.heading) {
            Some(k) if slots[k].is_none() => {
                let text = if k == LOG_SLOT { sorted_log(s.text, &mut r) } else { s.text.to_string() };
                slots[k] = Some(Unit { text, ord });
            }
            Some(k) => {
                r.reasons.push(format!("more than one {} section", TRACKER_SECTIONS[k]));
                rest.push(Unit { text: s.text.to_string(), ord });
            }
            None => rest.push(Unit { text: s.text.to_string(), ord }),
        }
    }
    let mut units = vec![Unit { text: p.opening.to_string(), ord: Some(0) }];
    for (k, u) in slots.into_iter().enumerate() {
        units.push(u.unwrap_or_else(|| {
            let h = format!("## {}", TRACKER_SECTIONS[k]);
            r.added.push(h.clone());
            Unit { text: format!("{h}\n"), ord: None }
        }));
    }
    units.extend(rest);
    (with_tail(&assemble(p.front, &units), page), r)
}

/// A Tracker note's Current state text (everything under its `## Current state`, its `###`
/// parts too), trimmed; None when it has none or it's empty. The project page shows it.
pub fn tracker_current_state(page: &str) -> Option<String> {
    let page = page.replace("\r\n", "\n");
    let p = parse(&page);
    let s = p.sections.iter().find(|s| tracker_slot(s.heading) == Some(0))?;
    let t = s.body().trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// `before` and `text` with a blank line between them (none when `before` is empty).
fn joined(before: &str, text: &str) -> String {
    let sep = if before.trim().is_empty() || ends_blank(before) {
        ""
    } else if before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{before}{sep}{text}")
}

/// Where a section starts on the page, by byte.
fn offset(page: &str, s: &Section) -> usize {
    s.text.as_ptr() as usize - page.as_ptr() as usize
}

/// `body` as a new `TRACKER_SECTIONS[k]` section where the tracker shape puts it among the shape's
/// sections the note has: right after the nearest one before it in the shape, else above the
/// nearest one after it, else above the note's first section (or at the end when it has none).
fn tracker_insert(page: &str, k: usize, body: &str) -> String {
    let text = format!("## {}\n\n{}\n", TRACKER_SECTIONS[k], body.trim());
    let p = parse(page);
    let shaped: Vec<(usize, &Section)> = p.sections.iter().filter_map(|s| tracker_slot(s.heading).map(|j| (j, s))).collect();
    let before = shaped.iter().filter(|(j, _)| *j < k).max_by_key(|(j, _)| *j);
    let after = shaped.iter().filter(|(j, _)| *j > k).min_by_key(|(j, _)| *j);
    // None: at the end of the page.
    let at = match (before, after) {
        (Some((_, s)), _) => Some(offset(page, s) + s.text.len()).filter(|&e| e < page.len()),
        (None, Some((_, s))) => Some(offset(page, s)),
        (None, None) => p.sections.first().map(|s| offset(page, s)),
    };
    match at {
        Some(at) => format!("{}\n{}", joined(&page[..at], &text), &page[at..]),
        None => {
            let end = content_end(page, 0, page.len());
            format!("{}{}", joined(&page[..end], &text), &page[end..])
        }
    }
}

/// A Tracker note with its Current state's text `state`, and with `log` (a day and one line) a
/// dated entry at the top of its Log, `- YYYY-MM-DD: …`. Only the Current state's own text
/// changes: its `###` parts stay. A section it lacks goes where the tracker shape puts it; nothing
/// else on the page changes, and the note keeps its line endings.
pub fn tracker_with_state(page: &str, state: Option<&str>, log: Option<(Date, &str)>) -> Result<String, String> {
    let crlf = page.contains("\r\n");
    let mut page = page.replace("\r\n", "\n");
    if state.is_none() && log.is_none() {
        return Err("Give the Current state's text, a Log line, or both.".into());
    }
    if let Some(t) = state {
        let t = t.trim();
        if t.is_empty() {
            return Err("The Current state's text is empty.".into());
        }
        if markdown::lines(t, 0).iter().any(|l| !l.code && markdown::heading(l.text).is_some()) {
            return Err("The Current state's text has headings in it: give the section's text only.".into());
        }
        let span = parse(&page).sections.iter().find(|s| tracker_slot(s.heading) == Some(0)).map(|s| (offset(&page, s), s.text.len()));
        page = match span {
            Some((start, len)) => {
                let head = page[start..start + len].split_inclusive('\n').next().unwrap_or("").len();
                // The section's own text: to its first sub-heading (outside code), else its end.
                let sub = markdown::lines(&page, start + head)
                    .into_iter()
                    .take_while(|l| l.start < start + len)
                    .find(|l| !l.code && markdown::heading(l.text).is_some())
                    .map(|l| l.start);
                let end = sub.unwrap_or(start + len);
                let lead = if page[..start + head].ends_with('\n') { "\n" } else { "\n\n" };
                let gap = if end < page.len() { "\n" } else { "" };
                format!("{}{lead}{t}\n{gap}{}", &page[..start + head], &page[end..])
            }
            None => tracker_insert(&page, 0, t),
        };
    }
    if let Some((date, line)) = log {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            return Err("The Log line is empty.".into());
        }
        let entry = format!("- {}: {line}", date.iso());
        let span =
            parse(&page).sections.iter().find(|s| tracker_slot(s.heading) == Some(LOG_SLOT)).map(|s| (offset(&page, s), s.text.len()));
        page = match span {
            Some((start, len)) => {
                let head = page[start..start + len].split_inclusive('\n').next().unwrap_or("").len();
                // Its entries: top-level bullets outside code, and whether the first two have a
                // blank line between them.
                let ls = markdown::lines(&page, start + head);
                let ls: Vec<_> = ls.iter().take_while(|l| l.start < start + len).collect();
                let entries: Vec<usize> = (0..ls.len()).filter(|&i| !ls[i].code && top_bullet(ls[i].text)).collect();
                let loose = entries.len() > 1 && entries[1] > 0 && ls[entries[1] - 1].text.trim().is_empty();
                match entries.first() {
                    // Above the newest entry, spaced as the entries are.
                    Some(&i) => {
                        let at = ls[i].start;
                        format!("{}{entry}\n{}{}", &page[..at], if loose { "\n" } else { "" }, &page[at..])
                    }
                    // None yet: after any text under its heading.
                    None => {
                        let end = content_end(&page, start, start + len);
                        let body = joined(&page[..end], &format!("{entry}\n"));
                        let after = &page[end..];
                        let sep = if after.is_empty() || after.starts_with('\n') { "" } else { "\n" };
                        format!("{body}{sep}{after}")
                    }
                }
            }
            None => tracker_insert(&page, LOG_SLOT, &entry),
        };
    }
    // Written with the line endings it had.
    Ok(if crlf { page.replace('\n', "\r\n") } else { page })
}

/// Which link targets are vault notes or sources rather than wiki pages, from the vault's file
/// paths (vault-relative, `/`-separated; anything in a dot folder is left out).
pub struct Sources(HashMap<String, bool>);

impl Sources {
    pub fn from_paths<'a>(rels: impl IntoIterator<Item = &'a str>) -> Self {
        let mut m: HashMap<String, bool> = HashMap::new();
        for rel in rels.into_iter().filter(|r| !r.split('/').any(|c| c.starts_with('.'))) {
            *m.entry(crate::links::key(rel)).or_default() |= !rel.starts_with("wiki/");
        }
        Sources(m)
    }

    pub fn is_source(&self, target: &str) -> bool {
        self.0.get(&crate::links::key(target)).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(h: &str) -> (Option<String>, String) {
        let d = read_date(h).unwrap_or_else(|| panic!("no date in {h:?}"));
        (d.when.year().and_then(|y| d.when.in_year(y)).map(Date::iso), d.title)
    }

    fn is_source(t: &str) -> bool {
        t.starts_with("Meeting.") || t.starts_with("1-1.") || t.ends_with(".pdf")
    }

    fn fixtures() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pageshape");
        let mut out: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
            .collect();
        out.sort();
        out
    }

    /// Pages that need the user, and why (in part).
    const REVIEW: &[(&str, &str)] = &[
        ("messy-timeline", "text outside its entries"),
        ("no-year-review", "different years"),
        ("repeated", "more than one entry cites"),
        ("two-summing", "2 summing-up sections"),
    ];

    #[test]
    fn every_fixture_keeps_the_invariants_and_reshapes_once() {
        for (name, page) in fixtures() {
            let (new, r) = reshape(&page, &is_source);
            assert!(r.broken.is_empty(), "{name}: {:?}", r.broken);
            assert!(check(&page, &new, &r, &is_source).is_empty(), "{name}");
            let (again, r2) = reshape(&new, &is_source);
            assert_eq!(again, new, "{name}: reshaping twice");
            assert!(r2.headings.is_empty() && r2.added.is_empty() && r2.removed.is_empty(), "{name}: {r2:?}");
            match REVIEW.iter().find(|x| x.0 == name) {
                Some((_, why)) => assert!(r.reasons.iter().any(|x| x.contains(why)), "{name}: {:?}", r.reasons),
                None => assert!(r.auto(), "{name}: {:?}", r.reasons),
            }
            if r.auto() {
                assert!(in_shape(&new, &is_source), "{name}");
            }
        }
    }

    fn fixture(name: &str) -> String {
        fixtures().into_iter().find(|f| f.0 == name).unwrap().1
    }

    #[test]
    fn a_page_in_the_shape_is_left_byte_for_byte() {
        let page = fixture("in-shape");
        assert!(in_shape(&page, &is_source));
        let (new, r) = reshape(&page, &is_source);
        assert_eq!(new, page);
        assert!(r.auto() && r.headings.is_empty() && r.added.is_empty(), "{r:?}");
    }

    /// The headings of a page, in order.
    fn headings(page: &str) -> Vec<String> {
        markdown::lines(page, 0).iter().filter(|l| !l.code && markdown::heading(l.text).is_some()).map(|l| l.text.to_string()).collect()
    }

    #[test]
    fn dated_sections_become_a_timeline_newest_first() {
        let (new, r) = reshape(&fixture("dated-only"), &is_source);
        assert_eq!(
            headings(&new),
            [
                "# Orbit App",
                "## Timeline",
                "### 2026-10-02 — Steerco: launch moves",
                "### 2026-09 — Pilot results",
                "### 2026-06-08 — Sprint review",
                "#### Follow-ups",
                "### 2025-H2 — Discovery",
                "## See also"
            ]
        );
        assert!(new.contains("### 2026-10-02 — Steerco: launch moves\nSource: [[Meeting. Orbit App Steerco - 2026-10-02]]\n\nLaunch moves"));
        // Links to wiki pages aren't sources.
        assert!(new.contains("### 2025-H2 — Discovery\n\nDiscovery ran"));
        assert!(new.starts_with(
            "---\ntype: entity\naliases: [OA]\nsources:\n  - \"[[Meeting. Orbit App Steerco - 2026-10-02]]\"\n---\n# Orbit App\n"
        ));
        assert_eq!(r.entries, 4);
        assert!(r.headings.contains(&("## Week of 8–12 Jun 2026 — Sprint review".into(), "### 2026-06-08 — Sprint review".into())));
    }

    #[test]
    fn sections_below_see_also_and_a_current_state_at_the_bottom_move() {
        let (new, _) = reshape(&fixture("see-also-middle"), &is_source);
        assert_eq!(
            headings(&new),
            [
                "# Acme Pilot",
                "## Current state",
                "## Architecture",
                "## Risks",
                "## Timeline",
                "### 2026-10-03 — Contract signed",
                "## See also"
            ]
        );
        assert!(new.ends_with("## See also\n\n- [[Orbit App]]\n"));
    }

    #[test]
    fn two_see_also_lists_merge_without_repeating_a_link() {
        let (new, r) = reshape(&fixture("two-see-also"), &is_source);
        assert!(new.ends_with("## See also\n\n- [[Orbit App]]\n- [[Acme Pilot]]\n- [[Lena|Lena's page]]\n"), "{new}");
        assert_eq!(r.removed, ["## See also", "- [[Acme Pilot]]"]);
        assert!(r.auto());
    }

    #[test]
    fn an_empty_see_also_and_a_missing_newline_are_kept() {
        let (new, _) = reshape(&fixture("empty-see-also"), &is_source);
        assert!(new.ends_with("[[1-1. Lena - 2026-08-04]].\n\n## See also\n"));
        let (new, r) = reshape(&fixture("no-newline"), &is_source);
        assert!(new.ends_with("Drafted [[Meeting. Comms - 2026-09-15]]."));
        assert!(new.contains("## Current state\n\nOwns the comms plan."));
        assert!(r.headings.contains(&("## Summary".into(), "## Current state".into())));
    }

    #[test]
    fn a_dated_summing_heading_becomes_current_state_with_its_date_under_it() {
        let page = "# Acme\n\nA supplier.\n\n## Status (May 2026) — Hard blocker\n\nWaiting on legal.\n\n## Architecture\n\nx\n";
        let (new, r) = reshape(page, &is_source);
        assert!(r.auto(), "{r:?}");
        assert!(new.contains("## Current state\n\nAs of May 2026 — Hard blocker.\n\nWaiting on legal.\n"), "{new}");
        assert_eq!(as_of("Current state (w/e 18 Sep 2026)").as_deref(), Some("As of w/e 18 Sep 2026."));
        assert_eq!(as_of("Current state (as of 3 Oct 2026)").as_deref(), Some("As of 3 Oct 2026."));
        assert_eq!(as_of("Status — needs work"), None);
        assert!(!reshape("# A\n\n## Status — needs work\n\nx\n", &is_source).1.auto());
    }

    #[test]
    fn a_missing_year_comes_from_the_sections_around_it_when_they_agree() {
        let (new, r) = reshape(&fixture("no-year"), &is_source);
        assert!(r.auto(), "{r:?}");
        assert!(new.contains("### 2026-10-03 — Steerco update\n"));
        let (_, r) = reshape(&fixture("no-year-review"), &is_source);
        assert!(!r.auto());
    }

    #[test]
    fn code_fences_are_not_cut_and_their_lines_not_demoted() {
        let (new, _) = reshape(&fixture("code-fence"), &is_source);
        assert!(new.contains("```bash\n## not a heading\necho build\n```"));
        assert!(new.contains("#### Notes"));
        assert_eq!(headings(&new)[1..3], ["## Usage", "## Timeline"]);
    }

    #[test]
    fn entries_from_one_source_on_one_date_become_one() {
        let (new, r) = reshape(&fixture("same-source-same-date"), &is_source);
        assert!(r.auto(), "{r:?}");
        assert_eq!(new.matches("Source: [[1-1. Maya - 2026-06-10]]").count(), 1);
        assert_eq!(
            headings(&new),
            [
                "# Orbit Pilot",
                "## Timeline",
                "### 2026-10-02 — Steerco",
                "### 2026-06 — Pilot scope agreed",
                "#### Actions",
                "#### Comms plan for the pilot"
            ]
        );
        assert!(r.headings.contains(&("## Jun 2026 — Comms plan for the pilot".into(), "#### Comms plan for the pilot".into())));
    }

    #[test]
    fn of_dated_summing_sections_the_newest_is_the_current_state() {
        let (new, r) = reshape(&fixture("two-status"), &is_source);
        assert!(r.auto(), "{r:?}");
        assert_eq!(headings(&new), ["# Orbit Strategy", "## Current state", "## Open questions", "## Timeline", "### 2026-03 — Status"]);
        assert!(new.contains("## Current state\n\nAs of Apr 2026.\n\nApproved by the board."));
    }

    #[test]
    fn repeated_sources_on_different_dates_are_listed_not_merged() {
        let (new, r) = reshape(&fixture("repeated"), &is_source);
        assert_eq!(r.repeated, ["Meeting. Orbit App Steerco - 2026-10-02"]);
        assert_eq!(new.matches("### 2026-10").count(), 2);
    }

    #[test]
    fn a_lossy_reshape_is_caught() {
        let is = &is_source;
        for (name, page) in fixtures() {
            let (new, r) = reshape(&page, is);
            if new == page {
                continue;
            }
            // A body line dropped.
            let line =
                new.lines().rev().skip(1).find(|l| !l.trim().is_empty() && !l.starts_with('#') && !l.starts_with("Source:")).unwrap();
            let lossy = new.replacen(&format!("{line}\n"), "", 1);
            assert!(check(&page, &lossy, &r, is).iter().any(|x| x.contains("went missing")), "{name}: dropped line");
            // A line repeated.
            let doubled = new.replacen(&format!("{line}\n"), &format!("{line}\n{line}\n"), 1);
            assert!(!check(&page, &doubled, &r, is).is_empty(), "{name}: doubled line");
            // The frontmatter touched.
            if new.starts_with("---\n") {
                let touched = new.replacen("---\n", "---\nupdated: 2026-10-06\n", 1);
                assert!(check(&page, &touched, &r, is).iter().any(|x| x.contains("frontmatter")), "{name}: frontmatter");
            }
            // Blank lines piled up.
            let spaced = new.replacen("\n\n", "\n\n\n\n", 1);
            assert!(check(&page, &spaced, &r, is).iter().any(|x| x.contains("blank")), "{name}: blank lines");
        }
        // Out of order: reshaping it again would move it.
        let page = fixture("dated-only");
        let (new, r) = reshape(&page, is);
        let a = new.find("### 2026-09").unwrap();
        let b = new.find("### 2026-06").unwrap();
        let swapped =
            format!("{}{}{}{}", &new[..a], &new[b..new.find("### 2025").unwrap()], &new[a..b], &new[new.find("### 2025").unwrap()..]);
        assert!(check(&page, &swapped, &r, is).iter().any(|x| x.contains("again")));
    }

    /// Reshape pages as the app makes it: each page's change from the survey, applied as Changes
    /// applies one (its instruction run on the page), then the run reverted as Revert does.
    #[test]
    fn a_reshape_run_reverts_byte_for_byte_and_leaves_review_pages_alone() {
        let d = tempfile::tempdir().unwrap();
        let wiki = d.path().join("wiki/entities");
        std::fs::create_dir_all(&wiki).unwrap();
        for (name, page) in fixtures() {
            std::fs::write(wiki.join(format!("{name}.md")), page).unwrap();
        }
        for src in ["Meeting. Orbit App Steerco - 2026-10-02", "Meeting. Comms - 2026-09-01", "1-1. Maya - 2026-06-10"] {
            std::fs::write(d.path().join(format!("{src}.md")), "x\n").unwrap();
        }
        let before: Vec<(String, Vec<u8>)> =
            vault_files(d.path()).into_iter().map(|p| (p.clone(), std::fs::read(d.path().join(&p)).unwrap())).collect();
        let mut made = Vec::new();
        for (r, new) in survey(d.path()) {
            let (Some(new), true) = (new, r.auto) else { continue };
            let old = std::fs::read_to_string(d.path().join(&r.path)).unwrap();
            let ins = crate::changes::from_texts(Some(&old), &new);
            let after = ins.text(&r.path, Some(&old)).unwrap();
            assert_eq!(after, new, "{}", r.path);
            std::fs::write(d.path().join(&r.path), &after).unwrap();
            made.push((r.path, old, after));
        }
        assert!(made.len() >= 6, "{}", made.len());
        // Every page that needs the user is as it was.
        for (r, _) in survey(d.path()) {
            if !r.in_shape {
                assert!(!r.auto, "{} left out", r.path);
                let was = &before.iter().find(|b| b.0 == r.path).unwrap().1;
                assert_eq!(&std::fs::read(d.path().join(&r.path)).unwrap(), was, "{}", r.path);
            }
        }
        for (path, old, after) in made.iter().rev() {
            let now = std::fs::read_to_string(d.path().join(path)).unwrap();
            std::fs::write(d.path().join(path), crate::changes::revert_text(old, after, &now).unwrap()).unwrap();
        }
        for (p, bytes) in &before {
            assert_eq!(&std::fs::read(d.path().join(p)).unwrap(), bytes, "{p}");
        }
    }

    #[test]
    fn a_current_state_goes_after_the_opening_and_keeps_the_shape() {
        let (page, _) = reshape(&fixture("dated-only"), &is_source);
        assert!(!has_current_state(&page));
        let prompt = current_state_prompt("wiki/entities/Orbit App.md", &page, 6000);
        assert!(prompt.contains("\"Orbit App\"") && prompt.contains("### 2026-10-02 — Steerco") && !prompt.contains("## See also"));
        let ans =
            current_state_answer(&page, "- Launch moves to 28 November [[Meeting. Orbit App Steerco - 2026-10-02]].", &|_| false).unwrap();
        let new = with_current_state(&page, &ans);
        assert!(has_current_state(&new));
        assert!(new.contains("on the [[Hub Platform]].\n\n## Current state\n\n- Launch moves to 28 November [[Meeting. Orbit App Steerco - 2026-10-02]].\n\n## Timeline\n"), "{new}");
        assert!(in_shape(&new, &is_source));
        // Only lines added: the heading and the text.
        assert_eq!(new.len(), page.len() + "## Current state\n\n".len() + ans.len() + 2);
    }

    #[test]
    fn a_current_state_answer_is_checked() {
        let page = "# Orbit App\n\nSee [[Lena]].\n";
        let no = |_: &str| false;
        assert_eq!(current_state_answer(page, "```\nLive with [[Lena]].\n```", &no).unwrap(), "Live with [[Lena]].");
        assert_eq!(current_state_answer(page, "## Current state\n\nLive.", &no).unwrap(), "Live.");
        assert!(current_state_answer(page, "NONE", &no).is_err());
        assert_eq!(current_state_answer(page, "Live with [[Maya]] and [[Theo|the PM]].", &no).unwrap(), "Live with Maya and the PM.");
        assert!(current_state_answer(page, "Live with [[Maya]].", &|t| t == "Maya").is_ok());
        assert!(current_state_answer(page, "Live.\n\n### More", &no).is_err());
        assert!(current_state_answer(page, &"word ".repeat(400), &no).is_err());
    }

    #[test]
    fn an_entry_goes_in_by_date_and_replaces_its_sources_entry() {
        let page = fixture("in-shape");
        let src = "Meeting. Orbit App Pilot - 2026-09-20";
        // Between 2026-10-02 and 2026-09.
        let (new, replaced) = with_entry(
            &page,
            Date::Day(2026, 9, 20),
            "Pilot review",
            src,
            "Ten users.\n\n## Actions\n\n- Lena writes up.\n\nSource: [[Meeting. Orbit App Pilot - 2026-09-20]]",
        );
        assert!(!replaced);
        assert!(in_shape(&new, &is_source), "{new}");
        let hs = headings(&new);
        let i = hs.iter().position(|h| h == "### 2026-09-20 — Pilot review").unwrap();
        assert_eq!(
            &hs[i - 2..i + 3],
            [
                "### 2026-10-02 — Steerco: launch moves",
                "#### Actions",
                "### 2026-09-20 — Pilot review",
                "#### Actions",
                "### 2026-09 — Pilot results"
            ]
        );
        assert!(new.contains("### 2026-09-20 — Pilot review\nSource: [[Meeting. Orbit App Pilot - 2026-09-20]]\n\nTen users.\n"));
        // Again, from the same source: one entry, replaced.
        let (again, replaced) = with_entry(&new, Date::Day(2026, 9, 20), "Pilot review: go", src, "Go for launch.");
        assert!(replaced);
        assert_eq!(again.matches("Source: [[Meeting. Orbit App Pilot - 2026-09-20]]").count(), 1);
        assert!(again.contains("### 2026-09-20 — Pilot review: go\nSource: [[Meeting. Orbit App Pilot - 2026-09-20]]\n\nGo for launch.\n\n### 2026-09 — Pilot results"), "{again}");
        assert!(in_shape(&again, &is_source));
        // Newest of all goes first; oldest of all last, before See also.
        let (top, _) = with_entry(&page, Date::Day(2026, 10, 5), "Launch prep", src, "Prep.");
        assert!(top.contains("## Timeline\n\n### 2026-10-05 — Launch prep\n"));
        let (low, _) = with_entry(&page, Date::Half(2025, 1), "Idea", src, "First idea.");
        assert!(
            low.contains(
                "Discovery ran.\n\n### 2025-H1 — Idea\nSource: [[Meeting. Orbit App Pilot - 2026-09-20]]\n\nFirst idea.\n\n## See also"
            ),
            "{low}"
        );
        assert!(in_shape(&top, &is_source) && in_shape(&low, &is_source));
    }

    #[test]
    fn a_page_with_no_timeline_gets_one_above_see_also() {
        let page = "# Lena\n\nLeads the pilot.\n\n## Role\n\nPM.\n\n## See also\n\n- [[Orbit App]]\n";
        let (new, _) = with_entry(page, Date::Day(2026, 10, 2), "Steerco", "Meeting. Steerco - 2026-10-02", "Spoke.");
        assert_eq!(new, "# Lena\n\nLeads the pilot.\n\n## Role\n\nPM.\n\n## Timeline\n\n### 2026-10-02 — Steerco\nSource: [[Meeting. Steerco - 2026-10-02]]\n\nSpoke.\n\n## See also\n\n- [[Orbit App]]\n");
        let (new, _) = with_entry("# Lena\n\nLeads it.", Date::Month(2026, 10), "", "Meeting. Steerco - 2026-10-02", "Spoke.");
        assert_eq!(new, "# Lena\n\nLeads it.\n\n## Timeline\n\n### 2026-10\nSource: [[Meeting. Steerco - 2026-10-02]]\n\nSpoke.\n");
    }

    #[test]
    fn a_topic_goes_above_the_timeline_and_current_state_after_the_opening() {
        let page = fixture("in-shape");
        let new = with_topic(&page, "Risks", "- Pen test.").unwrap();
        assert!(new.contains("Built on the [[Hub Platform]].\n\n## Risks\n\n- Pen test.\n\n## Timeline\n"), "{new}");
        assert!(in_shape(&new, &is_source));
        let new = with_topic(&page, "Architecture", "Built on Hub 2.").unwrap();
        assert!(new.contains("## Architecture\n\nBuilt on Hub 2.\n\n## Timeline"));
        assert!(with_topic(&page, "See also", "x").is_err() && with_topic(&page, "2 Oct 2026 — x", "x").is_err());
        let new = with_current_state_text(&page, "Launch on 5 December.").unwrap();
        assert!(new.contains("## Current state\n\nLaunch on 5 December.\n\n## Architecture"));
        let (bare, _) = reshape(&fixture("dated-only"), &is_source);
        assert!(in_shape(&with_current_state_text(&bare, "Live.").unwrap(), &is_source));
    }

    #[test]
    fn an_edit_puts_a_new_heading_where_the_shape_does() {
        let page = fixture("in-shape");
        let rel = "wiki/entities/Orbit App.md";
        let new = with_section(rel, &page, "## Risks", "- Pen test.").unwrap();
        assert!(new.contains("## Risks\n\n- Pen test.\n\n## Timeline\n"), "{new}");
        assert!(in_shape(&new, &is_source));
        // A summing heading is the page's Current state.
        let new = with_section(rel, &page, "Status", "Live.").unwrap();
        assert!(new.contains("## Current state\n\nLive.\n\n## Architecture") && !new.contains("## Status"), "{new}");
        // A heading it has is rewritten where it is; a page outside the shape as before.
        let new = with_section(rel, &page, "architecture", "Hub 2.").unwrap();
        assert!(new.contains("## Architecture\n\nHub 2.\n\n## Timeline"));
        let new = with_section("Projects/Orbit.md", &page, "Risks", "- Pen test.").unwrap();
        assert!(new.contains("## Risks\n\n- Pen test.\n\n## See also"), "{new}");
    }

    #[test]
    fn a_page_edited_out_of_shape_is_reported_and_reshaped() {
        let sources = Sources::from_paths(["Meeting. Orbit App Steerco - 2026-10-02.md", "Meeting. Orbit App Sync - 2026-10-05.md"]);
        let page = fixture("in-shape");
        let (ok, _) = one("wiki/entities/Orbit App.md", &page, &sources);
        assert_eq!(drift_line(std::slice::from_ref(&ok)), "every wiki page in the page shape");
        // Edited elsewhere: a dated section added at the end, below See also.
        let drifted = format!("{page}\n\n## 5 Oct 2026 — Sync\n\nScope agreed [[Meeting. Orbit App Sync - 2026-10-05]].\n");
        let (r, new) = one("wiki/entities/Orbit App.md", &drifted, &sources);
        assert!(!r.in_shape && r.auto, "{r:?}");
        assert_eq!(drift_line(&[ok, r]), "1 wiki page not in the page shape (1 Reshape pages can do by itself, in Knowledge health)");
        let new = new.unwrap();
        assert!(new.contains("## Timeline\n\n### 2026-10-05 — Sync\nSource: [[Meeting. Orbit App Sync - 2026-10-05]]"), "{new}");
        assert!(in_shape(&new, &|t| sources.is_source(t)));
    }

    #[test]
    fn sources_are_files_outside_the_wiki() {
        let s = Sources::from_paths([
            "wiki/entities/Lena.md",
            "Meeting. Steerco - 2026-10-02.md",
            "sources/Plan.pdf",
            ".trash/Old.md",
            "wiki/summaries/Plan.pdf.md",
        ]);
        assert!(s.is_source("Meeting. Steerco - 2026-10-02"));
        assert!(s.is_source("Plan.pdf"));
        assert!(!s.is_source("Lena"));
        assert!(!s.is_source("Old"));
        assert!(!s.is_source("Missing"));
    }

    #[test]
    fn dates_in_every_heading_form() {
        let cases = [
            ("2026-10-02 — Steerco", "2026-10-02", "Steerco"),
            ("2026-10 — Pilot results", "2026-10", "Pilot results"),
            ("2 Oct 2026 — Steerco: launch moves", "2026-10-02", "Steerco: launch moves"),
            ("02 October 2026 — Steerco", "2026-10-02", "Steerco"),
            ("2nd Oct 2026 — Steerco", "2026-10-02", "Steerco"),
            ("Oct 2026 — Roadmap", "2026-10", "Roadmap"),
            ("October 2026 — Roadmap", "2026-10", "Roadmap"),
            ("Sept 2026 — Roadmap", "2026-09", "Roadmap"),
            ("September 2026: Roadmap", "2026-09", "Roadmap"),
            ("October 2, 2026 — Steerco", "2026-10-02", "Steerco"),
            ("Week of 8–12 Jun 2026 — Sprint review", "2026-06-08", "Sprint review"),
            ("Week of 8-12 June 2026", "2026-06-08", ""),
            ("Week of 29 Sep – 3 Oct 2026 — Planning", "2026-09-29", "Planning"),
            ("Week of 29 Dec – 2 Jan 2026 — Planning", "2025-12-29", "Planning"),
            ("w/c 8 Jun 2026 — Planning", "2026-06-08", "Planning"),
            ("H2 2025 — Discovery", "2025-H2", "Discovery"),
            ("H2 2025 discovery", "2025-H2", "discovery"),
            ("Q3 2026 — Build", "2026-Q3", "Build"),
            ("2026-Q3 — Build", "2026-Q3", "Build"),
            ("Steerco — 2 Oct 2026", "2026-10-02", "Steerco"),
            ("Steerco decision (3 Oct 2026)", "2026-10-03", "Steerco decision"),
            ("Oct 2026 — Steerco (3 Oct)", "2026-10-03", "Steerco"),
            ("Oct 2026 — Steerco (3 Oct, via Lena)", "2026-10-03", "Steerco (via Lena)"),
            ("Jan 2027 — Planning (30 Dec)", "2026-12-30", "Planning"),
            ("2026-10-02", "2026-10-02", ""),
            ("May 2026 — Kick-off", "2026-05", "Kick-off"),
            ("Sep–Oct 2026 — Landing zone", "2026-09", "Landing zone"),
            ("Dec 2025 – Jan 2026 — Planning", "2025-12", "Jan 2026 — Planning"),
            ("Oct 2026 — Status in the 2 Oct dashboard (25 Sep update)", "2026-10", "Status in the 2 Oct dashboard (25 Sep update)"),
            ("May 2026 — RFC (superseded 2026-09-08, see below)", "2026-05", "RFC (superseded 2026-09-08, see below)"),
        ];
        for (h, iso, title) in cases {
            assert_eq!(date(h), (Some(iso.to_string()), title.to_string()), "{h}");
        }
    }

    #[test]
    fn a_day_and_month_that_disagree_take_the_day_and_say_so() {
        let d = read_date("Oct 2026 — Steerco (30 Sep)").unwrap();
        assert_eq!(d.when, When::Day(Some(2026), 9, 30));
        assert_eq!(d.title, "Steerco");
        assert!(d.note.is_some());
    }

    #[test]
    fn a_missing_year_is_left_to_find() {
        let d = read_date("Steerco (3 Oct)").unwrap();
        assert_eq!(d.when, When::Day(None, 10, 3));
        assert_eq!(d.title, "Steerco");
        assert_eq!(read_date("3 Oct — Steerco").unwrap().when, When::Day(None, 10, 3));
        assert_eq!(read_date("Oct — Steerco").unwrap().when, When::Month(None, 10));
    }

    #[test]
    fn topical_headings_have_no_date() {
        for h in [
            "Architecture",
            "Key relationships",
            "Archive",
            "Plans for Oct 2026 launch",
            "2026 roadmap",
            "Q3",
            "May we",
            "Mayhem 2026",
            "Ratio 2026-27",
        ] {
            assert_eq!(classify(h), Kind::Topical, "{h}");
        }
        assert_eq!(read_date("31 Feb 2026 — Nope"), None);
    }

    #[test]
    fn sections_are_classified() {
        assert_eq!(classify("See also"), Kind::Closing);
        assert_eq!(classify("Related"), Kind::Closing);
        assert_eq!(classify("References"), Kind::Closing);
        assert_eq!(classify("Current state"), Kind::Summing);
        assert_eq!(classify("Summary:"), Kind::Summing);
        assert_eq!(classify("Current state (as of 3 Oct 2026)"), Kind::Summing);
        assert_eq!(classify("Executive summary"), Kind::Summing);
        assert_eq!(classify("Timeline"), Kind::Timeline);
        assert!(matches!(classify("2 Oct 2026 — Steerco"), Kind::Dated(_)));
        assert_eq!(classify("Statuses of teams"), Kind::Topical);
        assert_eq!(classify("Status of teams"), Kind::Topical);
    }

    #[test]
    fn iso_entry_headings_round_trip() {
        for d in [Date::Day(2026, 10, 2), Date::Month(2026, 9), Date::Quarter(2026, 3), Date::Half(2025, 2)] {
            assert_eq!(Date::parse_iso(&format!("{} — Title", d.iso())), Some(d));
            assert_eq!(Date::parse_iso(&d.iso()), Some(d));
        }
        assert_eq!(Date::parse_iso("2026-10-02 Steerco"), None);
        assert_eq!(Date::parse_iso("2026-13"), None);
    }

    #[test]
    fn newest_first_puts_a_month_after_its_days() {
        let mut ds = vec![
            Date::Month(2026, 10),
            Date::Day(2026, 10, 2),
            Date::Quarter(2026, 4),
            Date::Day(2026, 11, 1),
            Date::Half(2026, 2),
            Date::Day(2026, 9, 30),
        ];
        ds.sort_by_key(|d| std::cmp::Reverse(d.key()));
        assert_eq!(
            ds,
            [
                Date::Day(2026, 11, 1),
                Date::Day(2026, 10, 2),
                Date::Month(2026, 10),
                Date::Quarter(2026, 4),
                Date::Day(2026, 9, 30),
                Date::Half(2026, 2)
            ]
        );
    }

    #[test]
    fn parse_cuts_at_level_two_headings_outside_code() {
        let page = "---\ntype: entity\n---\n# Orbit App\n\nOpening.\n\n## A\n\nx\n\n```\n## not a heading\n```\n\n### Sub\n\n## B\ny\n";
        let p = parse(page);
        assert_eq!(p.front, "---\ntype: entity\n---\n");
        assert_eq!(p.opening, "# Orbit App\n\nOpening.\n\n");
        assert_eq!(p.sections.iter().map(|s| s.heading).collect::<Vec<_>>(), ["A", "B"]);
        assert!(p.sections[0].text.contains("### Sub"));
        assert_eq!(format!("{}{}{}", p.front, p.opening, p.sections.iter().map(|s| s.text).collect::<String>()), page);
    }

    #[test]
    fn knows_tracker_notes() {
        assert!(is_tracker("Tracker. Orbit rollout.md") && is_tracker("Work/Tracker. Orbit rollout.md"));
        assert!(
            !is_tracker("wiki/concepts/Tracker. X.md") && !is_tracker("Templates/Tracker. X.md") && !is_tracker("sources/Tracker. X.md")
        );
        assert!(!is_tracker("Plan. Orbit.md") && !is_tracker("Tracker. X.txt"));
        assert!(has_shape("Tracker. X.md") && has_shape("wiki/entities/Maya.md") && !has_shape("Plan. X.md"));
    }

    const TRACKER: &str = "---\nupdated: 2026-10-01\n---\n\nProject: [[Project. Orbit]]\n\nWhere the rollout stands.\n\n## Decisions\n\n1. Ship in stages.\n\n## Log\n\n- 2026-09-01: Started.\n- 2026-10-02: Staged in Acme's test site.\n  Lena signed it off.\n- **2026-09-15** Plan agreed.\n\n## Next\n\n- [ ] Book the cutover\n";

    #[test]
    fn reshapes_a_tracker_keeping_everything() {
        let (new, r) = reshape_tracker(TRACKER);
        assert!(r.auto(), "{r:?}");
        assert_eq!(
            new,
            "---\nupdated: 2026-10-01\n---\n\nProject: [[Project. Orbit]]\n\nWhere the rollout stands.\n\n## Current state\n\n## Blockers\n\n## Next\n\n- [ ] Book the cutover\n\n## Log\n\n- 2026-10-02: Staged in Acme's test site.\n  Lena signed it off.\n- **2026-09-15** Plan agreed.\n- 2026-09-01: Started.\n\n## Decisions\n\n1. Ship in stages.\n"
        );
        assert_eq!(r.added, ["## Current state", "## Blockers"]);
        assert_eq!(r.entries, 3);
        // Idempotent: in the shape now, and reshaping it again changes nothing.
        let (again, r2) = reshape_tracker(&new);
        assert_eq!(again, new);
        assert!(r2.added.is_empty() && r2.auto());
        let (res, changed) = one("Tracker. Orbit.md", &new, &Sources::from_paths(std::iter::empty()));
        assert!(res.in_shape && changed.is_none());
        let (res, changed) = one("Tracker. Orbit.md", TRACKER, &Sources::from_paths(std::iter::empty()));
        assert!(!res.in_shape && res.auto && changed.as_deref() == Some(new.as_str()));
    }

    #[test]
    fn a_tracker_in_the_shape_is_left_alone() {
        let page = "Project: [[Project. Orbit]]\n## current state\nGoing well.\n## Blockers\n## Next\n## Log\n- 2026-10-02: b\n- 2026-10-01: a\n## Notes\nx";
        let (new, r) = reshape_tracker(page);
        assert_eq!(new, page);
        assert!(r.auto() && r.added.is_empty());
    }

    #[test]
    fn a_tracker_log_it_cannot_sort_needs_the_user() {
        for (log, why) in [
            ("- 2026-09-01: a\nSome prose.\n- 2026-10-01: b\n", "text between its entries"),
            ("- 2026-09-01: a\n- Undated thing\n- 2026-10-01: b\n", "1 Log entry with no date"),
        ] {
            let page = format!("Project: [[Project. Orbit]]\n\n## Current state\n\n## Blockers\n\n## Next\n\n## Log\n\n{log}");
            let (new, r) = reshape_tracker(&page);
            assert_eq!(new, page, "{why}");
            assert!(r.reasons.iter().any(|x| x.contains(why)), "{r:?}");
            let (res, _) = one("Tracker. Orbit.md", &page, &Sources::from_paths(std::iter::empty()));
            assert!(!res.in_shape && !res.auto);
        }
        // A second Log is kept, below, and named.
        let page = "## Log\n\n- 2026-10-01: a\n\n## Log\n\n- 2026-09-01: b\n";
        let (new, r) = reshape_tracker(page);
        assert!(new.contains("- 2026-09-01: b") && new.contains("- 2026-10-01: a"));
        assert!(r.reasons.iter().any(|x| x.contains("more than one Log")));
        // A Project: line inside a section would move with it.
        let (_, r) = reshape_tracker("## Notes\n\nProject: [[Project. Orbit]]\n");
        assert!(r.reasons.iter().any(|x| x.contains("Project: line")));
    }

    #[test]
    fn a_loose_tracker_log_stays_loose() {
        let page = "## Current state\n\n## Blockers\n\n## Next\n\n## Log\n\nNewest first.\n\n- 2026-09-01: a\n\n- 2026-10-01: b\n  more\n\n## Other\n\ntext\n";
        let (new, r) = reshape_tracker(page);
        assert!(r.auto(), "{r:?}");
        assert_eq!(
            new,
            "## Current state\n\n## Blockers\n\n## Next\n\n## Log\n\nNewest first.\n\n- 2026-10-01: b\n  more\n\n- 2026-09-01: a\n\n## Other\n\ntext\n"
        );
    }

    #[test]
    fn writes_a_trackers_current_state() {
        let d = Date::parse_iso("2026-11-02").unwrap();
        // Replaced where it is; nothing else changes.
        let shaped = reshape_tracker(TRACKER).0;
        let new = tracker_with_state(&shaped, Some("Staged; cutover next week."), None).unwrap();
        assert_eq!(new, shaped.replace("## Current state\n\n## Blockers", "## Current state\n\nStaged; cutover next week.\n\n## Blockers"));
        // And again, over the text it has.
        let again = tracker_with_state(&new, Some("Live."), None).unwrap();
        assert!(again.contains("## Current state\n\nLive.\n\n## Blockers") && !again.contains("Staged; cutover"));
        // With a Log line: at the top of the Log, dated.
        let logged = tracker_with_state(&new, None, Some((d, "Cutover booked\nfor Friday."))).unwrap();
        assert!(logged.contains("## Log\n\n- 2026-11-02: Cutover booked for Friday.\n- 2026-10-02: Staged"), "{logged}");
        assert!(reshape_tracker(&logged).1.auto());

        // A tracker without the sections: Current state after the opening, above the others; the
        // Log made where the shape puts it.
        let bare = "---\nupdated: 2026-10-01\n---\n\nProject: [[Project. Orbit]]\n\nIntro.\n\n## Decisions\n\n1. Ship.\n";
        let new = tracker_with_state(bare, Some("- Staged\n- Cutover Friday"), Some((d, "Wrote it."))).unwrap();
        assert_eq!(
            new,
            "---\nupdated: 2026-10-01\n---\n\nProject: [[Project. Orbit]]\n\nIntro.\n\n## Current state\n\n- Staged\n- Cutover Friday\n\n## Log\n\n- 2026-11-02: Wrote it.\n\n## Decisions\n\n1. Ship.\n"
        );
        // A note with Windows line endings keeps them.
        let crlf = tracker_with_state(&bare.replace('\n', "\r\n"), Some("Staged."), None).unwrap();
        assert!(crlf.contains("## Current state\r\n\r\nStaged.\r\n\r\n## Decisions") && !crlf.replace("\r\n", "").contains('\n'));
        // An empty Log at the end of the note.
        let new = tracker_with_state("Project: [[Project. Orbit]]\n\n## Log\n", None, Some((d, "First."))).unwrap();
        assert_eq!(new, "Project: [[Project. Orbit]]\n\n## Log\n\n- 2026-11-02: First.\n");
        // What it won't write.
        assert!(tracker_with_state(bare, Some("## Heading\n\ntext"), None).is_err());
        assert!(tracker_with_state(bare, Some("  "), None).is_err());
        assert!(tracker_with_state(bare, None, Some((d, " "))).is_err());
        assert!(tracker_with_state(bare, None, None).is_err());
    }

    #[test]
    fn an_edit_puts_a_trackers_section_where_the_shape_does() {
        let page = "Project: [[Project. Orbit]]\n\n## Next\n\n- [ ] x\n\n## Notes\n\ny\n";
        let new = with_section("Tracker. Orbit.md", page, "Blockers", "Waiting on Lena.").unwrap();
        assert_eq!(new, "Project: [[Project. Orbit]]\n\n## Blockers\n\nWaiting on Lena.\n\n## Next\n\n- [ ] x\n\n## Notes\n\ny\n");
        let new = with_section("Tracker. Orbit.md", page, "Log", "- 2026-11-02: x").unwrap();
        assert_eq!(new, "Project: [[Project. Orbit]]\n\n## Next\n\n- [ ] x\n\n## Log\n\n- 2026-11-02: x\n\n## Notes\n\ny\n");
        // Not a tracker: as before, at the end.
        let plain = with_section("Plan. Orbit.md", page, "Blockers", "z").unwrap();
        assert!(plain.ends_with("## Blockers\n\nz\n"), "{plain}");
    }

    #[test]
    fn writes_the_trackers_own_current_state_only() {
        // An H3 of the same name above it is left alone; the H2 is the one written.
        let page = "Intro\n\n### Current state\nold h3\n\n## Current state\n\nold h2\n\n## Log\n";
        let new = tracker_with_state(page, Some("new"), None).unwrap();
        assert_eq!(new, "Intro\n\n### Current state\nold h3\n\n## Current state\n\nnew\n\n## Log\n");
        // Its ### parts stay; only the text above them changes.
        let page = "## Current state\n\nold\n\n### Risks\n\n- late\n\n## Log\n";
        let new = tracker_with_state(page, Some("new"), None).unwrap();
        assert_eq!(new, "## Current state\n\nnew\n\n### Risks\n\n- late\n\n## Log\n");
        // At the end of the note, and with no text yet.
        assert_eq!(tracker_with_state("## Current state", Some("x"), None).unwrap(), "## Current state\n\nx\n");
        assert_eq!(tracker_current_state("## Current state\r\n\r\nx\r\n### Part\r\ny\r\n## Log\r\n").as_deref(), Some("x\n### Part\ny"));
        assert_eq!(tracker_current_state("## Current state\n\n## Log\n"), None);
    }

    #[test]
    fn a_log_line_skips_code_and_keeps_the_logs_spacing() {
        let d = Date::parse_iso("2026-11-02").unwrap();
        // A bullet in a code block isn't an entry.
        let page = "## Log\n\n```\n- not an entry\n```\n\n- 2026-10-01: a\n";
        let new = tracker_with_state(page, None, Some((d, "b"))).unwrap();
        assert_eq!(new, "## Log\n\n```\n- not an entry\n```\n\n- 2026-11-02: b\n- 2026-10-01: a\n");
        // A Log with a blank line between its entries gets one after the new line too.
        let page = "## Log\n\n- 2026-10-02: b\n\n- 2026-10-01: a\n";
        let new = tracker_with_state(page, None, Some((d, "c"))).unwrap();
        assert_eq!(new, "## Log\n\n- 2026-11-02: c\n\n- 2026-10-02: b\n\n- 2026-10-01: a\n");
        // Sorting keeps the first two entries' spacing: tight here, though a later pair was loose.
        let page = "## Current state\n\n## Blockers\n\n## Next\n\n## Log\n\n- 2026-09-01: a\n- 2026-09-02: b\n\n- 2026-09-03: c\n";
        let (new, r) = reshape_tracker(page);
        assert!(r.auto(), "{r:?}");
        assert!(new.ends_with("## Log\n\n- 2026-09-03: c\n- 2026-09-02: b\n- 2026-09-01: a\n"), "{new}");
    }

    #[test]
    fn a_crlf_tracker_keeps_its_line_endings() {
        let page = TRACKER.replace('\n', "\r\n");
        let (new, r) = reshape_tracker(&page);
        assert!(r.auto(), "{r:?}");
        assert_eq!(new, reshape_tracker(TRACKER).0.replace('\n', "\r\n"));
        assert!(!new.replace("\r\n", "").contains('\n'));
        let (again, _) = reshape_tracker(&new);
        assert_eq!(again, new);
    }

    #[test]
    fn missing_tracker_sections_go_in_shape_order() {
        // Among shape sections out of order and others: after the nearest before it in the shape.
        let page = "## Jira\n\nj\n\n## Current state\n\ncs\n\n## Log\n\nl\n";
        assert_eq!(tracker_insert(page, 1, "b"), "## Jira\n\nj\n\n## Current state\n\ncs\n\n## Blockers\n\nb\n\n## Log\n\nl\n");
        // None before it: above the nearest after it, not above an unknown section.
        let page = "## Jira\n\nj\n\n## Next\n\nn\n";
        assert_eq!(tracker_insert(page, 0, "cs"), "## Jira\n\nj\n\n## Current state\n\ncs\n\n## Next\n\nn\n");
        // After the last section when that's the nearest before it.
        assert_eq!(tracker_insert("## Next\n\nn\n", 3, "- x"), "## Next\n\nn\n\n## Log\n\n- x\n");
    }
}
