// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { ProjectDoc } from "./api";
import { documentGroups, isWorkingNote } from "./projectDocs";

const doc = (title: string, type: string | null): ProjectDoc => ({
  path: `${type ? `${type}. ` : ""}${title}.md`,
  title,
  type,
  summary: null,
  mtime: 0,
  currentState: null,
});

describe("project documents", () => {
  it("groups member notes by type, Other last", () => {
    const groups = documentGroups([
      doc("Rollout", "Tracker"),
      doc("Outage", "Findings"),
      doc("Steerco", "Meeting"),
      doc("Comms", "Plan"),
      doc("Cutover", "Plan"),
      doc("Loose", null),
    ]);
    expect(groups.map((g) => [g.type, g.docs.map((d) => d.title)])).toEqual([
      ["Tracker", ["Rollout"]],
      ["Plan", ["Comms", "Cutover"]],
      ["Findings", ["Outage"]],
      ["Other", ["Loose", "Steerco"]],
    ]);
  });

  it("knows a project's working notes", () => {
    const p = "Project. Orbit.md";
    expect(isWorkingNote({ path: "Tracker. Orbit.md", type: "Tracker", project: p })).toBe(true);
    expect(isWorkingNote({ path: "Prompt. Orbit.md", type: "Prompt", project: p })).toBe(true);
    // Findings and Reviews stay; so does a Plan no project claims, and the project note itself.
    expect(isWorkingNote({ path: "Findings. Orbit.md", type: "Findings", project: p })).toBe(false);
    expect(isWorkingNote({ path: "Review. Orbit.md", type: "Review", project: p })).toBe(false);
    expect(isWorkingNote({ path: "Plan. Garden.md", type: "Plan", project: null })).toBe(false);
    expect(isWorkingNote({ path: p, type: "Project", project: p })).toBe(false);
  });
});
