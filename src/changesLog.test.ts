// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { ChangeRow, LogEntry } from "./api";
import { allTimeline, changeAction, logOnly, typesIn, writtenBy } from "./changesLog";

const change = (o: Partial<ChangeRow>): ChangeRow => ({
  id: "c1",
  created: "2026-10-01T09:30:20",
  origin: { kind: "chat" } as ChangeRow["origin"],
  model: null,
  page: "wiki/entities/Orbit App.md",
  kind: "edit",
  title: "Add the launch",
  reason: "",
  status: "applied",
  decided: null,
  flags: [],
  warnings: [],
  group: "g1",
  revertable: true,
  ...o,
});
const line = (date: string, time: string | null, action: string, title: string): LogEntry => ({
  date,
  time,
  action,
  title,
  description: "",
});

describe("Changes › All", () => {
  it("names a change's log.md action as its line does", () => {
    expect(changeAction(change({}))).toBe("update");
    expect(changeAction(change({ kind: "new" }))).toBe("create");
    expect(changeAction(change({ origin: { kind: "ingest" } as ChangeRow["origin"] }))).toBe("ingest");
    expect(changeAction(change({ origin: { kind: "lint" } as ChangeRow["origin"] }))).toBe("lint-fix");
  });

  it("leaves out the line a change wrote, and keeps every other", () => {
    const c = change({});
    const own = line("2026-10-01", "09:31", "update", "Orbit App");
    expect(writtenBy(own, c)).toBe(true);
    // Another page, too long after, another action, or a change that was never made.
    expect(writtenBy(line("2026-10-01", "09:31", "update", "Lena"), c)).toBe(false);
    expect(writtenBy(line("2026-10-01", "09:40", "update", "Orbit App"), c)).toBe(false);
    expect(writtenBy(line("2026-10-01", "09:31", "ingest", "Orbit App"), c)).toBe(false);
    expect(writtenBy(own, change({ status: "rejected" }))).toBe(false);
    // A held change accepted later wrote its line when it was accepted.
    expect(writtenBy(line("2026-10-02", "08:00", "update", "Orbit App"), change({ decided: "2026-10-02T08:00:10" }))).toBe(true);
    const older = line("2025-01-01", null, "update", "Orbit App");
    expect(logOnly([own, older], [c])).toEqual([older]);
  });

  it("puts runs and lines in one history, newest first, and offers their types", () => {
    const c = change({});
    const late = line("2026-10-02", "10:00", "weekly-review", "2026-W40");
    const undated = line("2026-10-01", null, "ingest", "Hub Platform");
    const early = line("2026-10-01", "08:00", "ingest", "Orbit");
    const t = allTimeline([c], [late, undated, early]);
    expect(t.map((x) => (x.kind === "run" ? x.run.group : x.entry.title))).toEqual(["2026-W40", "g1", "Orbit", "Hub Platform"]);
    expect(typesIn([c], [late, undated, early])).toEqual(["ingest", "update", "weekly-review"]);
  });
});
