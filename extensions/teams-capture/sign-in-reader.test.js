// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Teams and Outlook extensions each carry a copy of sign-in-reader.js (an unpacked extension can
// only load files from its own folder); this keeps the two the same. Vitest runs it with `make check`.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("sign-in-reader.js", () => {
  it("is the same in both extensions", () => {
    const teams = readFileSync("extensions/teams-capture/sign-in-reader.js", "utf8");
    const outlook = readFileSync("extensions/outlook-capture/sign-in-reader.js", "utf8");
    expect(outlook).toBe(teams);
  });
});
