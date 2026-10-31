// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { TaskRow } from "./api";
import { prepareCapture } from "./Capture";
import { invoke } from "@tauri-apps/api/core";
import { inParkedProject, leftOutNote, moveTask, parkedLeftOut, todayRows, viewRows, VIEWS, weekOf, withHiddenRows } from "./taskModel";
import parkedTable from "../tests/fixtures/parked-projects.json";

const t = (text: string, o: Partial<TaskRow> = {}): TaskRow => ({
  path: "A.md",
  title: "A",
  line: 0,
  lineText: `- [ ] ${text}`,
  text,
  done: false,
  due: null,
  scheduled: null,
  start: null,
  doneOn: null,
  created: null,
  rank: null,
  tags: [],
  heading: null,
  ...o,
});

const FRI = "2026-10-02";
const view = (id: string) => VIEWS.find((v) => v.id === id)!;

describe("task views", () => {
  const all = [
    t("plain"),
    t("follow", { tags: ["followup/Maya"] }),
    t("wait", { tags: ["waiting-for"] }),
    t("later", { tags: ["someday-maybe"] }),
    t("deferred", { scheduled: "2026-10-01" }),
    t("not yet", { scheduled: "2026-10-05" }),
    t("done mon", { done: true, doneOn: "2026-09-28" }),
    t("done last fri", { done: true, doneOn: "2026-09-25" }),
  ];
  const labels = (id: string) => viewRows(all, view(id), FRI).map((x) => x.text);
  it("sorts tasks into the previous app's lists", () => {
    expect(labels("next")).toEqual(["plain", "deferred"]);
    expect(labels("followups")).toEqual(["follow"]);
    expect(labels("waiting")).toEqual(["wait"]);
    expect(labels("someday")).toEqual(["later"]);
    expect(labels("scheduled")).toEqual(["deferred", "not yet"]);
    expect(labels("done-this-week")).toEqual(["done mon"]);
    expect(labels("done-last-week")).toEqual(["done last fri"]);
  });
  it("leaves Inbox tasks off Next actions until they're clarified", () => {
    const todo = "Me. To Do List.md";
    const rows = [t("raw", { path: todo, line: 3 }), t("ring Maya", { path: todo, line: 4, contexts: ["calls"] }), t("elsewhere")];
    const inbox = [
      { kind: "task" as const, path: todo, line: 3, text: "raw", lineText: "- [ ] raw", stamp: null },
      { kind: "task" as const, path: todo, line: 4, text: "ring Maya", lineText: "- [ ] ring Maya #context/calls", stamp: null },
    ];
    expect(viewRows(rows, view("next"), FRI, inbox).map((x) => x.text)).toEqual(["ring Maya", "elsewhere"]);
    expect(viewRows(rows, view("next"), FRI).map((x) => x.text)).toEqual(["raw", "ring Maya", "elsewhere"]);
  });
  it("leaves on-hold and someday projects' tasks off Next actions unless asked for", () => {
    const p = (status: TaskRow["projectStatus"], name: string) => t(name, { project: `Project. ${name}.md`, projectStatus: status });
    const rows = [p("active", "Run"), p("on-hold", "Held"), p("someday", "Maybe"), p("done", "Finished"), t("Loose")];
    // Off (the default): only active, done and no-project tasks.
    expect(viewRows(rows, view("next"), FRI).map((x) => x.text)).toEqual(["Run", "Finished", "Loose"]);
    expect(viewRows(rows, view("next"), FRI, null, false).map((x) => x.text)).toEqual(["Run", "Finished", "Loose"]);
    // On: every one of them.
    expect(viewRows(rows, view("next"), FRI, null, true).map((x) => x.text)).toEqual(["Run", "Held", "Maybe", "Finished", "Loose"]);
    // Other lists don't change with it, on or off.
    const waiting = rows.map((r) => ({ ...r, tags: ["waiting-for"] }));
    for (const on of [false, true]) expect(viewRows(waiting, view("waiting"), FRI, null, on)).toHaveLength(5);
  });
  it("counts what Next actions leaves out for on-hold and someday projects, none while they're included", () => {
    const p = (status: TaskRow["projectStatus"], name: string, o: Partial<TaskRow> = {}) =>
      t(name, { project: `Project. ${name}.md`, projectStatus: status, ...o });
    const rows = [p("active", "Run"), p("on-hold", "Held"), p("someday", "Maybe"), p("on-hold", "Waits", { tags: ["waiting-for"] })];
    // Only what Next actions would otherwise show: not the waiting-for.
    expect(parkedLeftOut(rows, FRI, null, false).map((x) => x.text)).toEqual(["Held", "Maybe"]);
    expect(parkedLeftOut(rows, FRI, null, true)).toEqual([]);
    expect(leftOutNote(1)).toMatch(/^1 task of on-hold or someday projects is left out/);
    expect(leftOutNote(2)).toMatch(/^2 tasks of on-hold or someday projects are left out/);
  });
  it("keeps hidden rows among their old neighbours when a list is renumbered", () => {
    const [a, h1, b, h2, c] = ["A", "Held 1", "B", "Held 2", "C"].map((x) => t(x));
    const every = [h1, a, b, h2, c];
    const names = (rs: TaskRow[]) => rs.map((x) => x.text);
    // C dragged to the top: each hidden row stays after the row it followed.
    expect(names(withHiddenRows([c, a, b], every, c))).toEqual(["Held 1", "C", "A", "B", "Held 2"]);
    // B dragged below C: Held 2 followed B, which has moved, so it stays after A.
    expect(names(withHiddenRows([a, c, b], [a, b, h2, c], b))).toEqual(["A", "Held 2", "C", "B"]);
    // Nothing hidden: the order as it is.
    expect(names(withHiddenRows([c, a, b], [a, b, c], c))).toEqual(["C", "A", "B"]);
  });
  it("ranks a drop between a hidden row and the shown row after it, not on the hidden row's rank", async () => {
    const [a, h, c, x] = [1024, 2048, 3072, 4096].map((rank, i) => t(["A", "Held", "C", "X"][i], { rank, line: i }));
    const mock = vi.mocked(invoke);
    mock.mockReset();
    mock.mockResolvedValue(true);
    // X dragged between A and C; Held, hidden, followed A.
    expect(await moveTask([a, x, c], x, [a, h, c, x])).toBe(true);
    expect(mock).toHaveBeenCalledTimes(1);
    expect(mock).toHaveBeenCalledWith("task_rank", expect.objectContaining({ line: 3, prev: 2048, next: 3072 }));
    // A hidden row with no rank yet: the whole list is renumbered, Held in its place.
    mock.mockClear();
    const unranked = t("Held", { rank: null, line: 1 });
    expect(await moveTask([a, x, c], x, [a, unranked, c, x])).toBe(true);
    const sent = mock.mock.calls[0];
    expect(sent[0]).toBe("tasks_rank_all");
    expect((sent[1] as { tasks: { line: number }[] }).tasks.map((r) => r.line)).toEqual([0, 1, 3, 2]);
  });
  it("parks the projects' tasks the shared table says, as the weekly review's preparation does (tests/fixtures/parked-projects.json)", () => {
    for (const c of parkedTable.statuses) {
      const row = t("x", { project: c.status && "Project. X.md", projectStatus: c.status as TaskRow["projectStatus"] });
      expect(inParkedProject(row), String(c.written)).toBe(c.parked);
    }
  });
  it("gives a task added from a list that list's tag, so it shows there", () => {
    for (const id of ["followups", "waiting", "someday"]) {
      const v = view(id);
      const added = t("new", { tags: [v.adds!.slice(1)] });
      expect(viewRows([added], v, FRI)).toHaveLength(1);
    }
    expect(view("next").adds).toBeUndefined();
  });
  it("counts weeks from Monday", () => {
    expect(weekOf(FRI)).toEqual(["2026-09-28", "2026-10-05"]);
    expect(weekOf("2026-09-28")).toEqual(["2026-09-28", "2026-10-05"]);
    expect(weekOf("2026-10-04", -1)).toEqual(["2026-09-21", "2026-09-28"]);
  });
  it("finds today's tasks", () => {
    const r = todayRows(
      [
        t("late", { due: "2026-09-30" }),
        t("now", { due: FRI }),
        t("soon", { due: "2026-10-03" }),
        t("def", { scheduled: FRI }),
        t("both", { due: FRI, scheduled: FRI }),
        t("x", { done: true, due: "2026-09-01" }),
        t("not today", { due: FRI, scheduled: "2026-10-07" }),
        t("not yet", { due: "2026-09-30", scheduled: "2026-10-03" }),
        t("next week", { due: "2026-10-09" }),
        t("too far", { due: "2026-10-10" }),
        t("later but deferred", { due: "2026-10-05", scheduled: "2026-10-04" }),
      ],
      FRI,
    );
    expect(r.overdue.map((x) => x.text)).toEqual(["late"]);
    expect(r.due.map((x) => x.text)).toEqual(["now", "both"]);
    expect(r.scheduled.map((x) => x.text)).toEqual(["def"]);
    // Coming up this week: due within seven days, soonest first; not one deferred to a later day.
    expect(r.soon.map((x) => x.text)).toEqual(["soon", "next week"]);
  });
});

describe("capture", () => {
  it("writes shorthand as dates and a task on one line", () => {
    expect(prepareCapture("task", "  Ask Maya\nabout bands #followup due: mon ", FRI)).toBe("Ask Maya about bands #followup 📅 2026-10-05");
    expect(prepareCapture("thought", "line one\nline two defer: tomorrow", FRI)).toBe("line one\nline two ⏳ 2026-10-03");
    expect(prepareCapture("task", "   ", FRI)).toBe("");
  });
});
