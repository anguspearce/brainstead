// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { isSelectingClick } from "./textSelect";

describe("isSelectingClick", () => {
  const at = { x: 10, y: 10 };
  it("lets a plain click through", () => {
    expect(isSelectingClick(at, { x: 10, y: 10, detail: 1 }, "")).toBe(false);
    expect(isSelectingClick(at, { x: 11, y: 10, detail: 1 }, "Call Maya")).toBe(false);
  });
  it("keeps a drag that selected text from clicking the row", () => {
    expect(isSelectingClick(at, { x: 60, y: 12, detail: 1 }, "Call Maya")).toBe(true);
  });
  it("keeps a double-click that picked a word from clicking twice", () => {
    expect(isSelectingClick(at, { x: 10, y: 10, detail: 2 }, "Maya")).toBe(true);
  });
  it("lets a drag through when nothing ended up selected", () => {
    expect(isSelectingClick(at, { x: 60, y: 12, detail: 1 }, "")).toBe(false);
  });
});
