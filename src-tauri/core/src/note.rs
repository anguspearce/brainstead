// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Everything the index keeps about one file, parsed in one pass.

use serde_json::Value;

use crate::frontmatter;
use crate::links::{clean_tag, parse_links, parse_tags, Link};
use crate::markdown::{heading, lines};
use crate::tasks::{parse_tasks, Task};
use crate::vault::Layer;

#[derive(Debug, Clone)]
pub struct Chunk {
    /// Headings above it, outermost first, joined with " › ".
    pub heading_path: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Parsed {
    pub kind: Option<String>,
    pub title: String,
    /// `YYYY-MM-DD` (or `YYYY-MM`) from the filename, else from the `date` or `created` property.
    pub date: Option<String>,
    pub frontmatter: Value,
    pub frontmatter_error: Option<String>,
    /// Frontmatter tags then inline tags, without `#`, no repeats.
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
    pub links: Vec<Link>,
    pub tasks: Vec<Task>,
    pub chunks: Vec<Chunk>,
    /// The 0-based line of its `Project: [[…]]` line, saying which project it belongs to
    /// (`projects::project_line`).
    pub project_line: Option<usize>,
    /// A Tracker note's Current state text (`pageshape::tracker_current_state`), for its project's page.
    pub current_state: Option<String>,
}

/// A section longer than this is split at its blank lines, so a search hit points somewhere near.
const CHUNK_MAX: usize = 2000;

/// A file that isn't markdown or text (a PDF in sources/): only its name is known.
pub fn file_only(rel: &str) -> Parsed {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    let title = match base.rfind('.') {
        Some(i) if i > 0 => &base[..i],
        _ => base,
    };
    Parsed {
        kind: None,
        title: title.to_string(),
        date: None,
        frontmatter: Value::Null,
        frontmatter_error: None,
        tags: vec![],
        aliases: vec![],
        links: vec![],
        tasks: vec![],
        chunks: vec![],
        project_line: None,
        current_state: None,
    }
}

/// A PDF or Office file by its text: one chunk per page, slide or sheet (headed "Page 6"), split
/// as long sections are.
pub fn parse_extracted(rel: &str, x: &crate::extract::Extracted) -> Parsed {
    let mut p = file_only(rel);
    for part in &x.parts {
        let mut start = 0;
        for piece in split_long(&part.text) {
            let end = start + piece.len();
            p.chunks.push(Chunk { heading_path: part.label.clone(), start, end, text: piece });
            start = end;
        }
    }
    p
}

/// A long text cut at blank lines (or anywhere, failing that) into pieces under `CHUNK_MAX`.
fn split_long(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for para in text.split("\n\n") {
        if !cur.is_empty() && cur.len() + para.len() + 2 > CHUNK_MAX {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push_str("\n\n");
        }
        cur.push_str(para);
        while cur.len() > CHUNK_MAX {
            let mut cut = CHUNK_MAX;
            while !cur.is_char_boundary(cut) {
                cut -= 1;
            }
            let rest = cur.split_off(cut);
            out.push(std::mem::replace(&mut cur, rest));
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

pub fn parse(rel: &str, layer: Layer, src: &str) -> Parsed {
    let fm = frontmatter::split(src);
    let ls = lines(src, fm.body_start);
    let name = crate::filename::parse(rel);
    let stem = crate::filename::stem(rel).to_string();

    let (kind, title) = match layer {
        Layer::Note => (name.kind.clone(), name.title.clone()),
        _ => (frontmatter::string(&fm.data, "type"), frontmatter::string(&fm.data, "title").unwrap_or(stem)),
    };
    let date = name.date.clone().or_else(|| {
        frontmatter::string(&fm.data, "date")
            .or_else(|| frontmatter::string(&fm.data, "created"))
            .filter(|d| d.len() >= 7)
            // Ten characters, not bytes: a date written in other scripts mustn't split a character.
            .map(|d| d.chars().take(10).collect())
    });

    let mut tags: Vec<String> = Vec::new();
    for t in frontmatter::list(&fm.data, &["tags", "tag"], true).iter().chain(parse_tags(&ls).iter()) {
        if let Some(t) = clean_tag(t) {
            if !tags.contains(&t) {
                tags.push(t);
            }
        }
    }
    let aliases = frontmatter::list(&fm.data, &["aliases", "alias"], false);

    Parsed {
        kind,
        title,
        date,
        frontmatter: fm.data,
        frontmatter_error: fm.error,
        tags,
        aliases,
        links: parse_links(&ls),
        tasks: parse_tasks(&ls),
        chunks: chunks(src, fm.body_start, &ls),
        project_line: crate::projects::project_line(&ls),
        current_state: (matches!(layer, Layer::Note) && crate::pageshape::is_tracker(rel))
            .then(|| crate::pageshape::tracker_current_state(src))
            .flatten(),
    }
}

fn chunks(src: &str, body_start: usize, ls: &[crate::markdown::Line]) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut path: Vec<(usize, String)> = Vec::new();
    let mut start = body_start;
    let mut heading_path = String::new();
    let push = |out: &mut Vec<Chunk>, hp: &str, a: usize, b: usize| {
        let text = src[a..b].trim();
        if !text.is_empty() {
            out.push(Chunk { heading_path: hp.to_string(), start: a, end: b, text: text.to_string() });
        }
    };
    let mut last_blank: Option<usize> = None;
    for l in ls {
        if !l.code {
            if let Some((level, h)) = heading(l.text) {
                push(&mut out, &heading_path, start, l.start);
                path.retain(|(lv, _)| *lv < level);
                path.push((level, h.to_string()));
                heading_path = path.iter().map(|(_, h)| h.as_str()).collect::<Vec<_>>().join(" › ");
                start = l.start;
                last_blank = None;
                continue;
            }
            if l.text.trim().is_empty() {
                last_blank = Some(l.start);
            }
        }
        if l.start - start > CHUNK_MAX {
            if let Some(b) = last_blank.filter(|&b| b > start) {
                push(&mut out, &heading_path, start, b);
                start = b;
                last_blank = None;
            }
        }
    }
    push(&mut out, &heading_path, start, src.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_note() {
        let src = "---\ntags: [hiring]\naliases: [JA]\n---\nIntro #followup [[Other]]\n# Top\ntext\n## Sub\n- [ ] do it 📅 2026-10-03\n# Next\nmore";
        let p = parse("Meeting. Role Framework - 2026-10-02.md", Layer::Note, src);
        assert_eq!(p.kind.as_deref(), Some("Meeting"));
        assert_eq!(p.title, "Role Framework");
        assert_eq!(p.date.as_deref(), Some("2026-10-02"));
        assert_eq!(p.tags, ["hiring", "followup"]);
        assert_eq!(p.aliases, ["JA"]);
        assert_eq!(p.links[0].target, "Other");
        assert_eq!(p.tasks[0].heading.as_deref(), Some("Sub"));
        let hp: Vec<&str> = p.chunks.iter().map(|c| c.heading_path.as_str()).collect();
        assert_eq!(hp, ["", "Top", "Top › Sub", "Next"]);
        assert_eq!(p.chunks[0].text, "Intro #followup [[Other]]");
        assert!(p.chunks.iter().all(|c| src[c.start..c.end].contains(&c.text)));
    }

    #[test]
    fn wiki_title_and_long_sections() {
        let p = parse("wiki/entities/orbit-app.md", Layer::Wiki, "---\ntitle: Orbit App\ntype: entity\ndate: 2026-09-18T10:00\n---\nx");
        assert_eq!((p.title.as_str(), p.kind.as_deref(), p.date.as_deref()), ("Orbit App", Some("entity"), Some("2026-09-18")));
        let long = (0..200).map(|i| format!("Paragraph {i} with some words in it.\n\n")).collect::<String>();
        let p = parse("Long.md", Layer::Note, &long);
        assert!(p.chunks.len() > 3);
        assert!(p.chunks.iter().all(|c| c.end - c.start <= CHUNK_MAX + 200));
    }

    #[test]
    fn non_ascii_dates_dont_panic() {
        let p = parse(
            "wiki/a.md",
            Layer::Wiki,
            "---
date: 二〇二六年十月三日です
---
x",
        );
        assert_eq!(p.date.as_deref(), Some("二〇二六年十月三日で"));
        let p = parse(
            "wiki/b.md",
            Layer::Wiki,
            "---
created: 2026-10-0é3
---
x",
        );
        assert_eq!(p.date.as_deref(), Some("2026-10-0é"));
    }
}
