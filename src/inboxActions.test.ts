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
import {
  actionTaskText,
  choiceGroups,
  doneNow,
  eachItem,
  eachSaid,
  inboxChoices,
  inboxItemKind,
  leadChoice,
  settleInboxItem,
  suggestKind,
} from "./inboxActions";
import table from "../tests/fixtures/inbox-choices.json";

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

const action = (owner: "me" | "delegated", initiative = "Orbit launch"): InboxItem => ({
  kind: "action",
  path: `wiki/initiatives/${initiative}.md`,
  line: 12,
  lineText: "- Book the room [delegated:: Maya] [status:: open] [due:: 2026-11-01] ^a-2",
  text: "Book the room",
  stamp: "2026-11-01",
  action: {
    id: "a-2",
    initiative,
    owner,
    person: owner === "me" ? null : "Maya",
    due: "2026-11-01",
    since: null,
    overdue: true,
    quotes: [],
  },
});

describe("an initiative's action in the Inbox", () => {
  it("leads with a task for the user's own and a waiting-for for a delegated one", () => {
    expect([inboxChoices(action("me")), leadChoice(action("me"))]).toEqual([
      ["next", "task", "waiting", "someday", "done", "move", "delete"],
      "next",
    ]);
    expect([inboxChoices(action("delegated")), leadChoice(action("delegated"))]).toEqual([
      ["waiting", "next", "task", "someday", "done", "move", "delete"],
      "waiting",
    ]);
    expect(inboxItemKind(action("delegated"))).toEqual({ icon: "waiting", label: "Waiting for Maya, overdue" });
  });

  it("becomes a task naming whose it is and linking the action, shown as its initiative", () => {
    expect(actionTaskText(action("delegated"))).toBe("Maya: Book the room [[Orbit launch#^a-2|Orbit launch]]");
    expect(actionTaskText(action("me", "Unfiled"))).toBe("Book the room [[Unfiled#^a-2|Unfiled]]");
  });

  it("accepted, makes the task and marks the action with where it went; dropped or done, marks it", async () => {
    invoke.mockClear();
    await settleInboxItem(
      action("delegated"),
      "- [ ] Maya: Book the room #waiting-for 📅 2026-11-01",
      "Project. Orbit App launch.md",
      "Waiting for",
    );
    expect(invoke.mock.calls.map((c) => c[0])).toEqual(["project_add_task", "initiative_action_set"]);
    expect(invoke.mock.calls[1][1]).toMatchObject({
      path: "wiki/initiatives/Orbit launch.md",
      line: 12,
      change: { task: "[[Project. Orbit App launch]]" },
    });
    invoke.mockClear();
    await settleInboxItem(action("me"), "- [ ] Book the room #context/calls");
    expect(invoke.mock.calls.map((c) => c[0])).toEqual(["capture", "initiative_action_set"]);
    expect(invoke.mock.calls[1][1]).toMatchObject({ change: { task: "[[Me. To Do List]]" } });
    invoke.mockClear();
    await settleInboxItem(action("me"), null);
    await doneNow(action("me"));
    expect(invoke.mock.calls.map((c) => (c[1] as { change: unknown }).change)).toEqual([{ status: "dropped" }, { status: "done" }]);
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
    expect(inboxChoices(mail)).toEqual(["reply", "ingest", "ask", "next", "task", "project", "waiting", "someday", "reference", "delete"]);
    expect(inboxChoices(doc)).toEqual(["ingest", "ask", "next", "task", "project", "someday", "reference", "delete"]);
    expect([leadChoice(mail), leadChoice(doc)]).toEqual(["reply", "ingest"]);
    expect([suggestKind(mail), suggestKind(doc)]).toEqual(["thread", "capture"]);
  });
  it("gives a thought or task every choice but the capture ones", () => {
    const t = { kind: "task", path: "To Do.md", line: 3, text: "Call Maya", lineText: "- [ ] Call Maya", stamp: null } as InboxItem;
    expect(inboxChoices(t)).toEqual(["next", "task", "project", "waiting", "done", "someday", "reference", "delete"]);
    expect(suggestKind(t)).toBe("task");
  });
  it("groups them as GTD asks: does it need doing? Ask is in neither", () => {
    const mail = cap("sources/Email. Thread. Re Launch - 2026-10-02.md");
    expect(choiceGroups(mail)).toEqual({
      doing: ["reply", "next", "task", "project", "waiting"],
      notDoing: ["ingest", "someday", "reference", "delete"],
      ask: true,
    });
    const t = cap("sources/Teams. Transcript. Steerco - 2026-10-02.md");
    expect(choiceGroups(t)).toEqual({ doing: [], notDoing: ["meeting", "ingest", "reference", "delete"], ask: true });
    const task = { kind: "task", path: "To Do.md", line: 3, text: "Call Maya", lineText: "- [ ] Call Maya", stamp: null } as InboxItem;
    expect(choiceGroups(task).doing).toEqual(["next", "task", "project", "waiting", "done"]);
  });
  it("knows a source's type in a subfolder of sources/ too, by its name", () => {
    expect(suggestKind(cap("sources/emails/Email. Steerco minutes - 2026-09-28.md"))).toBe("thread");
    expect(suggestKind(cap("sources/meetings/2026/Teams. Transcript. Steerco - 2026-10-02.md"))).toBe("transcript");
    expect(suggestKind(cap("sources/teams/Teams. Chat. Standup - 2026-10-03.md"))).toBe("thread");
    expect(suggestKind(cap("notes/Email. Draft.md"))).toBe("capture");
  });
  it("offers what the shared table says, as Suggest's Rust check does (tests/fixtures/inbox-choices.json)", () => {
    for (const k of table.kinds) {
      const i = { ...cap(k.item.path), kind: k.item.kind } as InboxItem;
      expect([suggestKind(i), inboxChoices(i)]).toEqual([k.kind, k.choices]);
    }
  });
  it("draws each kind of item with its own icon", () => {
    const icon = (path: string) => inboxItemKind(cap(path)).icon;
    expect(icon("sources/Teams. Transcript. Steerco - 2026-10-02.md")).toBe("mic");
    expect(icon("sources/emails/Email. Thread. Re Launch - 2026-10-02.md")).toBe("mail");
    expect(icon("sources/Teams. Chat. Standup - 2026-10-03.md")).toBe("chat");
    expect(inboxItemKind(cap("sources/slack/Slack. Chat. Lena - 2026-11-16.md"))).toEqual({ icon: "chat", label: "Slack conversation" });
    expect(
      ["Plan.docx", "Plan.PDF", "Deck.pptx", "Budget.xlsx", "Board.png", "Minutes.md", "Data.zip"].map((f) => icon(`sources/${f}`)),
    ).toEqual(["file-word", "file-pdf", "slides", "sheet", "image", "note", "source"]);
    expect(inboxItemKind({ ...cap("Scratchpad.md"), kind: "thought" }).icon).toBe("bulb");
    expect(inboxItemKind({ ...cap("To Do.md"), kind: "task" }).icon).toBe("tasks");
  });
});

describe("a choice for several ticked items", () => {
  it("carries on past one that fails, and says which went through", async () => {
    const r = await eachItem(["a", "b", "c"], async (x) => {
      if (x === "b") throw { code: "changed", message: "the file changed since it was read" };
    });
    expect(r.ok).toEqual(["a", "c"]);
    expect(r.failed).toEqual([["b", "the file changed since it was read"]]);
    expect(eachSaid("Kept in Sources", 2, [])).toBe("Kept in Sources (2)");
    expect(eachSaid("Kept in Sources", 2, ["Plan: the file changed since it was read"])).toBe(
      "2 done, 1 failed: Plan: the file changed since it was read",
    );
  });
});
