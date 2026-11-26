// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { SyncSignIn } from "./api";
import { syncStatus } from "./syncStatus";

const ok: SyncSignIn = {
  signedIn: true,
  handedOver: "2026-10-28T07:00:00Z",
  stale: [],
  rejected: [],
  extensionOutdated: null,
  extensionShipped: "1.1.16",
  teams: true,
  outlook: true,
};

describe("syncStatus", () => {
  it("is connected when every service is live", () => {
    expect(syncStatus(ok)).toEqual({ tone: "ok", text: "Teams and Outlook connected." });
  });

  it("is off when not signed in", () => {
    expect(syncStatus(null).tone).toBe("off");
    expect(syncStatus({ ...ok, signedIn: false }).tone).toBe("off");
  });

  it("names what needs attention, and which tab to keep open", () => {
    const mail = syncStatus({ ...ok, stale: ["mail"] });
    expect(mail.tone).toBe("warn");
    expect(mail.text).toContain("Keep Outlook open");
    expect(syncStatus({ ...ok, stale: ["chats"] }).text).toContain("Keep Teams open");
    expect(syncStatus({ ...ok, rejected: ["mail"] }).text).toContain("Signed out: mail");
    expect(syncStatus({ ...ok, extensionOutdated: "1.1.7" }).text).toContain("version 1.1.7");
  });

  it("speaks only of the services switched on, each with its own tab", () => {
    const mailOnly = { teams: false, mail: true };
    expect(syncStatus({ ...ok, stale: ["chats", "transcripts"], teams: false }, mailOnly)).toEqual({
      tone: "ok",
      text: "Outlook connected.",
    });
    expect(syncStatus({ ...ok, outlook: false, stale: ["mail"] }, mailOnly)).toEqual({
      tone: "off",
      text: "Outlook: not signed in. Open Outlook in Chrome so the extension hands the sign-in over.",
    });
    expect(syncStatus({ ...ok, stale: ["mail"], outlook: false }, { teams: true, mail: false }).text).toBe("Teams connected.");
  });
});
