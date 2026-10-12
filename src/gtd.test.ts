// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { InboxItem, ProjectRow, TaskRow } from "./api";
import { prepareCapture } from "./Capture";
import {
  applyGtdShorthand,
  contextName,
  effortMinutes,
  fmtEffort,
  moveInProjectOrder,
  movedTo,
  orderedAreas,
  orderedProjects,
  projectFlag,
  projectName,
  projectTasks,
  renameInProjectOrder,
} from "./gtd";
import { taskLine, unclarified } from "./Inbox";
import { taskLabel } from "./md/TaskBlock";
import { isoWeek, reviewBody, STEPS } from "./Weekly";
import { settings } from "./store";

const proj = (o: Partial<ProjectRow> = {}): ProjectRow => ({
  path: "Project. Orbit App launch.md",
  name: "Orbit App launch",
  status: "active",
  area: "Work",
  outcome: null,
  next: 2,
  waiting: 0,
  someday: 0,
  done: 0,
  lastTouched: Date.parse("2026-10-01"),
  links: [],
  ...o,
});

describe("GTD formats", () => {
  it("reads and writes effort", () => {
    expect(["15m", "1h", "1h30m", "90m", "2d", "45", "1.5h"].map(effortMinutes)).toEqual([15, 60, 90, 90, 960, 45, 90]);
    expect(effortMinutes("soon")).toBeNull();
    expect([5, 60, 90, 480].map(fmtEffort)).toEqual(["5 min", "1 h", "1 h 30", "1 d"]);
  });

  it("turns @context and effort: into their formats, leaving email addresses alone", () => {
    expect(applyGtdShorthand("Call Lena @calls effort:15m")).toBe("Call Lena #context/calls [effort:: 15m]");
    expect(applyGtdShorthand("Mail bob@example.com effort:later")).toBe("Mail bob@example.com effort:later");
    expect(prepareCapture("task", "Book the car  @Errands due:2026-10-05", "2026-10-02")).toBe(
      "Book the car #context/errands 📅 2026-10-05",
    );
    expect(contextName("@Deep Work")).toBe("deep-work");
    expect(projectName("Projects/Project. Orbit App launch.md")).toBe("Orbit App launch");
  });

  it("keeps the formats out of a task's label", () => {
    expect(taskLabel("Call Lena #context/calls [effort:: 15m] [[Project. Orbit App launch]]")).toBe("Call Lena");
  });

  it("flags active projects with nothing next or gone quiet", () => {
    const now = Date.parse("2026-10-02");
    expect(projectFlag(proj({ next: 0 }), now)).toBe("none");
    expect(projectFlag(proj({ lastTouched: now - 15 * 86400000 }), now)).toBe("quiet");
    expect(projectFlag(proj(), now)).toBeNull();
    expect(projectFlag(proj({ status: "on-hold", next: 0 }), now)).toBeNull();
  });

  it("splits a project's tasks", () => {
    const t = (text: string, o: Partial<TaskRow> = {}) => ({ text, tags: [], done: false, project: "P.md", ...o }) as TaskRow;
    const r = projectTasks(
      [t("a"), t("b", { tags: ["waiting-for"] }), t("c", { tags: ["someday-maybe"] }), t("d", { done: true }), t("e", { project: "Q.md" })],
      "P.md",
    );
    expect([r.next, r.waiting, r.someday, r.done].map((x) => x.map((y) => y.text))).toEqual([["a"], ["b"], ["c"], ["d"]]);
  });
});

describe("Clarify", () => {
  const item = (lineText: string): InboxItem => ({
    kind: "task",
    path: "Me. To Do List.md",
    line: 3,
    text: lineText.slice(6),
    lineText,
    stamp: null,
  });

  it("counts a task as clarified once it has a project, context or GTD tag, or is done", () => {
    expect(unclarified(item("- [ ] ring the bank"))).toBe(true);
    expect(unclarified(item("- [ ] ring the bank #context/calls"))).toBe(false);
    expect(unclarified(item("- [ ] ring the bank [[Project. Money]]"))).toBe(false);
    expect(unclarified(item("- [ ] ring the bank #waiting-for"))).toBe(false);
    expect(unclarified(item("- [x] ring the bank"))).toBe(false);
    expect(unclarified({ ...item(""), kind: "thought" })).toBe(true);
  });

  it("writes the outcome's task line", () => {
    expect(
      taskLine({ text: "Call Lena", project: "Project. Orbit App launch.md", context: "@calls", effort: "15m", due: "2026-10-05" }),
    ).toBe("- [ ] Call Lena [[Project. Orbit App launch]] #context/calls [effort:: 15m] 📅 2026-10-05");
    expect(taskLine({ text: "Sam: the export", tag: "waiting-for" })).toBe("- [ ] Sam: the export #waiting-for");
    expect(taskLine({ text: "x", effort: "whenever" })).toBe("- [ ] x");
  });
});

