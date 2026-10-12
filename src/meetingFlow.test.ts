// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { IngestRun, Transcript } from "./api";

const meetingDraft = vi.fn();
const ingestStart = vi.fn();
const trashMove = vi.fn();
const meetingTranscripts = vi.fn();
const inboxCaptureDone = vi.fn();
const docRead = vi.fn();
const healthLinkGhost = vi.fn();
const changeSubmit = vi.fn();
const changesList = vi.fn();
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
      inboxCaptureDone: (...a: unknown[]) => inboxCaptureDone(...a),
      docRead: (...a: unknown[]) => docRead(...a),
      healthLinkGhost: (...a: unknown[]) => healthLinkGhost(...a),
      changeSubmit: (...a: unknown[]) => changeSubmit(...a),
      changesList: () => changesList(),
    },
  };
});
const toast = vi.fn();
vi.mock("./Toast", () => ({ toast: (...a: unknown[]) => toast(...a) }));

import {
  draftNotes,
  ingestRunEnded,
  keepNote,
  makeMeetingNotes,
  meetingRunEnded,
  resumeMeetingNotes,
  sureOf,
  trashTranscript,
} from "./meetingFlow";
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
  inboxCaptureDone.mockResolvedValue(undefined);
  docRead.mockRejectedValue("no such file");
  healthLinkGhost.mockResolvedValue(1);
  changesList.mockResolvedValue([]);
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

describe("a transcript in the Inbox", () => {
  const spec = { type: "1-1", name: "Maya", date: "2026-10-01" };

  it("stays until its note is made and ingested", async () => {
    await draftNotes([[MAYA, spec]]);
    expect(inboxCaptureDone).not.toHaveBeenCalled();
    meetingRunEnded(run({}), true);
    await flush();
    expect(inboxCaptureDone).not.toHaveBeenCalled();
    ingestRunEnded(run({ id: "i1", kind: "ingest", status: "done" }));
    await flush();
    expect(inboxCaptureDone).toHaveBeenCalledWith(MAYA);
  });

  it("stays, to be chosen again, when the note or its ingest fails", async () => {
    await draftNotes([[MAYA, spec]]);
    meetingRunEnded(run({ status: "failed", error: "the model stopped" }), false);
    await flush();
    await draftNotes([[MAYA, spec]]);
    meetingRunEnded(run({}), true);
    await flush();
    ingestRunEnded(run({ id: "i1", kind: "ingest", status: "failed" }));
    await flush();
    expect(inboxCaptureDone).not.toHaveBeenCalled();
  });

  it("leaves once the note is made, when ingesting is off", async () => {
    settings.update({ meetingIngest: false, meetingTrash: false });
    await draftNotes([[MAYA, spec]]);
    meetingRunEnded(run({}), true);
    await flush();
    expect(inboxCaptureDone).toHaveBeenCalledWith(MAYA);
  });

  it("starts one note however often Make a meeting note is pressed before its run ends", async () => {
    meetingTranscripts.mockResolvedValue([transcript(MAYA)]);
    await Promise.all([makeMeetingNotes([MAYA]), makeMeetingNotes([MAYA, MAYA])]);
    await makeMeetingNotes([MAYA]);
    expect(meetingDraft).toHaveBeenCalledTimes(1);
    // Once its run has ended (here it failed), it can be chosen again.
    meetingRunEnded(run({ status: "failed" }), false);
    await makeMeetingNotes([MAYA]);
    expect(meetingDraft).toHaveBeenCalledTimes(2);
    meetingRunEnded(run({ status: "failed" }), false);
  });

  it("is trashed with its links pointed at its note first, however it's trashed", async () => {
    docRead.mockResolvedValue({ meta: { backlinks: [{ path: "wiki/entities/Maya.md" }, { path: "1-1. Maya - 2026-10-01.md" }] } });
    await trashTranscript(MAYA, "1-1. Maya - 2026-10-01.md");
    expect(healthLinkGhost).toHaveBeenCalledWith(
      "Teams. Transcript. 11 Maya, Lena - 2026-10-01",
      "1-1. Maya - 2026-10-01.md",
      ["wiki/entities/Maya.md"],
      false,
    );
    expect(trashMove).toHaveBeenCalledWith(MAYA);
    // With no note given, the one its name says, if it's in the vault; with none, it's only trashed.
    healthLinkGhost.mockClear();
    meetingTranscripts.mockResolvedValue([transcript(MAYA, { exists: true })]);
    await trashTranscript(MAYA);
    expect(healthLinkGhost).toHaveBeenCalledWith(expect.anything(), "1-1. Maya - 2026-10-01.md", ["wiki/entities/Maya.md"], false);
    healthLinkGhost.mockClear();
    meetingTranscripts.mockResolvedValue([transcript(MAYA)]);
    await trashTranscript(MAYA);
    expect(healthLinkGhost).not.toHaveBeenCalled();
    expect(trashMove).toHaveBeenCalledTimes(3);
  });
});

