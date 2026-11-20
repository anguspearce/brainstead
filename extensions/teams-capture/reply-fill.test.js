// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Put in Teams's link key (reply-fill.js). Vitest runs it with the app's tests (`make check`).

import { beforeAll, describe, expect, it } from "vitest";

let keyOf;
beforeAll(async () => {
  await import("./reply-fill.js");
  ({ keyOf } = globalThis.brainsteadReply);
});

describe("keyOf", () => {
  it("reads the key Brainstead put on the chat's link", () => {
    expect(keyOf("https://teams.microsoft.com/_?brainstead-reply=18a2f9c1#/conversations/19%3Aa")).toBe("18a2f9c1");
    expect(keyOf("https://teams.microsoft.com/v2/?x=1&brainstead-reply=ab12")).toBe("ab12");
  });
  it("takes nothing else", () => {
    expect(keyOf("https://teams.microsoft.com/_#/conversations/19%3Aa")).toBeNull();
    expect(keyOf("https://teams.microsoft.com/?brainstead-reply=../x")).toBeNull();
    expect(keyOf("https://teams.microsoft.com/?not-brainstead-reply=ab")).toBeNull();
    expect(keyOf(undefined)).toBeNull();
  });
});
