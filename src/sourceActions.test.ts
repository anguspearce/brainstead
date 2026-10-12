// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, vi } from "vitest";

const makeMeetingNotes = vi.fn();
const ingest = vi.fn();
const askAboutNote = vi.fn();
vi.mock("./meetingFlow", async (orig) => ({
  ...(await orig<typeof import("./meetingFlow")>()),
  makeMeetingNotes: (...a: unknown[]) => makeMeetingNotes(...a),
}));
vi.mock("./Ingest", async (orig) => ({ ...(await orig<typeof import("./Ingest")>()), ingest: (...a: unknown[]) => ingest(...a) }));
vi.mock("./Ask", () => ({ askAboutNote: (...a: unknown[]) => askAboutNote(...a) }));

import { place } from "./nav";
import { runSourceAction, sourceActions } from "./sourceActions";

const TRANSCRIPT = "sources/Teams. Transcript. Orbit App steerco - 2026-10-02.md";
const THREAD = "sources/emails/Email. Thread. Re Orbit App launch - 2026-10-02.md";

beforeEach(() => vi.clearAllMocks());

describe("a source's actions", () => {
  it("lead with what it usually becomes: a transcript's meeting note, a thread's reply, else ingest", () => {
    expect(sourceActions(TRANSCRIPT)).toEqual(["meeting", "ingest", "ask"]);
    expect(sourceActions(THREAD)).toEqual(["reply", "ingest", "ask"]);
    expect(sourceActions("sources/Orbit App plan.pdf")).toEqual(["ingest", "ask"]);
  });

  it("lead with Re-ingest once it's ingested, and are none for a file ingest can't read", () => {
    expect(sourceActions(THREAD, true)).toEqual(["ingest", "reply", "ask"]);
    expect(sourceActions("sources/Orbit App data.zip")).toEqual([]);
    expect(sourceActions("wiki/entities/Orbit App.md")).toEqual([]);
  });

  it("do the same wherever they're offered", () => {
    runSourceAction("meeting", TRANSCRIPT);
    expect(makeMeetingNotes).toHaveBeenCalledWith([TRANSCRIPT]);
    runSourceAction("ingest", THREAD);
    expect(ingest).toHaveBeenCalledWith([THREAD]);
    runSourceAction("ask", THREAD);
    expect(askAboutNote).toHaveBeenCalledWith(THREAD);
    runSourceAction("reply", THREAD);
    expect(place.get().place).toMatchObject({ screen: "reply", path: THREAD });
  });
});