describe("after the window reloads", () => {
  const note = "1-1. Maya - 2026-10-01.md";
  const meeting = run({ id: "m1", started: "2026-10-12T10:00:00" });
  const ing = (id: string, started: string, status = "done") => run({ id, kind: "ingest", source: note, started, status, note: undefined });

  it("follows a note's ingest through though it started before the reload, but not a later ingest of it", async () => {
    const first = ing("i9", "2026-10-12T10:01:00");
    ingestRunEnded(first, [first, meeting]);
    await flush();
    expect(inboxCaptureDone).toHaveBeenCalledWith(MAYA);
    expect(trashMove).toHaveBeenCalledWith(MAYA);
    vi.clearAllMocks();
    const again = ing("i10", "2026-10-12T11:00:00");
    ingestRunEnded(again, [again, first, meeting]);
    await flush();
    expect(inboxCaptureDone).not.toHaveBeenCalled();
    expect(trashMove).not.toHaveBeenCalled();
  });

  it("takes a transcript out of the Inbox only when its own run made the note and its ingest finished", async () => {
    // Maya's 1-1 made its note and that was ingested; a second 1-1 with Maya that day, a same-named
    // meeting and a transcript kept with a note written by hand all infer a note that's there and
    // ingested, but have no run of their own that made it.
    const MAYA2 = "sources/Teams. Transcript. 11 Maya - 2026-10-01 (2).md";
    const STEERCO2 = "sources/meetings/Teams. Transcript. Orbit App steerco - 2026-10-02.md";
    const KEPT = "sources/Teams. Transcript. Board - 2026-09-30.md";
    const ingested = (p: string) => ({ ...transcript(p, { exists: true }), noteIngested: true });
    meetingTranscripts.mockResolvedValue([ingested(MAYA), ingested(MAYA2), ingested(STEERCO), ingested(STEERCO2), ingested(KEPT)]);
    const steerco = run({ id: "m2", source: STEERCO, note: { type: "Meeting", name: "Orbit App steerco", date: "2026-10-02" } });
    const all = [
      ing("i1", "2026-10-12T10:01:00"),
      meeting,
      // Steerco's note was made but not ingested yet.
      steerco,
    ];
    await resumeMeetingNotes(all);
    await flush();
    expect(inboxCaptureDone.mock.calls).toEqual([[MAYA]]);
    // Its note reverted (or rejected) in Changes since: left in the Inbox.
    inboxCaptureDone.mockClear();
    changesList.mockResolvedValue([{ id: "c1", status: "reverted", origin: { kind: "meeting", run: "m1" } }]);
    await resumeMeetingNotes(all);
    await flush();
    expect(inboxCaptureDone).not.toHaveBeenCalled();
  });
});

describe("a note reverted while it's being ingested", () => {
  it("keeps the transcript, in the Inbox and out of the Trash", async () => {
    await draftNotes([[MAYA, { type: "1-1", name: "Maya", date: "2026-10-01" }]]);
    meetingRunEnded(run({}), true);
    await flush();
    changesList.mockResolvedValue([{ id: "c1", status: "reverted", origin: { kind: "meeting", run: "r1" } }]);
    await ingestRunEnded(run({ id: "i1", kind: "ingest", status: "done" }));
    expect(inboxCaptureDone).not.toHaveBeenCalled();
    expect(trashMove).not.toHaveBeenCalled();
    expect(toast).toHaveBeenLastCalledWith(expect.stringContaining("its note was reverted in Changes"), expect.anything(), "bad");
  });
});

describe("following a note through once", () => {
  const note = "1-1. Maya - 2026-10-01.md";
  const meeting = run({ id: "m1", started: "2026-10-12T10:00:00" });

  it("doesn't trash a transcript again when Ingest the existing note's ingest ends", async () => {
    await keepNote(MAYA, note);
    await flush();
    expect(trashMove).toHaveBeenCalledTimes(1);
    const ingest = run({ id: "i1", kind: "ingest", source: note, started: "2026-10-12T10:05:00", note: undefined });
    ingestRunEnded(ingest, [ingest, meeting]);
    await flush();
    expect(trashMove).toHaveBeenCalledTimes(1);
    expect(toast).not.toHaveBeenCalledWith(expect.stringContaining("couldn't move"), expect.anything(), "bad");
  });

  it("says nothing's wrong when the transcript is gone already", async () => {
    await draftNotes([[MAYA, { type: "1-1", name: "Maya", date: "2026-10-01" }]]);
    trashMove.mockRejectedValue({ code: "not-found", message: "No such file" });
    docRead.mockRejectedValue("no such file");
    meetingRunEnded(run({}), true);
    await flush();
    ingestRunEnded(run({ id: "i1", kind: "ingest", status: "done" }));
    await flush();
    await flush();
    expect(toast).toHaveBeenLastCalledWith(expect.stringContaining("the transcript is in the Trash"), expect.anything(), "ok");
  });
});

describe("an assistant trashing a transcript", () => {
  const by = { origin: { kind: "chat", run: "s1", trigger: "scheduled" }, reason: "the user asked", model: "haiku" };

  it("records every write as the assistant's, unattended when nobody's watching", async () => {
    docRead.mockResolvedValue({ meta: { backlinks: [{ path: "wiki/entities/Maya.md" }] } });
    changeSubmit.mockResolvedValue({ applied: true, message: "Moved it." });
    const r = await trashTranscript(MAYA, "1-1. Maya - 2026-10-01.md", by);
    expect(healthLinkGhost).toHaveBeenCalledWith(expect.anything(), "1-1. Maya - 2026-10-01.md", ["wiki/entities/Maya.md"], true);
    expect(changeSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ page: MAYA, kind: "trash", instruction: { op: "trash" }, origin: by.origin, model: "haiku" }),
    );
    expect(trashMove).not.toHaveBeenCalled();
    expect(r.outcome?.applied).toBe(true);
  });

  it("passes it through Ingest the existing note, and says a held move isn't made", async () => {
    changeSubmit.mockResolvedValue({ applied: false, message: "Held." });
    expect(await keepNote(MAYA, "1-1. Maya - 2026-10-01.md", by)).toEqual({ ingested: true, trashed: false });
    expect(ingestStart).toHaveBeenCalledWith(["1-1. Maya - 2026-10-01.md"], true);
    expect(trashMove).not.toHaveBeenCalled();
  });
});
