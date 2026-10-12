// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Knowledge health: the previous app's `scripts/lint_wiki.py` ported check for check, with its
//! rules and its wording (the tests compare with the script's own output, frozen in
//! `tests/fixtures/lint/`), plus three checks of Brainstead's own: possible duplicate pages, stale
//! pages that others rely on, and sources changed since the pages citing them. It reads the files
//! as they are on disk, as the script did, and changes nothing; the safe fixes at the end return
//! a page's new text for the app to write.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use chrono::{DateTime, NaiveDate};
use regex::Regex;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::reviews::Zone;

pub(crate) const WIKI_SUBDIRS: [&str; 3] = ["entities", "concepts", "summaries"];
const LOG: &str = "log.md";
/// A wiki page unchanged this long, that other notes link to, is worth a look.
pub const STALE_DAYS: i64 = 90;
/// Checks that are worth a look but aren't problems: not counted as needing a decision.
pub const ADVISORY: [&str; 3] = ["stale-pages", "uncited-claims", "no-current-state"];
const STALE_MIN_BACKLINKS: usize = 3;

pub(crate) static FRONTMATTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)\A---\n(.*?)\n---\n").unwrap());
/// lint_wiki.py's wikilink: the target, without a heading or alias.
pub(crate) static WIKILINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]|#\n]+?)(?:[|#][^\]\n]*)?\]\]").unwrap());

/// lint_wiki.py's comparison key: NFC, then lower case.
pub(crate) fn lint_key(s: &str) -> String {
    s.nfc().collect::<String>().to_lowercase()
}

/// Python's `Path.stem`: the name without its last suffix.
pub fn stem(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => &name[..i],
        _ => name,
    }
}

pub fn name_of(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

pub(crate) fn frontmatter_block(text: &str) -> &str {
    FRONTMATTER.captures(text).and_then(|c| c.get(1)).map_or("", |m| m.as_str())
}

pub(crate) fn body(text: &str) -> &str {
    FRONTMATTER.find(text).map_or(text, |m| &text[m.end()..])
}

pub(crate) fn fm_field<'a>(fm: &'a str, key: &str) -> &'a str {
    fm.lines().find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix(':'))).map_or("", str::trim)
}

/// The `sources:` property, from its key to the next top-level key, as lint_wiki.py cuts it.
pub(crate) fn sources_block(fm: &str) -> &str {
    let mut at = 0;
    let mut start = None;
    for line in fm.split_inclusive('\n') {
        match start {
            None if line.starts_with("sources:") => start = Some(at),
            Some(_) if line.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_') => return &fm[start.unwrap()..at],
            _ => {}
        }
        at += line.len();
    }
    start.map_or("", |s| &fm[s..])
}

/// A page of the wiki as lint_wiki.py lists them: `wiki/<entities|concepts|summaries>/<name>.md`.
pub(crate) fn is_wiki_page(rel: &str) -> bool {
    let segs: Vec<&str> = rel.split('/').collect();
    segs.len() == 3 && segs[0] == "wiki" && WIKI_SUBDIRS.contains(&segs[1]) && segs[2].ends_with(".md")
}

/// Every name a wikilink could mean, as lint_wiki.py's stem index has them: each file's name with
/// and without its extension, and its vault path with and without, for any file outside
/// `.obsidian` and `.trash`.
pub(crate) fn names(root: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    for f in walk(root) {
        let name = name_of(&f.rel);
        let dir = &f.rel[..f.rel.len() - name.len()];
        for k in [stem(name).to_string(), name.to_string(), format!("{dir}{}", stem(name)), f.rel.clone()] {
            out.insert(lint_key(&k));
        }
    }
    out
}

struct File {
    rel: String,
    abs: PathBuf,
    mtime: i64,
    size: u64,
}

/// The notes' text from the last run, by path with the modified time and size it was read at. The
/// checks rerun after every change to the vault, and reading two thousand notes through a synced
/// folder takes most of a run (about half a second), so only the files that changed are read again.
static TEXTS: LazyLock<std::sync::Mutex<Kept>> = LazyLock::new(Default::default);

/// A file's text by path, with the modified time and size it was read at.
type Kept = HashMap<PathBuf, (i64, u64, String)>;

/// Each markdown file's text: from TEXTS when it's unchanged since, else read (and kept).
/// Forgets what's kept of these files' text, so the next run reads them again: for a write that
/// keeps a file's modification time and size (a safe fix changing `updated:` to a date of the
/// same length, say), which the cache's check can't see.
pub fn forget(paths: &[PathBuf]) {
    let mut kept = TEXTS.lock().unwrap_or_else(|e| e.into_inner());
    for p in paths {
        kept.remove(p);
    }
}

fn read_all(files: &[File]) -> Vec<(&File, String)> {
    let mut kept = TEXTS.lock().unwrap_or_else(|e| e.into_inner());
    let mut next = HashMap::with_capacity(kept.len());
    let out = files
        .iter()
        .filter(|f| f.rel.ends_with(".md"))
        .map(|f| {
            let text = match kept.remove(&f.abs) {
                Some((m, s, t)) if m == f.mtime && s == f.size && f.mtime != 0 => t,
                _ => read(&f.abs),
            };
            next.insert(f.abs.clone(), (f.mtime, f.size, text.clone()));
            (f, text)
        })
        .collect();
    *kept = next;
    out
}

