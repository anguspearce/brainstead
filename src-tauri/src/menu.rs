// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The menu bar: the platform's default menus plus a Note menu with every action on the open
//! note. A choice is sent to the window as a `menu` event carrying the item's id (src/noteMenu.ts
//! acts on it); the note's items are enabled only while a note is open. Help gets Brainstead Help
//! and Keyboard Shortcuts (src/help/HelpDrawer.tsx acts on those). The app menu gets Brainstead's
//! own About and Check for Updates… (src/updates.rs).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, MenuItemBuilder, MenuItemKind, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, State, Wry};

/// The Note menu's items that need an open note.
#[derive(Default)]
pub struct NoteMenu(Mutex<Vec<MenuItem<Wry>>>);

/// (id, label, accelerator); "-" is a separator. Grouped as the note's right-click menu
/// (src/FileMenu.tsx): copying, then out to a file or Finder, then changing it, Move to the Trash alone. Ids are `note:` + the action in src/noteMenu.ts.
const ITEMS: &[(&str, &str, Option<&str>)] = &[
    ("note:save", "Save", Some("CmdOrCtrl+S")),
    ("-", "", None),
    ("note:view", "View", Some("CmdOrCtrl+1")),
    ("note:edit", "Edit", Some("CmdOrCtrl+2")),
    ("note:source", "Source", Some("CmdOrCtrl+3")),
    ("note:focus", "Focus Mode", Some("CmdOrCtrl+.")),
    ("-", "", None),
    ("note:copy-markdown", "Copy Markdown", None),
    ("note:copy-rich", "Copy Rich Text", None),
    ("note:copy-path", "Copy Path", Some("Alt+CmdOrCtrl+C")),
    ("note:copy-title", "Copy Title", None),
    ("-", "", None),
    ("note:read-aloud", "Read Aloud", Some("Shift+CmdOrCtrl+P")),
    ("note:export-pdf", "Export PDF…", Some("CmdOrCtrl+P")),
    ("note:reveal", "Reveal in Finder", Some("Alt+CmdOrCtrl+R")),
    ("-", "", None),
    ("note:rename", "Rename…", Some("Shift+CmdOrCtrl+R")),
    ("note:discard-draft", "Discard Draft", None),
    ("-", "", None),
    ("note:trash", "Move to the Trash", None),
];

/// Puts the menu bar up and routes its Note items to the window.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::default(app)?;
    let new = MenuItemBuilder::with_id("note:new", "New Note…").accelerator("CmdOrCtrl+N").build(app)?;
    let mut sub = SubmenuBuilder::new(app, "Note").item(&new).separator();
    let mut doc_items = Vec::new();
    for (id, label, acc) in ITEMS {
        if *id == "-" {
            sub = sub.item(&PredefinedMenuItem::separator(app)?);
            continue;
        }
        let mut b = MenuItemBuilder::with_id(*id, *label).enabled(false);
        if let Some(a) = acc {
            b = b.accelerator(*a);
        }
        let item = b.build(app)?;
        sub = sub.item(&item);
        doc_items.push(item);
    }
    // After the app menu and File on macOS; first elsewhere.
    let at = if cfg!(target_os = "macos") { 2 } else { 0 };
    menu.insert(&sub.build()?, at.min(menu.items()?.len()))?;
    // Help: the drawer (src/help/HelpDrawer.tsx) and the shortcut sheet, added to the platform's
    // Help menu (macOS keeps its search field at the top), or a Help menu of its own.
    let help_open = MenuItemBuilder::with_id("help:open", "Brainstead Help").accelerator("Shift+CmdOrCtrl+Slash").build(app)?;
    let help_keys = MenuItemBuilder::with_id("help:shortcuts", "Keyboard Shortcuts").build(app)?;
    let help_site = MenuItemBuilder::with_id("help:website", "Brainstead Website").build(app)?;
    let existing = menu.items()?.into_iter().find_map(|i| match i {
        MenuItemKind::Submenu(s) if s.text().is_ok_and(|t| t == "Help") => Some(s),
        _ => None,
    });
    match existing {
        Some(h) => {
            h.append(&help_open)?;
            h.append(&help_keys)?;
            h.append(&help_site)?;
        }
        None => menu.append(&SubmenuBuilder::new(app, "Help").item(&help_open).item(&help_keys).item(&help_site).build()?)?,
    }
    app.set_menu(menu)?;
    *app.state::<NoteMenu>().0.lock().unwrap() = doc_items;
    app.on_menu_event(|app, ev| {
        let id = ev.id().as_ref();
        if id == "help:website" {
            use tauri_plugin_opener::OpenerExt;
            if let Err(e) = app.opener().open_url(WEBSITE, None::<&str>) {
                crate::applog!("website: {e}");
            }
            return;
        }
        if id == "app:about" {
            about(app);
            return;
        }
        if id == "app:updates" {
            crate::updates::check_now(app);
            return;
        }
        if id.starts_with("note:") || id.starts_with("help:") {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            let _ = app.emit_to("main", "menu", id);
        }
    });
    Ok(())
}

/// Brainstead's website, under the copyright in the About dialog. The same as `WEBSITE` in
/// src/Settings.tsx, which About's Website button opens (a test below keeps them the same).
const WEBSITE: &str = "https://brainstead.davies.co.za/";

/// Whether the window is in dark mode, for the About panel's icon.
static DARK: AtomicBool = AtomicBool::new(false);

/// The app menu's About item: macOS's standard panel, opened by Brainstead (`platform::about_panel`)
/// so the website under the copyright can be clicked, with the app's icon in the theme's version:
/// the charcoal tile when dark, the white one when light. Called again when the theme changes, to
/// keep the icon in step. macOS only; elsewhere the platform's own About item stays.
pub fn set_about(app: &AppHandle, dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
    if cfg!(target_os = "macos") {
        if let Err(e) = about_item(app) {
            crate::applog!("about: {e}");
        }
    }
}

/// Puts Brainstead's own About item first in the app menu, in place of the platform's, with
/// Check for Updates… under it.
fn about_item(app: &AppHandle) -> tauri::Result<()> {
    let Some(menu) = app.menu() else { return Ok(()) };
    let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() else { return Ok(()) };
    if app_menu.get("app:about").is_some() {
        return Ok(());
    }
    app_menu.remove_at(0)?;
    app_menu.insert(&MenuItemBuilder::with_id("app:about", "About Brainstead").build(app)?, 0)?;
    app_menu.insert(&MenuItemBuilder::with_id("app:updates", "Check for Updates…").build(app)?, 1)?;
    Ok(())
}

/// Opens the About panel, with the website under the copyright.
fn about(app: &AppHandle) {
    let png: &[u8] =
        if DARK.load(Ordering::Relaxed) { include_bytes!("../icons/128x128@2x.png") } else { include_bytes!("../icons/about-light.png") };
    let version = app.package_info().version.to_string();
    let link = WEBSITE.trim_start_matches("https://").trim_end_matches('/');
    let _ = app.run_on_main_thread(move || {
        crate::platform::about_panel("Brainstead", &version, "© 2026 Wayne Davies · GPL-3.0-or-later", png, link, WEBSITE);
    });
}

/// The window says whether a note is open, and so whether the Note items apply.
#[tauri::command]
pub fn menu_note_open(menu: State<NoteMenu>, open: bool) {
    for i in menu.0.lock().unwrap().iter() {
        let _ = i.set_enabled(open);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn website_matches_the_settings_screen() {
        let settings = include_str!("../../src/Settings.tsx");
        assert!(settings.contains(&format!("export const WEBSITE = \"{}\";", super::WEBSITE)));
    }
}
