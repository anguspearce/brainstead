// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! `index.md`'s catalogue of the wiki (§7 skill verdicts: "regenerated catalogue, kept current"):
//! every entity, concept, initiative and summary page with its description, by kind, between
//! `<!-- brainstead:index -->` markers so the rest of the file stays the user's. the daily check
//! brings it up to date; adding the markers the first time is a proposal.

use std::path::Path;

pub const START: &str = "<!-- brainstead:index -->";
pub const END: &str = "<!-- /brainstead:index -->";

/// The catalogue, markers included.
pub fn render(root: &Path) -> String {
    let mut out = format!("{START}\n");
    for (sub, title) in [("entities", "Entities"), ("concepts", "Concepts"), ("initiatives", "Initiatives"), ("summaries", "Summaries")] {
        let Ok(rd) = std::fs::read_dir(root.join("wiki").join(sub)) else { continue };
        let mut rows: Vec<(String, String)> = rd
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let stem = name.strip_suffix(".md")?.to_string();
                let text = std::fs::read_to_string(e.path()).unwrap_or_default();
                let fm = crate::frontmatter::split(&text);
                let d = crate::frontmatter::string(&fm.data, "description").unwrap_or_default();
                Some((stem, d.split_whitespace().collect::<Vec<_>>().join(" ")))
            })
            .collect();
        if rows.is_empty() {
            continue;
        }
        rows.sort_by_key(|r| r.0.to_lowercase());
        out.push_str(&format!("\n## {title} ({})\n\n", rows.len()));
        for (stem, d) in rows {
            if d.is_empty() {
                out.push_str(&format!("- [[{stem}]]\n"));
            } else {
                out.push_str(&format!("- [[{stem}]] — {d}\n"));
            }
        }
    }
    out.push_str(&format!("\n{END}"));
    out
}

/// `index.md` with the catalogue between its markers replaced; None when it has no markers (or
/// the catalogue is unchanged).
pub fn update(text: &str, catalogue: &str) -> Option<String> {
    let a = text.find(START)?;
    let b = text[a..].find(END)? + a + END.len();
    let crlf = text.contains("\r\n");
    let cat = if crlf { catalogue.replace('\n', "\r\n") } else { catalogue.to_string() };
    let out = format!("{}{cat}{}", &text[..a], &text[b..]);
    (out != text).then_some(out)
}

/// `index.md` with the catalogue added at its end, markers and all: the first time, as a
/// proposal.
pub fn with_markers(text: &str, catalogue: &str) -> String {
    let base = text.trim_end();
    if base.is_empty() {
        format!("# Index\n\n{catalogue}\n")
    } else {
        format!("{base}\n\n{catalogue}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_between_markers() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        for (p, s) in [
            ("wiki/entities/Orbit App.md", "---\ndescription: The staff app.\n---\n"),
            ("wiki/entities/maya.md", "---\nname: Maya\n---\n"),
            ("wiki/concepts/Hub Platform.md", "---\ndescription: Integration\n  layer.\n---\n"),
        ] {
            std::fs::create_dir_all(root.join(p).parent().unwrap()).unwrap();
            std::fs::write(root.join(p), s).unwrap();
        }
        let cat = render(root);
        assert_eq!(
            cat,
            format!("{START}\n\n## Entities (2)\n\n- [[maya]]\n- [[Orbit App]] — The staff app.\n\n## Concepts (1)\n\n- [[Hub Platform]] — Integration layer.\n\n{END}")
        );
        let index = format!("# Index\n\nMy own notes.\n\n{START}\nold\n{END}\n\nMore of mine.\n");
        let up = update(&index, &cat).unwrap();
        assert!(
            up.starts_with("# Index\n\nMy own notes.\n\n<!-- brainstead:index -->\n\n## Entities")
                && up.ends_with(&format!("{END}\n\nMore of mine.\n"))
        );
        assert_eq!(update(&up, &cat), None);
        assert_eq!(update("# Index\n", &cat), None);
        assert!(with_markers("# Index\n\nMine.\n", &cat).starts_with("# Index\n\nMine.\n\n<!-- brainstead:index -->"));
    }
}
