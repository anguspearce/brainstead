// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { IngestRun, Transcript } from "./api";

const meetingDraft = vi.fn();
const ingestStart = vi.fn();
const trashMove = vi.fn();
const meetingTranscripts = vi.fn();
vi.mock("./api", async (orig) => {
  const m = await orig<typeof import("./api")>();
  return {
    ...m,
    api: {
      ...m.api,
      meetingDraft: (...a: unknown[]) => meetingDraft(...a),
      ingestStart: (...a: unknown[]) => ingestStart(...a),
      trashMove: (...a: unknown[]) => trashMove(...a),
      meetingTranscripts: () => meetingTranscripts(),
      inboxCaptureDone: () => Promise.resolve(),
    },
  };
});
const toast = vi.fn();
vi.mock("./Toast", () => ({ toast: (...a: unknown[]) => toast(...a) }));

import { draftNotes, ingestRunEnded, makeMeetingNotes, meetingRunEnded, sureOf } from "./meetingFlow";
import { place } from "./nav";
import { settings } from "./store";

const MAYA = "sources/Teams. Transcript. 11 Maya, Lena - 2026-10-01.md";
const STEERCO = "sources/Teams. Transcript. Orbit App steerco - 2026-10-02.md";

function transcript(path: string, over: Partial<Transcript["inferred"]> = {}): Transcript {
  return {
    path,
    mtime: 0,
    noteIngested: false,
    done: null,
    inferred: {
      filename: path,
      kind: "transcript",
      type: "1-1",
      name: "Maya",
      date: "2026-10-01",
      topic: null,
      confident: true,
      ask: [],
      suggestedFilename: null,
      existingNotes: [],
      exists: false,
      dateCheck: false,
      ...over,
    },
  };
}

const run = (over: Partial<IngestRun>): IngestRun => ({
  id: "r1",
  kind: "meeting",
  note: { type: "1-1", name: "Maya", date: "2026-10-01" },
  source: MAYA,
  started: "",
  finished: null,
  model: "",
  status: "done",
  steps: [{ status: "done" } as never],
  proposals: [],
  dropped: [],
  pages: [],
  error: null,
  ...over,
});

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  vi.clearAllMocks();
  settings.update({ meetingIngest: undefined, meetingTrash: undefined });
  meetingDraft.mockResolvedValue(["r1"]);
  ingestStart.mockResolvedValue(["i1"]);
  trashMove.mockResolvedValue({});
});

describe("sure of a transcript", () => {
  it("is when its name gives the type, name and date, and there's no note yet", () => {
    expect(sureOf(transcript(MAYA))).toBe(true);
    expect(sureOf(transcript(MAYA, { dateCheck: true }))).toBe(false);
    expect(sureOf(transcript(MAYA, { exists: true }))).toBe(false);
    expect(sureOf(transcript(MAYA, { confident: false, ask: ["Which Maya?"] }))).toBe(false);
  });
});

describe("one step", () => {
  it("drafts the sure ones and opens the meeting screen at the first unsure one", async () => {
    meetingTranscripts.mockResolvedValue([transcript(MAYA), transcript(STEERCO, { dateCheck: true })]);
    await makeMeetingNotes([MAYA, STEERCO]);
    expect(meetingDraft).toHaveBeenCalledWith([[MAYA, { type: "1-1", name: "Maya", date: "2026-10-01" }]], false);
    expect(place.get().place).toMatchObject({ screen: "meeting", path: STEERCO });
  });

  it("ingests the note made, then trashes the transcript, with one toast at the end", async () => {
    await draftNotes([[MAYA, { type: "1-1", name: "Maya", date: "2026-10-01" }]]);
    expect(meetingRunEnded(run({}), true)).toBe(true);
    await flush();
    expect(ingestStart).toHaveBeenCalledWith(["1-1. Maya - 2026-10-01.md"], false);
    expect(trashMove).not.toHaveBeenCalled();
    ingestRunEnded(run({ id: "i1", kind: "ingest", status: "done" }));
    await flush();
    expect(trashMove).toHaveBeenCalledWith(MAYA);
    expect(toast).toHaveBeenLastCalledWith(expect.stringContaining("ingested; the transcript is in the Trash"), expect.anything(), "ok");
  });

  it("ingests unattended when an unattended assistant started the note, so its failing changes are held", async () => {
    await draftNotes([[MAYA, { type: "1-1", name: "Maya", date: "2026-10-01" }]], true);
    meetingRunEnded(run({ trigger: "scheduled" }), true);
    await flush();
    expect(ingestStart).toHaveBeenCalledWith(["1-1. Maya - 2026-10-01.md"], true);
  });

  it("keeps the transcript when the ingest fails", async () => {
    await draftNotes([[MAYA, { type: "1-1", name: "Maya", date: "2026-10-01" }]]);
    meetingRunEnded(run({}), true);
    await flush();
    ingestRunEnded(run({ id: "i1", kind: "ingest", status: "failed" }));
    await flush();
    expect(trashMove).not.toHaveBeenCalled();
    expect(toast).toHaveBeenLastCalledWith(expect.stringContaining("its ingest failed"), expect.anything(), "bad");
  });

  it("leaves it to the offer when both are switched off", async () => {
    settings.update({ meetingIngest: false, meetingTrash: false });
    await draftNotes([[MAYA, { type: "1-1", name: "Maya", date: "2026-10-01" }]]);
    expect(meetingRunEnded(run({}), true)).toBe(false);
    expect(ingestStart).not.toHaveBeenCalled();
  });

  it("says once for a batch, when the last one is through", async () => {
    settings.update({ meetingIngest: false });
    meetingDraft.mockResolvedValue(["r1", "r2"]);
    await draftNotes([
      [MAYA, { type: "1-1", name: "Maya", date: "2026-10-01" }],
      [STEERCO, { type: "Meeting", name: "Orbit App steerco", date: "2026-10-02" }],
    ]);
    meetingRunEnded(run({}), true);
    await flush();
    expect(toast).not.toHaveBeenCalled();
    meetingRunEnded(run({ id: "r2", source: STEERCO, note: { type: "Meeting", name: "Orbit App steerco", date: "2026-10-02" } }), true);
    await flush();
    expect(toast).toHaveBeenCalledTimes(1);
    expect(toast).toHaveBeenCalledWith("Made 2 meeting notes", expect.anything(), "ok");
  });
});
