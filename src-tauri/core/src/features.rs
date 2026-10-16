// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which of the features that aren't on every system this one has: one table, read by the app's
//! `platform_features` command for the window, which hides what is missing (src/features.ts), and
//! by the MCP server's `app_status`, so an assistant doesn't offer what the window hasn't got.

/// What this system has of the features that aren't everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformFeatures {
    /// "macos", "windows" or "linux".
    pub platform: &'static str,
    pub quick_look: bool,
    pub spelling: bool,
    pub grammar: bool,
    pub read_aloud: bool,
    pub export_pdf: bool,
    pub rich_copy: bool,
    pub full_disk_access: bool,
    pub open_at_login: bool,
    /// The text in images (OCR), for search and the index.
    pub image_text: bool,
}

/// This system's features: all of them on macOS, rich copy and Open at login on Windows, none of
/// these on Linux yet.
pub fn platform_features() -> PlatformFeatures {
    let none = PlatformFeatures {
        platform: "linux",
        quick_look: false,
        spelling: false,
        grammar: false,
        read_aloud: false,
        export_pdf: false,
        rich_copy: false,
        full_disk_access: false,
        open_at_login: false,
        image_text: false,
    };
    if cfg!(target_os = "macos") {
        PlatformFeatures {
            platform: "macos",
            quick_look: true,
            spelling: true,
            grammar: true,
            read_aloud: true,
            export_pdf: true,
            rich_copy: true,
            full_disk_access: true,
            open_at_login: true,
            image_text: true,
        }
    } else if cfg!(windows) {
        PlatformFeatures { platform: "windows", rich_copy: true, open_at_login: true, ..none }
    } else {
        none
    }
}

impl PlatformFeatures {
    /// The system, and what it doesn't have, as `app_status` says it.
    pub fn describe(&self) -> String {
        let system = match self.platform {
            "macos" => "macOS",
            "windows" => "Windows",
            _ => "Linux",
        };
        let missing: Vec<&str> = [
            (self.quick_look, "Quick Look"),
            (self.spelling, "spelling"),
            (self.grammar, "grammar"),
            (self.read_aloud, "Read aloud"),
            (self.export_pdf, "Export PDF"),
            (self.rich_copy, "rich copy"),
            (self.full_disk_access, "Full Disk Access"),
            (self.open_at_login, "Open at login"),
            (self.image_text, "the text in images"),
        ]
        .into_iter()
        .filter(|(has, _)| !has)
        .map(|(_, name)| name)
        .collect();
        if missing.is_empty() {
            format!("System: {system}, with every feature.")
        } else {
            format!("System: {system}. Not on this system, so not in the window: {}.", missing.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_this_systems_and_says_what_is_missing() {
        let f = platform_features();
        let mac = cfg!(target_os = "macos");
        assert_eq!(
            f.platform,
            if mac {
                "macos"
            } else if cfg!(windows) {
                "windows"
            } else {
                "linux"
            }
        );
        assert_eq!(f.quick_look, mac);
        assert_eq!(f.rich_copy, mac || cfg!(windows));
        let text = f.describe();
        if mac {
            assert_eq!(text, "System: macOS, with every feature.");
        } else if cfg!(windows) {
            assert!(text.starts_with("System: Windows. Not on this system, so not in the window: Quick Look, spelling"), "{text}");
            assert!(!text.contains("rich copy") && !text.contains("Open at login"), "{text}");
        }
        // The window and the MCP server's structured output read it in camelCase.
        let v = serde_json::to_value(f).unwrap();
        assert_eq!(v["quickLook"], f.quick_look);
        assert_eq!(v["openAtLogin"], f.open_at_login);
    }
}