/// Every file outside `.obsidian` and `.trash`, at any depth.
fn walk(root: &Path) -> Vec<File> {
    let it = walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !matches!(e.file_name().to_str(), Some(".obsidian" | ".trash")));
    it.flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let rel = crate::vault::rel_of(root, e.path())?;
            let meta = e.metadata().ok();
            let mtime = meta.as_ref().map(crate::vault::mtime_ms).unwrap_or(0);
            Some(File { rel, abs: e.path().to_path_buf(), mtime, size: meta.map_or(0, |m| m.len()) })
        })
        .collect()
}

fn read(p: &Path) -> String {
    std::fs::read(p).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// One check and what it found.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: &'static str,
    pub title: &'static str,
    /// The script's checks; false for Brainstead's own.
    pub classic: bool,
    /// Issues the user ignored, left out of `items` until their page changes.
    #[serde(skip_serializing_if = "is_zero")]
    pub ignored: usize,
    pub items: Vec<Item>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// One issue. `text` is the script's line for it; the rest is for the screen.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub text: String,
    /// The page the issue is on, or the file it's about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// The link target, name or source it's about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Other pages involved: those linking a missing page, the other duplicate, those citing a
    /// changed source.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<String>,
    /// Backlinks, days, similarity: whatever number the check shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Brainstead can fix it without asking: a cross-link, a date, a log line.
    pub safe: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub checks: Vec<Check>,
    pub wiki_pages: usize,
    pub sources: usize,
    /// How long the checks took.
    pub ms: u64,
}

impl Report {
    pub fn check(&self, id: &str) -> Option<&Check> {
        self.checks.iter().find(|c| c.id == id)
    }
    /// Issues Brainstead can't fix on its own. Stale pages and uncited claims are worth a look,
    /// not problems, so they aren't counted.
    pub fn needs_decision(&self) -> usize {
        self.checks.iter().filter(|c| !ADVISORY.contains(&c.id)).flat_map(|c| &c.items).filter(|i| !i.safe).count()
    }
    pub fn total(&self) -> usize {
        self.checks.iter().map(|c| c.items.len()).sum()
    }
}

/// The checks, in the script's order (broken references, then rot, then cleanup) and then
/// Brainstead's. `dismissed` holds duplicate pairs marked as not duplicates (`pair_key`).
pub fn run(root: &Path, today: NaiveDate, dismissed: &HashSet<String>) -> Report {
    run_in(root, today, dismissed, Zone::Local)
}

