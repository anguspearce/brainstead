// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! YAML frontmatter at the top of a note: `---` on the first line, closed by `---` or `...`.

use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    /// The parsed YAML as JSON (an object), or Null when there is none or it doesn't parse.
    pub data: Value,
    /// Why the YAML didn't parse, for Wiki health later.
    pub error: Option<String>,
    /// Byte offset where the body starts (0 when there's no frontmatter).
    pub body_start: usize,
}

pub fn split(src: &str) -> Frontmatter {
    let src_nb = src.strip_prefix('\u{feff}').unwrap_or(src);
    let bom = src.len() - src_nb.len();
    let first = src_nb.split_inclusive('\n').next().unwrap_or("");
    if first.trim_end() != "---" {
        return Frontmatter::default();
    }
    let mut pos = bom + first.len();
    for line in src[pos..].split_inclusive('\n') {
        let t = line.trim_end();
        if t == "---" || t == "..." {
            let yaml = &src[bom + first.len()..pos];
            let body_start = pos + line.len();
            return match serde_yaml_ng::from_str::<Value>(yaml) {
                Ok(Value::Object(m)) => Frontmatter { data: Value::Object(m), error: None, body_start },
                Ok(Value::Null) => Frontmatter { data: Value::Null, error: None, body_start },
                Ok(_) => Frontmatter { data: Value::Null, error: Some("frontmatter is not a set of properties".into()), body_start },
                Err(e) => Frontmatter { data: Value::Null, error: Some(e.to_string()), body_start },
            };
        }
        pos += line.len();
    }
    // Never closed: other editors show it as text, so it's body.
    Frontmatter::default()
}

/// A list property that may be written as a list or as one string (`tags: a, b` or `tags: a b`).
pub fn list(data: &Value, keys: &[&str], split_spaces: bool) -> Vec<String> {
    let mut out = Vec::new();
    for k in keys {
        match data.get(*k) {
            Some(Value::Array(a)) => {
                for v in a {
                    match v {
                        Value::String(s) => out.push(s.trim().to_string()),
                        Value::Number(n) => out.push(n.to_string()),
                        _ => {}
                    }
                }
            }
            Some(Value::String(s)) => {
                let parts: Vec<&str> = if split_spaces { s.split([',', ' ']).collect() } else { s.split(',').collect() };
                out.extend(parts.into_iter().map(str::trim).filter(|p| !p.is_empty()).map(String::from));
            }
            _ => {}
        }
    }
    out.retain(|s| !s.is_empty());
    out
}

pub fn string(data: &Value, key: &str) -> Option<String> {
    match data.get(key)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits() {
        let f = split("---\ntitle: Orbit App\ntags: [project, hub]\naliases:\n  - OA\n---\n# Body\n");
        assert_eq!(f.data["title"], "Orbit App");
        assert_eq!(list(&f.data, &["tags"], true), ["project", "hub"]);
        assert_eq!(list(&f.data, &["aliases"], false), ["OA"]);
        assert_eq!(&"---\ntitle: Orbit App\ntags: [project, hub]\naliases:\n  - OA\n---\n# Body\n"[f.body_start..], "# Body\n");
    }

    #[test]
    fn none_or_bad() {
        assert_eq!(split("# Just a note\n---\n").body_start, 0);
        assert_eq!(split("---\nnever closed\n").body_start, 0);
        let bad = split("---\n: : :\n  - [\n---\nbody");
        assert!(bad.error.is_some());
        assert_eq!(&"---\n: : :\n  - [\n---\nbody"[bad.body_start..], "body");
        let crlf = split("---\r\ntags: a, b\r\n---\r\nx");
        assert_eq!(list(&crlf.data, &["tags"], true), ["a", "b"]);
        assert_eq!(split("---\n---\nx").body_start, 8);
    }
}
