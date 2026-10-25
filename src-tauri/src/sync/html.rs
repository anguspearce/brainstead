// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Turns the HTML that chat messages and mail bodies come as into plain text for the vault. Not a
//! full HTML renderer: it keeps the line breaks that matter (paragraphs, list items, `<br>`), drops
//! the rest of the tags, and decodes the handful of entities that actually turn up, so a captured
//! source reads cleanly and its text can be quoted on ingest.

/// Plain text from an HTML fragment.
pub fn to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' => {
                // Read the tag name (and whether it's a closing tag) to decide on a line break.
                let mut tag = String::new();
                for t in chars.by_ref() {
                    if t == '>' {
                        break;
                    }
                    tag.push(t);
                }
                let name: String =
                    tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
                match name.as_str() {
                    "br" => out.push('\n'),
                    "p" | "div" | "li" | "tr" | "ul" | "ol" | "table" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote"
                        if !out.ends_with('\n') =>
                    {
                        out.push('\n');
                    }
                    _ => {}
                }
            }
            '&' => {
                let mut ent = String::new();
                while let Some(&t) = chars.peek() {
                    if t == ';' {
                        chars.next();
                        break;
                    }
                    if ent.len() > 10 || t == '<' || t == '&' {
                        break;
                    }
                    ent.push(t);
                    chars.next();
                }
                out.push_str(&decode_entity(&ent));
            }
            _ => out.push(c),
        }
    }
    // Tidy whitespace: trim each line, drop runs of blank lines, trim the ends.
    let mut lines: Vec<String> = out.lines().map(|l| l.trim_end().to_string()).collect();
    let mut tidy: Vec<String> = Vec::with_capacity(lines.len());
    for l in lines.drain(..) {
        if l.trim().is_empty() && tidy.last().map(|p: &String| p.trim().is_empty()).unwrap_or(true) {
            continue;
        }
        tidy.push(l);
    }
    tidy.join("\n").trim().to_string()
}

fn decode_entity(ent: &str) -> String {
    match ent {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" | "#39" => "'".into(),
        "nbsp" | "#160" => " ".into(),
        "hellip" => "…".into(),
        "mdash" => "—".into(),
        "ndash" => "–".into(),
        _ if ent.starts_with('#') => {
            let n = ent.trim_start_matches('#');
            let code = if let Some(h) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                u32::from_str_radix(h, 16).ok()
            } else {
                n.parse::<u32>().ok()
            };
            code.and_then(char::from_u32).map(String::from).unwrap_or_else(|| format!("&{ent};"))
        }
        "" => "&".into(),
        _ => format!("&{ent};"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_structure_and_decodes_entities() {
        let html = "<p>Hi Maya,</p><div>Line one<br>Line two</div><ul><li>a &amp; b</li><li>c</li></ul>";
        assert_eq!(to_text(html), "Hi Maya,\nLine one\nLine two\na & b\nc");
    }

    #[test]
    fn strips_tags_and_collapses_blank_lines() {
        let html = "<div></div><div>Only line</div><div></div>";
        assert_eq!(to_text(html), "Only line");
        assert_eq!(to_text("plain text, no tags"), "plain text, no tags");
        assert_eq!(to_text("<b>bold</b> and <i>italic</i>"), "bold and italic");
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(to_text("tab&#8594;end &#x2014; dash"), "tab→end — dash");
    }
}
