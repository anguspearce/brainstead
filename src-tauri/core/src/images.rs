// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Images pasted or dropped into the editor, saved to the vault's `images/` folder with names as
//! the previous app gives them (bff/src/routes/vault-asset.ts): a slug of the original name, or
//! `Pasted-image-YYYYMMDDHHMMSS` for a clipboard's generic "image.png", then `-2`, `-3`… when taken.

use std::fs;
use std::path::Path;

use unicode_normalization::UnicodeNormalization;

use crate::write::{create_file_bytes, Result, WriteError};

pub const IMAGES_DIR: &str = "images";
/// The largest image an assistant can add (add_image).
pub const MAX_BYTES: usize = 20 << 20;
const EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif"];

/// The extension from the bytes: for a paste with no file name to go by, and for an image an
/// assistant adds, whose name isn't trusted.
pub fn sniff(b: &[u8]) -> Option<&'static str> {
    let text = b.trim_ascii_start();
    match b {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [0xff, 0xd8, 0xff, ..] => Some("jpg"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("webp"),
        [_, _, _, _, b'f', b't', b'y', b'p', b'a', b'v', b'i', b'f', ..] => Some("avif"),
        [b'B', b'M', ..] => Some("bmp"),
        _ if text.starts_with(b"<svg") || text.starts_with(b"<?xml") => Some("svg"),
        _ => None,
    }
}

fn slugify(base: &str) -> String {
    let plain: String = base.nfkd().filter(|c| !('\u{300}'..='\u{36f}').contains(c)).collect();
    let mut out = String::new();
    let mut dash = false;
    for c in plain.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    out.trim_matches(['-', '.']).chars().take(80).collect()
}

/// The file name to save as: `stamp` is local "YYYYMMDDHHMMSS", for generic names.
pub fn file_name(original: &str, bytes: &[u8], stamp: &str) -> Result<String> {
    let (raw, ext) = match original.rsplit_once('.') {
        Some((b, x)) if EXTS.contains(&x.to_ascii_lowercase().as_str()) => (b, x.to_ascii_lowercase()),
        _ => (original, sniff(bytes).ok_or_else(|| WriteError::Invalid("That isn't an image Brainstead can save.".into()))?.to_string()),
    };
    let generic = matches!(raw.trim().to_lowercase().as_str(), "" | "image" | "blob" | "untitled");
    let slug = if generic { String::new() } else { slugify(raw) };
    let base = if slug.is_empty() { format!("Pasted-image-{stamp}") } else { slug };
    Ok(format!("{base}.{ext}"))
}

/// Saves the image under `root/images/`, taking the next free name. Returns the vault-relative path.
pub fn save(root: &Path, original: &str, bytes: &[u8], stamp: &str) -> Result<String> {
    let name = file_name(original, bytes, stamp)?;
    let dir = root.join(IMAGES_DIR);
    fs::create_dir_all(&dir).map_err(crate::write::io)?;
    let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, ""));
    for n in 1.. {
        let candidate = if n == 1 { name.clone() } else { format!("{stem}-{n}.{ext}") };
        match create_file_bytes(&dir.join(&candidate), bytes) {
            Ok(()) => return Ok(format!("{IMAGES_DIR}/{candidate}")),
            Err(WriteError::Exists(_)) => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

/// Local time as `YYYYMMDDHHMMSS`.
pub fn stamp_now() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0, 1];

    #[test]
    fn sniffs_the_type_from_the_bytes() {
        assert_eq!(sniff(PNG), Some("png"));
        assert_eq!(sniff(b"\0\0\0\x1cftypavif\0"), Some("avif"));
        assert_eq!(sniff(b"\n  <svg xmlns='http://www.w3.org/2000/svg'/>"), Some("svg"));
        assert_eq!(sniff(b"%PDF-1.7"), None);
    }

    #[test]
    fn names() {
        assert_eq!(file_name("image.png", PNG, "20261002101500").unwrap(), "Pasted-image-20261002101500.png");
        assert_eq!(file_name("", PNG, "20261002101500").unwrap(), "Pasted-image-20261002101500.png");
        assert_eq!(file_name("Café plan (v2).JPG", PNG, "x").unwrap(), "Cafe-plan-v2.jpg");
        assert!(file_name("notes.txt", b"hello", "x").is_err());
    }

    #[test]
    fn takes_the_next_free_name() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(save(d.path(), "chart.png", PNG, "x").unwrap(), "images/chart.png");
        assert_eq!(save(d.path(), "chart.png", PNG, "x").unwrap(), "images/chart-2.png");
        assert_eq!(save(d.path(), "chart.png", PNG, "x").unwrap(), "images/chart-3.png");
        assert_eq!(fs::read(d.path().join("images/chart-2.png")).unwrap(), PNG);
    }
}
