// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

import { HelpDrawer, helpOpen, openHelp } from "./HelpDrawer";

afterEach(() => {
  cleanup();
  helpOpen.set(null);
});

describe("the help drawer's sections", () => {
  it("open one at a time: opening one closes the one that was open", () => {
    act(() => helpOpen.set({ topic: "tasks" }));
    const { container } = render(<HelpDrawer />);
    const secs = [...container.querySelectorAll<HTMLDetailsElement>("details.help-sec")];
    expect(secs.length).toBeGreaterThan(2);
    // They share a group, so the web view closes the others itself.
    expect(new Set(secs.map((d) => d.getAttribute("name"))).size).toBe(1);
    act(() => {
      secs[1].open = true;
      secs[1].dispatchEvent(new Event("toggle"));
    });
    act(() => {
      secs[2].open = true;
      secs[2].dispatchEvent(new Event("toggle"));
    });
    expect(secs.filter((d) => d.open)).toEqual([secs[2]]);
  });
});

describe("the help drawer's top", () => {
  it("has Getting started and a topic picker, and nothing below the topic", () => {
    act(() => helpOpen.set({ topic: "tasks" }));
    const { container, getByText } = render(<HelpDrawer />);
    expect(container.querySelector(".help-nav .spick")?.textContent).toContain("Tasks");
    expect(container.querySelector(".help-guides")).toBeNull();
    expect(container.querySelector(".chip")).toBeNull();
    act(() => getByText("Getting started").click());
    expect(helpOpen.get()).toEqual({ start: true });
    expect(container.querySelectorAll(".help-guide").length).toBeGreaterThan(6);
  });

  it("opens the keyboard topic on the screen's keys", () => {
    act(() => helpOpen.set({ topic: "keyboard" }));
    const { container } = render(<HelpDrawer />);
    const open = [...container.querySelectorAll<HTMLDetailsElement>("details.help-sec")].filter((d) => d.open);
    // Today's keys are the task lists'.
    expect(open.map((d) => d.querySelector("summary")?.textContent)).toEqual(["Task lists"]);
  });
});

describe("the help drawer's back and forward", () => {
  it("go through the pages it showed since it opened", () => {
    act(() => {
      openHelp({});
      openHelp({ topic: "tasks" });
      openHelp({ start: true });
    });
    const { getByLabelText } = render(<HelpDrawer />);
    const back = getByLabelText("Back") as HTMLButtonElement;
    const fwd = getByLabelText("Forward") as HTMLButtonElement;
    expect(fwd.disabled).toBe(true);
    act(() => back.click());
    expect(helpOpen.get()).toEqual({ topic: "tasks" });
    act(() => back.click());
    expect(helpOpen.get()).toEqual({});
    expect(back.disabled).toBe(true);
    act(() => fwd.click());
    expect(helpOpen.get()).toEqual({ topic: "tasks" });
    // A new page drops what was ahead.
    act(() => openHelp({ topic: "inbox" }));
    expect(fwd.disabled).toBe(true);
  });
});
