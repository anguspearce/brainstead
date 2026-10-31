// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterEach, describe, expect, it } from "vitest";
import { applyLook, systemAccent } from "./accent";

const root = document.documentElement;

describe("the look a long-lived window puts in force when it opens", () => {
  afterEach(() => {
    systemAccent.set(null);
    applyLook({ theme: "system", accent: "app" });
  });

  it("follows a switch of the accent colour either way, not only the theme", () => {
    systemAccent.set("#107c10");
    applyLook({ theme: "dark", accent: "system" });
    expect(root.getAttribute("data-theme")).toBe("dark");
    expect(root.getAttribute("data-accent")).toBe("system");
    expect(document.getElementById("system-accent")?.textContent).toContain("--accent");
    // Back to the app's own blue, as when the main window switches from System to App.
    applyLook({ theme: "light", accent: "app" });
    expect(root.getAttribute("data-theme")).toBe("light");
    expect(root.hasAttribute("data-accent")).toBe(false);
    expect(document.getElementById("system-accent")).toBeNull();
  });

  it("keeps the app's accent where there's no system colour to follow", () => {
    applyLook({ theme: "system", accent: "system" });
    expect(root.hasAttribute("data-theme")).toBe(false);
    expect(root.hasAttribute("data-accent")).toBe(false);
  });
});
