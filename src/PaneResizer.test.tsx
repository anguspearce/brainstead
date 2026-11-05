// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined), convertFileSrc: (p: string) => p }));

import { usePanes } from "./PaneResizer";
import { MIN_FLEX, PANES } from "./panes";
import { DEFAULT_SETTINGS } from "./api";
import { settings } from "./store";

// jsdom lays nothing out: the table is 1000px wide, and pointer capture is a no-op.
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 1000 });
  Element.prototype.setPointerCapture = () => {};
});
beforeEach(() => settings.loadFrom({ ...DEFAULT_SETTINGS }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/** Activity's log: When before the flexible Item column, Action after it. */
function Log() {
  const cols = usePanes({ "activity.when": PANES["activity.when"], "activity.action": PANES["activity.action"] }, { mode: "column" });
  return (
    <div data-testid="log" {...cols.root}>
      <span>When{cols.resizer("activity.when")}</span>
      <span>Item</span>
      <span>Action{cols.resizer("activity.action")}</span>
    </div>
  );
}

const divider = (name: string) => screen.getByRole("separator", { name: `Resize the ${name} column` });
const drag = (el: HTMLElement, dx: number) => {
  fireEvent.pointerDown(el, { button: 0, clientX: 500, pointerId: 1 });
  fireEvent.pointerMove(el, { clientX: 500 + dx, pointerId: 1 });
  fireEvent.pointerUp(el, { clientX: 500 + dx, pointerId: 1 });
};
const stored = () => settings.get().paneWidths ?? {};

describe("a table's column dividers", () => {
  it("sit on the edge away from the flexible column, so each follows the pointer", () => {
    render(<Log />);
    expect(divider("When").dataset.edge).toBe("end");
    expect(divider("Action").dataset.edge).toBe("start");
    // When is left of Item: its right edge moves right with the pointer, so it widens.
    act(() => drag(divider("When"), 40));
    expect(stored()["activity.when"]).toBe(PANES["activity.when"].def + 40);
    // Action is right of Item, stacked against the table's right edge: its left edge moves right
    // with the pointer, so it narrows, and widens as it's dragged left.
    act(() => drag(divider("Action"), 30));
    expect(stored()["activity.action"]).toBe(PANES["activity.action"].def - 30);
    act(() => drag(divider("Action"), -50));
    expect(stored()["activity.action"]).toBe(PANES["activity.action"].def - 30 + 50);
    expect(screen.getByTestId("log").style.getPropertyValue("--col-action")).toBe(`${PANES["activity.action"].def + 20}px`);
  });

  it("are one per column, each named for its own", () => {
    render(<Log />);
    expect(screen.getAllByRole("separator").map((s) => s.getAttribute("aria-label"))).toEqual([
      "Resize the When column",
      "Resize the Action column",
    ]);
  });
});

/** Tasks: the lists on the left, the task's pane on the right, the flexible list between. */
function Tasks({ grid }: { grid: string }) {
  const panes = usePanes({ "tasks.left": PANES["tasks.left"], "tasks.side": PANES["tasks.side"] });
  return (
    <div data-testid="tasks" {...panes.root} style={{ ...panes.root.style, display: "grid", columnGap: "20px", gridTemplateColumns: grid }}>
      {panes.resizers}
    </div>
  );
}

describe("a screen's pane dividers", () => {
  it("take the grid's gaps out of the room, one beside each fixed pane", () => {
    render(<Tasks grid="260px 400px 340px" />);
    const side = screen.getByRole("separator", { name: "Resize the task's pane" });
    // 1000px less the lists' 260, two 20px gaps and the flexible pane's 240 leaves the side 460.
    expect(side.getAttribute("aria-valuemax")).toBe(String(Math.min(PANES["tasks.side"].max, 1000 - 260 - 40 - MIN_FLEX)));
    act(() => drag(side, -400));
    expect(stored()["tasks.side"]).toBe(1000 - 260 - 40 - MIN_FLEX);
  });
});

describe("a divider's value", () => {
  it("is the width shown: the stylesheet's at its breakpoint, then the one set", () => {
    // The stylesheet narrows the lists to 220px at a breakpoint; nothing is stored.
    const sheet = document.head.appendChild(document.createElement("style"));
    sheet.textContent = '[data-testid="tasks"] { --pane-left: 220px; }';
    render(<Tasks grid="220px 400px 340px" />);
    const lists = screen.getByRole("separator", { name: "Resize the task lists" });
    expect(lists.getAttribute("aria-valuenow")).toBe("220");
    fireEvent.keyDown(lists, { key: "ArrowRight" });
    expect(stored()["tasks.left"]).toBe(228);
    expect(lists.getAttribute("aria-valuenow")).toBe("228");
    sheet.remove();
  });
});

describe("a scrolling screen's divider", () => {
  it("stays in view, one view tall, as the screen scrolls", () => {
    render(<Tasks grid="260px 400px 340px" />);
    const root = screen.getByTestId("tasks");
    const side = screen.getByRole("separator", { name: "Resize the task's pane" });
    expect(side.style.top).toBe("0px");
    Object.defineProperty(root, "scrollTop", { configurable: true, get: () => 300 });
    fireEvent.scroll(root);
    expect([side.style.top, side.style.bottom]).toEqual(["300px", "-300px"]);
  });
});

describe("a size change", () => {
  it("moves the dividers' room without re-rendering the screen, unless what the screen draws changes", () => {
    let width = 1000;
    let resized = () => {};
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(() => width);
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(cb: () => void) {
          resized = cb;
        }
        observe() {}
        disconnect() {}
      },
    );
    let renders = 0;
    function Host() {
      renders++;
      const panes = usePanes({ "tasks.left": PANES["tasks.left"], "tasks.side": PANES["tasks.side"] });
      return (
        <div {...panes.root} style={{ ...panes.root.style, display: "grid", columnGap: "20px", gridTemplateColumns: "260px 400px 340px" }}>
          {panes.resizers}
        </div>
      );
    }
    render(<Host />);
    const side = screen.getByRole("separator", { name: "Resize the task's pane" });
    const before = renders;
    // A window resize, a frame at a time: the room follows, the screen isn't drawn again.
    for (const w of [1010, 1020, 1030]) {
      width = w;
      act(() => resized());
    }
    expect(renders).toBe(before);
    expect(side.getAttribute("aria-valuemax")).toBe(String(1030 - 260 - 40 - MIN_FLEX));
    // Too narrow for the defaults: the lists are narrowed on the root, so the screen draws again.
    width = 700;
    act(() => resized());
    expect(renders).toBeGreaterThan(before);
    expect(screen.getAllByRole("separator")[0].getAttribute("aria-valuenow")).not.toBe("260");
    vi.unstubAllGlobals();
  });
});

