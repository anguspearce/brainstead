// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { IngestRun, SyncLog, SyncPass } from "./api";
import { ago, groupLine, serviceOf, syncFeed, syncLine } from "./syncActivity";

const pass = (p: Partial<SyncPass>): SyncPass => ({
  at: "2026-11-19T10:00:00",
  backfill: false,
  added: [],
  toppedUp: [],
  problems: [],
  blocked: null,
  repeats: 0,
  last: null,
  ...p,
});

const run = (r: Partial<IngestRun>): IngestRun => ({
  id: "r1",
  kind: "ingest",
  source: "sources/Teams. Chat. Maya - 2026-11-19.md",
  started: "2026-11-19T10:01:00",
  finished: "2026-11-19T10:02:00",
  model: "m",
  status: "done",
  steps: [],
  proposals: [],
  dropped: [],
  pages: [],
  error: null,
  ...r,
});

const log: SyncLog = {
  lastPass: "2026-11-19T10:40:00",
  passes: [
    pass({ at: "2026-11-19T10:30:00", blocked: "Not signed in.", repeats: 2, last: "2026-11-19T10:40:00" }),
    pass({
      at: "2026-11-19T10:00:00",
      added: ["sources/Teams. Chat. Maya - 2026-11-19.md", "sources/Slack. Chat. Lena - 2026-11-19.md"],
      toppedUp: ["sources/Email. Thread. Launch - 2026-11-18.md"],
    }),
  ],
};
const runs = [
  run({ pages: ["wiki/entities/Orbit App.md"] }),
  run({ id: "r2", source: "sources/Slack. Chat. Lena - 2026-11-19.md", started: "2026-11-19T10:03:00", trashed: true }),
  run({
    id: "r3",
    source: "sources/Email. Thread. Launch - 2026-11-18.md",
    started: "2026-11-19T10:05:00",
    status: "failed",
    error: "The model timed out.",
  }),
  run({ id: "r4", started: "2026-11-19T10:06:00", status: "running", steps: [{ name: "Read the source", status: "running" } as never] }),
];

describe("Activity › Sync & ingest", () => {
  it("knows each source's service by its name", () => {
    expect(serviceOf("sources/Teams. Transcript. Steerco - 2026-11-19.md")).toBe("teams");
    expect(serviceOf("sources/emails/Email. Thread. Launch - 2026-11-18.md")).toBe("outlook");
    expect(serviceOf("sources/Slack. Chat. Lena - 2026-11-19.md")).toBe("slack");
    expect(serviceOf("sources/Web. Page - 2026-11-19.md")).toBe("other");
  });

  it("puts passes and ingests in one timeline, newest first, saying what each did", () => {
    const rows = syncFeed(log, runs);
    expect(rows.map((r) => r.title)).toEqual([
      "Sync couldn't run (3 times, last at 10:40)",
      "Ingested Teams. Chat. Maya - 2026-11-19: read the source",
      "Ingested Email. Thread. Launch - 2026-11-18: failed",
      "Ingested Slack. Chat. Lena - 2026-11-19: nothing in it, moved to the Trash",
      "Ingested Teams. Chat. Maya - 2026-11-19: 1 page changed",
      "Synced: 2 new, 1 topped up",
    ]);
    const synced = rows[5];
    expect(synced.groups.map(groupLine)).toEqual([
      "New from Teams: Teams. Chat. Maya - 2026-11-19",
      "New from Slack: Slack. Chat. Lena - 2026-11-19",
      "Topped up from Outlook: Email. Thread. Launch - 2026-11-18",
    ]);
    expect(synced.detail).toEqual([]);
    expect(synced.services).toEqual(["teams", "outlook", "slack"]);
    expect(rows[2].detail).toEqual(["The model timed out."]);
    expect(rows[4].groups.map(groupLine)).toEqual(["Source: Teams. Chat. Maya - 2026-11-19", "Changed: Orbit App"]);
    expect(rows[4].paths).toEqual(["sources/Teams. Chat. Maya - 2026-11-19.md", "wiki/entities/Orbit App.md"]);
  });

  it("filters by service or to the problems, as the chips do", () => {
    expect(syncFeed(log, runs, "slack").map((r) => r.key)).toEqual(["ingest:r2", "sync:2026-11-19T10:00:00"]);
    expect(syncFeed(log, runs, "failed").map((r) => r.key)).toEqual(["sync:2026-11-19T10:30:00", "ingest:r3"]);
  });

  it("sums it up in a line for the menu-bar window", () => {
    const now = new Date(2026, 10, 19, 10, 44);
    expect(syncLine(log, runs, now)).toEqual({
      text: "Synced 4 min ago · 2 in today · 1 ingesting · 2 problems today",
      problem: true,
      problems: "2 problems today",
    });
    expect(syncLine({ lastPass: null, passes: [] }, [], now)).toBeNull();
    expect(syncLine({ lastPass: "2026-11-19T10:44:00", passes: [] }, [], now)).toEqual({
      text: "Synced just now",
      problem: false,
      problems: "",
    });
    expect(ago("2026-11-19T08:00:00", now)).toBe("2 h ago");
    expect(ago("2026-11-17T08:00:00", now)).toBe("on 2026-11-17");
  });
});
