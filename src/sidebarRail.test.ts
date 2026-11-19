// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterEach, describe, expect, it } from "vitest";
import { narrowWindow, railOn, setRail, toggleRail } from "./sidebarRail";
import { settings } from "./store";

afterEach(() => {
  settings.update({ sidebarRail: undefined });
  narrowWindow.set(false);
});

describe("the sidebar's rail", () => {
  it("is a rail on a narrow window and in full on a wide one, unless chosen otherwise", () => {
    expect(railOn({}, true)).toBe(true);
    expect(railOn({}, false)).toBe(false);
    expect(railOn({ sidebarRail: { wide: true } }, false)).toBe(true);
    expect(railOn({ sidebarRail: { narrow: false } }, true)).toBe(false);
  });

  it("keeps each width's choice on its own, and the usual one as no key", () => {
    setRail("wide", true);
    expect(settings.get().sidebarRail).toEqual({ wide: true });
    setRail("narrow", false);
    expect(settings.get().sidebarRail).toEqual({ wide: true, narrow: false });
    setRail("wide", false);
    setRail("narrow", true);
    expect(settings.get().sidebarRail).toBeUndefined();
  });

  it("toggles the width the window is at, never the other", () => {
    narrowWindow.set(true);
    toggleRail();
    expect(settings.get().sidebarRail).toEqual({ narrow: false });
    expect(railOn(settings.get(), false)).toBe(false);
    narrowWindow.set(false);
    toggleRail();
    expect(settings.get().sidebarRail).toEqual({ narrow: false, wide: true });
    narrowWindow.set(true);
    toggleRail();
    expect(settings.get().sidebarRail).toEqual({ wide: true });
  });
});
