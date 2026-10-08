// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The canonical documents register (Doc check): which version of each governing document is in
//! force, kept as a markdown table in the vault's `Me. Canonical Docs.md`, as the previous app's
//! `canonical_docs.py` reads it: any table whose header starts with `key`, columns key, title,
//! version, status (canonical, draft or superseded), path, aliases. Fenced blocks are examples,
//! not rows. Paths are vault-relative, `~/…` or absolute.

use std::path::{Path, PathBuf};

use serde::Serialize;

pub const REGISTER: &str = "Me. Canonical Docs.md";
const COLUMNS: [&str; 6] = ["key", "title", "version", "status", "path", "aliases"];
const STATUSES: [&str; 3] = ["canonical", "draft", "superseded"];

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Lower case.
    pub key: String,
    pub title: String,
    pub version: String,
    /// canonical, draft or superseded.
    pub status: String,
    pub path: String,
    pub resolved_path: String,
    pub exists: bool,
    pub aliases: Vec<String>,
    /// 1-based, in the register.
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Register {
    pub entries: Vec<Entry>,
    /// Rows that couldn't be read, by line.
    pub problems: Vec<String>,
}

fn cells(line: &str) -> Vec<String> {
    let s = line.trim();
    let s = s.strip_prefix('|').unwrap_or(s);
    let s = s.strip_suffix('|').unwrap_or(s);
    s.split('|').map(|c| c.trim().to_string()).collect()
}

fn separator(c: &[String]) -> bool {
    !c.is_empty() && c.iter().all(|x| x.contains('-') && x.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

/// A register path, vault-relative, `~/…` or absolute (`/docs/x.pdf`; on Windows also `~\…`,
/// `\Docs\x.pdf` and `C:\Docs\x.pdf`), kept as written rather than joined onto the vault, which a
/// rooted path would replace. Outside Windows a backslash is an ordinary character in a name.
pub fn resolve_path(vault: &Path, raw: &str, home: &Path) -> PathBuf {
    let home_rest = raw.strip_prefix("~/").or_else(|| if cfg!(windows) { raw.strip_prefix("~\\") } else { None });
    if let Some(rest) = home_rest {
        return home.join(rest);
    }
    if crate::paths::is_rooted(raw) {
        PathBuf::from(raw)
    } else {
        vault.join(raw)
    }
}

pub fn parse(text: &str, vault: &Path, home: &Path) -> Register {
    let mut r = Register::default();
    let (mut table, mut fence) = (false, false);
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if line.trim_start().starts_with("```") {
            fence = !fence;
            table = false;
            continue;
        }
        if fence {
            continue;
        }
        if !line.contains('|') {
            if table && !line.trim().is_empty() {
                table = false;
            }
            continue;
        }
        let c = cells(line);
        if !table {
            table = c.first().is_some_and(|x| x.eq_ignore_ascii_case("key"));
            continue;
        }
        if separator(&c) {
            continue;
        }
        if c.len() != COLUMNS.len() {
            r.problems.push(format!("line {n}: expected {} columns ({}), found {}", COLUMNS.len(), COLUMNS.join(", "), c.len()));
            continue;
        }
        let empty: Vec<&str> = [(0, "key"), (1, "title"), (2, "version"), (4, "path")]
            .iter()
            .filter(|(k, _)| c[*k].is_empty())
            .map(|(_, name)| *name)
            .collect();
        if !empty.is_empty() {
            r.problems.push(format!("line {n}: empty {}", empty.join(", ")));
            continue;
        }
        let status = c[3].to_lowercase();
        if !STATUSES.contains(&status.as_str()) {
            r.problems.push(format!("line {n}: status '{}' is not one of {}", c[3], STATUSES.join(", ")));
            continue;
        }
        let resolved = resolve_path(vault, &c[4], home);
        r.entries.push(Entry {
            key: c[0].to_lowercase(),
            title: c[1].clone(),
            version: c[2].clone(),
            status,
            path: c[4].clone(),
            exists: resolved.is_file(),
            resolved_path: resolved.to_string_lossy().into_owned(),
            aliases: c[5].split(',').map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect(),
            line: n,
        });
    }
    r
}

/// One document family: the version in force, and its drafts and superseded versions.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub canonical: Option<Entry>,
    /// More than one row marked canonical: the user must say which is in force.
    pub ambiguous: Vec<Entry>,
    pub drafts: Vec<Entry>,
    pub superseded: Vec<Entry>,
}

