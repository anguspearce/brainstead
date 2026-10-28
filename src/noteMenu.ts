// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The menu bar's Note menu (src-tauri/src/menu.rs). Its items arrive as `menu` events; the open
// document registers what to do with them, and the note's items are enabled only while it does.
// Windows has no menu bar, so there the window takes the menu's keys itself (WINDOW_KEYS), the
// View menu's Interface size keys among them (src/interfaceSize.ts acts on those).

import { emit } from "@tauri-apps/api/event";
import { api } from "./api";
import { openNewNote } from "./notes/dialogs";
import { isWindows } from "./platform";

export type NoteMenuAction =
  | "save"
  | "view"
  | "edit"
  | "source"
  | "focus"
  | "copy-markdown"
  | "copy-rich"
  | "copy-path"
  | "copy-title"
  | "export-pdf"
  | "read-aloud"
  | "rename"
  | "reveal"
  | "discard-draft"
  | "trash";

let handler: ((a: NoteMenuAction) => void) | null = null;

/** The open document's handler, or null when it closes. */
export function setNoteMenu(h: ((a: NoteMenuAction) => void) | null) {
  handler = h;
  void api.menuNoteOpen(!!h).catch(() => {});
}

/**
 * The menu items whose keys only the menu bar gives, as menu.rs has them (a test keeps the two the
 * same), for Windows, which has none. Save, New Note, View, Edit and Source have handlers of their
 * own in the window, so they aren't here.
 */
export const WINDOW_KEYS: [string, string][] = [
  ["note:focus", "CmdOrCtrl+."],
  ["note:copy-path", "Alt+CmdOrCtrl+C"],
  ["note:read-aloud", "Shift+CmdOrCtrl+P"],
  ["note:export-pdf", "CmdOrCtrl+P"],
  ["note:reveal", "Alt+CmdOrCtrl+R"],
  ["note:rename", "Shift+CmdOrCtrl+R"],
  ["view:interface-bigger", "Shift+CmdOrCtrl+Equal"],
  ["view:interface-smaller", "Shift+CmdOrCtrl+Minus"],
  ["view:interface-usual", "Shift+CmdOrCtrl+0"],
  ["help:open", "Shift+CmdOrCtrl+Slash"],
];

const CODES: Record<string, string> = { ".": "Period", Slash: "Slash" };

type KeyEvent = { ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean; code: string };

/** The menu item an accelerator in tauri's terms ("Shift+CmdOrCtrl+P") names for a key press, by
 *  the key's place (`code`), as Shift and Alt change what's typed. Ctrl stands for CmdOrCtrl. */
export function menuKey(e: KeyEvent, keys = WINDOW_KEYS): string | null {
  for (const [id, acc] of keys) {
    const parts = acc.split("+");
    const key = parts.pop()!;
    const code = CODES[key] ?? (/^[A-Z]$/.test(key) ? `Key${key}` : /^[0-9]$/.test(key) ? `Digit${key}` : key);
    if (
      e.code === code &&
      e.ctrlKey === parts.includes("CmdOrCtrl") &&
      e.altKey === parts.includes("Alt") &&
      e.shiftKey === parts.includes("Shift") &&
      !e.metaKey
    )
      return id;
  }
  return null;
}

/** Listens for the menu once, at startup; on Windows, for its keys too. */
export function startNoteMenu() {
  if (isWindows)
    window.addEventListener("keydown", (e) => {
      const id = menuKey(e);
      if (!id) return;
      e.preventDefault();
      // As the menu bar's item would: every `menu` listener (this one, the help drawer's) acts on it.
      void emit("menu", id);
    });
  return api.onMenu((id) => {
    if (!id.startsWith("note:")) return;
    const a = id.slice(5);
    if (a === "new") openNewNote();
    else handler?.(a as NoteMenuAction);
  });
}
