// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The app's help, for Ask: the topics in `src/help/` (embedded by build.rs, the same files the
//! window's `?` drawer shows), listed, read by topic or section, and searched by word. Each file is
//! frontmatter (`title`, `kind`: screen or guide, `screens`, `order`, `summary`) and `##` sections.

use std::sync::LazyLock;

include!(concat!(env!("OUT_DIR"), "/help_files.rs"));

#[derive(Debug, Clone, Default)]
struct Meta {
    title: String,
    kind: String,
    screens: Vec<String>,
    order: i64,
    summary: String,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct Topic {
    /// The file name without `.md`: `tasks`, `guide-start`.
    pub id: String,
    pub title: String,
    /// `screen` or `guide`.
    pub kind: String,
    pub screens: Vec<String>,
    pub order: i64,
    pub summary: String,
    /// The text before the first section.
    pub intro: String,
    pub sections: Vec<Section>,
}

/// The frontmatter read as the window reads it (src/help/index.ts): `key: value` lines, and
/// `[a, b]` lists, so a summary may hold a colon.
fn meta(fm: &str) -> Meta {
    let mut m = Meta::default();
    for line in fm.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim();
        match k.trim() {
            "title" => m.title = v.to_string(),
            "kind" => m.kind = v.to_string(),
            "summary" => m.summary = v.to_string(),
            "order" => m.order = v.parse().unwrap_or(0),
            "screens" => {
                m.screens = v.trim_matches(['[', ']']).split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()
            }
            _ => {}
        }
    }
    m
}

fn parse(name: &str, text: &str) -> Topic {
    let text = text.replace("\r\n", "\n");
    let (meta, body) = match text.strip_prefix("---\n").and_then(|r| r.split_once("\n---\n")) {
        Some((fm, rest)) => (meta(fm), rest.to_string()),
        None => (Meta::default(), text.clone()),
    };
    let mut intro = String::new();
    let mut sections: Vec<Section> = Vec::new();
    for line in body.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            sections.push(Section { title: h.trim().to_string(), body: String::new() });
        } else if let Some(s) = sections.last_mut() {
            s.body.push_str(line);
            s.body.push('\n');
        } else {
            intro.push_str(line);
            intro.push('\n');
        }
    }
    for s in &mut sections {
        s.body = s.body.trim().to_string();
    }
    let id = name.trim_end_matches(".md").to_string();
    Topic {
        title: if meta.title.is_empty() { id.clone() } else { meta.title },
        id,
        kind: if meta.kind.is_empty() { "screen".into() } else { meta.kind },
        screens: meta.screens,
        order: meta.order,
        summary: meta.summary,
        intro: intro.trim().to_string(),
        sections,
    }
}

/// Every topic: screens first, then guides, each in their order.
pub static TOPICS: LazyLock<Vec<Topic>> = LazyLock::new(|| {
    let mut t: Vec<Topic> = HELP_FILES.iter().map(|(n, s)| parse(n, s)).collect();
    t.sort_by(|a, b| (a.kind != "screen", a.order, &a.title).cmp(&(b.kind != "screen", b.order, &b.title)));
    t
});

/// The list of topics, one line each, for a model to choose from.
pub fn list() -> String {
    let mut out = String::from("Brainstead's help topics (id: title — what it covers). Each is `<id>.md`.\n");
    for t in TOPICS.iter() {
        let kind = if t.kind == "guide" { " (getting-started guide)" } else { "" };
        out.push_str(&format!("- {}: {}{kind} — {}\n", t.id, t.title, t.summary));
    }
    out
}

fn render(t: &Topic, only: Option<&Section>) -> String {
    let mut out = format!("# {}\n", t.title);
    match only {
        Some(s) => out.push_str(&format!("\n## {}\n\n{}\n", s.title, s.body)),
        None => {
            if !t.intro.is_empty() {
                out.push_str(&format!("\n{}\n", t.intro));
            }
            for s in &t.sections {
                out.push_str(&format!("\n## {}\n\n{}\n", s.title, s.body));
            }
        }
    }
    out
}