pub(crate) fn run_in(root: &Path, today: NaiveDate, dismissed: &HashSet<String>, zone: Zone) -> Report {
    let t0 = std::time::Instant::now();
    let files = walk(root);
    let day = |ms: i64| zone.ms_to_local(ms).date();
    let md: Vec<(&File, String)> = read_all(&files);
    let wiki: Vec<&(&File, String)> = md.iter().filter(|(f, _)| is_wiki_page(&f.rel)).collect();
    let mut index: HashSet<String> = HashSet::new();
    for f in &files {
        let name = name_of(&f.rel);
        let dir = &f.rel[..f.rel.len() - name.len()];
        for k in [stem(name).to_string(), name.to_string(), format!("{dir}{}", stem(name)), f.rel.clone()] {
            index.insert(lint_key(&k));
        }
    }
    // Inbound links from every markdown file, by key, with the files they come from.
    let mut inbound: HashMap<String, BTreeSet<&str>> = HashMap::new();
    for (f, text) in &md {
        for c in WIKILINK.captures_iter(text) {
            inbound.entry(lint_key(c[1].trim())).or_default().insert(&f.rel);
        }
    }
    let entity_names: Vec<String> = wiki
        .iter()
        .filter(|(f, _)| f.rel.starts_with("wiki/entities/") || f.rel.starts_with("wiki/concepts/"))
        .map(|(f, _)| stem(name_of(&f.rel)).to_string())
        .collect();

    let mut checks = Vec::new();

    // 1. Missing pages.
    let mut missing: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for (f, text) in &wiki {
        for c in WIKILINK.captures_iter(text) {
            let t = c[1].trim();
            if !index.contains(&lint_key(t)) {
                missing.entry(t.to_string()).or_default().insert(&f.rel);
            }
        }
    }
    let mut items: Vec<Item> = missing
        .into_iter()
        .map(|(t, srcs)| Item {
            text: format!("[[{t}]]  — linked from {} wiki page(s)", srcs.len()),
            name: Some(t),
            count: Some(srcs.len() as i64),
            pages: srcs.into_iter().map(str::to_string).collect(),
            ..Default::default()
        })
        .collect();
    sort(&mut items);
    checks.push(Check { id: "missing-pages", title: "Missing pages — linked but never written", classic: true, ignored: 0, items });

    // 2. Broken sources.
    let mut items = Vec::new();
    for (f, text) in &wiki {
        for c in WIKILINK.captures_iter(sources_block(frontmatter_block(text))) {
            let t = c[1].trim();
            if !index.contains(&lint_key(t)) {
                items.push(Item {
                    text: format!("{}: [[{}]] not found", f.rel, &c[1]),
                    page: Some(f.rel.clone()),
                    name: Some(t.to_string()),
                    ..Default::default()
                });
            }
        }
    }
    sort(&mut items);
    checks.push(Check { id: "broken-sources", title: "Broken sources", classic: true, ignored: 0, items });

    // 3. Orphans.
    let mut items: Vec<Item> = wiki
        .iter()
        .filter(|(f, _)| !inbound.contains_key(&lint_key(stem(name_of(&f.rel)))))
        .map(|(f, _)| Item { text: f.rel.clone(), page: Some(f.rel.clone()), ..Default::default() })
        .collect();
    sort(&mut items);
    checks.push(Check { id: "orphans", title: "Orphan pages — nothing links here", classic: true, ignored: 0, items });

    // 4. Missing cross-references.
    let names: Vec<(&str, String)> = entity_names.iter().filter(|n| n.chars().count() >= 4).map(|n| (n.as_str(), lint_key(n))).collect();
    let mut items = Vec::new();
    for (f, text) in &wiki {
        let page_body = body(text);
        let linked: Vec<String> = WIKILINK.captures_iter(text).map(|c| format!(" {} ", lint_key(c[1].trim()))).collect();
        let stem_lc = lint_key(stem(name_of(&f.rel)));
        let stem_words: HashSet<&str> = stem_lc.split_whitespace().collect();
        for (name, name_lc) in &names {
            if !page_body.contains(name) || stem_lc == *name_lc || stem_words.contains(name_lc.as_str()) {
                continue;
            }
            let padded = format!(" {name_lc} ");
            if linked.iter().any(|l| l.contains(&padded)) {
                continue;
            }
            // The fix's own rule, so every item it lists is one Fix can make.
            if plain_mention(text, name).is_some() {
                items.push(Item {
                    text: format!("{}: '{name}' mentioned without [[link]]", f.rel),
                    page: Some(f.rel.clone()),
                    name: Some(name.to_string()),
                    safe: true,
                    ..Default::default()
                });
            }
        }
    }
    sort(&mut items);
    checks.push(Check {
        id: "missing-crossrefs",
        title: "Missing cross-links — mentioned without [[ ]]",
        classic: true,
        ignored: 0,
        items,
    });

    // 5. Stale `updated:` dates.
    let mut items = Vec::new();
    for (f, text) in &wiki {
        let u = fm_field(frontmatter_block(text), "updated");
        let Ok(fm_date) = NaiveDate::parse_from_str(u, "%Y-%m-%d") else { continue };
        if u.len() != 10 {
            continue;
        }
        let m = day(f.mtime);
        if (m - fm_date).num_days() > 1 {
            items.push(Item {
                text: format!("{}  (updated: {fm_date}, mtime: {m})", f.rel),
                page: Some(f.rel.clone()),
                detail: Some(m.to_string()),
                safe: true,
                ..Default::default()
            });
        }
    }
    sort(&mut items);
    checks.push(Check { id: "stale-dates", title: "Stale “updated” dates", classic: true, ignored: 0, items });

    // 6. Unlogged writes.
    let mut items = Vec::new();
    if let Some(log) = files.iter().find(|f| f.rel == LOG) {
        for (f, _) in &wiki {
            if f.mtime > log.mtime {
                let utc = DateTime::from_timestamp_millis(f.mtime).map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();
                items.push(Item {
                    text: format!("{}  (mtime: {utc} UTC)", f.rel),
                    page: Some(f.rel.clone()),
                    safe: true,
                    ..Default::default()
                });
            }
        }
    }
    sort(&mut items);
    checks.push(Check { id: "unlogged-writes", title: "Unlogged writes", classic: true, ignored: 0, items });

    // 7. Uningested sources: files directly in sources/ that no page's sources: names.
    let cited: HashSet<String> = wiki
        .iter()
        .flat_map(|(_, t)| WIKILINK.captures_iter(sources_block(frontmatter_block(t))).map(|c| lint_key(c[1].trim())).collect::<Vec<_>>())
        .collect();
    let top_sources: Vec<&File> = files.iter().filter(|f| f.rel.starts_with("sources/") && !f.rel[8..].contains('/')).collect();
    let mut items: Vec<Item> = top_sources
        .iter()
        .filter(|f| {
            let n = name_of(&f.rel);
            !n.starts_with('.') && !cited.contains(&lint_key(stem(n))) && !cited.contains(&lint_key(n))
        })
        .map(|f| Item { text: f.rel.clone(), page: Some(f.rel.clone()), ..Default::default() })
        .collect();
    sort(&mut items);
    checks.push(Check { id: "uningested-sources", title: "Sources not yet ingested", classic: true, ignored: 0, items });

    // 8. Unreferenced images: files directly in images/ that no .md or .txt file embeds or links.
    // Every image's ways of being referred to, looked for in one pass over each file (rather than
    // each image's in every file: images × text).
    let images: Vec<&File> =
        files.iter().filter(|f| f.rel.starts_with("images/") && !f.rel[7..].contains('/') && !name_of(&f.rel).starts_with('.')).collect();
    let mut needles: Vec<String> = Vec::new();
    let mut owner: Vec<usize> = Vec::new();
    for (i, f) in images.iter().enumerate() {
        let n = name_of(&f.rel);
        let s = stem(n);
        for x in [f.rel.clone(), format!("[[{n}]]"), format!("[[{n}|"), format!("[[{s}]]"), format!("[[{s}|")] {
            needles.push(x.nfc().collect());
            owner.push(i);
        }
    }
    let mut used = vec![false; images.len()];
    if !images.is_empty() {
        let ac = aho_corasick::AhoCorasick::new(&needles).expect("image names make a matcher");
        let texts: HashMap<&str, &String> = md.iter().map(|(f, t)| (f.rel.as_str(), t)).collect();
        for f in files.iter().filter(|f| f.rel.ends_with(".md") || f.rel.ends_with(".txt")) {
            let owned;
            let t: &str = match texts.get(f.rel.as_str()) {
                Some(t) => t,
                None => {
                    owned = read(&f.abs);
                    &owned
                }
            };
            // Most notes are NFC already: normalised only when they might not be.
            let t: std::borrow::Cow<str> = match unicode_normalization::is_nfc_quick(t.chars()) {
                unicode_normalization::IsNormalized::Yes => t.into(),
                _ => t.nfc().collect::<String>().into(),
            };
            for m in ac.find_overlapping_iter(t.as_ref()) {
                used[owner[m.pattern().as_usize()]] = true;
            }
            if used.iter().all(|u| *u) {
                break;
            }
        }
    }
    let mut items = Vec::new();
    for (f, _) in images.iter().zip(&used).filter(|(_, u)| !**u) {
        items.push(Item { text: f.rel.clone(), page: Some(f.rel.clone()), ..Default::default() });
    }
    sort(&mut items);
    checks.push(Check { id: "unreferenced-images", title: "Images nothing uses", classic: true, ignored: 0, items });

    // Brainstead's own. Possible duplicates.
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    for (f, text) in &wiki {
        if !(f.rel.starts_with("wiki/entities/") || f.rel.starts_with("wiki/concepts/")) {
            continue;
        }
        let fm = crate::frontmatter::split(text);
        let aliases = crate::frontmatter::list(&fm.data, &["aliases", "alias"], false);
        entries.push((f.rel.clone(), aliases));
    }
    let mut items = duplicates(&entries)
        .into_iter()
        .filter(|d| !dismissed.contains(&pair_key(&d.a, &d.b)))
        .map(|d| Item {
            text: format!("{} and {}", stem(name_of(&d.a)), stem(name_of(&d.b))),
            page: Some(d.a.clone()),
            pages: vec![d.b.clone()],
            detail: Some(d.why.to_string()),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    sort(&mut items);
    checks.push(Check { id: "duplicates", title: "Possible duplicates", classic: false, ignored: 0, items });

    // Stale pages others rely on.
    let mut items = Vec::new();
    for (f, text) in &wiki {
        let k = lint_key(stem(name_of(&f.rel)));
        let backlinks = inbound.get(&k).map_or(0, |s| s.iter().filter(|r| **r != f.rel).count());
        if backlinks < STALE_MIN_BACKLINKS {
            continue;
        }
        let updated = NaiveDate::parse_from_str(fm_field(frontmatter_block(text), "updated"), "%Y-%m-%d").ok();
        let last = updated.map_or(day(f.mtime), |u| u.max(day(f.mtime)));
        let age = (today - last).num_days();
        if age >= STALE_DAYS {
            items.push(Item {
                text: format!("{}  ({backlinks} backlinks, {age} days)", f.rel),
                page: Some(f.rel.clone()),
                count: Some(backlinks as i64),
                detail: Some(format!("{age} days")),
                ..Default::default()
            });
        }
    }
    items.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.text.cmp(&b.text)));
    checks.push(Check { id: "stale-pages", title: "Stale pages others rely on", classic: false, ignored: 0, items });

    // Sources changed since the pages citing them.
    let mut citing: HashMap<String, Vec<&File>> = HashMap::new();
    for (f, text) in &wiki {
        for c in WIKILINK.captures_iter(sources_block(frontmatter_block(text))) {
            citing.entry(lint_key(c[1].trim())).or_default().push(f);
        }
    }
    let mut items = Vec::new();
    for s in files.iter().filter(|f| f.rel.starts_with("sources/")) {
        let n = name_of(&s.rel);
        let mut by: Vec<&File> = citing.get(&lint_key(stem(n))).into_iter().chain(citing.get(&lint_key(n))).flatten().copied().collect();
        by.sort_by(|a, b| a.rel.cmp(&b.rel));
        by.dedup_by(|a, b| a.rel == b.rel);
        if by.is_empty() || by.iter().any(|p| p.mtime >= s.mtime) {
            continue;
        }
        items.push(Item {
            text: format!("{}  (changed {}, after the {} page(s) citing it)", s.rel, day(s.mtime), by.len()),
            page: Some(s.rel.clone()),
            pages: by.iter().map(|p| p.rel.clone()).collect(),
            detail: Some(day(s.mtime).to_string()),
            ..Default::default()
        });
    }
    sort(&mut items);
    checks.push(Check { id: "changed-sources", title: "Sources changed since they were cited", classic: false, ignored: 0, items });

    // Wiki pages not in the page shape (D-20261006-06): those Reshape pages can do by itself are safe.
    let shape = crate::pageshape::Sources::from_paths(files.iter().map(|f| f.rel.as_str()));
    let items = md
        .iter()
        .filter(|(f, _)| crate::pageshape::shaped(&f.rel))
        .filter_map(|(f, t)| crate::pageshape::item(&f.rel, t, &shape))
        .collect();
    checks.push(Check { id: "page-shape", title: "Pages not in the page shape", classic: false, ignored: 0, items });
    // Pages with a Timeline but no Current state: Write Current state (a model run) can write it.
    let items = md
        .iter()
        .filter(|(f, t)| crate::pageshape::shaped(&f.rel) && crate::pageshape::wants_current_state(t))
        .map(|(f, _)| Item { text: format!("{}: no Current state", f.rel), page: Some(f.rel.clone()), ..Default::default() })
        .collect();
    checks.push(Check { id: "no-current-state", title: "Pages with no Current state", classic: false, ignored: 0, items });

    Report {
        checks,
        wiki_pages: wiki.len(),
        sources: files.iter().filter(|f| f.rel.starts_with("sources/")).count(),
        ms: t0.elapsed().as_millis() as u64,
    }
}

