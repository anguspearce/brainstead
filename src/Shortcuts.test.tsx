// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null), convertFileSrc: (p: string) => p }));

import { inline, SECTIONS } from "./Shortcuts";

describe("the shortcut sheet's descriptions", () => {
  it("show keys as keys and links as their words, with no markdown left", () => {
    const { container } = render(<p>{inline("Focus mode; `Esc` leaves it ([Settings › General](app:settings/general), **Quit**)")}</p>);
    expect(container.textContent).toBe("Focus mode; Esc leaves it (Settings › General, Quit)");
    expect(container.querySelector(".kbd")?.textContent).toBe("Esc");
    expect(container.querySelector("b")?.textContent).toBe("Quit");
  });
  it("leave none of the help's markdown in any row", () => {
    for (const [, rows] of SECTIONS("⌃⌥Space"))
      for (const [, what] of rows) {
        const { container, unmount } = render(<p>{inline(what)}</p>);
        expect(container.textContent).not.toMatch(/`|\*\*|\]\(/);
        unmount();
      }
  });
});
