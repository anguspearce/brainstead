// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined), convertFileSrc: (p: string) => p }));

import {
  clampPaneWidth,
  fitPanes,
  listColumn,
  lockedMessage,
  paneName,
  MIN_FLEX,
  PANES,
  paneRoom,
  paneSpecs,
  resetPaneWidth,
  resetPaneWidths,
  resizeLocked,
  setPaneWidth,
} from "./panes";
import { DEFAULT_SETTINGS } from "./api";
import { settings } from "./store";

describe("pane widths", () => {
  it("clamp to the pane's range, then to the room the screen has, and nonsense gives the default", () => {
    const left = PANES["tasks.left"];
    expect(clampPaneWidth(left, 300)).toBe(300);
    expect(clampPaneWidth(left, 300.4)).toBe(300);
    expect(clampPaneWidth(left, 10)).toBe(left.min);
    expect(clampPaneWidth(left, 5000)).toBe(left.max);
    expect(clampPaneWidth(left, undefined)).toBe(left.def);
    expect(clampPaneWidth(left, NaN)).toBe(left.def);
    // A narrow window: the room wins over the range's maximum, never over its minimum.
    expect(clampPaneWidth(left, 400, 300)).toBe(300);
    expect(clampPaneWidth(left, 400, 100)).toBe(left.min);
  });

  it("fit together: the defaults give way before the widths set, each down to its least", () => {
    const [type, folder, deleted, size] = (["trash.type", "trash.folder", "trash.deleted", "trash.size"] as const).map((k) => PANES[k]);
    const at = (want: number, stored = false) => ({ want, stored });
    // Room for all of them as they are: nothing changes.
    expect(
      fitPanes(
        [
          { spec: type, ...at(90) },
          { spec: size, ...at(72) },
        ],
        500,
      ),
    ).toEqual([90, 72]);
    // 40px short: the default gives way, the width set keeps.
    expect(
      fitPanes(
        [
          { spec: type, ...at(90) },
          { spec: folder, ...at(220, true) },
        ],
        270,
      ),
    ).toEqual([60, 210]);
    // Each default in proportion to how far it's above its least.
    expect(
      fitPanes(
        [
          { spec: type, ...at(90) },
          { spec: folder, ...at(220) },
        ],
        270,
      ),
    ).toEqual([82, 187]);
    // The Trash at the least window with a 400px sidebar: every column at its least, never below.
    const all = [type, folder, deleted, size].map((spec) => ({ spec, want: spec.def, stored: false }));
    expect(fitPanes(all, 0)).toEqual([60, 80, 70, 50]);
  });

  it("leave the flexible pane its least width once the gaps and the columns that never move are taken", () => {
    expect(paneRoom(1000)).toBe(1000 - MIN_FLEX);
    expect(paneRoom(1000, 24)).toBe(1000 - 24 - MIN_FLEX);
  });

  it("have their custom property in the stylesheet at their usual width, with no width stored", () => {
    // A screen's divider is placed by its pane's property: one only the grid's fallback gave would
    // be unset until a width was stored, and the divider would sit at the root's left edge.
    const css = readFileSync(resolve(__dirname, "styles.css"), "utf8");
    for (const [key, spec] of Object.entries(PANES).filter(([, p]) => p.type === "panes"))
      expect(css, key).toMatch(new RegExp(`^ *${spec.cssVar}: ${spec.def}px;$`, "m"));
    expect(css).not.toMatch(/var\(--pane-[a-z]+,/);
  });

  it("are kept per pane in settings, one or all put back", () => {
    settings.loadFrom({ ...DEFAULT_SETTINGS });
    expect(setPaneWidth("sidebar", 300)).toBe(300);
    expect(setPaneWidth("tasks.side", 9000)).toBe(PANES["tasks.side"].max);
    expect(settings.get().paneWidths).toEqual({ sidebar: 300, "tasks.side": PANES["tasks.side"].max });
    resetPaneWidth("sidebar");
    expect(settings.get().paneWidths).toEqual({ "tasks.side": PANES["tasks.side"].max });
    // The last one gone leaves no key, so settings.json loses it too.
    resetPaneWidth("tasks.side");
    expect(settings.get().paneWidths).toBeUndefined();
    setPaneWidth("sidebar", 250);
    resetPaneWidths();
    expect(settings.get().paneWidths).toBeUndefined();
    expect(() => setPaneWidth("nowhere", 100)).toThrow(/No pane/);
  });

  it("know every list table's columns before any table is drawn, and give the tables their widths", () => {
    // The settings tool can set or reset one with no screen shown (the key is the table's, not its noun).
    expect(paneSpecs()["notes.type"]).toMatchObject({
      cssVar: "--col-type",
      def: 110,
      side: "right",
      label: "the Type column",
      type: "columns",
    });
    expect(Object.keys(paneSpecs()).filter((k) => k.startsWith("wiki.") && k !== "wiki.left")).toEqual([
      "wiki.type",
      "wiki.sources",
      "wiki.health",
      "wiki.tags",
      "wiki.mtime",
    ]);
    expect(listColumn("wiki.tags")).toEqual({ label: "Tags", width: "0 0 200px" });
    settings.loadFrom({ ...DEFAULT_SETTINGS });
    expect(setPaneWidth("templates.size", 90)).toBe(90);
    resetPaneWidth("templates.size");
    expect(settings.get().paneWidths).toBeUndefined();
  });

  it("give every table's columns a key, a range around its default and the columns lock", () => {
    // Each column's side of its table's flexible column (Name, Item, Document): after it, "right".
    const tables = {
      trash: { type: "right", folder: "right", deleted: "right", size: "right" },
      activity: { when: "left", time: "left", action: "right" },
      register: { version: "right", status: "right", file: "right" },
    };
    for (const [table, cols] of Object.entries(tables))
      for (const [c, side] of Object.entries(cols)) {
        const spec = PANES[`${table}.${c}` as keyof typeof PANES];
        expect(spec, `${table}.${c}`).toMatchObject({ cssVar: `--col-${c}`, side, type: "columns" });
        expect(spec.min).toBeLessThan(spec.def);
        expect(spec.def).toBeLessThan(spec.max);
      }
    // A column's label is its heading, as its table draws it; every key names its screen.
    for (const [key, spec] of Object.entries(PANES)) {
      if (spec.type === "columns") expect(spec.label, key).toBe(`the ${spec.heading} column`);
      if (key !== "sidebar") expect(paneName(key), key).toMatch(/^[A-Z][^›]* › the /);
    }
    expect(paneName("trash.folder")).toBe("Trash › the Was in column");
    expect(paneName("sidebar")).toBe("the sidebar");
    // The screens' panes and the sidebar are the panes lock's.
    expect(PANES.sidebar.type).toBe("panes");
    expect(PANES["sources.side"].type).toBe("panes");
    expect(
      Object.values(PANES)
        .filter((p) => p.cssVar.startsWith("--col-"))
        .every((p) => p.type === "columns"),
    ).toBe(true);
  });

  it("refuse a locked type's widths, keep the stored ones, and still reset them all", () => {
    settings.update({ paneWidths: undefined, resizeLocks: undefined });
    setPaneWidth("sidebar", 300);
    setPaneWidth("trash.type", 120);
    settings.update({ resizeLocks: { columns: true } });
    expect(resizeLocked("columns")).toBe(true);
    expect(resizeLocked("panes")).toBe(false);
    expect(() => setPaneWidth("trash.type", 150)).toThrow(lockedMessage("trash.type", "columns"));
    expect(() => resetPaneWidth("trash.type")).toThrow(/Column widths are locked/);
    expect(lockedMessage("trash.type", "columns")).toMatch(/turn off Lock column widths \(resizeLocks\.columns\)/);
    // The other type isn't locked by it, and the locked one's width is kept (PaneResizer.test.tsx
    // shows it drawn).
    expect(setPaneWidth("sidebar", 320)).toBe(320);
    expect(settings.get().paneWidths).toEqual({ sidebar: 320, "trash.type": 120 });
    settings.update({ resizeLocks: { columns: true, panes: true } });
    expect(() => setPaneWidth("sidebar", 340)).toThrow(/^Pane widths are locked, so sidebar can't be resized/);
    resetPaneWidths();
    expect(settings.get().paneWidths).toBeUndefined();
    settings.update({ resizeLocks: undefined });
  });
});
