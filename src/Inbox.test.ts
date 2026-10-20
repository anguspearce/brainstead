// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { InboxItem } from "./api";
import { bulkChoices } from "./Inbox";
import { keyLabel } from "./platform";

const cap = (path: string) => ({ kind: "capture", path, line: 0, text: "", lineText: "", stamp: null }) as InboxItem;
const task = (line: number) =>
  ({ kind: "task", path: "Me. To Do List.md", line, text: "Call Maya", lineText: "- [ ] Call Maya", stamp: null }) as InboxItem;
const choices = (ps: InboxItem[]) => bulkChoices(ps).map(([k, b]) => `${k} ${b}`);

describe("choices for several ticked items", () => {
  it("are those every one of them has, needing no form each", () => {
    const transcripts = [
      cap("sources/Teams. Transcript. Steerco - 2026-10-02.md"),
      cap("sources/Teams. Transcript. Standup - 2026-10-03.md"),
    ];
    expect(choices(transcripts)).toEqual(["M meeting", "I ingest", "R reference", "⌫ delete"]);
    expect(choices([task(3), task(4)])).toEqual(["2 done", "S someday", "⌫ delete"]);
    // A transcript and a PDF share only ingesting, keeping and trashing.
    expect(choices([transcripts[0], cap("sources/Orbit App plan.pdf")])).toEqual(["I ingest", "R reference", "⌫ delete"]);
  });

  it("keep in Sources only when all are captures, and say how many", () => {
    const mixed = [task(3), cap("sources/Orbit App plan.pdf")];
    expect(choices(mixed)).toEqual(["S someday", "⌫ delete"]);
    const [, , label, tip] = bulkChoices(mixed).find(([, b]) => b === "delete")!;
    expect([label, tip]).toEqual(["Delete 2", keyLabel("Delete them from the Inbox (captures go to the Trash); ⌘Z undoes one at a time")]);
    expect(bulkChoices([cap("sources/a.pdf"), cap("sources/b.pdf"), cap("sources/c.pdf")]).find(([, b]) => b === "reference")![2]).toBe(
      "Keep 3 in Sources",
    );
  });
});
