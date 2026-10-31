// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Include on-hold and someday projects (`includeOnHoldSomeday`) reaches every screen that shows Next
// actions: the sidebar's count and the weekly review's Next actions step (D-20261031-02).

import { act, cleanup, render, screen } from "@testing-library/react";
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
});
