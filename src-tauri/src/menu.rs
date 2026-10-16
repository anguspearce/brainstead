// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The menu bar: the platform's default menus plus a Note menu with every action on the open
//! note. A choice is sent to the window as a `menu` event carrying the item's id (src/noteMenu.ts
//! acts on it); the note's items are enabled only while a note is open. Help gets Brainstead Help
//! and Keyboard Shortcuts (src/help/HelpDrawer.tsx acts on those). The app menu gets Brainstead's
//! own About and Check for Updates… (src/updates.rs). On Windows File's Exit is the app's own item,
//! which quits as the tray's Quit does.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, MenuItemBuilder, MenuItemKind, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime, State, Wry};

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

/// The label an item has on this system: Windows names its file manager in its own words.
fn label(id: &str, label: &'static str) -> &'static str {
    if cfg!(windows) && id == "note:reveal" {
        return "Show in Explorer";
    }
    label
}

/// Whether this system has what the item needs (`PlatformFeatures`, which also decides what the
/// window shows).
fn available(id: &str, f: &crate::platform::PlatformFeatures) -> bool {
    match id {
        "note:read-aloud" => f.read_aloud,
        "note:export-pdf" => f.export_pdf,
        "note:copy-rich" => f.rich_copy,
        _ => true,
    }
}

/// `ITEMS` without what this system lacks, and without the separator a missing group would leave
/// doubled or at either end.
fn items(f: &crate::platform::PlatformFeatures) -> Vec<(&'static str, &'static str, Option<&'static str>)> {
    let mut out: Vec<(&'static str, &'static str, Option<&'static str>)> = Vec::new();
    for &(id, l, acc) in ITEMS {
        if id == "-" {
            if out.last().is_some_and(|(i, _, _)| *i != "-") {
                out.push((id, l, acc));
            }
        } else if available(id, f) {
            out.push((id, label(id, l), acc));
        }
    }
    if out.last().is_some_and(|(i, _, _)| *i == "-") {
        out.pop();
    }
    out
}

/// Windows' File › Exit, in place of the predefined Quit (`windows_file`).
pub const EXIT: &str = "app:exit";

/// Windows' File menu: Close Window, then Exit. tauri's default File menu ends with the
/// predefined Quit, which muda carries out as `PostQuitMessage(0)`: tao's event loop ends at the
/// `WM_QUIT` (`GetMessageW` returns 0) and tauri sends `RunEvent::Exit` with no `ExitRequested`
/// before it, so the window never asks about unsaved edits (Save, Discard, Stay), its size and
/// place aren't saved and assistant runs aren't stopped. Exit is an item of the app's own, which
/// goes the way the tray's Quit does (`tray::tray_quit`).
pub fn windows_file<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>) -> tauri::Result<()> {
    let file = SubmenuBuilder::new(app, "File")
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .separator()
        .item(&MenuItemBuilder::with_id(EXIT, "E&xit").build(app)?)
        .build()?;
    match menu.items()?.iter().position(|i| matches!(i, MenuItemKind::Submenu(s) if s.text().is_ok_and(|t| t == "File"))) {
        Some(at) => {
            menu.remove_at(at)?;
            menu.insert(&file, at)
        }
        None => menu.prepend(&file),
    }
}

/// Puts the menu bar up and routes its Note items to the window.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::default(app)?;
    #[cfg(windows)]
    windows_file(app, &menu)?;
    let new = MenuItemBuilder::with_id("note:new", "New Note…").accelerator("CmdOrCtrl+N").build(app)?;
    let mut sub = SubmenuBuilder::new(app, "Note").item(&new).separator();
    let mut doc_items = Vec::new();
    for (id, label, acc) in items(&crate::platform::platform_features()) {
        if id == "-" {
            sub = sub.item(&PredefinedMenuItem::separator(app)?);
            continue;
        }
        let mut b = MenuItemBuilder::with_id(id, label).enabled(false);
        if let Some(a) = acc {
            b = b.accelerator(a);
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
    // The standard Quit quits at once (macOS's terminate:, which Tauri can't stop); this one asks
    // first, as the menu bar's Quit does (tray::tray_quit, src/Quit.tsx).
    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next().filter(|_| cfg!(target_os = "macos")) {
        let items = app_menu.items()?;
        if let Some(at) =
            items.iter().position(|i| matches!(i, MenuItemKind::Predefined(p) if p.text().is_ok_and(|t| t.starts_with("Quit"))))
        {
            app_menu.remove_at(at)?;
            app_menu.insert(&MenuItemBuilder::with_id("app:quit", "Quit Brainstead").accelerator("CmdOrCtrl+Q").build(app)?, at)?;
        }
    }
    crate::platform::set_menu_bar(app, menu)?;
    *app.state::<NoteMenu>().0.lock().unwrap() = doc_items;
    app.on_menu_event(|app, ev| {
        let id = ev.id().as_ref();
        match id {
            "help:website" => {
                use tauri_plugin_opener::OpenerExt;
                if let Err(e) = app.opener().open_url(WEBSITE, None::<&str>) {
                    crate::applog!("website: {e}");
                }
            }
            "app:about" => about(app),
            // macOS's Quit Brainstead and Windows' File › Exit quit as the tray's Quit does: the
            // window asks about unsaved edits, then "Quit Brainstead?".
            "app:quit" | EXIT => crate::tray::tray_quit(app.clone()),
            "app:updates" => crate::updates::check_now(app),
            // The window acts on the Note and Help items (src/noteMenu.ts, src/help/HelpDrawer.tsx).
            _ if id.starts_with("note:") || id.starts_with("help:") => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
                let _ = app.emit_to("main", "menu", id);
            }
            // A predefined item, which the system carries out itself.
            _ => {}
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
    use super::*;

    #[test]
    fn the_menu_has_what_the_system_has() {
        let f = crate::platform::platform_features();
        let ids: Vec<&str> = items(&f).iter().map(|(i, _, _)| *i).collect();
        assert_eq!(ids.contains(&"note:read-aloud"), f.read_aloud);
        assert_eq!(ids.contains(&"note:export-pdf"), f.export_pdf);
        assert_eq!(ids.contains(&"note:copy-rich"), f.rich_copy);
        assert!(ids.contains(&"note:trash"));
        assert_ne!(ids.first(), Some(&"-"));
        assert_ne!(ids.last(), Some(&"-"));
        assert!(!ids.windows(2).any(|w| w == ["-", "-"]));
    }

    #[test]
    fn every_item_is_kept_and_labelled_as_written_when_every_feature_is_there() {
        let all = crate::platform::PlatformFeatures {
            read_aloud: true,
            export_pdf: true,
            rich_copy: true,
            ..crate::platform::platform_features()
        };
        let got = items(&all);
        assert_eq!(got.len(), ITEMS.len());
        let trash = got.iter().find(|(i, _, _)| *i == "note:trash").unwrap();
        assert_eq!(trash.1, "Move to the Trash");
    }

    #[test]
    fn website_matches_the_settings_screen() {
        let settings = include_str!("../../src/Settings.tsx");
        assert!(settings.contains(&format!("export const WEBSITE = \"{}\";", super::WEBSITE)));
    }
}
