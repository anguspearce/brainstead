// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fix a name everywhere (the previous app's `/fix-name` and `name_audit.py`): every occurrence
//! of a wrongly captured name, a plan of what changes (notes rewritten, the right person's wiki
//! page given the wrong spelling as an alias, sources left alone as the raw record), and the
//! write, with the notes keeping their modification times and the correction registered in the
//! vault's `scripts/substitutions.json` so it stops recurring.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::names::{Substitution, SUBSTITUTIONS};
use crate::write::{self, FileChange, WriteError};

/// Folders the audit never looks in.
const SKIP_DIRS: [&str; 7] = [".trash", ".obsidian", ".claude", "node_modules", ".search-cache", ".git", "scripts"];
/// Lines shown per file.
const SAMPLE: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    /// From 1.
    pub line: usize,
    pub text: String,
}

/// One file the name is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub file: String,
    /// note, wiki, source or template.
    pub layer: String,
    /// The name is in the file's name (renaming is a separate, deliberate step).
    pub in_filename: bool,
    /// Lines it's on.
    pub count: usize,
    pub lines: Vec<Hit>,
    /// What the plan does: rewrite, alias (the right page gets the alias), leave, guarded, or
    /// conflict (a sync conflict copy sits beside it: sort that out, then fix it).
    pub action: String,
    /// The file's version when audited: `apply` refuses it if it has changed since.
    #[serde(default)]
    pub version: String,
}

fn layer_of(rel: &str) -> &'static str {
    match rel.split_once('/').map(|(t, _)| t) {
        Some("wiki") => "wiki",
        Some("sources") => "source",
        Some("Templates") => "template",
        _ => "note",
    }
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Whole-word, case-sensitive places: `(?<![\w-])name(?![\w-])`.
fn places(text: &str, name: &str) -> Vec<std::ops::Range<usize>> {
    if name.is_empty() {
        return vec![];
    }
    text.match_indices(name)
        .map(|(i, _)| i..i + name.len())
        .filter(|r| !text[..r.start].chars().next_back().is_some_and(word) && !text[r.end..].chars().next().is_some_and(word))
        .collect()
}

static WIKILINK: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\[\[([^\[\]\n]+?)\]\]").unwrap());

/// Where a markdown link points (`](…)`), a bare web address, and an autolink (`<https://…>`,
/// `<maya@example.com>`).
static ADDRESS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"\]\([^)\n]*\)|https?://[^\s<>]+|<(?:[A-Za-z][A-Za-z0-9+.-]*:|[^<>\s]*@)[^<>\s]*>").unwrap()
});

/// Byte ranges a rewrite must leave alone: the frontmatter, fenced code, code spans, and the
/// target of each wikilink or embed (up to its `|`), which would stop resolving if changed. An
/// alias after the `|` is text and can change. Web addresses, markdown link destinations and tags
/// too: they name a page, a file or a tag, which the fix doesn't rename.
fn protected(text: &str) -> Vec<std::ops::Range<usize>> {
    let fm = crate::frontmatter::split(text);
    let mut out = Vec::new();
    out.push(0..fm.body_start);
    for l in crate::markdown::lines(text, fm.body_start) {
        let end = l.start + l.text.len();
        if l.code {
            out.push(l.start..end);
            continue;
        }
        let blanked = crate::markdown::blank_code_spans(l.text);
        let (a, b) = (l.text.as_bytes(), blanked.as_bytes());
        let mut i = 0;
        while i < a.len() {
            if a[i] == b[i] {
                i += 1;
                continue;
            }
            let j = (i..a.len()).find(|&j| a[j] == b[j] && a[j] != b' ').unwrap_or(a.len());
            out.push(l.start + i..l.start + j);
            i = j;
        }
        for c in WIKILINK.captures_iter(&blanked) {
            let inner = c.get(1).unwrap();
            let t = &l.text[inner.start()..inner.end()];
            let target = t.find('|').map(|i| if i > 0 && t.as_bytes()[i - 1] == b'\\' { i - 1 } else { i }).unwrap_or(t.len());
            out.push(l.start + inner.start()..l.start + inner.start() + target);
        }
        out.extend(ADDRESS.find_iter(&blanked).map(|m| l.start + m.start()..l.start + m.end()));
        out.extend(crate::links::TAG.captures_iter(&blanked).filter_map(|c| c.get(1)).map(|m| l.start + m.start()..l.start + m.end()));
    }
    out
}

