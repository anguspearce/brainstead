// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The facts ingest checked, kept (D-20261006-08, -16): one JSON file per wiki page at
//! `wiki/.claims/<folder>/<page>.json`, written with the page's change. Each claim names the
//! source it came from and the Timeline entry it belongs to; a source's claims on a page are one
//! set, replaced when it's ingested again. A newer claim on the same subject and attribute
//! supersedes an older one and both are kept: `facts` gives the latest with the history.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::contradictions::{normalise_attribute, parse_as_of};

pub const DIR: &str = "wiki/.claims";

/// One fact, as kept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    pub subject: String,
    pub attribute: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
    #[serde(default)]
    pub quote: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// The source's vault path.
    pub source: String,
    /// The Timeline entry's heading it belongs to ("2026-10-02 — Steerco"), when it made one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// YYYY-MM-DD.
    #[serde(default)]
    pub recorded: String,
}

impl Kept {
    /// A checked claim from an ingest of `source`, its attribute in the contradiction check's form.
    pub fn from_claim(c: &crate::ingest::Claim, source: &str, entry: Option<&str>, recorded: &str) -> Kept {
        Kept {
            subject: c.subject.trim().to_string(),
            attribute: normalise_attribute(&c.attribute),
            value: c.value.trim().to_string(),
            as_of: c.as_of.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from),
            quote: c.quote.trim().to_string(),
            anchor: c.anchor.clone(),
            source: source.to_string(),
            entry: entry.map(String::from),
            recorded: recorded.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub page: String,
    #[serde(default)]
    pub claims: Vec<Kept>,
}

/// What a change does to its page's claims: `source`'s set becomes `claims`, or, with `from`, the
/// claims from `from` (as `is_from` matches it) say `source` instead. Kept in the change with the
/// claims file before and after it (by hash in the change store), for Revert.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub source: String,
    pub claims: Vec<Kept>,
    /// A source renamed (Link to… on a missing page): `claims` is unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// The claims file of a wiki page (`wiki/entities/Orbit App.md` → `wiki/.claims/entities/Orbit
/// App.json`); None for anything else.
pub fn path(page: &str) -> Option<String> {
    let rest = page.strip_prefix("wiki/")?.strip_suffix(".md")?;
    if rest.is_empty() || rest.starts_with('.') || !rest.contains('/') {
        return None;
    }
    Some(format!("{DIR}/{rest}.json"))
}

/// A claims file's text read; an unreadable one reads as empty.
pub fn parse(text: &str) -> File {
    serde_json::from_str(text).unwrap_or_default()
}

/// The claims file with `source`'s claims on `page` replaced by `claims` (in its place when it had
/// some, else at the end). None when nothing is left: the file goes.
pub fn with_source(current: Option<&str>, page: &str, source: &str, claims: &[Kept]) -> Option<String> {
    let mut f = current.map(parse).unwrap_or_default();
    f.page = page.to_string();
    let at = f.claims.iter().position(|c| c.source == source).unwrap_or(f.claims.len());
    f.claims.retain(|c| c.source != source);
    let at = at.min(f.claims.len());
    f.claims.splice(at..at, claims.iter().cloned());
    if f.claims.is_empty() {
        return None;
    }
    Some(format!("{}\n", serde_json::to_string_pretty(&f).unwrap_or_default()))
}

/// `source`'s claims in a claims file's text.
pub fn of_source(text: Option<&str>, source: &str) -> Vec<Kept> {
    text.map(parse).map(|f| f.claims.into_iter().filter(|c| c.source == source).collect()).unwrap_or_default()
}

/// The claims file once `u` is made on it.
pub fn updated(current: Option<&str>, page: &str, u: &Update) -> Option<String> {
    match &u.from {
        Some(from) => current.map(|t| renamed(t, from, &u.source).unwrap_or_else(|| t.to_string())),
        None => with_source(current, page, &u.source, &u.claims),
    }
}

/// The claims file with `u` undone (`before` is the file before it). An ingest's: its source's
/// claims as they were before it, whatever other sources have added since; a source renamed since
/// is followed, the claims `u` wrote, found under another source, being that source's now. A
/// source renamed (`from`): only the claims it renamed, still as they were, take their old source
/// back; claims written under the new name since (a later ingest) stay.
pub fn reverted(before: Option<&str>, current: Option<&str>, page: &str, u: &Update) -> Option<String> {
    let Some(from) = &u.from else {
        let now = current_name(current, &u.source, &u.claims);
        let was: Vec<Kept> = of_source(before, &u.source).into_iter().map(|c| Kept { source: now.clone(), ..c }).collect();
        return with_source(current, page, &now, &was);
    };
    let text = current?;
    let mut left: Vec<Kept> = before.map(parse).unwrap_or_default().claims.into_iter().filter(|c| is_from(&c.source, from)).collect();
    let mut f = parse(text);
    let mut back = 0;
    for c in f.claims.iter_mut().filter(|c| c.source == u.source) {
        if let Some(i) = left.iter().position(|w| Kept { source: u.source.clone(), ..w.clone() } == *c) {
            c.source = left.remove(i).source;
            back += 1;
        }
    }
    if back == 0 {
        return Some(text.to_string());
    }
    Some(format!("{}\n", serde_json::to_string_pretty(&f).unwrap_or_default()))
}

/// What `source` is called in the claims file now: itself while it has claims there, else the
/// source whose claims are `wrote` (the claims made from it, renamed since), else itself.
fn current_name(text: Option<&str>, source: &str, wrote: &[Kept]) -> String {
    let f = text.map(parse).unwrap_or_default();
    if wrote.is_empty() || f.claims.iter().any(|c| c.source == source) {
        return source.to_string();
    }
    let bare = |cs: Vec<&Kept>| cs.into_iter().map(|c| Kept { source: String::new(), ..c.clone() }).collect::<Vec<_>>();
    let wrote = bare(wrote.iter().collect());
    let mut names: Vec<&str> = f.claims.iter().map(|c| c.source.as_str()).collect();
    names.dedup();
    names.into_iter().find(|n| bare(f.claims.iter().filter(|c| c.source == *n).collect()) == wrote).unwrap_or(source).to_string()
}

/// Whether a claim's `source` is `from`: the same path, or, `from` having no folder (a link's
/// name, `Teams. Transcript. X.md`), a file of that name in any folder.
pub fn is_from(source: &str, from: &str) -> bool {
    source == from || (!from.contains('/') && source.rsplit('/').next() == Some(from))
}

/// A claims file's text with every claim from `from` (`is_from`) saying `to` instead; None when
/// none was.
pub fn renamed(text: &str, from: &str, to: &str) -> Option<String> {
    if !text.contains(from) {
        return None;
    }
    let mut f = parse(text);
    let mut here = 0;
    for c in f.claims.iter_mut().filter(|c| is_from(&c.source, from)) {
        c.source = to.to_string();
        here += 1;
    }
    (here > 0).then(|| format!("{}\n", serde_json::to_string_pretty(&f).unwrap_or_default()))
}

/// Every claims file in the vault, by page.
pub fn all(root: &std::path::Path) -> Vec<File> {
    fn walk(d: &std::path::Path, out: &mut Vec<File>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "json") {
                let f = std::fs::read_to_string(&p).map(|t| parse(&t)).unwrap_or_default();
                if !f.page.is_empty() && !f.claims.is_empty() {
                    out.push(f);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join(DIR), &mut out);
    out.sort_by(|a, b| a.page.cmp(&b.page));
    out
}

/// A claims file as it is before a write and as the write leaves it (None: no file).
pub type Rewrite = (std::path::PathBuf, Option<String>, Option<String>);

/// What a note's rename or move from `from` to `to` does to the claims files, worked out before
/// anything is written: a wiki page's claims file follows it (to `to`'s, saying the new page; gone
/// when `to` isn't a wiki page), and every claim made from it, wherever it is, names it by `to`,
/// so a later ingest doesn't write links to the old name. Only the files it changes.
pub fn on_rename(root: &std::path::Path, from: &str, to: &str) -> Vec<Rewrite> {
    let mut out: Vec<Rewrite> = Vec::new();
    if let Some((old, text)) = path(from).map(|p| root.join(p)).and_then(|p| std::fs::read_to_string(&p).ok().map(|t| (p, t))) {
        // The old file goes before the new one is written: after a change of case only, on a disk
        // that ignores case, they're the same file.
        out.push((old, Some(text.clone()), None));
        if let Some(new) = path(to).map(|p| root.join(p)) {
            let mut f = parse(&text);
            f.page = to.to_string();
            let moved = format!("{}\n", serde_json::to_string_pretty(&f).unwrap_or_default());
            let moved = renamed(&moved, from, to).unwrap_or(moved);
            out.push((new.clone(), std::fs::read_to_string(&new).ok(), Some(moved)));
        }
    }
    for e in walkdir::WalkDir::new(root.join(DIR)).into_iter().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "json")) {
        if out.iter().any(|w| w.0 == e.path()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        if let Some(next) = renamed(&text, from, to) {
            out.push((e.path().to_path_buf(), Some(text), Some(next)));
        }
    }
    out
}

/// A claims file written, or taken away when `text` is None.
pub fn put(abs: &std::path::Path, text: Option<&str>) -> std::io::Result<()> {
    match text {
        Some(t) => {
            abs.parent().map_or(Ok(()), std::fs::create_dir_all)?;
            crate::write::write_atomic(abs, t.as_bytes(), false).map_err(|e| std::io::Error::other(e.to_string()))
        }
        None if abs.exists() => std::fs::remove_file(abs),
        None => Ok(()),
    }
}

/// One subject and attribute: the latest value, and the ones it superseded, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub subject: String,
    pub attribute: String,
    pub latest: Kept,
    pub earlier: Vec<Kept>,
}

