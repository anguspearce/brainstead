// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => p }));
import { Command, commands, match, paletteMove, sections } from "./Palette";

describe("palette matching", () => {
  const all = commands();
  it("lists the screens when nothing is typed", () => {
    const r = match(all, "");
    expect(r.every((c) => c.group === "Go to")).toBe(true);
    expect(r.map((c) => c.label)).toContain("Knowledge health");
  });
  it("needs every word, and puts labels that start with it first", () => {
    expect(match(all, "set")[0].label).toBe("Settings");
    expect(match(all, "rebuild").map((c) => c.id)).toEqual(["rebuild"]);
    expect(match(all, "theme dark").map((c) => c.id)).toEqual(["dark"]);
    expect(match(all, "zzz")).toEqual([]);
  });
  it("matches keywords", () => {
    expect(match(all, "full disk").map((c) => c.id)).toEqual(["perm"]);
  });
});

describe("palette layout", () => {
  const all = commands();
  it("shows each screen's hotkey", () => {
    const key = (label: string) => all.find((c) => c.group === "Go to" && c.label === label)?.kbd;
    expect([key("Today"), key("Changes"), key("Search"), key("Ask"), key("Sources"), key("Settings"), key("Graph")]).toEqual([
      "⌘1",
      undefined,
      "⌘7",
      "⌘6",
      "⌘0",
      "⌘,",
      undefined,
    ]);
  });
  it("splits the results into runs of one group, covering every result in order", () => {
    const s = sections(match(all, "s"));
    expect(s.reduce((n, x) => (expect(x.from).toBe(n), n + x.rows.length), 0)).toBe(match(all, "s").length);
  });
  it("moves by the layout as drawn, a Go to run in two columns", () => {
    const row = (id: string, group: string) => ({ id, group, icon: "note", label: id, run: () => {} }) as Command;
    // Sorted results: Go to rows apart in the list, with a Files row between them.
    const items = [row("g1", "Go to"), row("f1", "Files"), row("g2", "Go to"), row("g3", "Go to"), row("g4", "Go to"), row("g5", "Go to")];
    const down = (k: number) => paletteMove(items, k, "down");
    const up = (k: number) => paletteMove(items, k, "up");
    // A lone Go to row and a Files row are lines: one step, never two.
    expect([down(0), down(1), up(2), up(1)]).toEqual([1, 2, 1, 0]);
    // In the run g2..g5 (two rows of two, from index 2): a column down, then out of the grid.
    expect([down(2), down(3), down(4), down(5)]).toEqual([4, 5, 5, 5]);
    expect([up(4), up(5), up(3)]).toEqual([2, 3, 1]);
    expect([paletteMove(items, 2, "right"), paletteMove(items, 3, "left"), paletteMove(items, 3, "right")]).toEqual([3, 2, 3]);
    expect(paletteMove(items, 0, "right")).toBe(0);
  });
});
