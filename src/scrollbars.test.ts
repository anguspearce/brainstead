// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

/** What Windows answers to "always show scroll bars?". */
let systemAlways = true;
vi.mock("./api", () => ({ api: { alwaysShowScrollbars: async () => systemAlways } }));

import { setPlatform } from "./platform";
import { pointerAt, showScrollbarsWhileScrolling } from "./scrollbars";

/** A pane with more to show than fits (jsdom lays nothing out, so the sizes are given). */
function pane(scrollable: boolean): HTMLElement {
  const el = document.createElement("div");
  Object.defineProperty(el, "scrollHeight", { value: scrollable ? 900 : 100 });
  Object.defineProperty(el, "clientHeight", { value: 100 });
  return el;
}

describe("scrollbars shown while the pointer is over a pane (Windows)", () => {
  beforeAll(() => {
    setPlatform("windows");
    showScrollbarsWhileScrolling();
  });
  afterEach(() => {
    pointerAt([]);
    document.body.replaceChildren();
    vi.useRealTimers();
  });

  it("marks only the panes that scroll, and unmarks the ones the pointer leaves", () => {
    const list = pane(true);
    const row = pane(false);
    list.append(row);
    const other = pane(true);
    document.body.append(list, other);
    pointerAt([row, list, document.body]);
    expect(list.hasAttribute("data-pointer-in")).toBe(true);
    // A row, like an editor line, has nothing to scroll and is left alone.
    expect(row.hasAttribute("data-pointer-in")).toBe(false);
    pointerAt([other, document.body]);
    expect(list.hasAttribute("data-pointer-in")).toBe(false);
    expect(other.hasAttribute("data-pointer-in")).toBe(true);
  });

  it("keeps the mark while the pointer crosses elements inside the pane, and drops it when it leaves the window", () => {
    const list = pane(true);
    const row = pane(false);
    list.append(row);
    document.body.append(list);
    row.dispatchEvent(new Event("pointerover", { bubbles: true }));
    expect(list.hasAttribute("data-pointer-in")).toBe(true);
    // Leaving one row for the next: pointerleave on the row must not clear the pane.
    row.dispatchEvent(new Event("pointerleave"));
    expect(list.hasAttribute("data-pointer-in")).toBe(true);
    document.documentElement.dispatchEvent(new Event("pointerleave"));
    expect(list.hasAttribute("data-pointer-in")).toBe(false);
  });

  it("shows a pane's bar while it scrolls and for a second after", () => {
    vi.useFakeTimers();
    const list = pane(true);
    document.body.append(list);
    list.dispatchEvent(new Event("scroll"));
    expect(list.hasAttribute("data-scrolling")).toBe(true);
    vi.advanceTimersByTime(600);
    // Scrolling again starts the second over.
    list.dispatchEvent(new Event("scroll"));
    vi.advanceTimersByTime(999);
    expect(list.hasAttribute("data-scrolling")).toBe(true);
    vi.advanceTimersByTime(1);
    expect(list.hasAttribute("data-scrolling")).toBe(false);
  });

  it("shows every scrollbar while the system shows them all the time, read again when the window takes the focus", async () => {
    const root = document.documentElement;
    // Started with Windows' Always show scrollbars on.
    await vi.waitFor(() => expect(root.getAttribute("data-scrollbars")).toBe("always"));
    // Turned off in Windows' settings, then back to the app.
    systemAlways = false;
    window.dispatchEvent(new Event("focus"));
    await vi.waitFor(() => expect(root.hasAttribute("data-scrollbars")).toBe(false));
  });
});

describe("scrollbars off Windows", () => {
  it("leaves the native ones alone: no listeners, no system read", async () => {
    vi.resetModules();
    const { setPlatform: set } = await import("./platform");
    set("macos");
    const add = vi.spyOn(document, "addEventListener");
    const fresh = await import("./scrollbars");
    fresh.showScrollbarsWhileScrolling();
    expect(add).not.toHaveBeenCalled();
    add.mockRestore();
    set("windows");
  });
});
