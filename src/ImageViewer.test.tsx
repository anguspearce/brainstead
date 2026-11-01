// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { ImageViewerHost, imageView } from "./ImageViewer";

afterEach(() => {
  cleanup();
  imageView.set(null);
  document.body.innerHTML = "";
});

/** useDismiss listens for presses from the next tick, so a press straight after opening wouldn't count. */
const settle = () => act(() => new Promise<void>((r) => setTimeout(r, 1)));

describe("an image full size", () => {
  it("stays open for the window's own buttons and a drag of the title bar, and closes for a press elsewhere", async () => {
    const bar = document.body.appendChild(document.createElement("div"));
    bar.setAttribute("data-tauri-drag-region", "true");
    const ctl = document.body.appendChild(document.createElement("div"));
    ctl.className = "winctl";
    const close = ctl.appendChild(document.createElement("button"));
    const elsewhere = document.body.appendChild(document.createElement("p"));

    render(<ImageViewerHost />);
    act(() => imageView.set({ url: "asset://images/diagram.png", alt: "Orbit App diagram" }));
    const dialog = screen.getByRole("dialog", { name: "Orbit App diagram" });
    expect(dialog.closest(".scrim.mfull")).not.toBeNull();
    await settle();

    fireEvent.mouseDown(close);
    fireEvent.mouseDown(bar);
    expect(screen.getByRole("dialog", { name: "Orbit App diagram" })).toBe(dialog);
    fireEvent.mouseDown(elsewhere);
    expect(screen.queryByRole("dialog", { name: "Orbit App diagram" })).toBeNull();
  });
});