/// The places a rewrite changes: the name in the body's text (not in `protected` places), and
/// when the right name is the wrong one plus a surname ("Sam" → "Sam Carter"), not where a surname
/// follows already, as normalize.py's rule has it.
fn rewrite_places(text: &str, r: &Request) -> Vec<std::ops::Range<usize>> {
    let extends = r.right.starts_with(&format!("{} ", r.wrong));
    let found = places(text, &r.wrong);
    if found.is_empty() {
        return found;
    }
    let guard = protected(text);
    found
        .into_iter()
        .filter(|x| !guard.iter().any(|g| g.start < x.end && x.start < g.end))
        .filter(|x| {
            let rest = text[x.end..].split('\n').next().unwrap_or("");
            let t = rest.trim_start();
            !(extends && t.len() < rest.len() && t.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
        })
        .collect()
}

fn text_files(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            e.depth() == 0
                || !(e.file_type().is_dir()
                    && (e.file_name().to_string_lossy().starts_with('.') || SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref())))
        })
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| crate::vault::rel_of(root, e.path()))
        .filter(|r| {
            let l = r.to_lowercase();
            l.ends_with(".md") || l.ends_with(".txt")
        })
        .collect();
    out.sort();
    out
}

/// What fixing `wrong` → `right` would do. `right_page` is the right name's wiki page when it has
/// one (it gets the alias); `guards` are file-name fragments the correction must never touch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub wrong: String,
    pub right: String,
    #[serde(default)]
    pub right_page: Option<String>,
    #[serde(default)]
    pub guards: Vec<String>,
    #[serde(default)]
    pub ambiguous: bool,
    #[serde(default)]
    pub note: String,
    /// Don't add the correction to the substitutions list.
    #[serde(default)]
    pub skip_substitution: bool,
}

/// Every file the wrong name is in, with what the fix does to it.
pub fn audit(root: &Path, r: &Request) -> Vec<Row> {
    let mut rows = Vec::new();
    for rel in text_files(root) {
        let Ok(text) = std::fs::read_to_string(root.join(&rel)) else { continue };
        let name = crate::lint::name_of(&rel);
        let all: Vec<&str> = text.lines().collect();
        let mut at: Vec<usize> = rewrite_places(&text, r).iter().map(|x| text[..x.start].matches('\n').count()).collect();
        at.dedup();
        let lines: Vec<Hit> = at.into_iter().map(|i| Hit { line: i + 1, text: all[i].trim().chars().take(160).collect() }).collect();
        let in_filename = !places(name, &r.wrong).is_empty();
        let is_right_page = r.right_page.as_deref() == Some(rel.as_str());
        if lines.is_empty() && !in_filename && !is_right_page {
            continue;
        }
        let layer = layer_of(&rel);
        let lower = name.to_lowercase();
        let conflict = crate::write::check_conflict_copies(&root.join(&rel)).is_err();
        let action = if conflict && (is_right_page || (!lines.is_empty() && !matches!(layer, "source" | "template"))) {
            "conflict"
        } else if is_right_page {
            "alias"
        } else if r.guards.iter().any(|g| !g.is_empty() && lower.contains(&g.to_lowercase())) {
            "guarded"
        } else if lines.is_empty() || matches!(layer, "source" | "template") {
            "leave"
        } else {
            "rewrite"
        };
        let count = lines.len();
        rows.push(Row {
            file: rel,
            layer: layer.into(),
            in_filename,
            count,
            lines: lines.into_iter().take(SAMPLE).collect(),
            action: action.into(),
            version: write::version(text.as_bytes()),
        });
    }
    rows
}

