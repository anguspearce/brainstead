// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! How much space the vault folder takes: every file in it, not only those the index keeps
//! (images, attachments, excluded folders). Walked once when the vault opens and kept current from
//! the watcher's batches, so asking never walks the vault. Dot folders and files (`.git`,
//! `.obsidian`, `.trash`) aren't counted: they're another app's, not the user's notes.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::vault::{rel_of, Vault};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// In a folder the user excluded (Settings › Vault).
    Excluded,
    /// A picture outside sources/ (sources' own are sources).
    Image,
    /// Everything else, the indexed files among them.
    Other,
}

/// The vault's files by vault-relative path, with their sizes.
#[derive(Debug, Default)]
pub struct Usage {
    files: BTreeMap<String, (Kind, u64)>,
}

/// What the vault takes, in bytes and files. `files`/`bytes` include the excluded folders'.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub files: u64,
    pub bytes: u64,
    pub images: u64,
    pub image_bytes: u64,
    pub excluded: u64,
    pub excluded_bytes: u64,
}

const IMAGE_EXTS: [&str; 9] = ["png", "jpg", "jpeg", "heic", "webp", "gif", "bmp", "avif", "svg"];

fn hidden(rel: &str) -> bool {
    rel.split('/').any(|s| s.starts_with('.'))
}

fn kind(vault: &Vault, rel: &str) -> Kind {
    let segs: Vec<&str> = rel.split('/').collect();
    if segs[..segs.len() - 1].iter().any(|s| vault.excluded.iter().any(|x| x == s)) {
        return Kind::Excluded;
    }
    let ext = rel.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if IMAGE_EXTS.contains(&ext.as_str()) && !(segs.len() > 1 && segs[0] == "sources") {
        Kind::Image
    } else {
        Kind::Other
    }
}

impl Usage {
    /// Every file in the vault, but those in dot folders.
    pub fn scan(vault: &Vault) -> Usage {
        let mut u = Usage::default();
        u.add_under(vault, &vault.root.clone());
        u
    }

    fn add_under(&mut self, vault: &Vault, dir: &Path) {
        let it = walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'));
        for e in it.flatten().filter(|e| e.file_type().is_file()) {
            let (Some(rel), Ok(md)) = (rel_of(&vault.root, e.path()), e.metadata()) else { continue };
            if !hidden(&rel) {
                self.files.insert(rel.clone(), (kind(vault, &rel), md.len()));
            }
        }
    }

    fn remove_under(&mut self, rel: &str) {
        self.files.remove(rel);
        let prefix = format!("{rel}/");
        let gone: Vec<String> =
            self.files.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix)).map(|(k, _)| k.clone()).collect();
        for k in gone {
            self.files.remove(&k);
        }
    }

    /// Brings the paths the watcher reported up to date: a file's size, a folder moved in or out,
    /// anything deleted.
    pub fn update(&mut self, vault: &Vault, paths: &[std::path::PathBuf]) {
        for p in paths {
            let Some(rel) = rel_of(&vault.root, p) else { continue };
            if hidden(&rel) {
                continue;
            }
            match std::fs::symlink_metadata(p) {
                Ok(md) if md.is_file() => {
                    self.files.insert(rel.clone(), (kind(vault, &rel), md.len()));
                }
                Ok(md) if md.is_dir() => {
                    self.remove_under(&rel);
                    self.add_under(vault, p);
                }
                Ok(_) => {}
                Err(_) => self.remove_under(&rel),
            }
        }
    }

    pub fn totals(&self) -> UsageTotals {
        let mut t = UsageTotals::default();
        for (k, n) in self.files.values() {
            t.files += 1;
            t.bytes += n;
            match k {
                Kind::Excluded => {
                    t.excluded += 1;
                    t.excluded_bytes += n;
                }
                Kind::Image => {
                    t.images += 1;
                    t.image_bytes += n;
                }
                Kind::Other => {}
            }
        }
        t
    }
}

/// The size of everything under `dir` (Brainstead's own data folder, or a part of it).
pub fn dir_bytes(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, n: usize) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![b'x'; n]).unwrap();
    }

    #[test]
    fn counts_and_keeps_current() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        write(root, "Note.md", 10);
        write(root, "images/diagram.png", 100);
        write(root, "sources/Scan.png", 50);
        write(root, "Archive/Old.md", 7);
        write(root, ".obsidian/workspace.json", 1000);
        write(root, "attachments/deck.pptx", 30);
        let v = Vault::new(root, vec!["Archive".into()]);
        let mut u = Usage::scan(&v);
        assert_eq!(u.totals(), UsageTotals { files: 5, bytes: 197, images: 1, image_bytes: 100, excluded: 1, excluded_bytes: 7 });

        // A file grows, a folder moves in, another goes.
        write(root, "Note.md", 20);
        write(root, "Moved/a.md", 5);
        write(root, "Moved/b.jpg", 5);
        std::fs::remove_dir_all(root.join("images")).unwrap();
        u.update(&v, &[root.join("Note.md"), root.join("Moved"), root.join("images"), root.join(".obsidian/workspace.json")]);
        assert_eq!(u.totals(), UsageTotals { files: 6, bytes: 117, images: 1, image_bytes: 5, excluded: 1, excluded_bytes: 7 });
    }
}