/// Files directly in `sources/` that no wiki page's `sources:` names: check 7 alone, without
/// reading the rest of the vault.
pub fn pending_sources(root: &Path) -> Vec<String> {
    let mut cited: HashSet<String> = HashSet::new();
    for sub in WIKI_SUBDIRS {
        let Ok(rd) = std::fs::read_dir(root.join("wiki").join(sub)) else { continue };
        for e in rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "md")) {
            let text = read(&e.path());
            cited.extend(WIKILINK.captures_iter(sources_block(frontmatter_block(&text))).map(|c| lint_key(c[1].trim())));
        }
    }
    let Ok(rd) = std::fs::read_dir(root.join("sources")) else { return vec![] };
    let mut out: Vec<String> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| !n.starts_with('.') && !cited.contains(&lint_key(stem(n))) && !cited.contains(&lint_key(n)))
        .map(|n| format!("sources/{n}"))
        .collect();
    out.sort();
    out
}

fn sort(items: &mut [Item]) {
    items.sort_by(|a, b| a.text.cmp(&b.text));
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Every place `name` appears as a whole word (the script's `(?<!\w)name(?!\w)`), as byte offsets.
fn mentions<'a>(text: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(name).map(|(i, _)| i).filter(move |&i| {
        let before = text[..i].chars().next_back().is_none_or(|c| !word_char(c));
        let after = text[i + name.len()..].chars().next().is_none_or(|c| !word_char(c));
        before && after
    })
}

