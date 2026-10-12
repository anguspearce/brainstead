// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { Transcript } from "./api";
import { cardFor, needsDate, noteExists, specOf } from "./Meeting";

const t = (dateCheck: boolean): Transcript => ({
  path: "sources/Teams. Transcript. Orbit App Steerco - 2026-10-03.md",
  mtime: 0,
  noteIngested: false,
  done: null,
  inferred: {
    filename: "Teams. Transcript. Orbit App Steerco - 2026-10-03.md",
    kind: "meeting",
    type: "Meeting",
    name: "Orbit App Steerco",
    date: "2026-10-03",
    topic: null,
    confident: !dateCheck,
    ask: [],
    suggestedFilename: null,
    existingNotes: [],
    exists: false,
    dateCheck,
  },
});

describe("meeting note date", () => {
  it("waits for the user when the date is only the capture day", () => {
    expect(needsDate(t(true))).toBe(true);
    expect(needsDate(t(true), { name: "Steerco" })).toBe(true);
    // Confirmed as it is, or changed.
    expect(needsDate(t(true), { date: "2026-10-03" })).toBe(false);
    expect(specOf(t(true), { date: "2026-10-02" }).date).toBe("2026-10-02");
    expect(needsDate(t(false))).toBe(false);
  });
});

describe("meeting note that exists already", () => {
  it("has nothing to draft until its name or date changes", () => {
    const x = { ...t(false), inferred: { ...t(false).inferred, exists: true } };
    expect(noteExists(x)).toBe(true);
    expect(noteExists(x, { date: "2026-10-03" })).toBe(true);
    expect(noteExists(x, { date: "2026-10-02" })).toBe(false);
    expect(noteExists(t(false))).toBe(false);
  });
});

describe("the Meeting notes card", () => {
  const ok = t(false);
  const first = (x: ReturnType<typeof cardFor>) => x.actions[0];

  it("makes the note when there's nothing to ask", () => {
    expect(cardFor(ok)).toEqual({ say: undefined, actions: ["make"] });
    expect(first(cardFor({ ...ok, inferred: { ...ok.inferred, ask: ["Two names: set your name in Settings › General so…"] } }))).toBe(
      "make",
    );
    expect(cardFor({ ...ok, inferred: { ...ok.inferred, ask: ["Two names: set your name in Settings › General so…"] } }).actions).toContain(
      "openGeneral",
    );
  });

  it("leads with confirming a capture-day date, and says what's missing", () => {
    expect(first(cardFor(t(true)))).toBe("confirmAndMake");
    expect(cardFor(ok, { name: " " }).makeDisabled).toMatch(/name/);
    expect(cardFor(ok, { date: "soon" }).makeDisabled).toMatch(/date/);
  });

  it("says what's under way, failed or held, with the step that's left first", () => {
    expect(cardFor(ok, {}, { progress: { label: "Writing note…", busy: true } })).toMatchObject({ tone: "busy", actions: [] });
    expect(cardFor(ok, {}, { sending: true }).actions).toEqual([]);
    expect(first(cardFor(ok, {}, { progress: { label: "Note held in Changes", busy: false } }))).toBe("openChanges");
    expect(cardFor(ok, {}, { progress: { label: "Note failed", busy: false, why: "No model" } })).toMatchObject({
      actions: ["make", "trash"],
    });
    expect(first(cardFor(ok, {}, { progress: { label: "Ingest failed", busy: false } }))).toBe("ingestNote");
  });

  it("with its note there already: ingest it, or only the transcript is left", () => {
    const there = { ...ok, inferred: { ...ok.inferred, exists: true } };
    expect(first(cardFor(there))).toBe("ingestNote");
    expect(first(cardFor({ ...there, noteIngested: true }))).toBe("trash");
    // A new name makes a new note.
    expect(first(cardFor(there, { name: "Orbit App kickoff" }))).toBe("make");
  });

  it("with the transcript dealt with already, leads with letting it go", () => {
    expect(first(cardFor({ ...ok, done: "ingested" }))).toBe("trash");
    expect(first(cardFor({ ...ok, done: "linked" }))).toBe("trash");
  });

  it("can't make anything while the vault is read-only", () => {
    expect(cardFor(ok, {}, { readOnly: true }).actions).toEqual(["openVault"]);
  });
});