/// Everything registered for the document named `needle` (its key, title or an alias).
pub fn resolve(entries: &[Entry], needle: &str) -> Resolved {
    let n = needle.trim().to_lowercase();
    let hit = |e: &Entry| e.key == n || e.title.to_lowercase() == n || e.aliases.iter().any(|a| a.to_lowercase() == n);
    let keys: Vec<&str> = entries.iter().filter(|e| hit(e)).map(|e| e.key.as_str()).collect();
    let family: Vec<&Entry> = entries.iter().filter(|e| keys.contains(&e.key.as_str())).collect();
    let canon: Vec<Entry> = family.iter().filter(|e| e.status == "canonical").map(|e| (*e).clone()).collect();
    Resolved {
        canonical: (canon.len() == 1).then(|| canon[0].clone()),
        ambiguous: if canon.len() > 1 { canon } else { vec![] },
        drafts: family.iter().filter(|e| e.status == "draft").map(|e| (*e).clone()).collect(),
        superseded: family.iter().filter(|e| e.status == "superseded").map(|e| (*e).clone()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REG: &str = "# Canonical docs\n\nWhich version is in force.\n\n```\n| key | title | version | status | path | aliases |\n| om | Fake | v0 | canonical | x.pdf | |\n```\n\n| key | title | version | status | path | aliases |\n|---|---|---|---|---|---|\n| om | Acme Operating Model | v2.3 | canonical | sources/acme-om-v2-3.pdf | operating model, OM |\n| om | Acme Operating Model | v2.4 | draft | ~/Downloads/acme-om-v2-4.docx | |\n| om | Acme Operating Model | v2.2 | Superseded | /docs/acme-om-v2-2.pdf | |\n| pods | Pod Model | v1.1 | canonical | sources/pod-model.pdf | |\n| bad | Missing path | v1 | canonical | | |\n| odd | Odd | v1 | retired | a.pdf | |\n| short | too | few |\n\nPlain prose after the table.\n";

    #[test]
    fn reads_the_register_as_the_previous_app_did() {
        let r = parse(REG, Path::new("/vault"), Path::new("/home/maya"));
        assert_eq!(r.entries.len(), 4, "{:?}", r.problems);
        let om = &r.entries[0];
        assert_eq!((om.key.as_str(), om.version.as_str(), om.status.as_str()), ("om", "v2.3", "canonical"));
        assert_eq!(om.aliases, vec!["operating model", "OM"]);
        // Joined with the platform's separator, so compare against a join too.
        let joined = |base: &str, rel: &str| Path::new(base).join(rel).to_string_lossy().into_owned();
        assert_eq!(om.resolved_path, joined("/vault", "sources/acme-om-v2-3.pdf"));
        assert_eq!(r.entries[1].resolved_path, joined("/home/maya", "Downloads/acme-om-v2-4.docx"));
        assert_eq!(r.entries[2].resolved_path, "/docs/acme-om-v2-2.pdf");
        assert_eq!(r.entries[2].status, "superseded");
        assert_eq!(r.problems.len(), 3);
        assert!(r.problems[0].contains("empty path"));
        assert!(r.problems[1].contains("retired"));
        assert!(r.problems[2].contains("found 3"));
    }

    #[test]
    fn register_paths_on_every_system() {
        let (vault, home) = (Path::new("/vault"), Path::new("/home/maya"));
        assert_eq!(resolve_path(vault, "~/Downloads/a.docx", home), home.join("Downloads/a.docx"));
        assert_eq!(resolve_path(vault, "/Docs/a.pdf", home), PathBuf::from("/Docs/a.pdf"));
        assert_eq!(resolve_path(vault, "sources/a.pdf", home), vault.join("sources/a.pdf"));
    }

    #[cfg(windows)]
    #[test]
    fn register_paths_written_the_windows_way_on_windows() {
        let (vault, home) = (Path::new("/vault"), Path::new("/home/maya"));
        assert_eq!(resolve_path(vault, r"~\Downloads\a.docx", home), home.join(r"Downloads\a.docx"));
        assert_eq!(resolve_path(vault, r"\Docs\a.pdf", home), PathBuf::from(r"\Docs\a.pdf"));
        assert_eq!(resolve_path(vault, r"C:\Docs\a.pdf", home), PathBuf::from(r"C:\Docs\a.pdf"));
    }

    #[cfg(not(windows))]
    #[test]
    fn a_backslash_or_colon_in_a_register_path_is_part_of_its_name_elsewhere() {
        let (vault, home) = (Path::new("/vault"), Path::new("/home/maya"));
        for raw in ["C: Notes/x.md", r"C:\Docs\a.pdf", r"\Docs\a.pdf", r"~\Downloads\a.docx"] {
            assert_eq!(resolve_path(vault, raw, home), vault.join(raw), "{raw}");
        }
    }

    #[test]
    fn resolves_a_family_by_key_title_or_alias() {
        let r = parse(REG, Path::new("/vault"), Path::new("/home/maya"));
        for needle in ["om", "Operating model", "acme operating model"] {
            let x = resolve(&r.entries, needle);
            assert_eq!(x.canonical.as_ref().map(|e| e.version.as_str()), Some("v2.3"), "{needle}");
            assert_eq!((x.drafts.len(), x.superseded.len()), (1, 1));
        }
        assert!(resolve(&r.entries, "nothing").canonical.is_none());
        let mut two = r.entries.clone();
        two[1].status = "canonical".into();
        let x = resolve(&two, "om");
        assert!(x.canonical.is_none());
        assert_eq!(x.ambiguous.len(), 2);
    }
}