/// Two pages that may be one: by path, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    pub a: String,
    pub b: String,
    pub why: &'static str,
}

/// The key a pair of pages is dismissed under.
pub fn pair_key(a: &str, b: &str) -> String {
    if a <= b {
        format!("{a}|{b}")
    } else {
        format!("{b}|{a}")
    }
}

/// A name with accents, case, spaces and punctuation taken out.
fn fold(s: &str) -> String {
    s.nfkd().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Pages whose names, or aliases, look like one thing: the same once spaces, case and accents go
/// ("North Wind", "Northwind"), an alias of one is the other's name, a spelling a letter or two
/// apart ("Hub Platfrom"), or a name that's the other's first word ("Nova", "Nova Platform").
/// Names differing only in their digits (Sprint 21, Sprint 22) aren't.
pub fn duplicates(pages: &[(String, Vec<String>)]) -> Vec<Duplicate> {
    let mut out = Vec::new();
    // Each name folded once, not once per pair.
    let named: Vec<(&str, String, Vec<String>, String)> = pages
        .iter()
        .map(|(rel, aliases)| {
            let name = stem(name_of(rel)).to_string();
            let folded = fold(&name);
            (rel.as_str(), name, aliases.iter().map(|a| fold(a)).filter(|a| !a.is_empty()).collect(), folded)
        })
        .collect();
    for i in 0..named.len() {
        for j in i + 1..named.len() {
            let (ra, na, aa, fa) = &named[i];
            let (rb, nb, ab, fb) = &named[j];
            let (fa, fb) = (fa.clone(), fb.clone());
            if fa.is_empty() || fb.is_empty() {
                continue;
            }
            let why = if fa == fb {
                Some("the same name written differently")
            } else if aa.contains(&fb) || ab.contains(&fa) || aa.iter().any(|x| ab.contains(x)) {
                Some("one's alias is the other's name")
            } else if similar_spelling(&fa, &fb) {
                Some("a letter or two apart")
            } else if first_word_of(na, nb) || first_word_of(nb, na) {
                Some("one name starts the other")
            } else {
                None
            };
            if let Some(why) = why {
                out.push(Duplicate { a: ra.to_string(), b: rb.to_string(), why });
            }
        }
    }
    out
}

fn similar_spelling(a: &str, b: &str) -> bool {
    let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
    if digits(a) != digits(b) {
        return false;
    }
    a.chars().count().min(b.chars().count()) >= 6 && strsim::levenshtein(a, b) <= 2 && strsim::jaro_winkler(a, b) >= 0.93
}

/// `short` is the first word of `long`, which has one more word: "Nova" and "Nova Platform".
fn first_word_of(short: &str, long: &str) -> bool {
    let s: Vec<String> = short.split_whitespace().map(fold).collect();
    let l: Vec<String> = long.split_whitespace().map(fold).collect();
    s.len() == 1 && l.len() == 2 && s[0].chars().count() >= 4 && l[0] == s[0]
}

// ── Ignored issues ────────────────────────────────────────────────────────────

/// The key an issue is ignored under: its check and its line.
pub fn ignore_key(check: &str, text: &str) -> String {
    format!("{check}|{text}")
}

/// Checks whose issue is one line of a page, given in full in the issue's text: one of these
/// stays ignored for as long as its page still has that line, whatever else on the page changes.
pub const LINE_KEYED: [&str; 1] = ["uncited-claims"];

/// An uncited claim's line as the check gives it: trimmed, at most 160 characters.
fn claim_line(l: &str) -> String {
    l.trim().chars().take(160).collect()
}

/// Takes the ignored issues out of the report, counting them on their check. `ignored` maps an
/// issue's key to its page's version when it was ignored; `version` gives a page's version now
/// (None when it's gone) and `text` its text. An issue whose page has changed since comes back,
/// and its key is dropped from `ignored`; so does the key of an issue no check gives any more.
/// A [`LINE_KEYED`] issue comes back only when its line does, and its key is kept while its
/// page still has the line (a claim not read again yet drops out of its check for a while).
pub fn without_ignored(
    r: &mut Report,
    ignored: &mut BTreeMap<String, String>,
    version: &dyn Fn(&str) -> Option<String>,
    text: &dyn Fn(&str) -> Option<String>,
) {
    let mut seen = HashSet::new();
    for c in &mut r.checks {
        let (id, n) = (c.id, &mut c.ignored);
        let by_line = LINE_KEYED.contains(&id);
        c.items.retain(|i| {
            let key = ignore_key(id, &i.text);
            let Some(was) = ignored.get(&key) else { return true };
            let now = i.page.as_deref().map(|p| version(p).unwrap_or_default()).unwrap_or_default();
            if !by_line && *was != now {
                return true;
            }
            seen.insert(key);
            *n += 1;
            false
        });
    }
    ignored.retain(|k, _| {
        if seen.contains(k) {
            return true;
        }
        let Some((check, issue)) = k.split_once('|') else { return false };
        let Some((page, line)) = issue.split_once(": ").filter(|_| LINE_KEYED.contains(&check)) else { return false };
        text(page).is_some_and(|t| t.lines().any(|l| claim_line(l) == line))
    });
}

// ── Safe fixes ────────────────────────────────────────────────────────────────

/// The page with the first plain mention of `name` in its body made a link.
pub fn link_first_mention(text: &str, name: &str) -> Option<String> {
    let at = plain_mention(text, name)?;
    Some(format!("{}[[{name}]]{}", &text[..at], &text[at + name.len()..]))
}

/// Where `name` first appears in the page's body as a whole word that isn't in code, a heading,
/// the properties, or a link already (a link's target or anchor too), as a byte offset.
fn plain_mention(text: &str, name: &str) -> Option<usize> {
    let start = FRONTMATTER.find(text).map_or(0, |m| m.end());
    let lines = crate::markdown::lines(text, 0);
    for l in lines.iter().filter(|l| !l.code && l.start >= start) {
        let raw = &text[l.start..l.start + l.text.len()];
        if crate::markdown::heading(raw).is_some() {
            continue;
        }
        let blanked = crate::markdown::blank_code_spans(raw);
        let busy = link_spans(&blanked);
        let found = mentions(&blanked, name).find(|i| !busy.iter().any(|r| r.contains(i)));
        if let Some(i) = found {
            return Some(l.start + i);
        }
    }
    None
}

static LINKISH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!?\[\[[^\]\n]*\]\]|!?\[[^\]\n]*\]\([^)\n]*\)|<[^>\n]+>|https?://\S+").unwrap());

