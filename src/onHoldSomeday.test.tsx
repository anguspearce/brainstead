// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Include on-hold and someday projects (`includeOnHoldSomeday`) reaches every screen that shows Next
// actions: the Tasks screen, the sidebar's count and the weekly review's Next actions step
// (D-20261031-01).

import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { TaskRow } from "./api";

const task = (line: number, text: string, o: Partial<TaskRow> = {}): TaskRow => ({
  path: "Me. To Do List.md",
  title: "To Do List",
  line,
  lineText: `- [ ] ${text}`,
  text,
  status: " ",
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
const rows = [
  task(1, "Call Sam about the launch", { project: "Project. Orbit App launch.md", projectStatus: "active" }),
  task(2, "Prune the roses", { project: "Project. Garden.md", projectStatus: "on-hold" }),
  task(3, "Learn the cello", { project: "Project. Cello.md", projectStatus: "someday" }),
];
const invoke = vi.hoisted(() => vi.fn(async (_cmd: string, ..._a: unknown[]): Promise<unknown> => undefined));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => p, invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { Sidebar } from "./Sidebar";
import { EMPTY_STATUS, vaultStatus } from "./state";
import { settings } from "./store";
import { TasksScreen } from "./Tasks";
import { Step } from "./Weekly";

invoke.mockImplementation(async (cmd: string) => (cmd === "tasks_query" ? rows : []));

afterEach(() => {
  cleanup();
  settings.update({ includeOnHoldSomeday: undefined });
});

describe("Include on-hold and someday projects", () => {
  it("decides the sidebar's Tasks count", async () => {
    vaultStatus.set({ ...EMPTY_STATUS, state: "ready" });
    render(<Sidebar onSearch={() => {}} />);
    const count = () => screen.getByRole("button", { name: /^Tasks/ }).querySelector(".n")?.textContent;
    await vi.waitFor(() => expect(count()).toBe("1"));
    act(() => settings.update({ includeOnHoldSomeday: true }));
    expect(count()).toBe("3");
  });

  it("decides the weekly review's Next actions, and an empty list says what it left out", async () => {
    const one = [rows[1]];
    invoke.mockImplementation(async (cmd: string) => (cmd === "tasks_query" ? one : []));
    render(<Step id="next" ctx={null} log={() => {}} />);
    await screen.findByText(/^No next actions\. 1 task of on-hold or someday projects is left out/);
    expect(screen.queryByText("Prune the roses")).toBeNull();
    act(() => settings.update({ includeOnHoldSomeday: true }));
    expect(await screen.findByText("Prune the roses")).toBeTruthy();
  });

  it("is a checkbox on the Tasks screen that shows the tasks and counts the ones it hides", async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === "tasks_query" ? rows : []));
    render(<TasksScreen />);
    const box = await screen.findByRole("checkbox", { name: /Include on-hold and someday projects/ });
    expect((box as HTMLInputElement).checked).toBe(false);
    // Two tasks left out of the list, its count on the left and the label by the checkbox.
    const list = screen.getByRole("table", { name: "Next actions" });
    expect(within(list).getByText("Call Sam about the launch")).toBeTruthy();
    expect(within(list).queryByText("Prune the roses")).toBeNull();
    expect(screen.getByText("2 hidden")).toBeTruthy();
    expect(screen.getByRole("button", { name: /^Next actions/ }).querySelector(".n")?.textContent).toBe("1");
    fireEvent.click(box);
    expect(settings.get().includeOnHoldSomeday).toBe(true);
    expect(await within(list).findByText("Prune the roses")).toBeTruthy();
    expect(within(list).getByText("Learn the cello")).toBeTruthy();
    expect(screen.queryByText(/hidden/)).toBeNull();
    expect(screen.getByRole("button", { name: /^Next actions/ }).querySelector(".n")?.textContent).toBe("3");
  });

  it("says in an empty list how many tasks it left out", async () => {
    const parked = rows.slice(1);
    invoke.mockImplementation(async (cmd: string) => (cmd === "tasks_query" ? parked : []));
    render(<TasksScreen />);
    await screen.findByText(/^No tasks in this list\. 2 tasks of on-hold or someday projects are left out/);
  });

  it("keeps the hidden tasks' places when a drag renumbers or ranks, by passing the whole list to the drop", async () => {
    // The hidden Garden task is last, at rank 4096: dragging the first task to the end ranks it
    // after that one, not on the rank of the last task shown.
    const ranked = [
      task(1, "Call Sam about the launch", { project: "Project. Orbit App launch.md", projectStatus: "active", rank: 1024 }),
      task(2, "Book the room", { rank: 2048 }),
      task(3, "Ask Maya", { rank: 3072 }),
      task(4, "Prune the roses", { project: "Project. Garden.md", projectStatus: "on-hold", rank: 4096 }),
    ];
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string) => (cmd === "tasks_query" ? ranked : cmd === "task_rank" ? true : []));
    render(<TasksScreen />);
    const list = await screen.findByRole("table", { name: "Next actions" });
    const grips = await vi.waitFor(() => {
      const g = list.querySelectorAll(".grip");
      expect(g).toHaveLength(3);
      return g;
    });
    fireEvent.pointerDown(grips[0], { button: 0, clientY: 0 });
    window.dispatchEvent(new MouseEvent("pointermove", { clientY: 40 }));
    window.dispatchEvent(new MouseEvent("pointerup"));
    await vi.waitFor(() => expect(invoke.mock.calls.some(([c]) => c === "task_rank")).toBe(true));
    expect(invoke.mock.calls.find(([c]) => c === "task_rank")![1]).toMatchObject({ line: 1, prev: 4096, next: null });
  });
});
