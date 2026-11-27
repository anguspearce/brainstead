// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import type { FileSummary } from "./api";
import { linkMatches, tagMatches } from "./Capture";

const f = (path: string, mtime: number, layer: FileSummary["layer"] = "note"): FileSummary => ({
  path,
  layer,
  type: null,
  title: path.split("/").pop()!.replace(/\.md$/, ""),
  date: null,
  size: 1,
  mtime,
  tags: [],
  project: null,
});

const files = [
  f("Project. Orbit App launch.md", 3),
  f("wiki/Orbit App.md", 2, "wiki"),
  f("Meeting. Acme kickoff - 2026-10-01.md", 5),
  f("1-1. Maya - 2026-09-28.md", 4),
  f("templates/Meeting.md", 9, "template"),
];

describe("[[ suggestions", () => {
  it("match a partial word, not only whole words", () => {
    expect(linkMatches(files, "Orb").map((x) => x.path)).toEqual(["wiki/Orbit App.md", "Project. Orbit App launch.md"]);
  });
  it("put names that start with it first, then word starts, then anywhere", () => {
    expect(linkMatches(files, "ki").map((x) => x.path)).toEqual(["Meeting. Acme kickoff - 2026-10-01.md"]);
    expect(linkMatches(files, "Proj")[0].path).toBe("Project. Orbit App launch.md");
    expect(linkMatches(files, "aya").map((x) => x.path)).toEqual(["1-1. Maya - 2026-09-28.md"]);
  });
  it("list the newest notes before anything is typed, without templates", () => {
    expect(linkMatches(files, "").map((x) => x.path)).toEqual([
      "Meeting. Acme kickoff - 2026-10-01.md",
      "1-1. Maya - 2026-09-28.md",
      "Project. Orbit App launch.md",
      "wiki/Orbit App.md",
    ]);
  });
  it("treat the typed text literally", () => {
    expect(linkMatches(files, "(")).toEqual([]);
  });
});

describe("# suggestions", () => {
  const tagged = (tags: string[]): FileSummary => ({ ...f("x.md", 1), tags });
  const vault = [tagged(["context/calls", "waiting-for"]), tagged(["context/calls"]), tagged(["calendar"]), tagged(["project/orbit"])];
  it("find a tag by its start or by a part after a slash", () => {
    expect(tagMatches(vault, "cal")).toEqual(["calendar", "context/calls"]);
    expect(tagMatches(vault, "orb")).toEqual(["project/orbit"]);
  });
  it("list the most used tags before anything is typed", () => {
    expect(tagMatches(vault, "")[0]).toBe("context/calls");
  });
});
