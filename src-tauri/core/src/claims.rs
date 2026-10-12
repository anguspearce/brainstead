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

/// What a change does to its page's claims: `source`'s set becomes `claims`. Kept in the change
/// with the claims file before and after it (by hash in the change store), for Revert.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub source: String,
    pub claims: Vec<Kept>,
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

/// A page's claims file follows it when it's renamed or moved: to `to`'s claims file, saying the
/// new page; gone when `to` isn't a wiki page. Nothing when it had none.
pub fn carry(root: &std::path::Path, from: &str, to: &str) -> std::io::Result<()> {
    let Some(old) = path(from).map(|p| root.join(p)) else { return Ok(()) };
    let Ok(text) = std::fs::read_to_string(&old) else { return Ok(()) };
    std::fs::remove_file(&old)?;
    let Some(new) = path(to).map(|p| root.join(p)) else { return Ok(()) };
    let mut f = parse(&text);
    f.page = to.to_string();
    let put = |p: &std::path::Path, t: &[u8]| {
        p.parent().map_or(Ok(()), std::fs::create_dir_all)?;
        crate::write::write_atomic(p, t, false).map_err(|e| std::io::Error::other(e.to_string()))
    };
    let out = format!("{}\n", serde_json::to_string_pretty(&f).unwrap_or_default());
    put(&new, out.as_bytes()).inspect_err(|_| {
        let _ = put(&old, text.as_bytes());
    })
}

/// Every claim whose source is `from` says `to` instead, in every claims file: what a source's
/// rename (or a missing name pointed at a real note, Link to…) means for the claims made from it,
/// so a later ingest doesn't write links to the old name. `from` without a folder (a link's name,
/// `Teams. Transcript. X.md`) matches a source of that name in any folder. How many claims changed.
pub fn rename_source(root: &std::path::Path, from: &str, to: &str) -> std::io::Result<usize> {
    let dir = root.join(DIR);
    let mut n = 0;
    for e in walkdir::WalkDir::new(&dir).into_iter().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "json")) {
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        if !text.contains(from) {
            continue;
        }
        let mut f = parse(&text);
        let mut here = 0;
        let bare = !from.contains('/');
        for c in f.claims.iter_mut().filter(|c| c.source == from || (bare && c.source.rsplit('/').next() == Some(from))) {
            c.source = to.to_string();
            here += 1;
        }
        if here == 0 {
            continue;
        }
        let out = format!("{}\n", serde_json::to_string_pretty(&f).unwrap_or_default());
        crate::write::write_atomic(e.path(), out.as_bytes(), false).map_err(|e| std::io::Error::other(e.to_string()))?;
        n += here;
    }
    Ok(n)
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
        carry(root, page, to).unwrap();
        assert!(!claims.exists());
        let moved = root.join(path(to).unwrap());
        let f = parse(&std::fs::read_to_string(&moved).unwrap());
        assert_eq!((f.page.as_str(), f.claims.len()), (to, 1));

        let e = crate::trash::move_to_trash(root, to, "wiki").unwrap();
        assert!(!moved.exists());
        crate::trash::restore(root, &e.id, None).unwrap();
        assert_eq!(parse(&std::fs::read_to_string(&moved).unwrap()).claims.len(), 1);
        // Out of the wiki, a page has no claims file.
        carry(root, to, "Notes/Orbit Platform.md").unwrap();
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
        assert_eq!(rename_source(root, from, to).unwrap(), 1);
        let f = parse(&std::fs::read_to_string(&claims).unwrap());
        assert_eq!(f.claims.iter().map(|c| c.source.as_str()).collect::<Vec<_>>(), [to, "b.md"]);
        // A link's name finds a source of that name in a folder.
        let t = "sources/Teams. Transcript. Steerco - 2026-10-02.md";
        let text = with_source(None, page, t, &[k("owner", "Theo", None, t)]).unwrap();
        std::fs::write(&claims, &text).unwrap();
        assert_eq!(rename_source(root, "Teams. Transcript. Steerco - 2026-10-02.md", to).unwrap(), 1);
        // Nothing named it: nothing changes.
        assert_eq!(rename_source(root, "other.md", "x.md").unwrap(), 0);
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
