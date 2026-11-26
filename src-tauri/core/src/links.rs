// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Wikilinks and tags. Links follow the usual wikilink format: `[[Note]]`, `[[Note|shown]]`, `[[Note#Heading]]`,
//! `[[Note#^block]]`, `[[#Heading]]` (this note) and embeds `![[...]]`. Like the previous app's
//! `WIKILINK_PATTERN` (bff/src/lib/wikilink.ts), a link never spans lines, and MDXEditor's escaped
//! `\[\[...\]\]` is read too.

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::markdown::{blank_code_spans, Line};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The note part as written (empty for a link within the same note).
    pub target: String,
    pub kind: LinkKind,
    pub heading: Option<String>,
    pub block: Option<String>,
    pub alias: Option<String>,
    /// 0-based line it's on.
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Link,
    Embed,
}

impl LinkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LinkKind::Link => "link",
            LinkKind::Embed => "embed",
        }
    }
}

static WIKILINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(!?)\\?\[\\?\[([^\[\]\n]+?)\\?\]\\?\]").unwrap());

/// Tags as markdown vault editors read them: `#` then letters, digits, `_`, `-` and `/` for nesting, with at
/// least one character that isn't a digit, after the start of the line or a space or bracket.
pub(crate) static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|[\s(\[,;])#([\p{L}\p{N}_\-/]+)").unwrap());

/// The key a link target and a file name are compared by: NFC, lower case, the base name without
/// `.md` or `.txt` (other extensions stay, so `![[pic.png]]` never resolves to a note called pic).
pub fn key(target: &str) -> String {
    let base = target.trim().rsplit(['/', '\\']).next().unwrap_or("");
    crate::filename::stem(base).trim().nfc().collect::<String>().to_lowercase()
}

pub fn nfc(s: &str) -> String {
    s.nfc().collect()
}

pub fn parse_links(lines: &[Line]) -> Vec<Link> {
    let mut out = Vec::new();
    for l in lines.iter().filter(|l| !l.code) {
        if !l.text.contains("[[") && !l.text.contains("\\[\\[") {
            continue;
        }
        let text = blank_code_spans(l.text);
        for c in WIKILINK.captures_iter(&text) {
            let embed = !c[1].is_empty();
            let inner = c[2].replace("\\|", "|");
            let (dest, alias) = match inner.split_once('|') {
                Some((d, a)) => (d.trim().to_string(), Some(a.trim().to_string()).filter(|a| !a.is_empty())),
                None => (inner.trim().to_string(), None),
            };
            let (target, frag) = match dest.split_once('#') {
                Some((t, f)) => (t.trim().to_string(), Some(f.trim().to_string())),
                None => (dest, None),
            };
            let (heading, block) = match frag {
                Some(f) if f.starts_with('^') => (None, Some(f[1..].to_string())),
                Some(f) if !f.is_empty() => (Some(f), None),
                _ => (None, None),
            };
            if target.is_empty() && heading.is_none() && block.is_none() {
                continue;
            }
            out.push(Link {
                target: nfc(&target),
                kind: if embed { LinkKind::Embed } else { LinkKind::Link },
                heading,
                block,
                alias,
                line: l.no,
            });
        }
    }
    out
}

/// Inline tags in the body, without the `#`, in the order first seen. Nested tags (`#a/b`) are
/// kept whole.
pub fn parse_tags(lines: &[Line]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in lines.iter().filter(|l| !l.code) {
        if !l.text.contains('#') {
            continue;
        }
        let text = blank_code_spans(l.text);
        for t in tags_in(&text) {
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// Tags in one line of text.
pub fn tags_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for c in TAG.captures_iter(text) {
        let t = c[1].trim_end_matches(['/', '-']);
        if t.is_empty() || t.chars().all(|ch| ch.is_ascii_digit()) || t.starts_with('/') {
            continue;
        }
        let t = nfc(t);
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// Normalises a tag from frontmatter or the body for storing: no leading `#`, NFC.
pub fn clean_tag(t: &str) -> Option<String> {
    let t = t.trim().trim_start_matches('#').trim();
    (!t.is_empty()).then(|| nfc(t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::lines;

    fn links(s: &str) -> Vec<Link> {
        parse_links(&lines(s, 0))
    }

    #[test]
    fn link_shapes() {
        let l =
            links("See [[Orbit App]], [[Orbit App|the app]], [[Orbit App#Current state]], [[Orbit App#^q3]], ![[pic.png]] and [[#Local]].");
        assert_eq!(l.len(), 6);
        assert_eq!((l[0].target.as_str(), l[0].kind), ("Orbit App", LinkKind::Link));
        assert_eq!(l[1].alias.as_deref(), Some("the app"));
        assert_eq!(l[2].heading.as_deref(), Some("Current state"));
        assert_eq!(l[3].block.as_deref(), Some("q3"));
        assert_eq!((l[4].target.as_str(), l[4].kind), ("pic.png", LinkKind::Embed));
        assert_eq!((l[5].target.as_str(), l[5].heading.as_deref()), ("", Some("Local")));
    }

    #[test]
    fn not_links() {
        assert!(links("`[[code]]` and\n```\n[[fenced]]\n```\n[[unclosed\nnext]]").is_empty());
        assert_eq!(links("escaped \\[\\[Foo\\]\\]")[0].target, "Foo");
        assert_eq!(links("| [[A\\|b]] |")[0].alias.as_deref(), Some("b"));
        assert_eq!(links("a\n[[X]]")[0].line, 1);
    }

    #[test]
    fn keys() {
        assert_eq!(key("wiki/concepts/Foo.md"), "foo");
        assert_eq!(key("Me. Daily Reviews - 2026-10"), "me. daily reviews - 2026-10");
        assert_eq!(key("pic.png"), "pic.png");
        // NFD "é" and NFC "é" are the same key.
        assert_eq!(key("Cafe\u{301}"), key("Caf\u{e9}"));
    }

    #[test]
    fn tags() {
        let t =
            parse_tags(&lines("# Heading\nWork on #project/orbit-app and #followup.\nNot #123 or a#b or `#code` or url#frag\n(#paren)", 0));
        assert_eq!(t, ["project/orbit-app", "followup", "paren"]);
        assert_eq!(tags_in("#waiting-for #someday-maybe"), ["waiting-for", "someday-maybe"]);
        assert_eq!(parse_tags(&lines("## Heading with #tag", 0)), ["tag"]);
    }
}
