// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { menuKey, WINDOW_KEYS } from "./noteMenu";

const press = (code: string, mods: { ctrl?: boolean; alt?: boolean; shift?: boolean } = {}) => ({
  code,
  ctrlKey: !!mods.ctrl,
  altKey: !!mods.alt,
  shiftKey: !!mods.shift,
  metaKey: false,
});

describe("the menu's keys where there's no menu bar", () => {
  it("has each item's key as menu.rs gives it", () => {
    const rs = readFileSync("src-tauri/src/menu.rs", "utf8");
    for (const [id, acc] of WINDOW_KEYS) {
      const quoted = id.startsWith("help:") ? `with_id("${id}", "Brainstead Help").accelerator("${acc}")` : `("${id}", "`;
      expect(rs, id).toContain(quoted);
      if (id.startsWith("note:") || id.startsWith("view:"))
        expect(rs).toMatch(new RegExp(`\\("${id}", "[^"]+", Some\\("${acc.replace(/[.+]/g, "\\$&")}"\\)\\)`));
    }
  });

  it("names the item for its key, by the key's place", () => {
    expect(menuKey(press("KeyP", { ctrl: true, shift: true }))).toBe("note:read-aloud");
    expect(menuKey(press("KeyP", { ctrl: true }))).toBe("note:export-pdf");
    expect(menuKey(press("KeyC", { ctrl: true, alt: true }))).toBe("note:copy-path");
    expect(menuKey(press("Period", { ctrl: true }))).toBe("note:focus");
    expect(menuKey(press("Slash", { ctrl: true, shift: true }))).toBe("help:open");
    // Interface size, by the key's place: Shift+= is + on a US keyboard and something else elsewhere.
    expect(menuKey(press("Equal", { ctrl: true, shift: true }))).toBe("view:interface-bigger");
    expect(menuKey(press("Minus", { ctrl: true, shift: true }))).toBe("view:interface-smaller");
    expect(menuKey(press("Digit0", { ctrl: true, shift: true }))).toBe("view:interface-usual");
    // Without Shift they're a note's Text size (src/Doc.tsx) and the Sources screen.
    expect(menuKey(press("Equal", { ctrl: true }))).toBeNull();
    expect(menuKey(press("Digit0", { ctrl: true }))).toBeNull();
    expect(menuKey(press("KeyC", { ctrl: true }))).toBeNull();
    expect(menuKey(press("KeyP", { shift: true }))).toBeNull();
    // Save and New Note have the window's own handlers.
    expect(menuKey(press("KeyS", { ctrl: true }))).toBeNull();
  });
});
