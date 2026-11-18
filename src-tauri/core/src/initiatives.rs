// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Initiatives (D-20261118-08): the handful of things the user is tracking, one wiki page each in
//! `wiki/initiatives/`, in the page shape with two sections of their own. **Actions** holds one list
//! item per action, its owner, status and due date as inline fields and a block id at the end, with
//! the quotes it rests on indented under it:
//!
//! ```markdown
//! - Send the revised pricing to Acme [owner:: me] [status:: open] [due:: 2026-11-25] ^a-3f9c
//!   - "Can you send the revised pricing by Tuesday?" — [[Mail. Acme pricing - 2026-11-17]]
//! ```
//!
//! `[owner:: me]` is the user's own, `[delegated:: Lena]` one the user handed to Lena (a
//! waiting-for), `[owner:: Lena]` Lena's own. **Decisions** holds one item per decision, dated,
//! with its quotes the same way. Plain list items, never checkboxes, so they stay out of Tasks.
//!
//! `parse` reads a page; `with_action` and `with_decision` change one item and leave every other
//! line as it was.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::markdown;

pub const FOLDER: &str = "wiki/initiatives";
/// Where ingest puts actions that fit no initiative (D-20261118-02).
pub const UNFILED: &str = "wiki/initiatives/Unfiled.md";
pub const ACTIONS: &str = "Actions";
pub const DECISIONS: &str = "Decisions";

/// Whether `rel` is an initiative page.
pub fn is_initiative(rel: &str) -> bool {
    rel.starts_with("wiki/initiatives/") && rel.ends_with(".md") && rel.matches('/').count() == 2
}

/// Whose an action is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Owner {
    /// The user's own.
    Me,
    /// Handed by the user to `person`: a waiting-for.
    Delegated,
    /// `person`'s own.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Open,
    Done,
    Dropped,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Done => "done",
            Status::Dropped => "dropped",
        }
    }
    fn parse(s: &str) -> Option<Status> {
        match s.trim().to_lowercase().as_str() {
            "open" => Some(Status::Open),
            "done" => Some(Status::Done),
            "dropped" => Some(Status::Dropped),
            _ => None,
        }
    }
}

/// A quote an action or decision rests on, and the note or source it's from (a link target, with
/// a heading or block if it has one: `Mail. Acme pricing - 2026-11-17#Branch 2`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub text: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    /// The block id, without `^` ("" for one written without).
    pub id: String,
    pub text: String,
    /// None when the line names no owner (Health says so).
    pub owner: Option<Owner>,
    /// Who it's delegated to, or whose it is; None for the user's own.
    pub person: Option<String>,
    pub status: Status,
    /// `YYYY-MM-DD`.
    pub due: Option<String>,
    pub quotes: Vec<Quote>,
    /// 0-based line in the page.
    pub line: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub id: String,
    /// `YYYY-MM-DD`.
    pub date: Option<String>,
    pub text: String,
    pub quotes: Vec<Quote>,
    pub line: usize,
}

/// Something about a line Health should say.
#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Initiative {
    pub path: String,
    pub name: String,
    /// The frontmatter's `status`: active (the default), paused or done.
    pub status: String,
    pub description: String,
    pub actions: Vec<Action>,
    pub decisions: Vec<Decision>,
    pub problems: Vec<Problem>,
}

/// An initiative's statuses, as its frontmatter has them.
pub const STATUSES: [&str; 3] = ["active", "paused", "done"];