/// The page's text with `alias` in its `aliases:` property (added when there's none), or None
/// when it's there already. Only the property's lines change.
pub fn with_alias(text: &str, alias: &str) -> Option<String> {
    with_list_value(text, &["aliases", "alias"], alias)
}

/// The page's text with `value` added to the list property `keys[0]` (or the first of `keys` it
/// has), which is added when there's none; None when the value is there already, or the
/// properties couldn't take it (`list_value_added` tells the two apart). Only the property's
/// lines change: an inline list gets it at the end, a block list a new item with the same indent,
/// a single value becomes a list of both.
pub fn with_list_value(text: &str, keys: &[&str], value: &str) -> Option<String> {
    match list_value_added(text, keys, value) {
        ListValue::Changed(t) => Some(t),
        ListValue::Already | ListValue::CannotEdit => None,
    }
}

/// What adding a value to a list property came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListValue {
    /// The value is there already.
    Already,
    /// The page's new text.
    Changed(String),
    /// The properties wouldn't read as YAML with it added, so nothing is to be written.
    CannotEdit,
}

/// `with_list_value`, saying why when there's no new text.
pub fn list_value_added(text: &str, keys: &[&str], value: &str) -> ListValue {
    let fm = crate::frontmatter::split(text);
    let have = crate::frontmatter::list(&fm.data, keys, false);
    if have.iter().any(|a| a == value) {
        return ListValue::Already;
    }
    // After a byte-order mark, which would otherwise hide the frontmatter's opening `---`.
    let bom = if text.starts_with('\u{feff}') { "\u{feff}" } else { "" };
    let (mut lines, eol, trailing) = write::split_lines(&text[bom.len()..]);
    let quoted = write::yaml_scalar(value);
    let main = keys[0];
    let fm_end = if lines.first().is_some_and(|l| l.trim_end() == "---") {
        // Closed by `---` or `...`, as `frontmatter::split` reads it.
        lines.iter().skip(1).position(|l| matches!(l.trim_end(), "---" | "...")).map(|i| i + 1)
    } else {
        None
    };
    match fm_end {
        None => {
            lines.splice(0..0, ["---".to_string(), format!("{main}: [{quoted}]"), "---".to_string()]);
        }
        Some(end) => {
            let key = (1..end).find(|&i| keys.iter().any(|k| lines[i].starts_with(&format!("{k}:"))));
            match key {
                None => lines.insert(end, format!("{main}: [{quoted}]")),
                Some(k) => {
                    let key_name = lines[k].split(':').next().unwrap_or(main).to_string();
                    let rest = lines[k].split_once(':').map(|(_, v)| v.trim().to_string()).unwrap_or_default();
                    if let Some(inner) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                        let inner = inner.trim();
                        lines[k] =
                            if inner.is_empty() { format!("{key_name}: [{quoted}]") } else { format!("{key_name}: [{inner}, {quoted}]") };
                    } else if rest.is_empty() {
                        // A block list: after its last item, with the same indent.
                        let mut last = k;
                        while last + 1 < end && lines[last + 1].trim_start().starts_with("- ") {
                            last += 1;
                        }
                        let indent = if last > k { lines[last].len() - lines[last].trim_start().len() } else { 2 };
                        lines.insert(last + 1, format!("{}- {quoted}", " ".repeat(indent)));
                    } else {
                        lines[k] = format!("{key_name}: [{rest}, {quoted}]");
                    }
                }
            }
        }
    }
    let out = format!("{bom}{}", write::join_lines(&lines, &eol, trailing));
    if crate::frontmatter::split(&out).error.is_none() {
        ListValue::Changed(out)
    } else {
        ListValue::CannotEdit
    }
}