/** Projects: the list on the left, the project beside it taking the rest. */
function Projects({ grid, minFlex }: { grid: string; minFlex?: number }) {
  const panes = usePanes({ "projects.list": PANES["projects.list"] }, { minFlex });
  return (
    <div
      data-testid="projects"
      {...panes.root}
      style={{ ...panes.root.style, display: "grid", columnGap: "20px", gridTemplateColumns: grid }}
    >
      {panes.resizers}
    </div>
  );
}

describe("a stored pane wider than the main area", () => {
  it("is fitted to the room, its divider kept, though the grid first laid the flexible pane out at 0px", () => {
    // Projects with its list kept at 720px in a 640px main area (a 1040px window, the sidebar at
    // 400): the grid's first measure is "720px 0px", the project squeezed to nothing. That column is
    // still a track, so the list is no pane a breakpoint dropped: it gives way to the reserve.
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(() => 640);
    settings.update({ paneWidths: { "projects.list": 720 } });
    render(<Projects grid="720px 0px" />);
    const list = screen.getByRole("separator", { name: "Resize the project list" });
    expect(list.getAttribute("aria-valuenow")).toBe(String(640 - 20 - MIN_FLEX));
    expect(screen.getByTestId("projects").style.getPropertyValue("--pane-list")).toBe(`${640 - 20 - MIN_FLEX}px`);
    // The width kept is untouched: a wider window shows it again.
    expect(stored()["projects.list"]).toBe(720);
  });

  it("leaves the flexible pane the screen's own reserve where it names one", () => {
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(() => 640);
    settings.update({ paneWidths: { "projects.list": 720 } });
    render(<Projects grid="720px 0px" minFlex={320} />);
    expect(screen.getByTestId("projects").style.getPropertyValue("--pane-list")).toBe(`${640 - 20 - 320}px`);
  });

  it("on a screen with two panes keeps both dividers and narrows both", () => {
    // Tasks with its lists kept at 480 and the task's pane at 640 in a 1000px main area: the first
    // measure is "480px 0px 640px"; both panes give way, in proportion, to leave the list MIN_FLEX.
    settings.update({ paneWidths: { "tasks.left": 480, "tasks.side": 640 } });
    render(<Tasks grid="480px 0px 640px" />);
    const [lists, side] = screen.getAllByRole("separator");
    expect([lists, side].map((s) => s.getAttribute("aria-label"))).toEqual(["Resize the task lists", "Resize the task's pane"]);
    const left = Number(lists.getAttribute("aria-valuenow"));
    const right = Number(side.getAttribute("aria-valuenow"));
    expect(left).toBeLessThan(480);
    expect(right).toBeLessThan(640);
    expect(left).toBeGreaterThanOrEqual(PANES["tasks.left"].min);
    expect(right).toBeGreaterThanOrEqual(PANES["tasks.side"].min);
    expect(left + right).toBeLessThanOrEqual(1000 - 40 - MIN_FLEX);
  });
});