fn link_spans(line: &str) -> Vec<std::ops::Range<usize>> {
    LINKISH.find_iter(line).map(|m| m.range()).collect()
}

/// The page with `updated:` set to `day`.
pub fn fix_updated(text: &str, day: &str) -> Option<String> {
    crate::write::with_property(text, "updated", Some(day)).ok().filter(|t| t != text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reviews::fixture;

    fn vault(tmp: &Path) -> PathBuf {
        let root = tmp.join("vault");
        let copy = |from: &Path| {
            for e in walkdir::WalkDir::new(from) {
                let e = e.unwrap();
                let dest = root.join(e.path().strip_prefix(from).unwrap());
                if e.file_type().is_dir() {
                    std::fs::create_dir_all(&dest).unwrap();
                } else {
                    std::fs::copy(e.path(), &dest).unwrap();
                }
            }
        };
        copy(&fixture::dir().join("vault"));
        copy(&fixture::dir().join("lint/vault"));
        for e in walkdir::WalkDir::new(&root).into_iter().flatten().filter(|e| e.file_type().is_file()) {
            fixture::set_mtime(e.path(), "2026-01-01 12:00");
        }
        let table = std::fs::read_to_string(fixture::dir().join("lint/mtimes.txt")).unwrap();
        for line in table.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let (path, when) = line.split_once('\t').unwrap();
            fixture::set_mtime(&root.join(path), when);
        }
        root
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 2).unwrap()
    }

    #[test]
    fn reads_a_note_again_once_it_changes() {
        let t = tempfile::tempdir().unwrap();
        let page = t.path().join("wiki/entities/A.md");
        std::fs::create_dir_all(page.parent().unwrap()).unwrap();
        let missing = |root: &Path| run_in(root, today(), &HashSet::new(), fixture::zone()).checks[0].items.len();
        std::fs::write(&page, "See [[Ghost]].\n").unwrap();
        fixture::set_mtime(&page, "2026-01-01 12:00");
        assert_eq!(missing(t.path()), 1);
        // The same time, another size; then the same size, another time: read again both ways.
        std::fs::write(&page, "See nothing.\n").unwrap();
        fixture::set_mtime(&page, "2026-01-01 12:00");
        assert_eq!(missing(t.path()), 0);
        std::fs::write(&page, "See [[Ghos]].\n").unwrap();
        fixture::set_mtime(&page, "2026-01-01 12:01");
        assert_eq!(missing(t.path()), 1);
    }

    #[test]
    fn reads_a_note_again_after_a_write_that_kept_its_time_and_size() {
        let t = tempfile::tempdir().unwrap();
        let page = t.path().join("wiki/entities/B.md");
        std::fs::create_dir_all(page.parent().unwrap()).unwrap();
        let stale = |root: &Path| {
            let r = run_in(root, today(), &HashSet::new(), fixture::zone());
            r.checks.iter().find(|c| c.id == "stale-dates").map_or(0, |c| c.items.len())
        };
        std::fs::write(&page, "---\nupdated: 2026-01-01\n---\n# B\n").unwrap();
        fixture::set_mtime(&page, "2026-01-20 12:00");
        assert_eq!(stale(t.path()), 1);
        // A safe fix: the same length of date, the file's time kept. Unseen until it's forgotten.
        std::fs::write(&page, "---\nupdated: 2026-01-20\n---\n# B\n").unwrap();
        fixture::set_mtime(&page, "2026-01-20 12:00");
        forget(std::slice::from_ref(&page));
        assert_eq!(stale(t.path()), 0);
    }

    #[test]
    fn matches_lint_wiki_py() {
        let t = tempfile::tempdir().unwrap();
        let root = vault(t.path());
        let r = run_in(&root, today(), &HashSet::new(), fixture::zone());
        let want: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture::dir().join("lint/expected.json")).unwrap()).unwrap();
        let classic: Vec<&Check> = r.checks.iter().filter(|c| c.classic).collect();
        assert_eq!(classic.len(), want.as_object().unwrap().len());
        for c in classic {
            let got: Vec<&str> = c.items.iter().map(|i| i.text.as_str()).collect();
            let exp: Vec<&str> = want[c.id].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(got, exp, "{}", c.id);
        }
    }

    #[test]
    fn brainsteads_own_checks() {
        let t = tempfile::tempdir().unwrap();
        let root = vault(t.path());
        let r = run_in(&root, today(), &HashSet::new(), fixture::zone());
        let dup = &r.check("duplicates").unwrap().items;
        assert_eq!(dup.len(), 1);
        assert_eq!(dup[0].text, "North Wind and Northwind");
        // Dismissed, it's gone.
        let gone = HashSet::from([pair_key("wiki/entities/Northwind.md", "wiki/entities/North Wind.md")]);
        assert!(run_in(&root, today(), &gone, fixture::zone()).check("duplicates").unwrap().items.is_empty());
        // Orbit App: linked from three notes and more, unchanged since January.
        let stale = &r.check("stale-pages").unwrap().items;
        assert_eq!(stale.first().and_then(|i| i.page.as_deref()), Some("wiki/entities/Orbit App.md"));
        // The roadmap changed on 1 October; the pages citing it date from January.
        let changed = &r.check("changed-sources").unwrap().items;
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].page.as_deref(), Some("sources/Roadmap Update 2026-09-18.md"));
        assert_eq!(changed[0].pages, ["wiki/concepts/Role Framework.md", "wiki/entities/Orbit App.md"]);
        let pending: Vec<String> = r.check("uningested-sources").unwrap().items.iter().map(|i| i.text.clone()).collect();
        assert_eq!(pending_sources(&root), pending);
        let safe = r.checks.iter().flat_map(|c| &c.items).filter(|i| i.safe).count();
        assert_eq!(r.needs_decision() + safe + stale.len(), r.total());
    }

    #[test]
    fn duplicate_rules() {
        let p = |n: &str, a: &[&str]| (format!("wiki/entities/{n}.md"), a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let d = duplicates(&[
            p("Nova", &[]),
            p("Nova Platform", &[]),
            p("Hub Platform", &[]),
            p("Hub Platfrom", &[]),
            p("Sprint 21", &[]),
            p("Sprint 22", &[]),
            p("Orbit App", &["OA"]),
            p("OA", &[]),
            p("Maya", &[]),
            p("Lena", &[]),
        ]);
        let pairs: Vec<(String, &str)> = d.iter().map(|d| (format!("{} | {}", stem(name_of(&d.a)), stem(name_of(&d.b))), d.why)).collect();
        assert_eq!(
            pairs,
            [
                ("Nova | Nova Platform".to_string(), "one name starts the other"),
                ("Hub Platform | Hub Platfrom".to_string(), "a letter or two apart"),
                ("Orbit App | OA".to_string(), "one's alias is the other's name"),
            ]
        );
    }

    #[test]
    fn linking_the_first_plain_mention() {
        let page = "---\nname: Northwind\n---\n\n## Orbit App\n\n`Orbit App` and [[Orbit App launch]] and [Orbit App](x) then Orbit Apps, then Orbit App here and Orbit App again.\n";
        let fixed = link_first_mention(page, "Orbit App").unwrap();
        assert!(fixed.ends_with("then Orbit Apps, then [[Orbit App]] here and Orbit App again.\n"));
        assert!(fixed.contains("## Orbit App\n"));
        assert_eq!(link_first_mention("---\nname: Orbit App\n---\nNothing.\n", "Orbit App"), None);
        let fenced = "```\nOrbit App\n```\nOrbit App\n";
        assert_eq!(link_first_mention(fenced, "Orbit App").unwrap(), "```\nOrbit App\n```\n[[Orbit App]]\n");
        // Only in a link's anchor: nothing to link, so the check doesn't list it either.
        assert_eq!(link_first_mention("At the demo ([[Meeting. Demo - 2026-10-02#Questions (Lena Park)]]).\n", "Lena Park"), None);
    }

    #[test]
    fn an_ignored_issue_comes_back_when_its_page_changes() {
        let item = |t: &str, p: &str| Item { text: t.into(), page: Some(p.into()), ..Default::default() };
        let report = || Report {
            checks: vec![Check {
                id: "orphans",
                title: "Orphans",
                classic: true,
                ignored: 0,
                items: vec![item("wiki/entities/Lena.md", "wiki/entities/Lena.md"), item("wiki/entities/Maya.md", "wiki/entities/Maya.md")],
            }],
            wiki_pages: 2,
            sources: 0,
            ms: 0,
        };
        let mut ignored = BTreeMap::from([
            (ignore_key("orphans", "wiki/entities/Lena.md"), "v1".to_string()),
            (ignore_key("orphans", "wiki/entities/Gone.md"), "v1".to_string()),
        ]);
        let mut r = report();
        without_ignored(&mut r, &mut ignored, &|_| Some("v1".into()), &|_| None);
        assert_eq!((r.checks[0].items.len(), r.checks[0].ignored, r.needs_decision()), (1, 1, 1));
        // An issue no check gives any more is forgotten.
        assert_eq!(ignored.len(), 1);
        let mut r = report();
        without_ignored(&mut r, &mut ignored, &|_| Some("v2".into()), &|_| None);
        assert_eq!((r.checks[0].items.len(), r.checks[0].ignored), (2, 0));
        assert!(ignored.is_empty());
    }

    #[test]
    fn an_ignored_claim_stays_ignored_while_its_line_does() {
        let claim = |line: &str| Check {
            id: "uncited-claims",
            title: "Claims with no citation",
            classic: false,
            ignored: 0,
            items: vec![Item {
                text: format!("wiki/Orbit App.md: {line}"),
                page: Some("wiki/Orbit App.md".into()),
                detail: Some(line.into()),
                ..Default::default()
            }],
        };
        let report = |checks: Vec<Check>| Report { checks, wiki_pages: 1, sources: 0, ms: 0 };
        let mut ignored = BTreeMap::from([(ignore_key("uncited-claims", "wiki/Orbit App.md: Launch is on 14 November"), "v1".to_string())]);
        let page = |t: &'static str| move |_: &str| Some(t.to_string());
        // The page changed elsewhere: still ignored.
        let mut r = report(vec![claim("Launch is on 14 November")]);
        without_ignored(&mut r, &mut ignored, &|_| Some("v2".into()), &page("# Orbit App\nLaunch is on 14 November\nNew line\n"));
        assert_eq!((r.checks[0].items.len(), r.checks[0].ignored), (0, 1));
        // Out of the check for a run while the page still has the line: kept.
        let mut r = report(vec![]);
        without_ignored(&mut r, &mut ignored, &|_| Some("v3".into()), &page("  Launch is on 14 November\n"));
        assert_eq!(ignored.len(), 1);
        let mut r = report(vec![claim("Launch is on 14 November")]);
        without_ignored(&mut r, &mut ignored, &|_| Some("v3".into()), &page("Launch is on 14 November\n"));
        assert_eq!(r.checks[0].items.len(), 0);
        // The line itself changed: the new line is listed and the old ignore forgotten.
        let mut r = report(vec![claim("Launch is on 21 November")]);
        without_ignored(&mut r, &mut ignored, &|_| Some("v4".into()), &page("Launch is on 21 November\n"));
        assert_eq!(r.checks[0].items.len(), 1);
        assert!(ignored.is_empty());
    }

    #[test]
    fn fixing_the_updated_date() {
        assert_eq!(fix_updated("---\nupdated: 2026-09-01\n---\nx\n", "2026-10-03").unwrap(), "---\nupdated: 2026-10-03\n---\nx\n");
        assert_eq!(fix_updated("---\nupdated: 2026-10-03\n---\n", "2026-10-03"), None);
    }
}
