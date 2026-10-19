// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { TaskRow } from "./api";
import { prepareCapture } from "./Capture";
import { todayRows, viewRows, VIEWS, weekOf } from "./taskModel";

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
