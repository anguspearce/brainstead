// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, beforeEach } from "vitest";

import "./slack-reader.js";

const g = /** @type {any} */ (globalThis);

describe("reading the Slack tab's sign-in", () => {
  beforeEach(() => localStorage.clear());

  it("takes each workspace's client token from localConfig_v2", () => {
    localStorage.setItem(
      "localConfig_v2",
      JSON.stringify({
        teams: {
          T1: { id: "T1", name: "Acme", url: "https://acme.slack.com/", token: "xoxc-1" },
          T2: { id: "T2", name: "Old", url: "https://old.slack.com/", token: "" },
        },
      }),
    );
    const r = g.brainsteadReadSlack();
    expect(r.workspaces).toEqual([{ id: "T1", name: "Acme", url: "https://acme.slack.com/", token: "xoxc-1" }]);
    expect(g.brainsteadReadSlack.call(null).problems).toEqual([]);
  });

  it("says when there's none", () => {
    expect(g.brainsteadReadSlack().problems).toEqual(["no workspace sign-in in this tab"]);
  });

  it("hands over each workspace once, and only a slack.com address", () => {
    const w = (id, url) => ({ id, name: id, url, token: "xoxc-x" });
    const pages = [
      { workspaces: [w("T1", "https://acme.slack.com/"), w("T2", "https://evil.example/")] },
      { workspaces: [w("T1", "https://acme.slack.com/")] },
    ];
    expect(g.brainsteadSlackWorkspaces(pages).map((x) => x.id)).toEqual(["T1"]);
  });
});
