// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The weekly review's prepared suggestions: their rows, accepting one and skipping one.

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const task = { path: "Me. To Do List.md", line: 3, lineText: "- [ ] Book the venue", text: "Book the venue" };
const invoke = vi.hoisted(() =>
  vi.fn(async (cmd: string, ..._a: unknown[]): Promise<unknown> => {
    if (cmd === "tasks_notes") return [{ ...task, tags: [], done: false }];
    return { line: 9, lineText: "", undo: `Did ${cmd}` };
  }),
);
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { ProjectRow, WeekPrepStatus, WeekPrepSuggestion } from "./api";
import { Toasts } from "./Toast";
import { PrepHeader, PrepRows } from "./weeklyPrep";

const project = { path: "Project. Orbit App launch.md", name: "Orbit App launch", status: "active" } as unknown as ProjectRow;

const add: WeekPrepSuggestion = {
  id: "a1",
  step: "back",
  text: "You said you'd send Lena the rota.",
  source: { path: "Meeting. Rota - 2026-10-01.md", quote: "I'll send Lena the rota" },
  action: { do: "add", text: "Send Lena the rota", project: "Orbit App launch", context: "computer", tags: [], due: null, scheduled: null },
};
const tick: WeekPrepSuggestion = {
  id: "t1",
  step: "next",
  text: "The venue was booked on Tuesday.",
  source: null,
  action: { do: "tick", task },
};
const defer: WeekPrepSuggestion = {
  id: "d1",
  step: "next",
  text: "Untouched for six weeks.",
  source: null,
  action: { do: "defer", task, date: "2026-10-30" },
};
const prompt: WeekPrepSuggestion = { id: "p1", step: "loose", text: "Anything else from Thursday's offsite?", source: null, action: null };

afterEach(() => {
  cleanup();
  invoke.mockClear();
});

function rows(list: WeekPrepSuggestion[], step: WeekPrepSuggestion["step"] = "back", handled = {}) {
  const onHandle = vi.fn();
  render(
    <>
      <PrepRows step={step} rows={list} handled={handled} onHandle={onHandle} week="2026-W40" projects={[project]} />
      <Toasts />
    </>,
  );
  return onHandle;
}

describe("the prepared suggestions", () => {
  it("shows each with its source, the action's verb and Skip", () => {
    rows([add, tick, defer], "next");
    expect(screen.getByText("You said you'd send Lena the rota.")).toBeTruthy();
    const src = screen.getByRole("button", { name: "Meeting. Rota - 2026-10-01" });
    expect(src.getAttribute("title")).toContain("I'll send Lena the rota");
    expect(screen.getByText(/→ Send Lena the rota · Orbit App launch/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Add task" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Tick" })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^Defer to / }).getAttribute("title")).toContain("2026-10-30");
    expect(screen.getAllByRole("button", { name: "Skip" })).toHaveLength(3);
    for (const b of screen.getAllByRole("button")) expect(b.getAttribute("title")).toBeTruthy();
  });

  it("shows prompts as a list without buttons", () => {
    rows([prompt], "loose");
    expect(screen.getByText(/Anything else from Thursday/)).toBeTruthy();
    expect(screen.queryAllByRole("button")).toHaveLength(0);
  });

  it("adds a task to its project on Add task, says so with Undo, and remembers it", async () => {
    const onHandle = rows([add]);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Add task" }));
    });
    await vi.waitFor(() => expect(onHandle).toHaveBeenCalled());
    const call = invoke.mock.calls.find((c) => c[0] === "project_add_task")!;
    expect(call[1]).toMatchObject({ path: "Project. Orbit App launch.md", heading: "Next actions" });
    expect((call[1] as { text: string }).text).toContain("Send Lena the rota");
    expect((call[1] as { text: string }).text).toContain("#context/computer");
    expect(onHandle).toHaveBeenCalledWith("a1", "accepted", "Did project_add_task");
    expect(screen.getByText("Did project_add_task")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Undo/ })).toBeTruthy();
  });

  it("adds one with no project under Other on the To Do list", async () => {
    const loose = { ...add, action: { ...add.action!, project: null } } as WeekPrepSuggestion;
    const onHandle = rows([loose]);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Add task" }));
    });
    await vi.waitFor(() => expect(onHandle).toHaveBeenCalled());
    expect(invoke.mock.calls.find((c) => c[0] === "task_add")?.[1]).toMatchObject({ text: "Send Lena the rota #context/computer" });
  });

  it("ticks the task as it is now", async () => {
    const onHandle = rows([tick], "next");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Tick" }));
    });
    await vi.waitFor(() => expect(onHandle).toHaveBeenCalledWith("t1", "accepted", "Did task_toggle"));
    const call = invoke.mock.calls.find((c) => c[0] === "task_toggle")!;
    expect(call[1]).toMatchObject({ done: true });
    expect(JSON.stringify(call[1])).toContain('"line":3');
  });

  it("remembers a skip without doing anything", () => {
    const onHandle = rows([add]);
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(onHandle).toHaveBeenCalledWith("a1", "skipped", "");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("hides the ones dealt with", () => {
    rows([add, tick], "next", { a1: "accepted" });
    expect(screen.queryByRole("button", { name: "Add task" })).toBeNull();
    expect(screen.getByRole("button", { name: "Tick" })).toBeTruthy();
    expect(screen.getByText(/1 dealt with/)).toBeTruthy();
  });
});

describe("the preparation's line", () => {
  const status = (s: Partial<WeekPrepStatus>): WeekPrepStatus => ({ prep: null, running: false, error: null, ...s });

  it("is empty when nothing was prepared", () => {
    const { container } = render(<PrepHeader status={status({})} week="2026-W40" />);
    expect(container.textContent).toBe("");
  });

  it("offers Retry after a failure, and Prepare again runs the week", async () => {
    render(<PrepHeader status={status({ error: "No assistant answered." })} week="2026-W40" />);
    expect(screen.getByText(/No assistant answered/)).toBeTruthy();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    });
    expect(invoke).toHaveBeenCalledWith("weekprep_run", { week: "2026-W40", unattended: false });
  });

  it("shows a bar while it runs", () => {
    render(<PrepHeader status={status({ running: true })} week="2026-W40" />);
    expect(screen.getByRole("progressbar")).toBeTruthy();
  });
});
