// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { CurrentStateRun, DailyCheckStatus, FindState, IngestRun, ReviewsStatus } from "./api";
import { nextRuns, RunStates, shownRuns } from "./runs";
import { runsLineText } from "./RunsLine";

const ingest = (id: string, status: string, proposals: string[] = []) =>
  ({ id, kind: "ingest", source: `sources/${id}.md`, status, steps: [], proposals }) as unknown as IngestRun;
const states = (o: Partial<RunStates>): RunStates => ({
  ingests: [],
  dailyCheck: { running: false, summary: "" } as DailyCheckStatus,
  reviews: { runs: [], next: { daily: null, weekly: null, brief: null }, running: [] } as unknown as ReviewsStatus,
  contradictions: false,
  weekprep: null,
  find: null as FindState | null,
  currentState: null as CurrentStateRun | null,
  ...o,
});

describe("the runs", () => {
  it("counts the ingests waiting their turn rather than listing them", () => {
    const { now } = nextRuns(
      { running: [], finished: [] },
      states({
        ingests: [ingest("a", "running"), ingest("b", "queued"), ingest("c", "queued")],
        dailyCheck: { running: true, doing: "Judging", progress: 0.62, summary: "" } as DailyCheckStatus,
      }),
      0,
    );
    const { shown, queued } = shownRuns(now.running);
    expect(shown.map((r) => r.short)).toEqual(["Ingest", "Daily check"]);
    expect(queued).toBe(2);
    expect(runsLineText(now.running)).toBe("2 running, 2 waiting");
    expect(runsLineText(now.running.filter((r) => r.key === "daily-check"))).toBe("Daily check · 62%");
  });

  it("notices a run ending, and says how it ended", () => {
    const going = nextRuns(
      { running: [], finished: [] },
      states({
        ingests: [ingest("a", "running"), ingest("b", "running")],
        dailyCheck: { running: true, doing: "", progress: 0.5, summary: "" } as DailyCheckStatus,
      }),
      1,
    ).now;
    const { now, ended } = nextRuns(
      going,
      states({
        ingests: [ingest("a", "done", ["c1", "c2"]), ingest("b", "failed")],
        dailyCheck: { running: false, summary: "115 pages read, 0 fixes proposed; index.md updated" } as DailyCheckStatus,
      }),
      2,
    );
    expect(now.running).toEqual([]);
    expect(ended.map((f) => [f.text, f.failed])).toEqual([
      ["Ingest of a finished: 2 changes", false],
      ["Ingest of b failed", true],
      ["Daily check finished: 115 pages read, 0 fixes proposed", false],
    ]);
    expect(ended[0].place).toEqual({ screen: "review" });
    expect(now.finished).toHaveLength(3);
  });
});
