// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Prevents an additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = brainstead_lib::mcp_main() {
        std::process::exit(code);
    }
    brainstead_lib::run()
}

// Here rather than in menu.rs: tauri-build links its Windows manifest (Common Controls 6) into
// the app's program alone, this test included, and the menu code (muda) imports functions that
// Windows has only with it, so a test program without it doesn't start.
#[cfg(all(test, windows))]
mod tests {
    use tauri::menu::{Menu, MenuItemKind};
    use tauri::Runtime;

    /// Where File is, and its items: the predefined ones by label (mnemonics left out), the
    /// app's by id.
    fn file_menu<R: Runtime>(menu: &Menu<R>) -> (usize, Vec<String>) {
        let items = menu.items().unwrap();
        let at = items.iter().position(|i| matches!(i, MenuItemKind::Submenu(s) if s.text().unwrap() == "File")).unwrap();
        let MenuItemKind::Submenu(file) = &items[at] else { unreachable!() };
        let names = file
            .items()
            .unwrap()
            .iter()
            .map(|i| match i {
                MenuItemKind::Predefined(p) => format!("predefined {}", p.text().unwrap().replace('&', "")),
                MenuItemKind::MenuItem(m) => m.id().as_ref().to_string(),
                _ => "other".into(),
            })
            .collect();
        (at, names)
    }

    #[test]
    fn the_windows_file_menu_has_the_apps_exit_not_the_predefined_quit() {
        let app = tauri::test::mock_app();
        let menu = Menu::default(app.handle()).unwrap();
        // tauri's: Close Window, then the predefined Quit (PostQuitMessage), labelled Exit.
        let (at, before) = file_menu(&menu);
        assert_eq!(before, ["predefined Close", "predefined Exit"]);
        brainstead_lib::windows_file(app.handle(), &menu).unwrap();
        let (now_at, after) = file_menu(&menu);
        assert_eq!(now_at, at);
        assert_eq!(after, ["predefined Close", "predefined ", brainstead_lib::EXIT]);
    }
}
