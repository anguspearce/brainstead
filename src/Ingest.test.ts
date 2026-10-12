// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { ChangeRow, IngestRun, SourceRow } from "./api";
import { isActive, meetingNoteMade, meetingProgress, merge, queueOffer, runMatches } from "./Ingest";
import { ingestable, sourceStatus } from "./Lists";

const run = (id: string, status = "running"): IngestRun => ({
  id,
  source: "sources/x.pdf",
  started: "2026-10-02T10:00:00",
  finished: null,
  model: "claude:sonnet",
  status,
  steps: [],
  proposals: [],
  dropped: [],
  pages: [],
  error: null,
});

describe("ingest runs", () => {
  it("updates a run in place, adds a new one on top", () => {
    const l = merge([run("a"), run("b")], run("b", "done"));
    expect(l.map((r) => [r.id, r.status])).toEqual([
      ["a", "running"],
      ["b", "done"],
    ]);
    expect(merge(l, run("c"))[0].id).toBe("c");
    expect(isActive(run("q", "queued"))).toBe(true);
    expect(isActive(run("d", "failed"))).toBe(false);
  });

  it("matches the pane's search on every word of what its card shows", () => {
    const r = { ...run("a", "done"), source: "sources/Orbit App plan.pdf", model: "claude:haiku" };
    expect(runMatches(r, "")).toBe(true);
    expect(runMatches(r, "orbit DONE")).toBe(true);
    expect(runMatches(r, "haiku 2026-10-02")).toBe(true);
    expect(runMatches(r, "orbit failed")).toBe(false);
    const m = {
      ...run("m", "failed"),
      kind: "meeting",
      source: "sources/Teams. Transcript. Steerco.md",
      note: { type: "Meeting", name: "Steerco", date: "2026-10-02" },
    };
    expect(runMatches(m as IngestRun, "steerco failed")).toBe(true);
  });

  it("source status: changed beats ingested", () => {
    const row = (path: string, ingested: boolean) => ({ path, ingested }) as SourceRow;
    const changed = new Set(["sources/b.md"]);
    expect(sourceStatus(row("sources/a.md", false), changed)).toBe("new");
    expect(sourceStatus(row("sources/b.md", true), changed)).toBe("changed");
    expect(sourceStatus(row("sources/c.md", true), changed)).toBe("ingested");
  });

  it("ingests notes, text, PDFs, Office files and images, but not SVG", () => {
    for (const p of ["sources/a.md", "sources/a.pdf", "sources/a.xlsx", "sources/Whiteboard photo.png", "sources/b.HEIC", "sources/c.jpeg"])
      expect(ingestable(p), p).toBe(true);
    for (const p of ["sources/diagram.svg", "sources/a.zip", "sources/clip.mp4"]) expect(ingestable(p), p).toBe(false);
  });
});

describe("after a meeting note", () => {
  const step = (detail: string) => ({ name: "Make the note", status: "done", detail, ms: 1 });
  it("is offered only when the note was made, not held in Changes", () => {
    expect(meetingNoteMade({ ...run("m", "done"), steps: [step("Made: ingest it, and trash the transcript")] })).toBe(true);
    expect(meetingNoteMade({ ...run("m", "done"), steps: [step("Held in Changes")] })).toBe(false);
    expect(meetingNoteMade(run("m", "done"))).toBe(false);
  });

  it("queues one offer per note", () => {
    const a = { note: "Meeting. Orbit App - 2026-10-05.md", transcript: "t/a.vtt" };
    const b = { note: "Meeting. Maya - 2026-10-05.md", transcript: "t/b.vtt" };
    const q = queueOffer(queueOffer([], a), b);
    expect(q).toEqual([a, b]);
    expect(queueOffer(q, a)).toBe(q);
  });
});

describe("meetingProgress", () => {
  const t = "sources/Teams. Transcript. Steerco - 2026-10-02.md";
  const run = (o: Partial<IngestRun>): IngestRun =>
    ({
      id: "r",
      kind: "ingest",
      source: "",
      started: "2026-10-12T10:00:00",
      finished: null,
      model: "",
      status: "done",
      steps: [],
      proposals: [],
      dropped: [],
      pages: [],
      error: null,
      ...o,
    }) as IngestRun;
  const made = { steps: [{ name: "Note", status: "done", detail: "Made", ms: 1 }] } as Partial<IngestRun>;
  const note = { type: "Meeting", name: "Steerco", date: "2026-10-02" };

  it("follows a transcript from its note being written to its note being ingested", () => {
    expect(meetingProgress([], t)).toBeNull();
    expect(meetingProgress([run({ kind: "meeting", source: t, status: "running" })], t)).toEqual({ label: "Writing note…", busy: true });
    const done = run({ kind: "meeting", source: t, note, ...made });
    expect(
      meetingProgress([run({ source: "Meeting. Steerco - 2026-10-02.md", status: "queued", started: "2026-10-12T10:01:00" }), done], t)
        ?.busy,
    ).toBe(true);
    expect(meetingProgress([run({ source: "Meeting. Steerco - 2026-10-02.md", started: "2026-10-12T10:01:00" }), done], t)).toBeNull();
  });

  it("says when it failed or is held, and frees the row", () => {
    expect(meetingProgress([run({ kind: "meeting", source: t, status: "failed", error: "No model" })], t)).toEqual({
      label: "Note failed",
      busy: false,
      why: "No model",
    });
    const held = run({
      kind: "meeting",
      source: t,
      note,
      steps: [{ name: "Note", status: "done", detail: "Held in Changes", ms: 1 }],
    } as Partial<IngestRun>);
    expect(meetingProgress([held], t)?.label).toBe("Note held in Changes");
    // As Changes has the run's note now: still held; accepted (made); rejected; gone from its history.
    const change = (status: string) => [{ id: "c", status, origin: { kind: "meeting", chat: t, run: "r" } }] as ChangeRow[];
    expect(meetingProgress([held], t, change("held"))?.label).toBe("Note held in Changes");
    expect(meetingProgress([held], t, change("applied"))).toBeNull();
    expect(
      meetingProgress(
        [run({ source: "Meeting. Steerco - 2026-10-02.md", status: "running", started: "2026-10-12T10:01:00" }), held],
        t,
        change("applied"),
      ),
    ).toEqual({ label: "Ingesting…", busy: true });
    expect(meetingProgress([held], t, change("rejected"))).toMatchObject({ label: "Note rejected in Changes", busy: false });
    expect(meetingProgress([held], t, [])).toBeNull();
    // A note made at once and reverted since: the transcript can be chosen again.
    const madeRun = run({ kind: "meeting", source: t, note, ...made });
    expect(meetingProgress([madeRun], t, change("reverted"))).toMatchObject({ label: "Note reverted in Changes", busy: false });
    expect(meetingProgress([madeRun], t, change("applied"))).toBeNull();
    const done = run({ kind: "meeting", source: t, note, ...made });
    expect(
      meetingProgress([run({ source: "Meeting. Steerco - 2026-10-02.md", status: "failed", started: "2026-10-12T10:01:00" }), done], t),
    ).toMatchObject({ label: "Ingest failed", busy: false });
  });
});
