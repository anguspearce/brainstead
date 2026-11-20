// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ hide: vi.fn(), setSize: vi.fn(async () => {}) }),
  LogicalSize: class {},
}));

import type { CurrentStateRun, DailyCheckStatus, FindState, IngestRun, ReviewsStatus, WeekPrepJob } from "./api";
import { runningOf, TrayData, trayFailed, trayStatus } from "./Tray";

const base: TrayData = { overdue: 0, due: 0, waiting: 0, inbox: 0, held: 0, running: [], nextReview: null };
const run = (label: string) => ({
  key: label,
  label,
  short: label,
  doing: "",
  progress: null,
  stop: async () => {},
  place: { screen: "today" as const },
});

describe("the menu-bar window", () => {
  it("sums things up without repeating the tiles, and says which icon to show", () => {
    expect(trayStatus(base)).toEqual({ headline: "All clear", busy: false, attention: false });
    expect(trayStatus({ ...base, due: 2 })).toMatchObject({ headline: "All clear", attention: false });
    expect(trayStatus({ ...base, overdue: 1, due: 3, held: 2 })).toEqual({
      headline: "Things need you",
      busy: false,
      attention: true,
    });
    expect(trayStatus({ ...base, inbox: 1 })).toMatchObject({ headline: "Things need you", attention: true });
    // A run going takes the headline and makes the icon busy.
    const busy = trayStatus({ ...base, inbox: 4, running: [run("Ingesting Steerco minutes"), run("Running the daily check")] });
    expect(busy).toEqual({ headline: "2 runs going", busy: true, attention: true });
  });

  it("says what couldn't be loaded instead of showing zeros", () => {
    const d = { ...base, overdue: null, due: null, waiting: null, inbox: null };
    expect(trayFailed(d)).toEqual(["tasks", "the Inbox"]);
    expect(trayStatus(d).headline).toBe("Couldn't load tasks and the Inbox");
    expect(trayStatus({ ...base, held: null }).headline).toBe("Couldn't load Changes");
  });

  it("lists every run going, as run_status does", () => {
    const ingest = (id: string, kind: string, source: string) =>
      ({ id, kind, source, status: "running", steps: [] }) as unknown as IngestRun;
    const going = runningOf({
      ingests: [ingest("i1", "ingest", "sources/Plan.pdf"), ingest("m1", "meeting", "sources/Standup transcript.vtt")],
      dailyCheck: { running: false } as DailyCheckStatus,
      reviews: { runs: [], next: { daily: null, weekly: null, brief: null }, running: ["daily"] } as unknown as ReviewsStatus,
      contradictions: true,
      weekprep: { running: "2026-W41" } as WeekPrepJob,
      find: { run: { status: "running", done: 2, batches: 4, found: 3 } } as FindState,
      currentState: { running: true, done: 1, total: 5 } as CurrentStateRun,
    });
    expect(going.map((r) => r.label)).toEqual([
      "Ingesting Plan.pdf",
      "Writing a meeting note from Standup transcript.vtt",
      "Writing the daily summary",
      "Checking for contradictions",
      "Preparing the weekly review",
      "Finding tasks and projects",
      "Writing Current state",
    ]);
    expect(going.find((r) => r.key === "find")!.progress).toBe(0.5);
    const idle = runningOf({
      ingests: [],
      dailyCheck: { running: false } as DailyCheckStatus,
      reviews: { runs: [], next: { daily: null, weekly: null, brief: null }, running: [] } as unknown as ReviewsStatus,
      contradictions: false,
      weekprep: null,
      find: { run: { status: "done" } } as FindState,
      currentState: { running: false } as CurrentStateRun,
    });
    expect(idle).toEqual([]);
  });
});