/// `[key:: value]`, the value plain words or a `[[link]]` (as Health's cross-link fix makes a name).
static FIELD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([A-Za-z][\w-]*)::[ \t]*((?:\[\[[^\]\n]*\]\]|[^\[\]\n])*?)[ \t]*\]").unwrap());
static LINKED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[\[([^\]|#]+)(?:[#|][^\]]*)?\]\]$").unwrap());

/// A person as a field names them: `Lena` or `[[Lena]]` (`[[Lena Smith|Lena]]` is Lena Smith).
fn person_of(v: &str) -> String {
    LINKED.captures(v.trim()).map_or_else(|| v.trim().to_string(), |c| c[1].trim().to_string())
}
static BLOCK_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|\s)\^([A-Za-z0-9-]+)\s*$").unwrap());
static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[-*+][ \t]+(.*)$").unwrap());
static QUOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\s+[-*+][ \t]+["“](.*)["”][ \t]*(?:[—–-][ \t]*)?\[\[([^\]|]+)(?:\|[^\]]*)?\]\][ \t]*$"#).unwrap());
static DATED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d{4}-\d{2}-\d{2})[ \t]*(?:[—–:-][ \t]*)?(.*)$").unwrap());
static DAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());

/// One list item in a section: its head line and the indented lines under it.
struct Item<'a> {
    /// Byte offsets: the head line's start and the end of the item's last line (after its `\n`).
    start: usize,
    end: usize,
    line: usize,
    head: &'a str,
    children: Vec<&'a str>,
}

/// A section's body and its list items: (body start, section end, items).
fn section_items<'a>(page: &'a str, heading: &str) -> Option<(usize, usize, Vec<Item<'a>>)> {
    let p = crate::pageshape::parse(page);
    let s = p.sections.iter().find(|s| s.heading.trim().eq_ignore_ascii_case(heading))?;
    let at = s.text.as_ptr() as usize - page.as_ptr() as usize;
    let end = at + s.text.len();
    let body = at + s.text.find('\n').map_or(s.text.len(), |i| i + 1);
    let mut items: Vec<Item> = Vec::new();
    let mut open = false;
    for l in markdown::lines(page, body) {
        if l.start >= end {
            break;
        }
        let after = (l.start + l.text.len() + 1).min(page.len());
        if l.code {
            open = false;
            continue;
        }
        if let Some(c) = ITEM.captures(l.text) {
            items.push(Item { start: l.start, end: after, line: l.no, head: c.get(1).map_or("", |m| m.as_str()), children: vec![] });
            open = true;
        } else if open && !l.text.trim().is_empty() && l.text.starts_with([' ', '\t']) {
            let it = items.last_mut().expect("open means an item");
            it.children.push(l.text);
            it.end = after;
        } else {
            open = false;
        }
    }
    Some((body, end, items))
}

