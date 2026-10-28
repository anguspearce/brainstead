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
});
