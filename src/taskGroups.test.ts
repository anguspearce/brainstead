// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { TaskRow } from "./api";
import { priorityOf } from "./taskFields";
import { groupTasks, matchesSearch, sortTasks, withinEffort } from "./taskGroups";

const t = (text: string, x: Partial<TaskRow> = {}) =>
  ({ text, due: null, contexts: [], project: null, effortMin: null, ...x }) as unknown as TaskRow;

describe("task groups", () => {
  const rows = [
    t("a", { project: "Project. Orbit App launch.md", contexts: ["calls"], due: "2026-10-02" }),
    t("b", { due: "2026-10-03" }),
    t("c", { project: "Project. Hub migration.md", due: "2026-10-04", effortMin: 15 }),
    t("d", { contexts: ["office"], due: "2026-10-20", effortMin: 45 }),
  ];
  it("groups by project, the ones with none last", () => {
    expect(groupTasks(rows, "project", "2026-10-03").map((g) => [g.label, g.rows.map((r) => r.text)])).toEqual([
      ["Hub migration", ["c"]],
      ["Orbit App launch", ["a"]],
      ["No project", ["b", "d"]],
    ]);
  });
  it("groups by due date in time order", () => {
    // 2026-10-03 is a Saturday: Sunday the 4th is still this week.
    expect(groupTasks(rows, "due", "2026-10-03").map((g) => g.label)).toEqual(["Overdue", "Today", "This week", "Later"]);
  });
  it("groups by first context and filters by effort", () => {
    expect(groupTasks(rows, "context", "2026-10-03").map((g) => g.label)).toEqual(["@calls", "@office", "No context"]);
    expect(rows.filter((r) => withinEffort(r, "30")).map((r) => r.text)).toEqual(["c"]);
    expect(rows.filter((r) => withinEffort(r, "")).length).toBe(4);
  });
  it("searches every word in the text, note, project and contexts, ignoring case and accents", () => {
    const r = t("Call Zoë about the launch", {
      title: "Weekly sync",
      tags: ["#context/calls"],
      contexts: ["calls"],
      project: "Project. Orbit App launch.md",
    });
    expect(matchesSearch(r, "zoe LAUNCH")).toBe(true);
    expect(matchesSearch(r, "orbit sync")).toBe(true);
    expect(matchesSearch(r, "@calls")).toBe(true);
    expect(matchesSearch(r, "zoe budget")).toBe(false);
    expect(matchesSearch(r, "  ")).toBe(true);
  });
});

describe("task priority", () => {
  // The row's text is what the chip reads, as the app gives it: the emoji where the line has it.
  const p = (text: string, emoji = "") => {
    const line = `${text}${emoji ? ` ${emoji}` : ""} #a`;
    return t(line, { lineText: `- [ ] ${line}`, tags: ["#a"] });
  };
  // File (manual) order, deliberately not by priority.
  const rows = [
    p("none-1"),
    p("low", "🔽"),
    p("highest", "🔺"),
    p("none-2"),
    p("high-1", "⏫"),
    p("lowest", "⏬"),
    p("medium", "🔼"),
    p("high-2", "⏫"),
  ];

  const name = (r: TaskRow) => r.text.split(" ")[0];

  it("reads the priority from the task line", () => {
    expect(rows.map(priorityOf)).toEqual(["none", "low", "highest", "none", "high", "lowest", "medium", "high"]);
  });
  it("sorts highest first and no priority after lowest, keeping the manual order within a priority", () => {
    expect(sortTasks(rows, "priority").map((r) => name(r))).toEqual([
      "highest",
      "high-1",
      "high-2",
      "medium",
      "low",
      "lowest",
      "none-1",
      "none-2",
    ]);
  });
  it("leaves the list as it is when the sort is manual, and never changes the rows it was given", () => {
    const before = rows.map((r) => name(r));
    expect(sortTasks(rows, "manual")).toBe(rows);
    sortTasks(rows, "priority");
    expect(rows.map((r) => name(r))).toEqual(before);
  });
  it("groups by priority in that order, the ungraded last, each group in the rows' order", () => {
    expect(groupTasks(rows, "priority", "2026-10-03").map((g) => [g.label, g.rows.map((r) => name(r))])).toEqual([
      ["Highest", ["highest"]],
      ["High", ["high-1", "high-2"]],
      ["Medium", ["medium"]],
      ["Low", ["low"]],
      ["Lowest", ["lowest"]],
      ["No priority", ["none-1", "none-2"]],
    ]);
  });
  it("leaves out priorities nothing has", () => {
    expect(groupTasks([p("a", "🔼"), p("b")], "priority", "2026-10-03").map((g) => g.label)).toEqual(["Medium", "No priority"]);
  });
  it("reads the priority where the row's chip does, not only at the end of the line", () => {
    // Capture writes the priority before the effort field; a priority can also open the words.
    const mid = t("Call Sam ⏫ [effort:: 15m]", { lineText: "- [ ] Call Sam ⏫ [effort:: 15m]" });
    const first = t("⏫ Review [[Project. Orbit]]", { lineText: "- [ ] ⏫ Review [[Project. Orbit]]" });
    const code = t("Mind the `⏫` in code", { lineText: "- [ ] Mind the `⏫` in code" });
    const none = t("Plain", { lineText: "- [ ] Plain" });
    expect([mid, first, code].map(priorityOf)).toEqual(["high", "high", "none"]);
    expect(sortTasks([none, code, mid, first], "priority")).toEqual([mid, first, none, code]);
    expect(groupTasks([none, mid, first], "priority", "2026-10-03").map((g) => [g.label, g.rows.length])).toEqual([
      ["High", 2],
      ["No priority", 1],
    ]);
  });
  it("keeps the list's own order for a sort it doesn't know, as a stale remembered one", () => {
    expect(sortTasks(rows, "urgency" as never)).toBe(rows);
  });
});
