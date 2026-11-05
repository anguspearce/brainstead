// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";

// The library draws nothing here: the component's own boxes and buttons are what's under test.
vi.mock("mermaid", () => ({
  default: { initialize: () => {}, render: async () => ({ svg: "<svg></svg>" }) },
}));

import { Mermaid } from "./Mermaid";

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

/** useDismiss listens for presses from the next tick, so a press straight after opening wouldn't count. */
const settle = () => act(() => new Promise<void>((r) => setTimeout(r, 1)));

describe("the diagram full screen on Windows", () => {
  const css = readFileSync("src/styles.css", "utf8");

  // jsdom lays nothing out, so this reads the rules themselves; the app's own window shows the result.
  it("starts below the title bar, whose height is the top bar's, so the window's buttons keep their own row", () => {
    expect(css).toMatch(/html\[data-platform="windows"\] \.scrim\.mfull \{\s*top: var\(--top-h\);\s*\}/);
    expect(css).toMatch(/\.winctl button \{[^}]*height: calc\(var\(--top-h\) - 1px\);/);
    // The image viewer's Close sits in the same scrim, so it moves down with it.
    expect(css).toMatch(/\.imgclose \{\s*position: absolute;/);
  });

  it("stays open for the window's own buttons and a drag of the title bar, and closes for a press elsewhere", async () => {
    const bar = document.body.appendChild(document.createElement("div"));
    bar.setAttribute("data-tauri-drag-region", "true");
    const ctl = document.body.appendChild(document.createElement("div"));
    ctl.className = "winctl";
    const min = ctl.appendChild(document.createElement("button"));
    const elsewhere = document.body.appendChild(document.createElement("p"));

    render(<Mermaid src="graph TD; A-->B" />);
    fireEvent.click(await screen.findByRole("button", { name: "Full screen" }));
    const dialog = screen.getByRole("dialog", { name: "Diagram" });
    expect(dialog.closest(".scrim.mfull")).not.toBeNull();
    await settle();

    fireEvent.mouseDown(min);
    fireEvent.mouseDown(bar);
    expect(screen.getByRole("dialog", { name: "Diagram" })).toBe(dialog);
    fireEvent.mouseDown(elsewhere);
    expect(screen.queryByRole("dialog", { name: "Diagram" })).toBeNull();
  });
});
