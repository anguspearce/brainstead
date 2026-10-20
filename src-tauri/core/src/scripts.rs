// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Templater's user scripts: the `.js` files in one vault folder (Settings › Notes, by default
//! `Templates/scripts`), which templates call as `tp.user.<name>()`. Only files inside that
//! folder are read, however the folder or a link inside it is spelled.

use std::fs;
use std::path::{Component, Path};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserScript {
    /// The file name without `.js`: `tp.user.<name>`.
    pub name: String,
    pub source: String,
}

/// The scripts in `folder` (vault-relative), and its folders below it, sorted by name. None
/// when the folder isn't there. Refuses a folder outside the vault.
pub fn user_scripts(root: &Path, folder: &str) -> Result<Vec<UserScript>, String> {
    let rel = Path::new(folder.trim_end_matches('/'));
    if folder.trim().is_empty() || rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("Not a folder in the vault: {folder}"));
    }
    let root = crate::paths::canonical(root).map_err(|e| e.to_string())?;
    let dir = match crate::paths::canonical(&root.join(rel)) {
        Ok(d) => d,
        Err(_) => return Ok(Vec::new()),
    };
    if !dir.starts_with(&root) {
        return Err(format!("Not a folder in the vault: {folder}"));
    }
    let mut out = Vec::new();
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(p) = crate::paths::canonical(&e.path()) else { continue };
            // A link that leads out of the folder is skipped.
            if !p.starts_with(&dir) {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("js")) {
                let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                if let Ok(source) = fs::read_to_string(&p) {
                    out.push(UserScript { name, source });
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_js_inside_the_folder() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir_all(root.join("Templates/scripts/sub")).unwrap();
        fs::write(root.join("Templates/scripts/greet.js"), "module.exports = () => 'hi';").unwrap();
        fs::write(root.join("Templates/scripts/sub/more.js"), "module.exports = {};").unwrap();
        fs::write(root.join("Templates/scripts/notes.md"), "not a script").unwrap();
        fs::write(root.join("secret.js"), "nope").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("secret.js"), root.join("Templates/scripts/leak.js")).unwrap();

        let s = user_scripts(root, "Templates/scripts").unwrap();
        assert_eq!(s.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["greet", "more"]);
        assert_eq!(user_scripts(root, "Missing").unwrap(), vec![]);
        for bad in ["../x", "/etc", "", "Templates/../.."] {
            assert!(user_scripts(root, bad).is_err(), "{bad}");
        }
    }
}
