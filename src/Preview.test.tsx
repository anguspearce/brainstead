// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The backend's answers, by command; what isn't here answers null.
const answers: Record<string, (a: Record<string, unknown> | undefined) => unknown> = {};
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ onScaleChanged: async () => () => {} }) }));

import { coveredBy, frameOf, holesIn, isLookKey, nextPreviewId, PreviewHost, previewView, quickLook } from "./Preview";
import { setPlatform } from "./platform";
import { keepOpen } from "./ui";

beforeEach(() => {
  invoke.mockImplementation(async (cmd: string, a?: Record<string, unknown>) => {
    const f = answers[cmd];
    return f ? f(a) : null;
  });
});

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
  vi.unstubAllGlobals();
  previewView.set(null);
  for (const k of Object.keys(answers)) delete answers[k];
});

const space = (target: Element, init: KeyboardEventInit = {}) => {
  const e = new KeyboardEvent("keydown", { key: " ", bubbles: true, cancelable: true, ...init });
  Object.defineProperty(e, "target", { value: target });
  return e;
};

describe("frameOf", () => {
  const body = (b: Partial<DOMRect>) => {
    const panel = document.createElement("div");
    panel.style.borderBottomRightRadius = "14px";
    panel.style.borderBottom = "1px solid";
    const el = panel.appendChild(document.createElement("div"));
    el.style.backgroundColor = "rgb(255, 254, 250)";
    el.style.color = "rgb(16, 24, 32)";
    el.getBoundingClientRect = () => ({ left: 0, top: 0, width: 0, height: 0, ...b }) as DOMRect;
    return el;
  };

  it("gives the body in the window's physical pixels, rounded, with its colours and the panel's inner corner", () => {
    vi.stubGlobal("devicePixelRatio", 1.5);
    expect(frameOf(body({ left: 16.2, top: 101, width: 800, height: 500.5 }), 7)).toEqual({
      x: 24,
      y: 152,
      w: 1200,
      h: 751,
      bg: "#fffefa",
      fg: "#101820",
      radius: 20,
      id: 7,
      hidden: false,
      holes: [],
    });
  });

  it("numbers each panel, so a close from one can never be taken for the next", () => {
    const a = nextPreviewId();
    const b = nextPreviewId();
    expect(b).toBeGreaterThan(a);
    expect(a).toBeGreaterThanOrEqual(Date.now() - 1000);
  });

  it("has no frame for a body with no room", () => {
    expect(frameOf(body({ width: 0, height: 300 }))).toBeNull();
    expect(frameOf(null)).toBeNull();
  });
});

describe("what the page shows over the panel", () => {
  const rect = (left: number, top: number, width: number, height: number) =>
    ({ left, top, width, height, right: left + width, bottom: top + height }) as DOMRect;

  it("hides the native window for a dialog of the page's own, never for the panel itself", () => {
    const own = document.body.appendChild(document.createElement("div"));
    own.className = "scrim";
    own.appendChild(document.createElement("div")).setAttribute("role", "dialog");
    expect(coveredBy(own)).toBe(false);
    const quit = document.body.appendChild(document.createElement("div"));
    quit.className = "scrim";
    expect(coveredBy(own)).toBe(true);
    quit.remove();
    expect(coveredBy(own)).toBe(false);
  });

  it("cuts the native window round a message over the body, in physical pixels from its corner", () => {
    vi.stubGlobal("devicePixelRatio", 2);
    const body = document.body.appendChild(document.createElement("div"));
    body.getBoundingClientRect = () => rect(10, 50, 800, 600);
    const toast = document.body.appendChild(document.createElement("div"));
    toast.className = "toast";
    toast.getBoundingClientRect = () => rect(600, 590, 200, 40);
    expect(holesIn(body, null)).toEqual([[1180, 1080, 400, 80]]);
    toast.getBoundingClientRect = () => rect(900, 700, 100, 30);
    expect(holesIn(body, null)).toEqual([]);
    // A button's tooltip in the bar above, which reaches down over the body.
    const tip = document.body.appendChild(document.createElement("div"));
    tip.className = "tip";
    tip.getBoundingClientRect = () => rect(700, 40, 120, 26);
    expect(holesIn(body, null)).toEqual([[1380, -20, 240, 52]]);
  });
});

describe("quickLook", () => {
  const req = () => ({ name: "Orbit App review.docx", look: vi.fn(async () => true), openInApp: vi.fn(async () => {}) });
  afterAll(() => setPlatform("linux"));

  it("shows Quick Look on a Mac, never the panel", async () => {
    setPlatform("macos");
    const r = req();
    await quickLook(r);
    expect(r.look).toHaveBeenCalledWith(null);
    expect(previewView.get()).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("preview_available", expect.anything());
  });

  it("opens the panel on Windows for a type with a preview, else the file's own app", async () => {
    setPlatform("windows");
    answers.preview_available = (a) => String(a?.name).endsWith(".docx");
    const r = req();
    await quickLook(r);
    expect(previewView.get()).toBe(r);
    expect(r.look).not.toHaveBeenCalled();
    previewView.set(null);
    const zip = { ...req(), name: "Orbit App.zip" };
    await quickLook(zip);
    expect(zip.look).toHaveBeenCalledWith(null);
    expect(previewView.get()).toBeNull();
    answers.preview_available = () => {
      throw new Error("no answer");
    };
    const r2 = req();
    await quickLook(r2);
    expect(r2.look).toHaveBeenCalledWith(null);
  });
});