/// A head line's fields (in order), block id and the words left.
fn split_head(head: &str) -> (Vec<(String, String)>, String, String) {
    let (rest, id) = match BLOCK_ID.captures(head) {
        Some(c) => (&head[..c.get(0).unwrap().start()], c[1].to_string()),
        None => (head, String::new()),
    };
    let fields = FIELD.captures_iter(rest).map(|c| (c[1].to_lowercase(), c[2].trim().to_string())).collect();
    let words = FIELD.replace_all(rest, " ");
    (fields, id, words.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn quotes_of(children: &[&str]) -> Vec<Quote> {
    children
        .iter()
        .filter_map(|c| QUOTE.captures(c))
        .map(|c| Quote { text: c[1].trim().to_string(), source: c[2].trim().to_string() })
        .collect()
}

fn action_of(it: &Item, problems: &mut Vec<Problem>) -> Action {
    let (fields, id, text) = split_head(it.head);
    let get = |k: &str| fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone()).filter(|v| !v.is_empty());
    let (owner, person) = match (get("owner"), get("delegated")) {
        (_, Some(p)) => (Some(Owner::Delegated), Some(person_of(&p))),
        (Some(o), None) if o.eq_ignore_ascii_case("me") => (Some(Owner::Me), None),
        (Some(o), None) => (Some(Owner::Other), Some(person_of(&o))),
        (None, None) => (None, None),
    };
    let short = short(&text);
    if owner.is_none() {
        problems.push(Problem {
            line: it.line,
            text: format!("“{short}” has no owner: add [owner:: me], [delegated:: <name>] or [owner:: <name>]"),
        });
    }
    let status = match get("status") {
        None => Status::Open,
        Some(s) => Status::parse(&s).unwrap_or_else(|| {
            problems.push(Problem { line: it.line, text: format!("“{short}” has the status “{s}”: open, done or dropped") });
            Status::Open
        }),
    };
    let due = get("due");
    if let Some(d) = due.as_deref().filter(|d| !valid_day(d)) {
        problems.push(Problem { line: it.line, text: format!("“{short}” is due “{d}”, not a date (YYYY-MM-DD)") });
    }
    Action { id, text, owner, person, status, due, quotes: quotes_of(&it.children), line: it.line }
}

fn decision_of(it: &Item) -> Decision {
    let (_, id, text) = split_head(it.head);
    let (date, text) = match DATED.captures(&text) {
        Some(c) if valid_day(&c[1]) => (Some(c[1].to_string()), c[2].trim().to_string()),
        _ => (None, text),
    };
    Decision { id, date, text, quotes: quotes_of(&it.children), line: it.line }
}

fn valid_day(d: &str) -> bool {
    DAY.is_match(d) && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()
}

fn short(text: &str) -> String {
    let t: String = text.chars().take(60).collect();
    if t.len() < text.len() {
        format!("{}…", t.trim_end())
    } else {
        t
    }
}

/// Reads an initiative page.
pub fn parse(rel: &str, page: &str) -> Initiative {
    let page = page.replace("\r\n", "\n");
    let fm = crate::frontmatter::split(&page);
    let stem = rel.rsplit('/').next().unwrap_or(rel).trim_end_matches(".md").to_string();
    let name = crate::frontmatter::string(&fm.data, "name").unwrap_or(stem);
    let status = crate::frontmatter::string(&fm.data, "status").map(|s| s.to_lowercase()).unwrap_or_else(|| "active".into());
    let description = crate::frontmatter::string(&fm.data, "description").unwrap_or_default();
    let mut problems = Vec::new();
    if !STATUSES.contains(&status.as_str()) {
        problems.push(Problem { line: 0, text: format!("The initiative's status is “{status}”: active, paused or done") });
    }
    let actions: Vec<Action> =
        section_items(&page, ACTIONS).map(|(_, _, items)| items.iter().map(|i| action_of(i, &mut problems)).collect()).unwrap_or_default();
    let decisions: Vec<Decision> =
        section_items(&page, DECISIONS).map(|(_, _, items)| items.iter().map(decision_of).collect()).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    for (id, line) in actions.iter().map(|a| (&a.id, a.line)).chain(decisions.iter().map(|d| (&d.id, d.line))) {
        if !id.is_empty() && !seen.insert(id.clone()) {
            problems.push(Problem { line, text: format!("^{id} is on more than one item: give one a new id") });
        }
    }
    Initiative { path: rel.to_string(), name, status, description, actions, decisions, problems }
}

/// A new initiative page: in the page shape, with empty Actions and Decisions.
pub fn new_page(name: &str, about: &str) -> String {
    let about = about.split_whitespace().collect::<Vec<_>>().join(" ");
    let description = if about.is_empty() { String::new() } else { format!(" {}", yaml_str(&about)) };
    let opening = if about.is_empty() { String::new() } else { format!("{about}\n\n") };
    format!(
        "---\ntype: initiative\nname: {}\nstatus: active\ndescription:{description}\nsources: []\n---\n\n{opening}## Current state\n\n## {ACTIONS}\n\n## {DECISIONS}\n",
        yaml_str(name.trim())
    )
}

/// A YAML scalar: as it is when that reads back the same, else double-quoted.
fn yaml_str(s: &str) -> String {
    match serde_yaml_ng::from_str::<serde_json::Value>(&format!("k: {s}")) {
        Ok(v) if v.get("k").and_then(|v| v.as_str()) == Some(s) => s.to_string(),
        _ => serde_json::to_string(s).unwrap_or_default(),
    }
}

/// A new id for `prefix` (`a` an action, `d` a decision) that isn't on the page yet, from `seed`.
pub fn new_id(page: &str, prefix: &str, seed: &str) -> String {
    let h = crate::write::version(seed.as_bytes());
    (4..h.len())
        .map(|n| format!("{prefix}-{}", &h[..n]))
        .find(|id| !page.contains(&format!("^{id}")))
        .unwrap_or_else(|| format!("{prefix}-{h}"))
}

/// What to change on an action, or a new one (`id` None).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActionEdit {
    /// The action's id (with or without `^`), or its words exactly; None makes a new action.
    pub id: Option<String>,
    pub text: Option<String>,
    pub owner: Option<Owner>,
    /// Who it's delegated to or whose it is; needed with delegated and other.
    pub person: Option<String>,
    pub status: Option<Status>,
    /// Some("") clears it.
    pub due: Option<String>,
    /// Added under it, each once.
    #[serde(default)]
    pub quotes: Vec<Quote>,
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A field value can't hold `]` or a newline, unless it's one `[[link]]`.
fn field_value(s: &str) -> Result<String, String> {
    let v = one_line(s);
    if v.contains([']', '[']) && !LINKED.is_match(&v) {
        return Err(format!("“{v}” can't have square brackets in it."));
    }
    Ok(v)
}

fn quote_line(q: &Quote) -> Result<String, String> {
    let text = one_line(&q.text).replace('"', "”");
    let source = q.source.trim().trim_start_matches("[[").trim_end_matches("]]").trim();
    if text.is_empty() || source.is_empty() {
        return Err("A quote needs its words and its source.".into());
    }
    Ok(format!("  - \"{text}\" — [[{source}]]\n"))
}

/// The quote lines of `quotes` not already among `have`.
fn new_quote_lines(have: &[Quote], quotes: &[Quote]) -> Result<String, String> {
    let norm = |q: &Quote| (crate::proposals::normalise_quote(&q.text), crate::links::key(q.source.split('#').next().unwrap_or("")));
    let mut seen: Vec<_> = have.iter().map(norm).collect();
    let mut out = String::new();
    for q in quotes {
        let k = norm(q);
        if !seen.contains(&k) {
            out.push_str(&quote_line(q)?);
            seen.push(k);
        }
    }
    Ok(out)
}

/// Puts `block` (whole lines) at the end of `heading`'s list, or at the end of the section when it
/// has none; a page without the section gets it where the page shape puts a topical section.
fn insert_in_section(page: &str, heading: &str, block: &str, at_item: Option<usize>) -> Result<String, String> {
    let Some((body, end, items)) = section_items(page, heading) else {
        return crate::pageshape::with_topic(page, heading, block.trim_end());
    };
    let at = at_item.or(items.last().map(|i| i.end));
    if let Some(at) = at {
        let nl = if page[..at].ends_with('\n') { "" } else { "\n" };
        return Ok(format!("{}{nl}{block}{}", &page[..at], &page[at..]));
    }
    let content = page[body..end].trim_end();
    let after = &page[end..];
    let lead = if content.is_empty() { "\n".to_string() } else { format!("{content}\n\n") };
    let tail = if after.is_empty() { "" } else { "\n" };
    Ok(format!("{}{lead}{block}{tail}{after}", &page[..body]))
}

fn find_item<'a>(items: &'a [Item<'a>], key: &str) -> Option<&'a Item<'a>> {
    let k = key.trim().trim_start_matches('^');
    items.iter().find(|i| split_head(i.head).1 == k).or_else(|| {
        let words = one_line(key).to_lowercase();
        let hits: Vec<_> = items.iter().filter(|i| split_head(i.head).2.to_lowercase() == words).collect();
        if hits.len() == 1 {
            Some(hits[0])
        } else {
            None
        }
    })
}

fn action_head(
    text: &str,
    owner: Owner,
    person: Option<&str>,
    status: Status,
    due: Option<&str>,
    extra: &[(String, String)],
    id: &str,
) -> String {
    let mut h = format!("- {text}");
    match (owner, person) {
        (Owner::Me, _) => h.push_str(" [owner:: me]"),
        (Owner::Delegated, Some(p)) => h.push_str(&format!(" [delegated:: {p}]")),
        (Owner::Other, Some(p)) => h.push_str(&format!(" [owner:: {p}]")),
        _ => {}
    }
    h.push_str(&format!(" [status:: {}]", status.as_str()));
    if let Some(d) = due {
        h.push_str(&format!(" [due:: {d}]"));
    }
    for (k, v) in extra {
        h.push_str(&format!(" [{k}:: {v}]"));
    }
    if !id.is_empty() {
        h.push_str(&format!(" ^{id}"));
    }
    h
}

/// The page with an action added or changed, and the action's id. Only that action's lines change.
pub fn with_action(page: &str, edit: &ActionEdit, seed: &str) -> Result<(String, String), String> {
    let page = page.replace("\r\n", "\n");
    let person = edit.person.as_deref().map(field_value).transpose()?.filter(|p| !p.is_empty());
    if person.as_deref().is_some_and(|p| p.eq_ignore_ascii_case("me")) {
        return Err("For the user's own action, give owner me and no person.".into());
    }
    let due = match edit.due.as_deref().map(str::trim) {
        Some("") => Some(None),
        Some(d) if valid_day(d) => Some(Some(d.to_string())),
        Some(d) => return Err(format!("“{d}” isn't a date: give due as YYYY-MM-DD.")),
        None => None,
    };
    let text = edit.text.as_deref().map(one_line).filter(|t| !t.is_empty());
    let items = section_items(&page, ACTIONS).map(|(_, _, i)| i).unwrap_or_default();
    match edit.id.as_deref().filter(|k| !k.trim().is_empty()) {
        Some(key) => {
            let it = find_item(&items, key).ok_or_else(|| format!("There's no action “{key}” on this page."))?;
            let mut problems = Vec::new();
            let a = action_of(it, &mut problems);
            let (fields, id, _) = split_head(it.head);
            // The person as the line spells them (a [[link]] stays one) while they don't change.
            let raw = ["delegated", "owner"].iter().find_map(|f| fields.iter().find(|(k, _)| k == f)).map(|(_, v)| v.clone());
            let extra: Vec<(String, String)> =
                fields.into_iter().filter(|(k, _)| !["owner", "delegated", "status", "due"].contains(&k.as_str())).collect();
            let owner = edit.owner.or(a.owner).ok_or("Give the action an owner: me, delegated or other.")?;
            let same = |p: &str| a.person.as_deref().is_some_and(|q| q.eq_ignore_ascii_case(&person_of(p)));
            let person = match owner {
                Owner::Me => None,
                _ => Some(match (person, raw) {
                    (Some(p), _) if !same(&p) => p,
                    (_, Some(r)) if a.person.is_some() => r,
                    _ => return Err("Give the person it's delegated to, or whose it is.".into()),
                }),
            };
            let due = due.unwrap_or(a.due.clone());
            let head = action_head(
                text.as_deref().unwrap_or(&a.text),
                owner,
                person.as_deref(),
                edit.status.unwrap_or(a.status),
                due.as_deref(),
                &extra,
                &id,
            );
            let quotes = new_quote_lines(&a.quotes, &edit.quotes)?;
            let head_end = it.head.as_ptr() as usize - page.as_ptr() as usize + it.head.len();
            let mut out = format!("{}{head}{}", &page[..it.start], &page[head_end..it.end]);
            if !quotes.is_empty() {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&quotes);
            }
            out.push_str(&page[it.end..]);
            let id = if id.is_empty() { a.text } else { id };
            Ok((out, id))
        }
        None => {
            let text = text.ok_or("Give the new action's words in text.")?;
            let owner = edit.owner.ok_or("Give the new action an owner: me, delegated or other.")?;
            if owner != Owner::Me && person.is_none() {
                return Err("Give the person it's delegated to, or whose it is.".into());
            }
            let id = new_id(&page, "a", &format!("{seed}\n{text}"));
            let head = action_head(
                &text,
                owner,
                person.as_deref().filter(|_| owner != Owner::Me),
                edit.status.unwrap_or_default(),
                due.flatten().as_deref(),
                &[],
                &id,
            );
            let block = format!("{head}\n{}", new_quote_lines(&[], &edit.quotes)?);
            Ok((insert_in_section(&page, ACTIONS, &block, None)?, id))
        }
    }
}

