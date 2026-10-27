// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// When the Teams extension renews the tab's sign-in (sign-in-freshness.js), over invented tokens.
// Vitest runs it with the app's tests (`make check`).

import { beforeAll, describe, expect, it } from "vitest";

let due;
beforeAll(async () => {
  await import("./sign-in-freshness.js");
  ({ due } = globalThis.brainsteadSignIn);
});

const now = 1_800_000_000;
const at = (target, inMin) => ({ target, expiresOn: String(now + inMin * 60) });
const fresh = [
  at("https://api.spaces.skype.com//Authorization.ReadWrite", 600),
  at("https://outlook.office.com//Mail.ReadWrite https://outlook.office.com//Calendars.Read", 600),
  at("https://substrate.office.com//SubstrateSearch-Internal.ReadWrite", 50),
  at("https://chatsvcagg.teams.microsoft.com//.default", 600),
];

describe("due", () => {
  it("is nothing while every token the sync uses has a while to run", () => {
    expect(due(fresh, now)).toEqual([]);
  });

  it("names a token that runs out soon, or has run out", () => {
    const soon = fresh.map((a) => (a.target.includes("substrate") ? at(a.target, 5) : a));
    expect(due(soon, now)).toEqual(["transcripts"]);
    const gone = fresh.map((a) => (a.target.includes("spaces") ? at(a.target, -30) : a));
    expect(due(gone, now)).toEqual(["chats"]);
  });

  it("goes by the longest-lived token for a service", () => {
    expect(due([...fresh, at("https://api.spaces.skype.com//x", -100)], now)).toEqual([]);
  });

  it("counts a missing token as due, and an Outlook token without mail as missing", () => {
    const noMail = fresh.map((a) => (a.target.includes("outlook") ? at("https://outlook.office.com//Calendars.Read", 600) : a));
    expect(due(noMail, now)).toEqual(["mail"]);
    expect(due([], now)).toEqual(["chats", "mail", "transcripts", "channel list"]);
  });
});