/// When a claim was true: its as of, else when it was recorded.
fn when(c: &Kept) -> Option<chrono::NaiveDate> {
    c.as_of.as_deref().and_then(parse_as_of).or_else(|| parse_as_of(&c.recorded))
}

/// The facts in a claims file, by subject and attribute, optionally only those whose subject or
/// attribute matches (case and spacing aside; an attribute's synonyms count).
pub fn facts(f: &File, subject: Option<&str>, attribute: Option<&str>) -> Vec<Fact> {
    let key = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let want_subject = subject.map(key).filter(|s| !s.is_empty());
    let want_attr = attribute.map(normalise_attribute).filter(|s| !s.is_empty());
    let mut groups: BTreeMap<(String, String), Vec<&Kept>> = BTreeMap::new();
    for c in &f.claims {
        if want_subject.as_ref().is_some_and(|s| *s != key(&c.subject)) || want_attr.as_ref().is_some_and(|a| *a != c.attribute) {
            continue;
        }
        groups.entry((key(&c.subject), c.attribute.clone())).or_default().push(c);
    }
    groups
        .into_values()
        .map(|mut v| {
            // Newest first; of two on one day, the one recorded later.
            v.sort_by(|a, b| (when(b), &b.recorded).cmp(&(when(a), &a.recorded)));
            let latest = v[0].clone();
            Fact {
                subject: latest.subject.clone(),
                attribute: latest.attribute.clone(),
                earlier: v[1..].iter().map(|c| (*c).clone()).collect(),
                latest,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(attr: &str, value: &str, as_of: Option<&str>, source: &str) -> Kept {
        Kept {
            subject: "Orbit App".into(),
            attribute: attr.into(),
            value: value.into(),
            as_of: as_of.map(String::from),
            quote: format!("launch {value}"),
            source: source.into(),
            recorded: "2026-10-06".into(),
            ..Default::default()
        }
    }

    #[test]
    fn paths() {
        assert_eq!(path("wiki/entities/Orbit App.md").as_deref(), Some("wiki/.claims/entities/Orbit App.json"));
        assert_eq!(path("wiki/concepts/a/b.md").as_deref(), Some("wiki/.claims/concepts/a/b.json"));
        assert_eq!(path("Notes/Orbit App.md"), None);
        assert_eq!(path("wiki/index.md"), None);
        assert_eq!(path("wiki/.claims/entities/x.md"), None);
    }

    #[test]
    fn a_source_is_replaced_in_place_and_the_file_goes_when_empty() {
        let a = [k("go_live_date", "14 Oct", Some("2026-09-01"), "a.md")];
        let b = [k("go_live_date", "28 Nov", Some("2026-09-28"), "b.md")];
        let one = with_source(None, "wiki/entities/Orbit App.md", "a.md", &a).unwrap();
        let two = with_source(Some(&one), "wiki/entities/Orbit App.md", "b.md", &b).unwrap();
        let again = [k("go_live_date", "15 Oct", Some("2026-09-01"), "a.md"), k("owner", "Maya", None, "a.md")];
        let three = with_source(Some(&two), "wiki/entities/Orbit App.md", "a.md", &again).unwrap();
        let f = parse(&three);
        assert_eq!(f.claims.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(), ["15 Oct", "Maya", "28 Nov"]);
        assert_eq!(of_source(Some(&three), "a.md").len(), 2);
        let gone = with_source(Some(&three), "wiki/entities/Orbit App.md", "a.md", &[]).unwrap();
        assert_eq!(with_source(Some(&gone), "wiki/entities/Orbit App.md", "b.md", &[]), None);
    }

    /// A rename's claims rewrites, written.
    fn rename(root: &std::path::Path, from: &str, to: &str) -> usize {
        let all = on_rename(root, from, to);
        for (abs, _, after) in &all {
            put(abs, after.as_deref()).unwrap();
        }
        all.len()
    }

    #[test]
    fn claims_follow_a_rename_and_the_trash() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let page = "wiki/entities/Orbit App.md";
        std::fs::create_dir_all(root.join("wiki/entities")).unwrap();
        std::fs::write(root.join(page), "# Orbit App\n").unwrap();
        let text = with_source(None, page, "a.md", &[k("owner", "Maya", None, "a.md")]).unwrap();
        let claims = root.join(path(page).unwrap());
        std::fs::create_dir_all(claims.parent().unwrap()).unwrap();
        std::fs::write(&claims, &text).unwrap();

        let to = "wiki/entities/Orbit Platform.md";
        std::fs::rename(root.join(page), root.join(to)).unwrap();
        // Worked out first, nothing written.
        assert_eq!(on_rename(root, page, to).len(), 2);
        assert!(claims.exists());
        rename(root, page, to);
        assert!(!claims.exists());
        let moved = root.join(path(to).unwrap());
        let f = parse(&std::fs::read_to_string(&moved).unwrap());
        assert_eq!((f.page.as_str(), f.claims.len()), (to, 1));

        let e = crate::trash::move_to_trash(root, to, "wiki").unwrap();
        assert!(!moved.exists());
        crate::trash::restore(root, &e.id, None).unwrap();
        assert_eq!(parse(&std::fs::read_to_string(&moved).unwrap()).claims.len(), 1);
        // Out of the wiki, a page has no claims file.
        rename(root, to, "Notes/Orbit Platform.md");
        assert!(!moved.exists());
    }

    #[test]
    fn claims_made_from_a_renamed_source_name_it_by_its_new_name() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let page = "wiki/entities/Maya.md";
        let from = "Meeting. Orbit Steerco meting - 2026-10-02.md";
        let text = with_source(None, page, from, &[k("owner", "Maya", None, from), k("owner", "Lena", None, "b.md")]).unwrap();
        let claims = root.join(path(page).unwrap());
        std::fs::create_dir_all(claims.parent().unwrap()).unwrap();
        std::fs::write(&claims, &text).unwrap();
        let to = "Meeting. Orbit Steerco meeting - 2026-10-02.md";
        assert_eq!(rename(root, from, to), 1);
        let f = parse(&std::fs::read_to_string(&claims).unwrap());
        assert_eq!(f.claims.iter().map(|c| c.source.as_str()).collect::<Vec<_>>(), [to, "b.md"]);
        // A link's name finds a source of that name in a folder.
        let t = "sources/Teams. Transcript. Steerco - 2026-10-02.md";
        let text = with_source(None, page, t, &[k("owner", "Theo", None, t)]).unwrap();
        assert!(renamed(&text, "Teams. Transcript. Steerco - 2026-10-02.md", to).is_some());
        assert!(renamed(&text, "Steerco - 2026-10-02.md", to).is_none(), "a name matches a whole file name");
        // Nothing named it: nothing changes.
        assert_eq!(rename(root, "other.md", "x.md"), 0);
    }

    #[test]
    fn a_source_renamed_by_a_change_is_put_back_by_its_revert() {
        let page = "wiki/entities/Orbit App.md";
        let ghost = "sources/Steerco.md";
        let before = with_source(None, page, ghost, &[k("owner", "Maya", None, ghost)]).unwrap();
        let before = with_source(Some(&before), page, "b.md", &[k("owner", "Lena", None, "b.md")]).unwrap();
        let u = Update { source: "sources/Steerco notes.md".into(), from: Some("Steerco.md".into()), ..Default::default() };
        let after = updated(Some(&before), page, &u).unwrap();
        let f = parse(&after);
        assert_eq!(f.claims.iter().map(|c| c.source.as_str()).collect::<Vec<_>>(), ["sources/Steerco notes.md", "b.md"]);
        // Another source added since stays; the renamed claims go back to their old name.
        let later = with_source(Some(&after), page, "c.md", &[k("owner", "Theo", None, "c.md")]).unwrap();
        let back = parse(&reverted(Some(&before), Some(&later), page, &u).unwrap());
        let mut names: Vec<&str> = back.claims.iter().map(|c| c.source.as_str()).collect();
        names.sort();
        assert_eq!(names, ["b.md", "c.md", ghost]);
        // Nothing of it in the file: nothing to do.
        assert_eq!(updated(Some(&later), page, &Update { from: Some("other.md".into()), ..u.clone() }).as_deref(), Some(later.as_str()));
    }

    #[test]
    fn reverting_a_link_keeps_claims_a_later_ingest_wrote() {
        let page = "wiki/entities/Orbit App.md";
        let (ghost, to) = ("sources/Steerco.md", "sources/Steerco notes.md");
        let before = with_source(None, page, ghost, &[k("owner", "Maya", None, ghost), k("go_live_date", "14 Oct", None, ghost)]).unwrap();
        let u = Update { source: to.into(), from: Some("Steerco.md".into()), ..Default::default() };
        let linked = updated(Some(&before), page, &u).unwrap();
        // The note ingested again: its set under the new name now one claim kept, one new.
        let again = [k("owner", "Maya", None, to), k("go_live_date", "28 Nov", None, to)];
        let ingested = with_source(Some(&linked), page, to, &again).unwrap();
        let back = parse(&reverted(Some(&before), Some(&ingested), page, &u).unwrap());
        let got: Vec<(&str, &str)> = back.claims.iter().map(|c| (c.value.as_str(), c.source.as_str())).collect();
        // Only the claim the link renamed, unchanged since, takes the old name back.
        assert_eq!(got, [("Maya", ghost), ("28 Nov", to)]);
    }

    #[test]
    fn a_change_of_case_keeps_the_claims_file() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let (from, to) = ("wiki/entities/Orbit App.md", "wiki/entities/orbit app.md");
        let text = with_source(None, from, "a.md", &[k("owner", "Maya", None, "a.md")]).unwrap();
        put(&root.join(path(from).unwrap()), Some(&text)).unwrap();
        // On a disk that ignores case the old and new files are one: it goes, then is written.
        let all = on_rename(root, from, to);
        assert_eq!((all[0].2.is_none(), all[1].2.is_some()), (true, true));
        rename(root, from, to);
        let f = parse(&std::fs::read_to_string(root.join(path(to).unwrap())).unwrap());
        assert_eq!((f.page.as_str(), f.claims.len()), (to, 1));
    }

    #[test]
    fn reverting_an_ingest_follows_its_source_renamed_since() {
        let page = "wiki/entities/Orbit App.md";
        let a = "sources/Steerco.md";
        let before = with_source(None, page, "b.md", &[k("owner", "Lena", None, "b.md")]).unwrap();
        let wrote = [k("go_live_date", "14 Oct", Some("2026-09-01"), a), k("owner", "Maya", None, a)];
        let u = Update { source: a.into(), claims: wrote.to_vec(), ..Default::default() };
        let after = updated(Some(&before), page, &u).unwrap();
        // The source renamed since: its claims name it by its new name.
        let now = renamed(&after, a, "sources/Steerco notes.md").unwrap();
        let back = reverted(Some(&before), Some(&now), page, &u).unwrap();
        assert_eq!(parse(&back).claims.iter().map(|c| c.source.as_str()).collect::<Vec<_>>(), ["b.md"]);
        // Not renamed: as before.
        assert_eq!(parse(&reverted(Some(&before), Some(&after), page, &u).unwrap()), parse(&before));
    }

    #[test]
    fn the_latest_value_supersedes_and_both_are_kept() {
        let f = File {
            page: "wiki/entities/Orbit App.md".into(),
            claims: vec![
                k("go_live_date", "14 Oct", Some("2026-09-01"), "a.md"),
                k("go_live_date", "28 Nov", Some("2026-09-28"), "b.md"),
                k("owner", "Maya", None, "a.md"),
            ],
        };
        let all = facts(&f, None, None);
        assert_eq!(all.len(), 2);
        let go = facts(&f, Some("orbit  app"), Some("launch date"));
        assert_eq!(go.len(), 1);
        assert_eq!(go[0].latest.value, "28 Nov");
        assert_eq!(go[0].earlier[0].value, "14 Oct");
        assert!(facts(&f, Some("Lena"), None).is_empty());
    }
}
