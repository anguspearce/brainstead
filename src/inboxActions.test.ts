// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (_cmd: string, _a?: Record<string, unknown>): Promise<unknown> => ({ line: 0, lineText: "", undo: "Done" }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (c: string, a?: Record<string, unknown>) => invoke(c, a),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { InboxItem } from "./api";
import { doneNow, inboxChoices, leadChoice, suggestKind } from "./inboxActions";

const item = (lineText: string): InboxItem =>
  ({ kind: "task", path: "Me. To Do List.md", line: 4, lineText, text: lineText.replace(/^- \[ \] /, "") }) as InboxItem;

describe("Done now in the Inbox", () => {
  it("adds a recurring task's next occurrence, as everywhere else", async () => {
    invoke.mockClear();
    await doneNow(item("- [ ] Water the plants 🔁 every week 📅 2026-10-01"));
    const [cmd, a] = invoke.mock.calls[0];
    expect(cmd).toBe("task_replace");
    const lines = a!.lines as string[];
    expect(lines).toHaveLength(2);
    expect(lines[0]).toMatch(/^- \[ \] Water the plants 🔁 every week 📅 2026-10-08$/);
    expect(lines[1]).toMatch(/^- \[x\] Water the plants 🔁 every week 📅 2026-10-01 ✅ \d{4}-\d\d-\d\d$/);
  });

  it("ticks a plain task", async () => {
    invoke.mockClear();
    await doneNow(item("- [ ] Call Sam"));
    expect(invoke.mock.calls[0][0]).toBe("task_toggle");
  });
});

describe("each kind of Inbox item's choices", () => {
  const cap = (path: string) => ({ kind: "capture", path, line: 0, text: "", lineText: "", stamp: null }) as InboxItem;
  it("leads a transcript with its meeting note, with the source actions and keeping or trashing it", () => {
    const t = cap("sources/Teams. Transcript. Steerco - 2026-10-02.md");
    expect(inboxChoices(t)).toEqual(["meeting", "ingest", "ask", "reference", "delete"]);
    expect(leadChoice(t)).toBe("meeting");
    expect(suggestKind(t)).toBe("transcript");
  });
  it("lets a thread be answered and waited on, but not another capture", () => {
    const mail = cap("sources/Email. Thread. Re Launch - 2026-10-02.md");
    const doc = cap("sources/Orbit App plan.pdf");
    expect(inboxChoices(mail)).toEqual(["reply", "ingest", "ask", "next", "project", "waiting", "someday", "reference", "delete"]);
    expect(inboxChoices(doc)).toEqual(["ingest", "ask", "next", "project", "someday", "reference", "delete"]);
    expect([leadChoice(mail), leadChoice(doc)]).toEqual(["reply", "ingest"]);
    expect([suggestKind(mail), suggestKind(doc)]).toEqual(["thread", "capture"]);
  });
  it("gives a thought or task every choice but the capture ones", () => {
    const t = { kind: "task", path: "To Do.md", line: 3, text: "Call Maya", lineText: "- [ ] Call Maya", stamp: null } as InboxItem;
    expect(inboxChoices(t)).toEqual(["next", "project", "waiting", "done", "someday", "reference", "delete"]);
    expect(suggestKind(t)).toBe("task");
  });
  it("knows a source's type in a subfolder of sources/ too, by its name", () => {
    expect(suggestKind(cap("sources/emails/Email. Steerco minutes - 2026-09-28.md"))).toBe("thread");
    expect(suggestKind(cap("sources/meetings/2026/Teams. Transcript. Steerco - 2026-10-02.md"))).toBe("transcript");
    expect(suggestKind(cap("sources/teams/Teams. Chat. Standup - 2026-10-03.md"))).toBe("thread");
    expect(suggestKind(cap("notes/Email. Draft.md"))).toBe("capture");
  });
});