describe("a breakpoint", () => {
  it("that drops one pane's column hides that pane's divider alone, and one that stacks the grid hides both", () => {
    // The Weekly review at 868px: the steps beside the step, the side column gone.
    const { unmount } = render(<Tasks grid="260px 600px" />);
    expect(screen.getAllByRole("separator").map((s) => s.getAttribute("aria-label"))).toEqual(["Resize the task lists"]);
    unmount();
    render(<Tasks grid="1000px" />);
    expect(screen.queryAllByRole("separator")).toEqual([]);
  });

  it("leaves the room of a pane it drops to the panes still shown", () => {
    // The Weekly review in a 768px main area, its side column dropped: the steps may take all but
    // the step's least, as if the side column weren't there.
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(768);
    function Weekly() {
      const panes = usePanes({ "weekly.left": PANES["weekly.left"], "weekly.side": PANES["weekly.side"] });
      return (
        <div data-testid="weekly" {...panes.root} style={{ ...panes.root.style, display: "grid", gridTemplateColumns: "260px 508px" }}>
          {panes.resizers}
        </div>
      );
    }
    render(<Weekly />);
    const steps = screen.getByRole("separator", { name: "Resize the review's steps" });
    expect(steps.getAttribute("aria-valuemax")).toBe(String(Math.min(PANES["weekly.left"].max, 768 - MIN_FLEX)));
    act(() => drag(steps, 140));
    expect(stored()["weekly.left"]).toBe(400);
    expect(screen.getByTestId("weekly").style.getPropertyValue("--pane-left")).toBe("400px");
  });
});

