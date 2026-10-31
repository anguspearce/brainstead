// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn(async () => {}) }));

// Every call answers with nothing, but for the log (one entry) and its links (none found).
const known: Record<string, (...a: unknown[]) => unknown> = {
  linksResolve: async (want) => (want as unknown[]).map(() => null),
  activity: async () => ({
    log: [{ date: "2026-10-02", time: "09:30", action: "ingest", title: "Orbit App plan", description: "" }],
    days: [],
  }),
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

afterEach(cleanup);

describe("Activity's log", () => {
  it("names its column headers by their headings, not by their dividers too", async () => {
    render(<ActivityScreen />);
    await screen.findByRole("table", { name: "The vault's log" });
    // A header's name is its heading alone: no "Resize the Action column" from the divider in it.
    expect(screen.getAllByRole("columnheader", { name: /^(When|Time|Item|Action)$/ })).toHaveLength(3);
    // The dividers are still there, under their own names.
    expect(screen.getByRole("separator", { name: "Resize the Action column" })).toBeTruthy();
  });
});