describe("the panel on Windows", () => {
  const css = readFileSync("src/styles.css", "utf8");
  beforeEach(() => {
    setPlatform("windows");
    // jsdom lays nothing out: every box has room.
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      left: 20,
      top: 100,
      width: 800,
      height: 500,
      right: 820,
      bottom: 600,
    } as DOMRect);
  });
  afterEach(() => vi.restoreAllMocks());
  afterAll(() => setPlatform("linux"));

  // jsdom lays nothing out, so this reads the rule itself; the app's own window shows the result.
  it("opens below the title bar, whose height is the top bar's, and leaves the bar uncovered", () => {
    expect(css).toMatch(/html\[data-platform="windows"\] \.scrim\.winprevscrim \{\s*top: var\(--top-h\);\s*padding-top: 0;/);
    expect(css).toMatch(/\.winctl button \{[^}]*height: calc\(var\(--top-h\) - 1px\);/);
  });

  it("stays open for a drag of the title bar and the window's own buttons, not for the bar's other buttons", () => {
    const bar = document.body.appendChild(document.createElement("div"));
    bar.setAttribute("data-tauri-drag-region", "true");
    const back = bar.appendChild(document.createElement("button"));
    const ctl = bar.appendChild(document.createElement("div"));
    ctl.className = "winctl";
    const min = ctl.appendChild(document.createElement("button"));
    expect(keepOpen(bar)).toBe(true);
    expect(keepOpen(min)).toBe(true);
    expect(keepOpen(back)).toBe(false);
  });

  it("draws the file at the body's frame, closes on Space and tells the window which panel closed", async () => {
    const look = vi.fn(async () => true);
    previewView.set({ name: "Orbit App review.docx", look, openInApp: async () => {} });
    const { container } = render(<PreviewHost />);
    expect(document.querySelector(".scrim.winprevscrim")).toBeTruthy();
    await waitFor(() => expect(look).toHaveBeenCalled());
    const frame = (look.mock.calls[0] as unknown[])[0] as { id: number; w: number };
    expect(frame.w).toBe(800);
    await act(async () => {
      fireEvent.keyDown(window, { key: " " });
    });
    expect(previewView.get()).toBeNull();
    expect(invoke).toHaveBeenCalledWith("preview_close", { id: frame.id });
    expect(container.ownerDocument.querySelector(".winprev")).toBeNull();
  });

  it("closes when the file opened in its own app instead, and stays open on the window's own buttons", async () => {
    const ctl = document.body.appendChild(document.createElement("div"));
    ctl.className = "winctl";
    const min = ctl.appendChild(document.createElement("button"));
    previewView.set({ name: "Orbit App review.docx", look: async () => true, openInApp: async () => {} });
    render(<PreviewHost />);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 5));
    });
    fireEvent.mouseDown(min);
    expect(previewView.get()).not.toBeNull();
    cleanup();
    previewView.set({ name: "Orbit App.pdf", look: async () => false, openInApp: async () => {} });
    render(<PreviewHost />);
    await waitFor(() => expect(previewView.get()).toBeNull());
  });
});

describe("isLookKey", () => {
  beforeEach(() => setPlatform("windows"));
  afterAll(() => setPlatform("linux"));

  it("on a Mac, leaves Space to any focused button, as before, however the focus got there", () => {
    setPlatform("macos");
    const b = document.body.appendChild(document.createElement("button"));
    b.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    b.focus();
    expect(isLookKey(space(b), "trash")).toBe(false);
    expect(isLookKey(space(b), "source")).toBe(false);
    expect(isLookKey(space(document.body), "trash")).toBe(true);
    // The Trash's rule from before: no check for modifiers or a dialog; the source view's has both.
    document.body.appendChild(document.createElement("div")).setAttribute("role", "dialog");
    expect(isLookKey(space(document.body, { shiftKey: true }), "trash")).toBe(true);
    expect(isLookKey(space(document.body), "source")).toBe(false);
  });

  it("is Space on the page, not typed and not with a modifier", () => {
    const input = document.body.appendChild(document.createElement("input"));
    expect(isLookKey(space(document.body), "source")).toBe(true);
    expect(isLookKey(space(input), "source")).toBe(false);
    expect(isLookKey(space(document.body, { ctrlKey: true }), "source")).toBe(false);
    expect(isLookKey(space(document.body, { repeat: true }), "source")).toBe(false);
    expect(isLookKey(space(document.body, { key: "Enter" }), "source")).toBe(false);
  });

  it("takes Space from a button a click focused, not from one the keyboard did", () => {
    const b = document.body.appendChild(document.createElement("button"));
    const icon = b.appendChild(document.createElement("span"));
    const other = document.body.appendChild(document.createElement("button"));
    icon.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    b.focus();
    expect(isLookKey(space(b), "trash")).toBe(true);
    other.focus();
    b.focus();
    expect(isLookKey(space(b), "trash")).toBe(false);
  });

  it("leaves Space to a dialog that's open", () => {
    document.body.appendChild(document.createElement("div")).setAttribute("role", "dialog");
    expect(isLookKey(space(document.body), "source")).toBe(false);
  });
});