/// The page with a decision added, newest first by date (undated ones last), and its id.
pub fn with_decision(page: &str, date: Option<&str>, text: &str, quotes: &[Quote], seed: &str) -> Result<(String, String), String> {
    let page = page.replace("\r\n", "\n");
    let text = one_line(text);
    if text.is_empty() {
        return Err("Give the decision's words in text.".into());
    }
    let date = date.map(str::trim).filter(|d| !d.is_empty());
    if let Some(d) = date.filter(|d| !valid_day(d)) {
        return Err(format!("“{d}” isn't a date: give date as YYYY-MM-DD."));
    }
    let id = new_id(&page, "d", &format!("{seed}\n{text}"));
    let head = match date {
        Some(d) => format!("- {d} — {text} ^{id}"),
        None => format!("- {text} ^{id}"),
    };
    let block = format!("{head}\n{}", new_quote_lines(&[], quotes)?);
    let items = section_items(&page, DECISIONS).map(|(_, _, i)| i).unwrap_or_default();
    let before = date.and_then(|d| items.iter().find(|i| decision_of(i).date.as_deref().is_none_or(|o| o < d)).map(|i| i.start));
    let out = match before {
        Some(at) => format!("{}{block}{}", &page[..at], &page[at..]),
        None => insert_in_section(&page, DECISIONS, &block, None)?,
    };
    Ok((out, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "---\ntype: initiative\nname: Acme renewal\nstatus: active\n---\n# Acme renewal\n\nRenewing Acme.\n\n## Current state\n\nPricing is out.\n\n## Actions\n\n- Send the revised pricing [owner:: me] [status:: open] [due:: 2026-11-25] ^a-1111\n  - \"Can you send the revised pricing by Tuesday?\" — [[Mail. Acme pricing - 2026-11-17]]\n- Book the legal review [delegated:: Lena] [status:: open] ^a-2222\n- Sign off the budget [owner:: Sam] [status:: done] ^a-3333\n\n## Decisions\n\n- 2026-11-12 — Annual billing ^d-1111\n  - \"We agreed on annual billing\" — [[Meeting. Acme - 2026-11-12]]\n\n## See also\n\n- [[Acme Pilot]]\n";

    #[test]
    fn reads_actions_and_decisions() {
        let i = parse("wiki/initiatives/Acme renewal.md", PAGE);
        assert_eq!(i.name, "Acme renewal");
        assert_eq!(i.status, "active");
        assert_eq!(i.actions.len(), 3);
        let a = &i.actions[0];
        assert_eq!(
            (a.id.as_str(), a.text.as_str(), a.owner, a.due.as_deref()),
            ("a-1111", "Send the revised pricing", Some(Owner::Me), Some("2026-11-25"))
        );
        assert_eq!(
            a.quotes,
            vec![Quote { text: "Can you send the revised pricing by Tuesday?".into(), source: "Mail. Acme pricing - 2026-11-17".into() }]
        );
        assert_eq!((i.actions[1].owner, i.actions[1].person.as_deref()), (Some(Owner::Delegated), Some("Lena")));
        assert_eq!((i.actions[2].owner, i.actions[2].status), (Some(Owner::Other), Status::Done));
        assert_eq!(i.decisions[0].date.as_deref(), Some("2026-11-12"));
        assert_eq!(i.decisions[0].text, "Annual billing");
        assert!(i.problems.is_empty(), "{:?}", i.problems);
    }

    #[test]
    fn says_what_is_wrong_with_a_line() {
        let p = PAGE.replace("[delegated:: Lena] [status:: open]", "[status:: maybe] [due:: soon]").replace("^a-3333", "^a-1111");
        let i = parse("wiki/initiatives/Acme renewal.md", &p);
        let t: Vec<_> = i.problems.iter().map(|p| p.text.as_str()).collect();
        assert!(t.iter().any(|t| t.contains("has no owner")), "{t:?}");
        assert!(t.iter().any(|t| t.contains("“maybe”")), "{t:?}");
        assert!(t.iter().any(|t| t.contains("“soon”")), "{t:?}");
        assert!(t.iter().any(|t| t.contains("^a-1111 is on more than one item")), "{t:?}");
    }

    #[test]
    fn adds_an_action_after_the_others() {
        let edit = ActionEdit {
            text: Some("Chase the signed order".into()),
            owner: Some(Owner::Delegated),
            person: Some("Maya".into()),
            quotes: vec![Quote { text: "Maya will chase it".into(), source: "[[Meeting. Acme - 2026-11-12]]".into() }],
            ..Default::default()
        };
        let (out, id) = with_action(PAGE, &edit, "seed").unwrap();
        assert!(id.starts_with("a-"));
        let want = format!("^a-3333\n- Chase the signed order [delegated:: Maya] [status:: open] ^{id}\n  - \"Maya will chase it\" — [[Meeting. Acme - 2026-11-12]]\n\n## Decisions");
        assert!(out.contains(&want), "{out}");
        assert_eq!(out.replace(&want, "^a-3333\n\n## Decisions"), PAGE);
    }

    #[test]
    fn changes_one_action_and_keeps_the_rest() {
        let edit = ActionEdit {
            id: Some("^a-2222".into()),
            status: Some(Status::Done),
            due: Some("2026-12-01".into()),
            quotes: vec![Quote { text: "Legal review booked".into(), source: "Mail. Legal - 2026-11-19".into() }],
            ..Default::default()
        };
        let (out, id) = with_action(PAGE, &edit, "s").unwrap();
        assert_eq!(id, "a-2222");
        let want = "- Book the legal review [delegated:: Lena] [status:: done] [due:: 2026-12-01] ^a-2222\n  - \"Legal review booked\" — [[Mail. Legal - 2026-11-19]]\n";
        assert!(out.contains(want), "{out}");
        assert_eq!(out.replace(want, "- Book the legal review [delegated:: Lena] [status:: open] ^a-2222\n"), PAGE);
    }

    #[test]
    fn reassigning_a_delegated_action_and_finding_it_by_its_words() {
        let edit = ActionEdit { id: Some("book the legal review".into()), owner: Some(Owner::Me), ..Default::default() };
        let (out, _) = with_action(PAGE, &edit, "s").unwrap();
        assert!(out.contains("- Book the legal review [owner:: me] [status:: open] ^a-2222\n"), "{out}");
        let edit = ActionEdit { id: Some("a-1111".into()), owner: Some(Owner::Delegated), ..Default::default() };
        assert!(with_action(PAGE, &edit, "s").unwrap_err().contains("person"));
    }

    #[test]
    fn a_quote_already_there_isnt_added_again() {
        let edit = ActionEdit {
            id: Some("a-1111".into()),
            quotes: vec![Quote {
                text: "can you send the  revised pricing by Tuesday?".into(),
                source: "Mail. Acme pricing - 2026-11-17#Branch 1".into(),
            }],
            ..Default::default()
        };
        assert_eq!(with_action(PAGE, &edit, "s").unwrap().0, PAGE);
    }

    #[test]
    fn a_name_health_linked_still_reads_and_stays_linked() {
        let p = PAGE.replace("[delegated:: Lena]", "[delegated:: [[Lena]]]").replace("[owner:: Sam]", "[owner:: [[Sam Lee|Sam]]]");
        let i = parse("wiki/initiatives/Acme renewal.md", &p);
        assert!(i.problems.is_empty(), "{:?}", i.problems);
        assert_eq!((i.actions[1].owner, i.actions[1].person.as_deref()), (Some(Owner::Delegated), Some("Lena")));
        assert_eq!(i.actions[1].id, "a-2222");
        assert_eq!(i.actions[2].person.as_deref(), Some("Sam Lee"));
        let edit = ActionEdit { id: Some("a-2222".into()), status: Some(Status::Done), person: Some("lena".into()), ..Default::default() };
        assert!(with_action(&p, &edit, "s")
            .unwrap()
            .0
            .contains("- Book the legal review [delegated:: [[Lena]]] [status:: done] ^a-2222\n"));
        // Lena's own now: the same person, as written.
        let edit = ActionEdit { id: Some("a-2222".into()), owner: Some(Owner::Other), ..Default::default() };
        assert!(with_action(&p, &edit, "s").unwrap().0.contains("- Book the legal review [owner:: [[Lena]]] [status:: open] ^a-2222\n"));
        let edit = ActionEdit { id: Some("a-2222".into()), person: Some("[[Maya]]".into()), ..Default::default() };
        assert!(with_action(&p, &edit, "s").unwrap().0.contains("[delegated:: [[Maya]]]"));
    }

    #[test]
    fn health_linking_a_name_keeps_the_page_readable() {
        let mut p = PAGE.to_string();
        for n in ["Lena", "Sam"] {
            if let Some(t) = crate::lint::link_first_mention(&p, n) {
                p = t;
            }
        }
        let i = parse("wiki/initiatives/Acme renewal.md", &p);
        assert!(i.problems.is_empty(), "{:?}\n{p}", i.problems);
        assert_eq!(i.actions[1].person.as_deref(), Some("Lena"), "{p}");
        assert_eq!(i.actions[2].person.as_deref(), Some("Sam"), "{p}");
    }

    #[test]
    fn keeps_fields_it_doesnt_know() {
        let p = PAGE.replace("[status:: open] ^a-2222", "[status:: open] [task:: Me. To Do List] ^a-2222");
        let edit = ActionEdit { id: Some("a-2222".into()), status: Some(Status::Dropped), ..Default::default() };
        assert!(with_action(&p, &edit, "s").unwrap().0.contains("[status:: dropped] [task:: Me. To Do List] ^a-2222"));
    }

    #[test]
    fn decisions_go_newest_first() {
        let (out, id) = with_decision(PAGE, Some("2026-11-18"), "Two-year term", &[], "s").unwrap();
        assert!(out.contains(&format!("## Decisions\n\n- 2026-11-18 — Two-year term ^{id}\n- 2026-11-12 — Annual billing")), "{out}");
        let (out, id) = with_decision(PAGE, Some("2026-11-01"), "Kick-off", &[], "s").unwrap();
        assert!(out.contains(&format!("[[Meeting. Acme - 2026-11-12]]\n- 2026-11-01 — Kick-off ^{id}\n\n## See also")), "{out}");
    }

    #[test]
    fn a_new_page_is_in_the_shape_and_takes_actions() {
        let p = new_page("Acme renewal", "Renewing Acme's contract.");
        assert!(crate::pageshape::in_shape(&p, &|_| false), "{p}");
        let i = parse("wiki/initiatives/Acme renewal.md", &p);
        assert_eq!((i.name.as_str(), i.description.as_str(), i.actions.len()), ("Acme renewal", "Renewing Acme's contract.", 0));
        let edit = ActionEdit { text: Some("Draft the renewal".into()), owner: Some(Owner::Me), ..Default::default() };
        let (out, id) = with_action(&p, &edit, "s").unwrap();
        assert!(out.contains(&format!("## Actions\n\n- Draft the renewal [owner:: me] [status:: open] ^{id}\n\n## Decisions\n")), "{out}");
        assert!(crate::pageshape::in_shape(&out, &|_| false), "{out}");
    }

    #[test]
    fn a_page_without_the_section_gets_it_above_the_timeline() {
        let p = "---\ntype: initiative\n---\n# X\n\n## Current state\n\nGoing.\n\n## Timeline\n\n### 2026-11-01 — Start\n\nSource: [[A]]\n";
        let (out, _) = with_decision(p, None, "Go", &[], "s").unwrap();
        assert!(out.contains("Going.\n\n## Decisions\n\n- Go ^d-"), "{out}");
        assert!(out.find("## Decisions").unwrap() < out.find("## Timeline").unwrap());
    }
}