describe("Weekly review", () => {
  it("knows the ISO week", () => {
    expect(isoWeek("2026-10-02")).toBe("2026-W40");
    expect(isoWeek("2027-01-01")).toBe("2026-W53");
    expect(isoWeek("2026-01-01")).toBe("2026-W01");
  });

  it("writes the body of its note", () => {
    const s = { week: "2026-W40", step: 10, done: [0, 1, 2], log: ["Orbit App launch completed"], notes: "A good week.\n", startedAt: 0 };
    expect(reviewBody(s)).toBe(`3 of ${STEPS.length} steps done.\n\n- Orbit App launch completed\n\nA good week.`);
    expect(reviewBody({ ...s, log: [], notes: " " })).toBe(`3 of ${STEPS.length} steps done.`);
  });
});

describe("a project's next actions", () => {
  it("go in their dragged order, unranked ones after", () => {
    const t = (text: string, rank: number | null) => ({ text, rank, project: "P.md", done: false, tags: [] }) as unknown as TaskRow;
    const next = projectTasks([t("c", null), t("b", 2048), t("a", 1024), t("d", null)], "P.md").next.map((x) => x.text);
    expect(next).toEqual(["a", "b", "c", "d"]);
  });
});

describe("the Projects screen's order", () => {
  it("puts the dragged ones first, the rest A–Z, No area last", () => {
    expect(orderedAreas(["", "Work", "Home", "Acme"], ["Work"])).toEqual(["Work", "Acme", "Home", ""]);
    const p = (name: string, area: string | null) => ({ name, path: `Project. ${name}.md`, area }) as ProjectRow;
    const ps = [p("Zed", "Work"), p("Alpha", "Work"), p("Loose", null), p("Garden", "Home")];
    expect(orderedProjects(ps, { areas: ["Work"], projects: ["Project. Zed.md"] }).map((x) => x.name)).toEqual([
      "Zed",
      "Alpha",
      "Garden",
      "Loose",
    ]);
  });

  it("moves an item before one, or after one when there's none after it", () => {
    expect(movedTo(["a", "b", "c", "d"], "d", null, "b")).toEqual(["a", "d", "b", "c"]);
    expect(movedTo(["a", "b", "c", "d"], "a", "c", null)).toEqual(["b", "c", "a", "d"]);
  });

  const order = () => settings.get().projectOrder;
  const p = (name: string, status: ProjectRow["status"] = "active") => proj({ name, path: `Project. ${name}.md`, status });

  it("keeps the projects another status tab hides where they were when one is dragged past them", () => {
    const all = [p("A"), p("H", "on-hold"), p("B"), p("C")];
    settings.update({ projectOrder: { projects: all.map((x) => x.path) } });
    // Active shows A, B, C: C dropped between A and B, with H hidden between them.
    moveInProjectOrder(all, "projects", "Project. C.md", "Project. A.md", "Project. B.md");
    expect(order()?.projects).toEqual(["Project. A.md", "Project. H.md", "Project. C.md", "Project. B.md"]);
    // A dropped after the last one shown.
    moveInProjectOrder(all, "projects", "Project. A.md", "Project. B.md", null);
    expect(order()?.projects).toEqual(["Project. H.md", "Project. C.md", "Project. B.md", "Project. A.md"]);
  });

  it("follows a renamed project note, and merges one renamed onto another where that one was", () => {
    settings.update({ projectOrder: { projects: ["Project. A.md", "Project. B.md", "Project. C.md"] } });
    renameInProjectOrder("projects", "Project. A.md", "Project. Alpha.md");
    expect(order()?.projects).toEqual(["Project. Alpha.md", "Project. B.md", "Project. C.md"]);
    renameInProjectOrder("projects", "Project. C.md", "Project. B.md");
    expect(order()?.projects).toEqual(["Project. Alpha.md", "Project. B.md"]);
  });

  it("keeps a renamed area's place, and its old place for an undo", () => {
    settings.update({ projectOrder: { areas: ["Home", "Work", "Acme"] } });
    renameInProjectOrder("areas", "Work", "Jobs", ["Home", "Work", "Acme"]);
    expect(orderedAreas(["Home", "Jobs", "Acme"], order()?.areas)).toEqual(["Home", "Jobs", "Acme"]);
    // Undone (⌘Z, the toast's Undo, Revert in Changes): Work is back where it was.
    expect(orderedAreas(["Home", "Work", "Acme"], order()?.areas)).toEqual(["Home", "Work", "Acme"]);
    // Jobs, left over, is no area to merge with: Acme renamed to it keeps Acme's place.
    renameInProjectOrder("areas", "Acme", "Jobs", ["Home", "Work", "Acme"]);
    expect(orderedAreas(["Home", "Work", "Jobs"], order()?.areas)).toEqual(["Home", "Work", "Jobs"]);
    // Renamed onto an area that's there: merged where that one was.
    settings.update({ projectOrder: { areas: ["Home", "Work", "Acme"] } });
    renameInProjectOrder("areas", "Acme", "Home", ["Home", "Work", "Acme"]);
    expect(orderedAreas(["Home", "Work"], order()?.areas)).toEqual(["Home", "Work"]);
    expect(orderedAreas(["Home", "Work", "Acme"], order()?.areas)).toEqual(["Home", "Work", "Acme"]);
    // The next drag keeps only the areas there are.
    moveInProjectOrder([proj({ area: "Work" }), proj({ area: "Home", path: "Project. G.md" })], "areas", "Work", null, "Home");
    expect(order()?.areas).toEqual(["Work", "Home"]);
  });
});
