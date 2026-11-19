// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn(async () => {}) }));

// Every call answers with nothing, but for the heatmap's days and the day's files.
const known: Record<string, (...a: unknown[]) => unknown> = {
  activity: async () => ({
    days: [
      { date: "2026-10-01", notes: 1, wiki: 0, sources: 0 },
      { date: "2026-10-02", notes: 0, wiki: 1, sources: 0 },
    ],
  }),
  activityDay: async (d) =>
    d === "2026-10-02" ? [{ path: "wiki/entities/Orbit App.md", title: "Orbit App", layer: "wiki", mtime: 0 }] : [],
};
const api = vi.hoisted(
  () =>
    new Proxy({} as Record<string, unknown>, {
      get: (_, k: string) =>
        known[k] ?? (k.startsWith("on") ? async () => () => {} : /List$/.test(k) ? async () => [] : async () => undefined),
    }),
);
vi.mock(import("./api"), async (orig) => ({ ...(await orig()), api: api as never }));

// jsdom has no ResizeObserver; the heatmap and the dividers watch their size with one.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};

const { ActivityScreen } = await import("./Activity");
const { place } = await import("./nav");

afterEach(cleanup);

describe("Activity's files by day", () => {
  it("lists the latest day's files, and opens Changes › All on that day", async () => {
    render(<ActivityScreen />);
    expect(await screen.findByText("Orbit App")).toBeTruthy();
    // No log here: it's Changes' (D-20261119-04).
    expect(screen.queryByRole("table", { name: "The vault's log" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Changes that day/ }));
    expect(place.get().place).toMatchObject({ screen: "review", view: { "review:view": "all", "review:day": "2026-10-02" } });
  });
});