/// The substitutions file with the correction added (its other entries, and their order, as
/// they were), or None when one for `wrong` is there already.
pub fn with_substitution(json: Option<&str>, r: &Request) -> Result<Option<String>, String> {
    let mut doc: serde_json::Value = match json {
        Some(t) => serde_json::from_str(t).map_err(|e| format!("The corrections file ({SUBSTITUTIONS}) doesn't read as JSON: {e}"))?,
        None => serde_json::json!({ "substitutions": [] }),
    };
    let list = match &mut doc {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(o) => {
            o.entry("substitutions").or_insert_with(|| serde_json::json!([])).as_array_mut().ok_or("Its substitutions aren't a list.")?
        }
        _ => return Err(format!("The corrections file ({SUBSTITUTIONS}) isn't a list of substitutions.")),
    };
    if list.iter().any(|e| e["wrong"] == r.wrong) {
        return Ok(None);
    }
    let note =
        if r.note.trim().is_empty() { format!("{} - registered via Brainstead's Fix name", r.right) } else { r.note.trim().to_string() };
    let entry = Substitution {
        wrong: r.wrong.clone(),
        right: r.right.clone(),
        ambiguous: r.ambiguous,
        note,
        skip_if_filename_contains: r.guards.iter().filter(|g| !g.trim().is_empty()).cloned().collect(),
        ..Default::default()
    };
    let mut v = serde_json::to_value(&entry).map_err(|e| e.to_string())?;
    // As name_audit.py writes them: one guard as a string, "ambiguous" only when true.
    if let Some(o) = v.as_object_mut() {
        if let Some(g) = o.get("skip_if_filename_contains").and_then(|g| g.as_array()).filter(|g| g.len() == 1).cloned() {
            o.insert("skip_if_filename_contains".into(), g[0].clone());
        }
        if !r.ambiguous {
            o.remove("ambiguous");
        }
    }
    list.push(v);
    Ok(Some(serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n"))
}

/// The notes `apply` would rewrite, each with its text before and after, without writing any: an
/// assistant's fix is made through Changes (the app's `fixname_changes`).
pub fn planned(root: &Path, r: &Request, rows: &[Row]) -> Result<Vec<(String, String, String)>, WriteError> {
    if r.wrong.trim().is_empty() || r.right.trim().is_empty() || r.wrong == r.right {
        return Err(WriteError::Invalid("Give the wrong name and the right one.".into()));
    }
    let mut plans: Vec<(String, String, String)> = Vec::new();
    for row in rows {
        crate::trash::safe_rel(&row.file)?;
        let abs = root.join(&row.file);
        let before = std::fs::read_to_string(&abs).map_err(|_| WriteError::NotFound(format!("{} isn't there any more.", row.file)))?;
        if matches!(row.action.as_str(), "rewrite" | "alias") && write::version(before.as_bytes()) != row.version {
            return Err(WriteError::Stale(format!("{} changed since the names were looked for. Look again, then fix.", row.file)));
        }
        let after = match row.action.as_str() {
            "rewrite" => {
                let p = rewrite_places(&before, r);
                if p.is_empty() {
                    continue;
                }
                let mut out = String::with_capacity(before.len());
                let mut last = 0;
                for x in p {
                    out.push_str(&before[last..x.start]);
                    out.push_str(&r.right);
                    last = x.end;
                }
                out.push_str(&before[last..]);
                out
            }
            "alias" => match list_value_added(&before, &["aliases", "alias"], &r.wrong) {
                ListValue::Changed(t) => t,
                ListValue::Already => continue,
                // Not done, so not reported as done: nothing has been written yet.
                ListValue::CannotEdit => {
                    return Err(WriteError::Invalid(format!(
                        "{}'s properties don't read as YAML, so the alias couldn't be added. Nothing was changed: fix them, then try again.",
                        row.file
                    )));
                }
            },
            _ => continue,
        };
        plans.push((row.file.clone(), before, after));
    }
    Ok(plans)
}

/// Applies the plan: each `rewrite` file's occurrences become the right name (modification time
/// kept), the right page gets the alias, the substitution is registered. All or nothing: a file
/// that changed since the audit stops it before anything is written. Returns the changes, for
/// one undo.
/// `subs_path` is the corrections file (`names::path`), which a remembered correction is added to.
pub fn apply(root: &Path, subs_path: &Path, r: &Request, rows: &[Row]) -> Result<Vec<FileChange>, WriteError> {
    let mut plans: Vec<(std::path::PathBuf, String, String, bool)> =
        planned(root, r, rows)?.into_iter().map(|(file, before, after)| (root.join(file), before, after, true)).collect();
    let subs_before = std::fs::read_to_string(subs_path).ok();
    let subs_after = if r.skip_substitution { None } else { with_substitution(subs_before.as_deref(), r).map_err(WriteError::Invalid)? };
    if let Some(after) = subs_after {
        if let Some(d) = subs_path.parent() {
            std::fs::create_dir_all(d).map_err(|e| WriteError::Io(e.to_string()))?;
        }
        plans.push((subs_path.to_path_buf(), subs_before.unwrap_or_default(), after, false));
    }
    let mut done: Vec<FileChange> = Vec::new();
    for (abs, before, after, keep_mtime) in plans {
        let exists = abs.exists();
        let res = if exists {
            write::save_file_as(&abs, &after, &write::version(before.as_bytes()), keep_mtime)
        } else {
            write::create_file(&abs, &after)
        };
        match res {
            Ok(v) => done.push(FileChange { path: abs, before: exists.then_some(before), version: v }),
            Err(e) => {
                // Put back what was written so far.
                let _ = write::restore_files(&done);
                return Err(e);
            }
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (tempfile::TempDir, std::path::PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("v");
        for (p, s) in [
            ("1-1. Lena - 2026-09-01.md", "Lina said the launch slips. Ask Lina-Marie, and Linas.\n"),
            ("Meeting. Steerco - 2026-09-02.md", "Present: Lina, Theo.\n"),
            ("Idea. Carter.md", "Sam Carter and Sam met.\n"),
            ("1-1. Lina Park - 2026-09-03.md", "Lina Park is someone else.\n"),
            ("sources/Teams. Transcript.md", "Lina: hello\n"),
            ("wiki/entities/Lena.md", "---\nname: Lena\naliases:\n  - Lena F\n---\n\n## Current state\n"),
            ("../data/substitutions.json", "{\n  \"substitutions\": [\n    {\n      \"wrong\": \"Zarah\",\n      \"right\": \"Zara\",\n      \"note\": \"x\"\n    }\n  ]\n}\n"),
        ] {
            let a = root.join(p);
            std::fs::create_dir_all(a.parent().unwrap()).unwrap();
            std::fs::write(a, s).unwrap();
        }
        (t, root)
    }

    fn req() -> Request {
        Request {
            wrong: "Lina".into(),
            right: "Lena".into(),
            right_page: Some("wiki/entities/Lena.md".into()),
            guards: vec!["Lina Park".into()],
            ambiguous: false,
            note: "confirmed 2026-10-02 via a 1-1".into(),
            skip_substitution: false,
        }
    }

    #[test]
    fn plan_by_layer() {
        let (_t, root) = vault();
        let rows = audit(&root, &req());
        let got: Vec<(&str, &str, usize)> = rows.iter().map(|r| (r.file.as_str(), r.action.as_str(), r.count)).collect();
        assert_eq!(
            got,
            [
                ("1-1. Lena - 2026-09-01.md", "rewrite", 1),
                ("1-1. Lina Park - 2026-09-03.md", "guarded", 1),
                ("Meeting. Steerco - 2026-09-02.md", "rewrite", 1),
                ("sources/Teams. Transcript.md", "leave", 1),
                ("wiki/entities/Lena.md", "alias", 0),
            ]
        );
        assert!(rows[1].in_filename);
    }

    #[test]
    fn apply_keeps_times_and_undoes() {
        let (_t, root) = vault();
        let note = root.join("1-1. Lena - 2026-09-01.md");
        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::File::options().write(true).open(&note).unwrap().set_modified(old).unwrap();
        let rows = audit(&root, &req());
        let subs_file = root.join("../data/substitutions.json");
        let changes = apply(&root, &subs_file, &req(), &rows).unwrap();
        assert_eq!(changes.len(), 4);
        assert_eq!(std::fs::read_to_string(&note).unwrap(), "Lena said the launch slips. Ask Lina-Marie, and Linas.\n");
        assert_eq!(std::fs::metadata(&note).unwrap().modified().unwrap(), old);
        assert_eq!(std::fs::read_to_string(root.join("1-1. Lina Park - 2026-09-03.md")).unwrap(), "Lina Park is someone else.\n");
        assert_eq!(std::fs::read_to_string(root.join("sources/Teams. Transcript.md")).unwrap(), "Lina: hello\n");
        assert!(std::fs::read_to_string(root.join("wiki/entities/Lena.md")).unwrap().contains("aliases:\n  - Lena F\n  - Lina\n"));
        let subs = crate::names::load(&subs_file);
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[1].skip_if_filename_contains, ["Lina Park"]);
        assert_eq!(subs[1].note, "confirmed 2026-10-02 via a 1-1");
        write::restore_files(&changes).unwrap();
        assert!(std::fs::read_to_string(&note).unwrap().starts_with("Lina said"));
        assert_eq!(crate::names::load(&subs_file).len(), 1);
    }

    #[test]
    fn conflict_copies_are_skipped() {
        let (_t, root) = vault();
        std::fs::write(root.join("Meeting. Steerco - 2026-09-02 2.md"), "Present: Lina.\n").unwrap();
        let rows = audit(&root, &req());
        assert_eq!(rows.iter().find(|r| r.file == "Meeting. Steerco - 2026-09-02.md").unwrap().action, "conflict");
        apply(&root, &root.join("../data/substitutions.json"), &req(), &rows).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("Meeting. Steerco - 2026-09-02.md")).unwrap(), "Present: Lina, Theo.\n");
        assert!(std::fs::read_to_string(root.join("1-1. Lena - 2026-09-01.md")).unwrap().starts_with("Lena said"));
    }

    #[test]
    fn a_surname_is_added_once() {
        let (_t, root) = vault();
        let r = Request { wrong: "Sam".into(), right: "Sam Carter".into(), right_page: None, guards: vec![], ..req() };
        let rows = audit(&root, &r);
        apply(&root, &root.join("../data/substitutions.json"), &r, &rows).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("Idea. Carter.md")).unwrap(), "Sam Carter and Sam Carter met.\n");
    }

    #[test]
    fn aliases_in_every_shape() {
        assert_eq!(with_alias("---\naliases: [OA]\n---\nx\n", "Orbit").unwrap(), "---\naliases: [OA, Orbit]\n---\nx\n");
        assert_eq!(with_alias("---\nname: A\n---\nx\n", "B").unwrap(), "---\nname: A\naliases: [B]\n---\nx\n");
        assert_eq!(with_alias("x\n", "B").unwrap(), "---\naliases: [B]\n---\nx\n");
        assert_eq!(with_alias("---\nalias: OA\n---\n", "Orbit").unwrap(), "---\nalias: [OA, Orbit]\n---\n");
        assert_eq!(with_alias("---\naliases: [OA]\n---\n", "OA"), None);
        // Properties closed with `...` are still the properties.
        assert_eq!(with_alias("---\nname: A\n...\nx\n", "B").unwrap(), "---\nname: A\naliases: [B]\n...\nx\n");
    }

    #[test]
    fn an_alias_that_cannot_be_added_stops_the_fix() {
        assert_eq!(list_value_added("---\naliases: [OA]\n---\n", &["aliases"], "OA"), ListValue::Already);
        assert_eq!(list_value_added("---\naliases: [OA\n---\n", &["aliases"], "Orbit"), ListValue::CannotEdit);
        let (_t, root) = vault();
        std::fs::write(root.join("wiki/entities/Lena.md"), "---\nname: Lena\naliases: [Lena F\n---\n").unwrap();
        let rows = audit(&root, &req());
        let subs = root.join("../data/substitutions.json");
        assert!(matches!(apply(&root, &subs, &req(), &rows), Err(WriteError::Invalid(_))));
        // Nothing written: not the notes, not the correction.
        assert!(std::fs::read_to_string(root.join("1-1. Lena - 2026-09-01.md")).unwrap().starts_with("Lina said"));
        assert_eq!(crate::names::load(&subs).len(), 1);
    }

    #[test]
    fn addresses_and_tags_are_left_alone() {
        let r = Request { wrong: "Sam".into(), right: "Samir".into(), right_page: None, guards: vec![], ..req() };
        let src = "Sam's [profile](https://example.com/users/Sam) and [Sam](people/Sam.md), https://example.com/Sam, <https://example.com/Sam>, <Sam@example.com> #people/Sam #Sam (#Sam) then Sam.\n";
        let p = rewrite_places(src, &r);
        let mut out = src.to_string();
        for x in p.iter().rev() {
            out.replace_range(x.clone(), &r.right);
        }
        assert_eq!(
            out,
            "Samir's [profile](https://example.com/users/Sam) and [Samir](people/Sam.md), https://example.com/Sam, <https://example.com/Sam>, <Sam@example.com> #people/Sam #Sam (#Sam) then Samir.\n"
        );
    }

    #[test]
    fn a_bom_keeps_one_frontmatter() {
        assert_eq!(with_alias("\u{feff}---\nname: A\n---\nx\n", "B").unwrap(), "\u{feff}---\nname: A\naliases: [B]\n---\nx\n");
        assert_eq!(with_alias("\u{feff}x\n", "B").unwrap(), "\u{feff}---\naliases: [B]\n---\nx\n");
    }

    #[test]
    fn links_properties_and_code_are_left_alone() {
        let (_t, root) = vault();
        let src = "---\nattendee: Lina\n---\nLina joined. [[Lina]] and [[Lina|Lina]], ![[Lina.png]], [[Notes#Lina]].\n`Lina` code\n```\nLina\n```\nLina again.\n";
        let p = root.join("Meeting. Links - 2026-09-04.md");
        std::fs::write(&p, src).unwrap();
        let rows = audit(&root, &req());
        let row = rows.iter().find(|r| r.file == "Meeting. Links - 2026-09-04.md").unwrap();
        assert_eq!((row.count, row.lines[0].line, row.lines[1].line), (2, 4, 9));
        apply(&root, &root.join("../data/substitutions.json"), &req(), &rows).unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "---\nattendee: Lina\n---\nLena joined. [[Lina]] and [[Lina|Lena]], ![[Lina.png]], [[Notes#Lina]].\n`Lina` code\n```\nLina\n```\nLena again.\n"
        );
    }

    #[test]
    fn apply_refuses_a_file_changed_since_the_audit() {
        let (_t, root) = vault();
        let rows = audit(&root, &req());
        let note = root.join("Meeting. Steerco - 2026-09-02.md");
        std::fs::write(&note, "Present: Lina, Theo, and Lina's notes.\n").unwrap();
        assert!(matches!(apply(&root, &root.join("../data/substitutions.json"), &req(), &rows), Err(WriteError::Stale(_))));
        // Nothing written, not even the files that hadn't changed.
        assert!(std::fs::read_to_string(root.join("1-1. Lena - 2026-09-01.md")).unwrap().starts_with("Lina said"));
        assert_eq!(std::fs::read_to_string(&note).unwrap(), "Present: Lina, Theo, and Lina's notes.\n");
    }

    #[test]
    fn substitution_entries() {
        let r = Request { guards: vec![], ambiguous: true, ..req() };
        let out = with_substitution(None, &r).unwrap().unwrap();
        assert!(out.contains("\"ambiguous\": true") && !out.contains("skip_if"));
        assert_eq!(with_substitution(Some(&out), &r).unwrap(), None);
        assert!(with_substitution(Some("not json"), &r).is_err());
    }
}