describe("a divider's keys and locks", () => {
  it("moves with the arrows the way they point, Shift further", () => {
    render(<Log />);
    fireEvent.keyDown(divider("When"), { key: "ArrowRight" });
    expect(stored()["activity.when"]).toBe(PANES["activity.when"].def + 8);
    fireEvent.keyDown(divider("When"), { key: "ArrowLeft", shiftKey: true });
    expect(stored()["activity.when"]).toBe(PANES["activity.when"].def + 8 - 40);
    // Action's divider is its left edge: → moves it right, so Action narrows.
    fireEvent.keyDown(divider("Action"), { key: "ArrowRight" });
    expect(stored()["activity.action"]).toBe(PANES["activity.action"].def - 8);
  });

  it("goes to the column's least on Home and its most on End", () => {
    render(<Log />);
    fireEvent.keyDown(divider("When"), { key: "Home" });
    expect(stored()["activity.when"]).toBe(PANES["activity.when"].min);
    fireEvent.keyDown(divider("When"), { key: "End" });
    expect(stored()["activity.when"]).toBe(PANES["activity.when"].max);
  });

  it("are reached by Tab left to right", () => {
    // Tasks lists its side pane's divider first; the lists' is drawn left of it, so comes first.
    function Backwards() {
      const panes = usePanes({ "tasks.side": PANES["tasks.side"], "tasks.left": PANES["tasks.left"] });
      return <div {...panes.root}>{panes.resizers}</div>;
    }
    render(<Backwards />);
    expect(screen.getAllByRole("separator").map((s) => s.getAttribute("aria-label"))).toEqual([
      "Resize the task lists",
      "Resize the task's pane",
    ]);
  });

  it("puts the column back on Enter or a double-click, and keeps Enter from the screen's own keys", () => {
    const screenKeys = vi.fn();
    window.addEventListener("keydown", screenKeys);
    settings.update({ paneWidths: { "activity.when": 200, "activity.action": 200 } });
    render(<Log />);
    fireEvent.keyDown(divider("When"), { key: "Enter" });
    expect(stored()).toEqual({ "activity.action": 200 });
    expect(screenKeys).not.toHaveBeenCalled();
    act(() => void fireEvent.doubleClick(divider("Action")));
    expect(settings.get().paneWidths).toBeUndefined();
    expect(screen.getByTestId("log").style.getPropertyValue("--col-action")).toBe("");
    window.removeEventListener("keydown", screenKeys);
  });

  it("keeps a drag within the room the table has, its flexible column keeping its least", () => {
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(600);
    render(<Log />);
    // 600px less Action's 120 and the flexible column's 240 leaves When 240, under its 320 maximum.
    act(() => drag(divider("When"), 200));
    expect(stored()["activity.when"]).toBe(600 - PANES["activity.action"].def - MIN_FLEX);
    vi.restoreAllMocks();
  });

  it("leaves resize mode when its divider goes mid-drag", () => {
    render(<Log />);
    fireEvent.pointerDown(divider("When"), { button: 0, clientX: 500, pointerId: 1 });
    fireEvent.pointerMove(divider("When"), { clientX: 560, pointerId: 1 });
    expect(document.body.classList.contains("presizing")).toBe(true);
    expect(screen.getByTestId("log").style.getPropertyValue("--col-when")).toBe(`${PANES["activity.when"].def + 60}px`);
    // An assistant locks the columns while the pointer is still down: the width dragged, never
    // kept, goes with the divider.
    act(() => settings.update({ resizeLocks: { columns: true } }));
    expect(screen.queryAllByRole("separator")).toEqual([]);
    expect(document.body.classList.contains("presizing")).toBe(false);
    expect(screen.getByTestId("log").style.getPropertyValue("--col-when")).toBe("");
    expect(settings.get().paneWidths).toBeUndefined();
  });

  it("draws no dividers for a locked type, and still shows the widths kept", () => {
    settings.update({ paneWidths: { "activity.when": 200 }, resizeLocks: { columns: true } });
    render(<Log />);
    expect(screen.queryAllByRole("separator")).toEqual([]);
    expect(screen.getByTestId("log").style.getPropertyValue("--col-when")).toBe("200px");
    // The panes lock is another type's: the columns' dividers come back.
    act(() => settings.update({ resizeLocks: { panes: true } }));
    expect(screen.getAllByRole("separator")).toHaveLength(2);
  });
});