/// One topic by id (or title), or one of its sections (by title, ignoring case).
pub fn read(topic: &str, section: Option<&str>) -> Result<String, String> {
    let want = topic.trim().to_lowercase();
    let t = TOPICS
        .iter()
        .find(|t| t.id == want || t.title.to_lowercase() == want)
        .ok_or_else(|| format!("No help topic called “{topic}”. The topics:\n{}", list()))?;
    match section.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(render(t, None)),
        Some(s) => {
            let w = s.to_lowercase();
            let sec = t.sections.iter().find(|x| x.title.to_lowercase() == w).ok_or_else(|| {
                let names: Vec<&str> = t.sections.iter().map(|x| x.title.as_str()).collect();
                format!("“{}” has no section called “{s}”. Its sections: {}.", t.title, names.join(", "))
            })?;
            Ok(render(t, Some(sec)))
        }
    }
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP.contains(w))
        .map(|w| w.trim_end_matches('s').to_string())
        .collect()
}

const STOP: &[&str] = &[
    "the", "and", "for", "how", "can", "you", "what", "does", "with", "this", "that", "from", "into", "are", "use", "where", "when", "why",
];

/// The sections that best match `query`, best first (a title word counts three times a body word).
pub fn search(query: &str, limit: usize) -> String {
    let q = words(query);
    if q.is_empty() {
        return list();
    }
    let mut hits: Vec<(usize, &Topic, &Section)> = Vec::new();
    for t in TOPICS.iter() {
        let head = words(&format!("{} {}", t.title, t.summary));
        for s in &t.sections {
            let title = words(&s.title);
            let body = words(&s.body);
            let score: usize = q
                .iter()
                .map(|w| {
                    3 * title.iter().filter(|x| *x == w).count().min(1)
                        + 2 * head.iter().filter(|x| *x == w).count().min(1)
                        + body.iter().filter(|x| *x == w).count().min(3)
                })
                .sum();
            let found = q.iter().filter(|w| title.contains(w) || head.contains(w) || body.contains(w)).count();
            if found > 0 {
                hits.push((score * found, t, s));
            }
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    if hits.is_empty() {
        return format!("Nothing in the help matches “{query}”.\n\n{}", list());
    }
    let mut out = String::new();
    for (_, t, s) in hits.into_iter().take(limit) {
        out.push_str(&format!("# {} › {}  (topic: {})\n\n{}\n\n", t.title, s.title, t.id, s.body));
    }
    out.trim_end().to_string()
}

/// The files as written, for a CLI that reads them from a folder (Antigravity).
pub fn files() -> &'static [(&'static str, &'static str)] {
    HELP_FILES
}

/// Shortcuts written with macOS glyphs, as Windows names them; text without them is unchanged.
/// The window's `keyLabel` (src/platform.ts) applies the same rule, and both are checked against
/// tests/fixtures/keylabels.json. A run of modifiers becomes words joined by `+` (⌘ and ⌃ Ctrl,
/// ⌥ Alt, ⇧ Shift), then `+` and the key that follows it, or `-click` as written: ⇧⌘R is
/// Ctrl+Shift+R and ⌥-click is Alt-click. ⌥ with ⌃, and ⌥⌘ before a digit, are Ctrl+Shift instead
/// (Ctrl+Alt is AltGr on many layouts), so ⌃⌥Space is Ctrl+Shift+Space and ⌥⌘1 Ctrl+Shift+1,
/// while ⌥⌘C stays Ctrl+Alt+C. Key glyphs (↩ ⌫ ⌦ ⇥ ⎋ ␣) become their names wherever they stand.
pub fn windows_keys(text: &str) -> String {
    const MODS: [char; 4] = ['⌃', '⌥', '⇧', '⌘'];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if !MODS.contains(&c) {
            out.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        let end = rest.find(|c: char| !MODS.contains(&c)).unwrap_or(rest.len());
        let mods = &rest[..end];
        rest = &rest[end..];
        let key = match rest.chars().next() {
            _ if rest.starts_with("-click") => "-click",
            Some(k) if k.is_ascii_alphanumeric() => &rest[..rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len())],
            Some(k) if !k.is_whitespace() => &rest[..k.len_utf8()],
            _ => "",
        };
        rest = &rest[key.len()..];
        let has = |g: char| mods.contains(g);
        let digit = key.len() == 1 && key.as_bytes()[0].is_ascii_digit();
        let screen = has('⌥') && (has('⌃') || (has('⌘') && digit));
        let mut words = Vec::new();
        if has('⌃') || has('⌘') {
            words.push("Ctrl");
        }
        if has('⌥') && !screen {
            words.push("Alt");
        }
        if has('⇧') || screen {
            words.push("Shift");
        }
        out.push_str(&words.join("+"));
        match key {
            "" => {}
            "-click" => out.push_str(key),
            _ => {
                out.push('+');
                out.push_str(key);
            }
        }
    }
    let names = [('↩', "Enter"), ('⌫', "Backspace"), ('⌦', "Delete"), ('⇥', "Tab"), ('⎋', "Esc"), ('␣', "Space")];
    out.chars().fold(String::with_capacity(out.len()), |mut s, c| {
        match names.iter().find(|(g, _)| *g == c) {
            Some((_, name)) => s.push_str(name),
            None => s.push(c),
        }
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter_and_sections() {
        let t = parse(
            "x.md",
            "---\ntitle: Inbox\nkind: screen\nscreens: [inbox]\norder: 2\nsummary: Clarify: what you captured.\n---\nIntro.\n\n## Keys\n\nJ and K.\n\n## Tips\nOne.\n",
        );
        assert_eq!((t.id.as_str(), t.title.as_str(), t.kind.as_str(), t.order), ("x", "Inbox", "screen", 2));
        assert_eq!(t.screens, ["inbox"]);
        assert_eq!(t.intro, "Intro.");
        assert_eq!(t.summary, "Clarify: what you captured.");
        assert_eq!(t.sections.len(), 2);
        assert_eq!((t.sections[0].title.as_str(), t.sections[0].body.as_str()), ("Keys", "J and K."));
    }

    #[test]
    fn every_file_is_a_proper_topic() {
        assert!(!TOPICS.is_empty());
        for t in TOPICS.iter() {
            assert!(!t.title.is_empty() && t.title != t.id, "{} has no title", t.id);
            assert!(t.kind == "screen" || t.kind == "guide", "{}: kind {}", t.id, t.kind);
            assert!(!t.summary.is_empty(), "{} has no summary", t.id);
            assert!(!t.sections.is_empty(), "{} has no sections", t.id);
        }
    }

    #[test]
    fn reads_and_searches() {
        let first = &TOPICS[0];
        assert!(read(&first.id, None).unwrap().starts_with(&format!("# {}", first.title)));
        let s = &first.sections[0];
        assert!(read(&first.title.to_uppercase(), Some(&s.title.to_lowercase())).unwrap().contains(&s.body));
        assert!(read("no such thing", None).unwrap_err().contains("The topics"));
        assert!(read(&first.id, Some("nope")).unwrap_err().contains("Its sections"));
        assert!(list().contains(&first.id));
        assert!(search(&s.title, 3).contains(&s.title));
        assert!(search("zzqx", 3).starts_with("Nothing in the help"));
    }

    #[test]
    fn windows_keys_matches_the_windows_labels() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/keylabels.json")).expect("keylabels.json");
        let labels = fixture["labels"].as_array().expect("labels");
        assert!(!labels.is_empty());
        for pair in labels {
            let (mac, windows) = (pair["mac"].as_str().unwrap(), pair["windows"].as_str().unwrap());
            assert_eq!(windows_keys(mac), windows, "{mac}");
        }
    }

    #[test]
    fn windows_keys_names_key_glyphs_and_bare_modifiers() {
        assert_eq!(windows_keys("Undo (⌘Z), then ⇧⌘Z"), "Undo (Ctrl+Z), then Ctrl+Shift+Z");
        assert_eq!(windows_keys("⌘ and ⇧ alone, ⌫ and ⎋"), "Ctrl and Shift alone, Backspace and Esc");
        assert_eq!(windows_keys("no shortcut here"), "no shortcut here");
    }
}
