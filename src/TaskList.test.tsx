// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The task lists' keys, what a row leaves out, changing a task's words, failed loads, error toasts,
// and Draft reply's thread names.

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn(async (..._a: unknown[]): Promise<unknown> => ({ undo: "" })));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { TaskRow } from "./api";
import { threadLabel } from "./skills/Reply";
import { TaskList, TasksFailed } from "./TaskList";
import { retitled, retryTasks, tasksFailed } from "./taskModel";
import { toast, Toasts } from "./Toast";

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

afterEach(() => {
  cleanup();
  invoke.mockClear();
});

const rows = [t("One", { line: 1 }), t("Two", { line: 2 }), t("Three", { line: 3 })];
const calls = (cmd: string) => invoke.mock.calls.filter((c) => c[0] === cmd);

describe("task list keys", () => {
  it("↑ ↓ choose a row, ↩ opens it, Space ticks it, ⌫ deletes it", () => {
    const onSelect = vi.fn();
    const onOpen = vi.fn();
    const { container, rerender } = render(
      <TaskList rows={rows} manual={false} keys onSelect={onSelect} onOpen={onOpen} selected={null} />,
    );
    const ul = container.querySelector("ul")!;
    fireEvent.keyDown(ul, { key: "ArrowDown" });
    expect(onSelect).toHaveBeenLastCalledWith(rows[0]);
    rerender(<TaskList rows={rows} manual={false} keys onSelect={onSelect} onOpen={onOpen} selected="A.md:1" />);
    fireEvent.keyDown(ul, { key: "ArrowDown" });
    expect(onSelect).toHaveBeenLastCalledWith(rows[1]);
    fireEvent.keyDown(ul, { key: "Enter" });
    expect(onOpen).toHaveBeenCalledWith(rows[0]);
    fireEvent.keyDown(ul, { key: " " });
    expect(calls("task_toggle")).toHaveLength(1);
    fireEvent.keyDown(ul, { key: "Backspace" });
    expect(calls("task_replace")[0][1]).toMatchObject({ path: "A.md", line: 1, lines: [] });
    // The row below takes its place, a line up.
    expect(onSelect).toHaveBeenLastCalledWith(expect.objectContaining({ text: "Two", line: 1 }));
  });

  it("leaves the keys alone while typing, and without `keys`", () => {
    const onSelect = vi.fn();
    const { container } = render(
      <>
        <TaskList rows={rows} manual={false} keys onSelect={onSelect} selected="A.md:1" rowAction={() => <input aria-label="x" />} />
      </>,
    );
    fireEvent.keyDown(container.querySelector("input")!, { key: " " });
    fireEvent.keyDown(container.querySelector("input")!, { key: "Backspace" });
    expect(invoke).not.toHaveBeenCalled();
    cleanup();
    const plain = render(<TaskList rows={rows} manual={false} onSelect={onSelect} selected="A.md:1" />);
    expect(plain.container.querySelector("ul")!.tabIndex).toBe(-1);
  });
});

describe("selecting a row", () => {
  it("selects it on a click, and lets it go on a second", () => {
    const rows = [t("One"), t("Two", { line: 1 })];
    const onSelect = vi.fn();
    const onDeselect = vi.fn();
    render(<TaskList rows={rows} manual={false} selected="A.md:0" onSelect={onSelect} onDeselect={onDeselect} />);
    fireEvent.click(screen.getByText("Two"));
    expect(onSelect).toHaveBeenCalledWith(rows[1]);
    fireEvent.click(screen.getByText("One"));
    expect(onDeselect).toHaveBeenCalledOnce();
  });
});

describe("task rows", () => {
  it("leave out the note link for the project's own note, and the chip inside the project", () => {
    const own = t("Book the venue", {
      path: "Project. Orbit App launch.md",
      title: "Project. Orbit App launch",
      project: "Project. Orbit App launch.md",
    });
    const { container } = render(<TaskList rows={[own]} manual={false} />);
    expect(container.querySelector(".tlink")).toBeNull();
    expect(container.querySelector(".chip.proj")).not.toBeNull();
    cleanup();
    const inside = render(<TaskList rows={[own]} manual={false} hideProject />);
    expect(inside.container.querySelector(".chip.proj")).toBeNull();
    expect(inside.container.querySelector(".tlink")).not.toBeNull();
  });
});

describe("changing a task's words", () => {
  it("keeps its dates and tags, and refuses words it can't find once", () => {
    expect(retitled("- [ ] Call Maya 📅 2026-10-09 #work", "Call Maya", "Call Lena")).toBe("- [ ] Call Lena 📅 2026-10-09 #work");
    expect(retitled("- [ ] go go", "go", "x")).toBeNull();
    expect(retitled("- [ ] Call Maya", "Call Maya", "  ")).toBeNull();
    expect(retitled("> - [x] x marks it", "x marks it", "Done")).toBe("> - [x] Done");
  });
});

describe("a failed task load", () => {
  it("says so, with Retry", () => {
    act(() => tasksFailed.set("disk error"));
    render(<TasksFailed />);
    expect(screen.getByRole("alert").textContent).toContain("Couldn’t load the tasks");
    expect(screen.getByRole("button", { name: /Retry/ }).title).toBeTruthy();
    expect(() => retryTasks()).not.toThrow();
    act(() => tasksFailed.set(null));
  });
});

describe("error toasts", () => {
  it("stay, as an alert, with a close button", () => {
    vi.useFakeTimers();
    render(<Toasts />);
    act(() => toast("Couldn't save", undefined, "bad"));
    act(() => void vi.advanceTimersByTime(20_000));
    expect(screen.getByRole("alert").textContent).toContain("Couldn't save");
    fireEvent.click(screen.getByTitle("Close this message"));
    expect(screen.queryByRole("alert")).toBeNull();
    act(() => toast("Saved", undefined, "ok"));
    act(() => void vi.advanceTimersByTime(3000));
    expect(screen.queryByRole("status")).toBeNull();
    vi.useRealTimers();
  });
});

describe("Draft reply's thread names", () => {
  it("are the subject and the day, not the file name", () => {
    expect(
      threadLabel({
        path: "sources/Email. Thread. Re Orbit App launch date - 2026-10-02.md",
        title: "Email. Thread. Re Orbit App launch date - 2026-10-02",
        date: "2026-10-02",
      }),
    ).toBe("Re Orbit App launch date · Fri 2 Oct");
    expect(threadLabel({ path: "sources/Teams. Chat. Lena - 2026-10-01.md", title: "Teams. Chat. Lena - 2026-10-01", date: null })).toBe(
      "Teams: Lena",
    );
  });
});
