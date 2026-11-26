// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Tasks screen's Sort and Group by priority: the rows shown follow the sort, and only an
// ungrouped list in its own order can be dragged.

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const rows = vi.hoisted(() => ({ list: [] as unknown[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => (cmd === "tasks_query" ? rows.list : [])),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { TaskRow } from "./api";
import { nav } from "./nav";
import { settings } from "./store";
import { TasksScreen } from "./Tasks";

const t = (text: string, line: number): TaskRow => ({
  path: "Me. To Do List.md",
  title: "Me. To Do List",
  line,
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
});

afterEach(() => {
  cleanup();
  settings.update({ viewMemory: {} });
});

const NAMES = ["Water the plants", "Book the venue", "Call Maya", "Draft the invite"];

async function screenWithTasks() {
  // In the list's own order, deliberately not by priority.
  rows.list = [t("Water the plants", 1), t("Book the venue 🔽", 2), t("Call Maya 🔺", 3), t("Draft the invite ⏫", 4)];
  nav.go("tasks");
  render(<TasksScreen />);
  await act(async () => {});
}

/** The tasks' names in the order their rows are drawn. */
const order = () =>
  [...document.querySelectorAll(".trow")].map((r) => NAMES.find((n) => r.textContent?.includes(n))).filter((n) => n !== undefined);
const grips = () => document.querySelectorAll(".trow .grip").length;
const sortBy = (v: string) => fireEvent.change(screen.getByRole("combobox", { name: "Sort" }), { target: { value: v } });

describe("the Tasks screen's Sort", () => {
  it("draws the rows in the list's own order, or by priority with none last", async () => {
    await screenWithTasks();
    expect(order()).toEqual(["Water the plants", "Book the venue", "Call Maya", "Draft the invite"]);
    sortBy("priority");
    expect(order()).toEqual(["Call Maya", "Draft the invite", "Book the venue", "Water the plants"]);
    expect(settings.get().viewMemory?.["tasks:sort"]).toBe("priority");
    sortBy("manual");
    expect(order()).toEqual(["Water the plants", "Book the venue", "Call Maya", "Draft the invite"]);
  });

  it("lets only an ungrouped list in its own order be dragged", async () => {
    await screenWithTasks();
    expect(grips()).toBe(4);
    sortBy("priority");
    expect(grips()).toBe(0);
    sortBy("manual");
    fireEvent.click(screen.getByRole("radio", { name: "Priority" }));
    expect(grips()).toBe(0);
    expect(order()).toEqual(["Call Maya", "Draft the invite", "Book the venue", "Water the plants"]);
  });
});
