// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { clampInterfaceSize, INTERFACE_SIZE } from "./interfaceSize";

describe("the interface size", () => {
  it("is a whole step within range, and the usual size when unset or nonsense", () => {
    expect(clampInterfaceSize(undefined)).toBe(INTERFACE_SIZE.default);
    expect(clampInterfaceSize(NaN)).toBe(INTERFACE_SIZE.default);
    expect(clampInterfaceSize(120)).toBe(120);
    // A hand-edited settings.json snaps to the setting's steps and stays readable.
    expect(clampInterfaceSize(125)).toBe(130);
    expect(clampInterfaceSize(112)).toBe(110);
    expect(clampInterfaceSize(40)).toBe(INTERFACE_SIZE.min);
    expect(clampInterfaceSize(400)).toBe(INTERFACE_SIZE.max);
  });
});
