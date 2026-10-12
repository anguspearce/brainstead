// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", async (orig) => {
  const m = await orig<typeof import("./api")>();
  return {
    ...m,
    api: { ...m.api, draftRead: vi.fn().mockResolvedValue(null), draftWrite: vi.fn(), draftDiscard: vi.fn() },
  };
});

import { fileMoved } from "./moves";
import { place, restorePlaces } from "./nav";
import { recentDocs } from "./recent";
import { settings } from "./store";

beforeEach(() => {
  settings.update({ recent: [], docLooks: {} });
});

describe("a file moved elsewhere", () => {
  it("follows a rename: the open note, the recent list and its look", async () => {
    restorePlaces({ stack: [{ screen: "today" }, { screen: "doc", path: "Orbit App.md" }], i: 1 });
    settings.update({
      recent: [
        { path: "Orbit App.md", title: "Orbit App", at: 2 },
        { path: "Lena.md", title: "Lena", at: 1 },
      ],
      docLooks: { "Orbit App.md": { font: "serif" } } as never,
    });
    await fileMoved({ from: "Orbit App.md", to: "Projects/Orbit App.md", draft: "Projects/Orbit App.md" });
    expect(place.get().place).toMatchObject({ screen: "doc", path: "Projects/Orbit App.md" });
    expect(recentDocs().map((r) => r.path)).toEqual(["Projects/Orbit App.md", "Lena.md"]);
    expect(Object.keys(settings.get().docLooks ?? {})).toEqual(["Projects/Orbit App.md"]);
  });

  it("lets another screen's file go when it's trashed, and follows it when it's renamed", async () => {
    const t = "sources/Email. Thread. Launch - 2026-10-02.md";
    restorePlaces({ stack: [{ screen: "reply", path: t }], i: 0 });
    settings.update({ viewMemory: { "reply.thread": t, "compare.b": t, "reply.tone": "warm" } });
    await fileMoved({ from: t, to: null, draft: "" });
    expect(place.get().place).toEqual({ screen: "reply" });
    expect(settings.get().viewMemory).toEqual({ "reply.thread": null, "compare.b": null, "reply.tone": "warm" });
    restorePlaces({ stack: [{ screen: "compare", path: "a.md" }], i: 0 });
    await fileMoved({ from: "a.md", to: "notes/a.md", draft: "notes/a.md" });
    expect(place.get().place).toMatchObject({ screen: "compare", path: "notes/a.md" });
  });

  it("leaves a note that was trashed, and drops it from the recent list", async () => {
    restorePlaces({ stack: [{ screen: "today" }, { screen: "doc", path: "Lena.md" }], i: 1 });
    settings.update({ recent: [{ path: "Lena.md", title: "Lena", at: 1 }] });
    await fileMoved({ from: "Lena.md", to: null, draft: "trashed:1" });
    expect(place.get().place.screen).toBe("today");
    expect(recentDocs()).toEqual([]);
  });

  it("stays put when another note moved", async () => {
    restorePlaces({ stack: [{ screen: "doc", path: "Lena.md" }], i: 0 });
    await fileMoved({ from: "Acme.md", to: "Acme Ltd.md", draft: "Acme Ltd.md" });
    expect(place.get().place).toMatchObject({ screen: "doc", path: "Lena.md" });
  });
});
