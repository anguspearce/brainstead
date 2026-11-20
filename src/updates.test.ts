// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { InboxItem, ProjectRow, TaskRow } from "./api";
import { outcomeLine, targetName, updatePrompt } from "./updates";

const action: InboxItem = {
  kind: "action",
  path: "wiki/initiatives/Orbit App launch.md",
  line: 12,
  text: "Close the loop with Maya",
  lineText: "- Close the loop with Maya",
  stamp: null,
  action: {
    id: "a7",
    initiative: "Orbit App launch",
    owner: "me",
    person: null,
    due: "2026-11-21",
    since: null,
    overdue: false,
    quotes: ["“Sam to come back to Maya” — [[Steerco minutes]]"],
  },
};

describe("Update…", () => {
  it("tells the assistant the update, what it's about and how to apply it", () => {
    const p = updatePrompt({ kind: "inbox", item: action }, " Lena has already replied to Maya ");
    expect(p).toContain('The user has an update: "Lena has already replied to Maya"');
    expect(p).toContain('initiative "Orbit App launch"');
    expect(p).toContain("Its Inbox id is action:a7.");
    expect(p).toContain("Due 2026-11-21.");
    expect(p).toContain("set_action");
    expect(p).toMatch(/End with one line starting `Done:`/);
  });

  it("describes a task and a project by what Brainstead knows of them", () => {
    const task = {
      path: "Project. Orbit App launch.md",
      line: 4,
      lineText: "- [ ] Chase the venue #waiting-for",
      text: "Chase the venue",
      tags: ["waiting-for"],
      project: "Project. Orbit App launch.md",
    } as unknown as TaskRow;
    const t = updatePrompt({ kind: "task", task }, "The venue confirmed");
    expect(t).toContain("line 5: - [ ] Chase the venue #waiting-for");
    expect(t).toContain("It's a waiting-for.");
    const project = {
      name: "Hub migration",
      path: "Project. Hub migration.md",
      status: "active",
      area: "Work",
      outcome: null,
      next: 2,
      waiting: 1,
    } as unknown as ProjectRow;
    expect(updatePrompt({ kind: "project", project }, "Lena signed off")).toContain(
      'The project "Hub migration" (Project. Hub migration.md), active, in Work.',
    );
    expect(targetName({ kind: "project", project })).toBe("Hub migration");
  });

  it("reads the outcome from the answer's last Done: or Not changed: line", () => {
    expect(outcomeLine("I marked it done.\n\n**Done:** action marked done, note added to Orbit App launch")).toEqual({
      text: "action marked done, note added to Orbit App launch",
      changed: true,
    });
    expect(outcomeLine("Not changed: which Maya do you mean?")).toEqual({ text: "which Maya do you mean?", changed: false });
    expect(outcomeLine("All sorted.")).toBeNull();
  });
});
