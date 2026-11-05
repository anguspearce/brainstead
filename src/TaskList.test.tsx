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
const openUrl = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl, openPath: vi.fn() }));

import type { TaskRow } from "./api";
import { threadLabel } from "./skills/Reply";
import { TaskEditor, TaskList, TasksFailed, TaskText } from "./TaskList";
import { retitled, retryTasks, tasksFailed } from "./taskModel";
import { toast, Toasts } from "./Toast";
import { settings } from "./store";

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
  openUrl.mockClear();
  settings.update({ referenceLinks: [] });
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

describe("a task's words", () => {
  it("draw a Tasks-format emoji as its field's icon, never the emoji", () => {
    const { container } = render(<TaskText text="Book the venue 📅 2026-10-20 **soon**" />);
    expect(container.textContent).not.toContain("📅");
    expect(container.querySelector(".femoji")).toBeTruthy();
    expect(container.querySelector("b")?.textContent).toBe("soon");
  });
});

describe("links in a task's words", () => {
  const text = "Review [the spec](https://example.com/spec) and https://www.example.com/a/b, then ORBIT-12.";

  it("draw a [label](url) as its label and a bare URL as its domain, and open them in the browser", () => {
    const { container } = render(<TaskText text={text} />);
    const links = [...container.querySelectorAll<HTMLAnchorElement>("a.tlinkword")];
    expect(links.map((a) => a.textContent)).toEqual(["the spec", "example.com"]);
    expect(container.textContent).toBe("Review the spec and example.com, then ORBIT-12.");
    fireEvent.click(links[0]);
    expect(openUrl).toHaveBeenLastCalledWith("https://example.com/spec");
    fireEvent.click(links[1]);
    expect(openUrl).toHaveBeenLastCalledWith("https://www.example.com/a/b");
  });

  it("draw a configured key as a link, and leave one alone with no pattern", () => {
    expect(render(<TaskText text={text} />).container.textContent).toContain("ORBIT-12");
    cleanup();
    settings.update({ referenceLinks: [{ prefix: "ORBIT-", url: "https://tickets.example.com/browse/{key}" }] });
    const { container } = render(<TaskText text={text} />);
    const key = [...container.querySelectorAll<HTMLAnchorElement>("a.tlinkword")].find((a) => a.textContent === "ORBIT-12")!;
    expect(key.title).toBe("https://tickets.example.com/browse/ORBIT-12");
    fireEvent.click(key);
    expect(openUrl).toHaveBeenLastCalledWith("https://tickets.example.com/browse/ORBIT-12");
  });

  it("leave a key inside a link, code or a wikilink alone, and draw a link's bold and code with it", () => {
    settings.update({ referenceLinks: [{ prefix: "ORBIT-", url: "https://tickets.example.com/browse/{key}" }] });
    const { container } = render(
      <TaskText text="[ORBIT-1](Note.md) `ORBIT-2` [[Bug ORBIT-3]] [**Spec** and `cmd`](https://example.com/s) ![diagram ORBIT-4](a.png) **ORBIT-5**" />,
    );
    const links = [...container.querySelectorAll<HTMLAnchorElement>("a.tlinkword")];
    // One link for the labelled address, holding its bold and code, and one for the key in bold.
    expect(links.map((a) => a.textContent)).toEqual(["Spec and cmd", "ORBIT-5"]);
    expect(links[0].querySelector("b")?.textContent).toBe("Spec");
    expect(links[0].querySelector("code")?.textContent).toBe("cmd");
    expect(container.querySelector("b > a")?.textContent).toBe("ORBIT-5");
    // An image is its alt text, not a "!" and a link; a link to a note stays as written.
    expect(container.textContent).toContain(" diagram ORBIT-4 ");
    expect(container.textContent).not.toContain("!");
    expect(container.textContent).toContain("[ORBIT-1](Note.md)");
  });

  it("are text, not links, where the row is itself a button", () => {
    const { container } = render(<TaskText text={text} plain />);
    expect(container.querySelector("a")).toBeNull();
    expect(container.querySelector(".tlinkword.ext")?.textContent).toBe("the spec");
  });

  it("open on a click without selecting the row, and keep ↩ and Space from acting on it", () => {
    const row = t("Check [the spec](https://example.com/spec)", { line: 1 });
    const onSelect = vi.fn();
    const onOpen = vi.fn();
    const { container } = render(<TaskList rows={[row]} manual={false} keys onSelect={onSelect} onOpen={onOpen} selected="A.md:1" />);
    const a = container.querySelector("a")!;
    fireEvent.click(a);
    expect(openUrl).toHaveBeenCalledWith("https://example.com/spec");
    expect(onSelect).not.toHaveBeenCalled();
    fireEvent.keyDown(a, { key: "Enter" });
    fireEvent.keyDown(a, { key: " " });
    expect(onOpen).not.toHaveBeenCalled();
    expect(calls("task_toggle")).toHaveLength(0);
    // Off the link, the row's own keys still work.
    fireEvent.keyDown(container.querySelector("ul")!, { key: "Enter" });
    expect(onOpen).toHaveBeenCalledWith(row);
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

describe("the task pane's project", () => {
  it("has no Clear for a project that comes from its note's Project: line", async () => {
    invoke.mockImplementation(async (cmd: unknown) =>
      ["projects_list", "tasks_all", "tasks_query"].includes(String(cmd)) ? [] : { undo: "" },
    );
    const inherited = t("From the tracker", { path: "Tracker. Orbit.md", project: "Project. Orbit.md", projectFrom: "member" });
    const { rerender } = render(<TaskEditor t={inherited} onClose={() => {}} wide={false} />);
    await act(async () => {});
    expect(screen.queryByRole("button", { name: "Clear project" })).toBeNull();
    expect(screen.getByText("From its note's Project: line")).toBeTruthy();
    // A project its line links can still be cleared.
    rerender(<TaskEditor t={{ ...inherited, projectFrom: "line" }} onClose={() => {}} wide={false} />);
    expect(screen.getByRole("button", { name: "Clear project" })).toBeTruthy();
    expect(screen.queryByText("From its note's Project: line")).toBeNull();
    invoke.mockImplementation(async () => ({ undo: "" }));
  });
});

describe("the task editor's priority", () => {
  const pressed = (name: string) => screen.getByRole("button", { name }).getAttribute("aria-pressed");

  it("shows the priority the row's chip shows, wherever it is on the line", () => {
    for (const text of ["Call Sam ⏫ [effort:: 15m]", "⏫ Review the plan", "Call Sam ⏫"]) {
      render(<TaskEditor t={t(text)} onClose={() => {}} wide={false} />);
      expect([pressed("High"), pressed("None"), pressed("Low")], text).toEqual(["true", "false", "false"]);
      cleanup();
    }
    render(<TaskEditor t={t("Plain words")} onClose={() => {}} wide={false} />);
    expect([pressed("High"), pressed("None")]).toEqual(["false", "true"]);
  });
});
