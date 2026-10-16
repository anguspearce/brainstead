// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, vi } from "vitest";

type Call = [string, Record<string, unknown> | undefined];
const calls: Call[] = [];
let answers: Record<string, (a: Record<string, unknown> | undefined) => unknown> = {};
const invoke = vi.fn(async (cmd: string, a?: Record<string, unknown>): Promise<unknown> => {
  calls.push([cmd, a]);
  const f = answers[cmd];
  return f ? f(a) : undefined;
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (c: string, a?: Record<string, unknown>) => invoke(c, a),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { InboxItem, TaskRow } from "./api";
import { findTask, Listing, runAction, taskLineOut } from "./mcpActions";
import { settings } from "./store";
import { focusMode, startFocus } from "./focus";
import { nav } from "./nav";
import { imageView } from "./ImageViewer";
import { followUp } from "./Ingest";
import { isMac, keyLabel, platform, setPlatform, terminalApp } from "./platform";

const row = (over: Partial<TaskRow>): TaskRow => ({
  path: "Me. To Do List.md",
  title: "To Do List",
  line: 11,
  lineText: "- [ ] Call Sam about the launch",
  text: "Call Sam about the launch",
  status: " ",
  done: false,
  due: null,
  scheduled: null,
  start: null,
  doneOn: null,
  created: null,
  rank: null,
  tags: [],
  heading: "Other",
  ...over,
});

const edited = (a: Record<string, unknown> | undefined) => ({
  line: (a?.line as number) ?? 0,
  lineText: "- [x] Call Sam about the launch",
  undo: "Ticked",
});
const raw = (action: string, args: Record<string, unknown> = {}) => runAction({ id: "1", action, args });
/** An action's text: a listing's text, or what it gives as it is. */
const run = async (action: string, args: Record<string, unknown> = {}) => {
  const r = await raw(action, args);
  return r && typeof r === "object" && "structured" in r ? (r as Listing).text : r;
};
/** A listing's rows as data, as the server passes them on as structuredContent. */
const data = async (action: string, args: Record<string, unknown> = {}) => ((await raw(action, args)) as Listing).structured;

describe("MCP actions", () => {
  beforeEach(() => {
    calls.length = 0;
    answers = {};
  });

  it("finds a task by its id, or by its text when the line moved", () => {
    const all = [row({}), row({ line: 20, text: "Book the room", lineText: "- [ ] Book the room" })];
    expect(findTask(all, "Me. To Do List.md:12").text).toBe("Call Sam about the launch");
    expect(findTask(all, "Me. To Do List.md:3", "Book the room").line).toBe(20);
    expect(() => findTask(all, "Me. To Do List.md:12", "Something else")).toThrow(/no task/);
    expect(() => findTask(all, "nope")).toThrow(/isn't a task id/);
  });

  it("lists a view, one line per task with its id", async () => {
    answers.tasks_all = () => [
      row({ due: "2026-10-06", project: "Project. Orbit App launch.md" }),
      row({ line: 3, text: "Old", done: true }),
    ];
    const out = (await run("tasks.list", { view: "next" })) as string;
    expect(out).toMatch(/^1 task in next:/);
    expect(out).toContain("- [ ] Call Sam about the launch · due 2026-10-06 · project Orbit App launch  (Me. To Do List.md:12)");
    expect(taskLineOut(row({ effort: "15m" }), true)).toContain("effort 15m");
    await expect(run("tasks.list", { view: "soon" })).rejects.toThrow(/No view/);
  });

  it("matches query as the Tasks screen's search does: project, heading and tags too, accents ignored", async () => {
    answers.tasks_all = () => [
      row({ text: "Book the café", lineText: "- [ ] Book the café", project: "Project. Orbit App launch.md" }),
      row({ line: 4, text: "Call Lena", lineText: "- [ ] Call Lena", heading: "Venue", tags: ["#followup"] }),
    ];
    const ids = async (query: string) => ((await data("tasks.list", { view: "all", query })).items as { id: string }[]).map((t) => t.id);
    expect(await ids("cafe orbit")).toEqual(["Me. To Do List.md:12"]);
    expect(await ids("venue")).toEqual(["Me. To Do List.md:5"]);
    expect(await ids("#followup")).toEqual(["Me. To Do List.md:5"]);
    expect(await run("tasks.list", { view: "all", query: "cafe" })).toMatch(/^1 task in all matching “cafe”, of 2:/);
  });

  /** change_submit_many answers: each made, or each held. */
  const outcomes = (applied: boolean) => (a: Record<string, unknown> | undefined) =>
    (a!.changes as { page: string }[]).map((c, i) => ({
      id: `c${i}`,
      applied,
      page: c.page,
      flags: applied ? [] : ["It changes or takes out text in your own note."],
      message: applied ? `Changed ${c.page}. Revert it in Changes.` : "Held for the user in Changes.",
    }));
  const submitted = () => calls.filter(([c]) => c === "changes_submit_many").flatMap(([, a]) => a!.changes as Record<string, unknown>[]);

  it("edits a task through Changes: worked out edit by edit, made as one change", async () => {
    answers.tasks_all = () => [row({})];
    answers.change_task_line = (a) => {
      const e = a!.edit as { op: string; date?: string; names?: string[] };
      const l = a!.line as string;
      return e.op === "toggle" ? l.replace("[ ]", "[x]") : e.op === "date" ? `${l} 📅 ${e.date}` : `${l} #context/${e.names![0]}`;
    };
    answers.changes_submit_many = outcomes(true);
    const out = (await run("task.edit", {
      task: "Me. To Do List.md:12",
      text: "Call Sam",
      done: true,
      due: "2026-10-09",
      contexts: ["Calls"],
      priority: "high",
    })) as string;
    // Nothing written straight away: no task command, one change.
    expect(calls.some(([c]) => c.startsWith("task_"))).toBe(false);
    const [c] = submitted();
    expect(c).toMatchObject({ page: "Me. To Do List.md", kind: "task", origin: { kind: "chat" } });
    expect(c.instruction).toEqual({
      op: "lines",
      at: 11,
      old: ["- [ ] Call Sam about the launch"],
      new: ["- [x] Call Sam about the launch ⏫ 📅 2026-10-09 #context/calls"],
    });
    expect(out).toMatch(/^Done \(ticked, due 2026-10-09, priority high, contexts calls\)/);
    expect(out).toContain("Revert it in Changes");
    await expect(run("task.edit", { task: "Me. To Do List.md:12", text: "Call Sam" })).rejects.toThrow(/Nothing to change/);
  });

  it("an unattended edit carries its origin, and says when it's held", async () => {
    answers.tasks_all = () => [row({})];
    answers.changes_submit_many = outcomes(false);
    const origin = { kind: "chat", run: "session-1", trigger: "scheduled" };
    const out = (await run("task.edit", { task: "Me. To Do List.md:12", text: "Call Sam", status: "cancelled", origin })) as string;
    expect(submitted()[0].origin).toEqual(origin);
    expect(out).toMatch(/^Held for the user in Changes: It changes or takes out text/);
  });

  it("moves a task by its rank, through Changes", async () => {
    answers.tasks_all = () => [row({ rank: 1024 }), row({ line: 12, lineText: "- [ ] Book the room", text: "Book the room", rank: 2048 })];
    answers.change_task_line = (a) => `${a!.line} [rank:: 1536]`;
    answers.changes_submit_many = outcomes(true);
    await run("task.move", { task: "Me. To Do List.md:13", text: "Book the room", after: "Me. To Do List.md:12" });
    expect(calls.find(([c]) => c === "change_task_line")![1]!.edit).toEqual({ op: "rank", prev: 1024, next: null });
    expect(submitted()[0].instruction).toMatchObject({ op: "lines", at: 12, new: ["- [ ] Book the room [rank:: 1536]"] });
  });

  it("moves a task between its two real neighbours, or renumbers the list, as the screen's drag does", async () => {
    const task = (line: number, text: string, rank: number | null) => row({ line, lineText: `- [ ] ${text}`, text, rank });
    answers.tasks_all = () => [task(1, "Ask Maya", 1024), task(2, "Book the room", 2048), task(3, "Call Lena", 3072)];
    answers.change_task_line = (a) => `${a!.line} ^rank-${JSON.stringify(a!.edit)}`;
    answers.changes_submit_many = outcomes(true);
    await run("task.move", { task: "Me. To Do List.md:4", text: "Call Lena", after: "Me. To Do List.md:2" });
    // Between Ask Maya and Book the room, not on Book the room's rank.
    expect(calls.find(([c]) => c === "change_task_line")![1]!.edit).toEqual({ op: "rank", prev: 1024, next: 2048 });
    expect(submitted()).toHaveLength(1);
    // A row with no rank: the list renumbered in its new order, each line a change made together.
    calls.length = 0;
    answers.tasks_all = () => [task(1, "Ask Maya", null), task(2, "Book the room", 2048), task(3, "Call Lena", 3072)];
    const out = (await run("task.move", { task: "Me. To Do List.md:4", text: "Call Lena", before: "Me. To Do List.md:3" })) as string;
    const edits = calls.filter(([c]) => c === "change_task_line").map(([, a]) => [a!.line, a!.edit]);
    expect(edits).toEqual([
      ["- [ ] Ask Maya", { op: "rank", prev: 0, next: null }],
      ["- [ ] Call Lena", { op: "rank", prev: 1024, next: null }],
      ["- [ ] Book the room", { op: "rank", prev: 2048, next: null }],
    ]);
    expect(submitted().map((c) => c.title)).toEqual([
      "Renumber “ask maya” in Next actions",
      "Move “call lena” before “book the room”",
      "Renumber “book the room” in Next actions",
    ]);
    expect(out).toContain("renumbered, 3 lines");
  });

  it("deletes a transcript from the Inbox as the screen's Delete does, its links pointed at its note first", async () => {
    const path = "sources/Teams. Transcript. Offsite - 2026-10-06.md";
    answers.inbox_list = () => [
      { kind: "capture", path, line: 0, text: "Teams. Transcript. Offsite - 2026-10-06", lineText: "", stamp: null },
    ];
    answers.meeting_transcripts = () => [
      { path, done: null, inferred: { type: "Meeting", name: "Offsite", date: "2026-10-06", exists: true, dateCheck: false, ask: [] } },
    ];
    answers.doc_read = () => ({ content: "", meta: { backlinks: [{ path: "wiki/entities/Offsite.md" }] } });
    answers.health_link_ghost = () => 1;
    answers.change_submit = () => ({ id: "c9", applied: true, page: path, flags: [], message: "Moved it to the Trash." });
    calls.length = 0;
    const origin = { kind: "chat", run: "s1", trigger: "scheduled" };
    expect(await run("inbox.clarify", { item: "capture:1", becomes: "delete", origin })).toBe(
      "Clarified as deleted. Moved it to the Trash.",
    );
    expect(calls.find(([c]) => c === "health_link_ghost")![1]).toMatchObject({ to: "Meeting. Offsite - 2026-10-06.md", unattended: true });
    expect(calls.find(([c]) => c === "change_submit")![1]).toMatchObject({ change: { page: path, kind: "trash", origin } });
    // Not marked done, and no plain trash: restored, it's back in the Inbox.
    expect(calls.some(([c]) => c === "trash_move" || c === "inbox_capture_done" || c === "changes_submit_many")).toBe(false);
  });

  it("clarifies an Inbox item through Changes, its edits held together", async () => {
    const task: InboxItem = {
      kind: "task",
      path: "Me. To Do List.md",
      line: 33,
      text: "ring the venue",
      lineText: "- [ ] ring the venue",
      stamp: null,
    };
    const capture: InboxItem = {
      kind: "capture",
      path: "sources/Email. Thread. Hi - 2026-10-02.md",
      line: 0,
      text: "Email. Thread. Hi - 2026-10-02",
      lineText: "Email thread",
      stamp: "2026-10-02",
    };
    const thought: InboxItem = {
      kind: "thought",
      path: "Me. Scratchpad.md",
      line: 4,
      text: "Ask Lena about the beta",
      lineText: "### 2026-10-02 09:00",
      stamp: "2026-10-02 09:00",
      block: "### 2026-10-02 09:00\nAsk Lena about the beta\n",
    };
    answers.inbox_list = () => [task, capture, thought];
    answers.projects_list = () => [{ name: "Orbit App launch", path: "Project. Orbit App launch.md", status: "active" }];
    answers.changes_submit_many = outcomes(true);
    await run("inbox.clarify", { item: "task:34", becomes: "next", text: "Ring the venue", context: "calls" });
    expect(submitted()[0].instruction).toEqual({
      op: "lines",
      at: 33,
      old: ["- [ ] ring the venue"],
      new: ["- [ ] Ring the venue #context/calls"],
    });
    // To a project: out of the To Do list and into the project, as one change.
    calls.length = 0;
    await run("inbox.clarify", { item: "thought:5", becomes: "next", project: "Orbit App launch" });
    expect(submitted().map((c) => [c.page, (c.instruction as { op: string }).op])).toEqual([
      ["Project. Orbit App launch.md", "add_task"],
      ["Me. Scratchpad.md", "lines"],
    ]);
    expect(submitted()[1].instruction).toMatchObject({ old: ["### 2026-10-02 09:00", "Ask Lena about the beta"], new: [] });
    // Waiting for in a project: under the project's Waiting for, as the Inbox files it.
    calls.length = 0;
    await run("inbox.clarify", { item: "task:34", becomes: "waiting", project: "Orbit App launch" });
    expect(submitted()[1]).toMatchObject({
      page: "Project. Orbit App launch.md",
      instruction: { op: "add_task", line: "- [ ] ring the venue [[Project. Orbit App launch]] #waiting-for", heading: "Waiting for" },
    });
    // A capture: trashed through Changes, and not marked done, so restored it's back in the Inbox.
    calls.length = 0;
    await run("inbox.clarify", { item: "capture:1", becomes: "delete" });
    expect(calls.map(([c]) => c)).toEqual(["inbox_list", "changes_submit_many"]);
    expect(submitted()[0]).toMatchObject({ kind: "trash", instruction: { op: "trash" } });
    // Held: the capture stays in the Inbox until the user accepts.
    calls.length = 0;
    answers.changes_submit_many = outcomes(false);
    const out = (await run("inbox.clarify", { item: "capture:1", becomes: "next", context: "office" })) as string;
    expect(calls.some(([c]) => c === "inbox_capture_done")).toBe(false);
    expect(out).toContain("stays in the Inbox");
    await expect(run("inbox.clarify", { item: "task:99", becomes: "next" })).rejects.toThrow(/No Inbox item/);
    // A next action with neither project nor context: refused, as the Inbox's form won't save it; waiting is fine.
    calls.length = 0;
    await expect(run("inbox.clarify", { item: "task:34", becomes: "next" })).rejects.toThrow(/needs a project or a context/);
    expect(calls.some(([c]) => c === "changes_submit_many")).toBe(false);
    answers.changes_submit_many = outcomes(true);
    await run("inbox.clarify", { item: "task:34", becomes: "waiting" });
    expect(submitted()[0].instruction).toMatchObject({ new: ["- [ ] ring the venue #waiting-for"] });
    // Only an active project, as the Inbox's forms list them.
    answers.projects_list = () => [{ name: "Acme rollout", path: "Project. Acme rollout.md", status: "on-hold" }];
    await expect(run("inbox.clarify", { item: "task:34", becomes: "next", project: "Acme rollout" })).rejects.toThrow(
      /isn't an active project/,
    );
  });

  it("makes and changes projects through Changes", async () => {
    answers.projects_list = () => [{ name: "Orbit App launch", path: "Project. Orbit App launch.md", status: "active" }];
    answers.change_project_note = (a) => ({ path: `Project. ${a!.name}.md`, content: "---\nstatus: active\n---\n" });
    answers.changes_submit_many = outcomes(true);
    await run("project.create", { name: "Orbit App beta" });
    expect(submitted()[0]).toMatchObject({ page: "Project. Orbit App beta.md", kind: "new", instruction: { op: "page" } });
    calls.length = 0;
    // The screen's names in, the vault's written: Completed is status done, Done looks like is outcome.
    await run("project.update", { project: "Orbit App launch", status: "completed", area: null, done_looks_like: "Live for all staff" });
    expect(submitted()[0].instruction).toEqual({
      op: "properties",
      set: [
        ["status", "done"],
        ["area", null],
        ["outcome", "Live for all staff"],
      ],
    });
    expect(calls.some(([c]) => c === "project_set" || c === "project_create")).toBe(false);
    await expect(run("project.update", { project: "Orbit App launch", status: "blocked" })).rejects.toThrow(/status is/);
    await expect(run("project.update", { project: "Orbit App launch", status: "done" })).rejects.toThrow(/completed/);
  });

  it("orders projects and areas as dragging on the Projects screen does, and lists them so", async () => {
    settings.update({ projectOrder: undefined });
    answers.projects_list = () => [
      { name: "Orbit App launch", path: "Project. Orbit App launch.md", status: "active", area: "Work" },
      { name: "Acme review", path: "Project. Acme review.md", status: "active", area: "Work" },
      { name: "Garden", path: "Project. Garden.md", status: "active", area: "Personal" },
      { name: "Loose end", path: "Project. Loose end.md", status: "active", area: null },
    ];
    expect(await run("project.move", { project: "Orbit App launch", before: "Acme review" })).toMatch(
      /Work is now: Orbit App launch, Acme review/,
    );
    expect(await run("project.move", { area: "work", before: "Personal" })).toMatch(/now: Work, Personal, No area/);
    expect(settings.get().projectOrder).toEqual({
      // Every project, so the order holds across tabs; only the order within an area matters.
      projects: ["Project. Garden.md", "Project. Orbit App launch.md", "Project. Acme review.md", "Project. Loose end.md"],
      areas: ["Work", "Personal"],
    });
    const listed = String(await run("projects.list", {}));
    expect(listed.indexOf("Orbit App launch")).toBeLessThan(listed.indexOf("Acme review"));
    expect(listed.indexOf("Acme review")).toBeLessThan(listed.indexOf("Garden"));
    await expect(run("project.move", { project: "Garden", after: "Acme review" })).rejects.toThrow(/own area/);
    await expect(run("project.move", { project: "Garden" })).rejects.toThrow(/after or before/);
    settings.update({ projectOrder: undefined });
  });

  it("renames an area on every project under it, as the area's pencil does", async () => {
    answers.projects_list = () => [
      { name: "Orbit App launch", path: "Project. Orbit App launch.md", status: "active", area: "Work" },
      { name: "Orbit App beta", path: "Project. Orbit App beta.md", status: "done", area: "Work" },
      { name: "Garden", path: "Project. Garden.md", status: "active", area: "Personal" },
    ];
    answers.changes_submit_many = outcomes(true);
    settings.update({ projectOrder: { areas: ["Personal", "Work"] } });
    await run("project.rename_area", { area: "Work", to: "Acme" });
    // Whatever their status, and one change each, made together.
    expect(submitted().map((c) => c.page)).toEqual(["Project. Orbit App launch.md", "Project. Orbit App beta.md"]);
    expect(submitted()[0].instruction).toEqual({ op: "properties", set: [["area", "Acme"]] });
    expect(calls.some(([c]) => c === "projects_set_area")).toBe(false);
    // Its place in the order, and Work's kept for a Revert in Changes.
    expect(settings.get().projectOrder?.areas).toEqual(["Personal", "Acme", "Work"]);
    settings.update({ projectOrder: undefined });
    await expect(run("project.rename_area", { area: "Hobbies", to: "Acme" })).rejects.toThrow(/Personal, Work/);
  });

  it("makes an assistant's change, and accepts, rejects and reverts held ones", async () => {
    answers.change_submit = () => ({
      id: "c1",
      applied: true,
      page: "wiki/entities/Orbit App.md",
      flags: [],
      message: "Changed Orbit App: x. Revert it in Changes.",
    });
    const change = {
      page: "wiki/entities/Orbit App.md",
      kind: "edit",
      title: "x",
      instruction: { op: "page", content: "y" },
      origin: { kind: "chat" },
    };
    expect(await run("change.submit", { change })).toBe("Changed Orbit App: x. Revert it in Changes.");
    expect(calls.find(([c]) => c === "change_submit")![1]).toEqual({ change });
    answers.changes_list = () => [];
    answers.change_accept = () => ({ id: "c2", applied: true, page: "a.md", flags: [], message: "Changed a." });
    expect(await run("changes", { action: "accept", id: "c2" })).toBe("Changed a.");
    // As an assistant: the app accepts only a change held for a failed check.
    expect(calls.find(([c]) => c === "change_accept")![1]).toEqual({ id: "c2", assistant: true });
    answers.change_revert = () => ({ ok: false, message: "Its lines have been edited since.", before: "old" });
    expect(await run("changes", { action: "revert", id: "c1" })).toMatch(/edited since/);
    answers.changes_accept_all = () => ({ done: 2, failed: [] });
    expect(await run("changes", { action: "accept_run", group: "daily-check-1" })).toBe("Accepted 2.");
    expect(calls.find(([c]) => c === "changes_accept_all")![1]).toEqual({ group: "daily-check-1", assistant: true, ids: null });
    await expect(run("changes", { action: "accept" })).rejects.toThrow(/Which change/);
    // Nobody watching: nothing is accepted.
    calls.length = 0;
    await expect(run("changes", { action: "accept", id: "c2", unattended: true })).rejects.toThrow(/nobody is watching/);
    await expect(run("changes", { action: "accept_run", group: "g", unattended: true })).rejects.toThrow(/nobody is watching/);
    expect(calls.length).toBe(0);
  });

  it("offers a held meeting note's follow-up once it's accepted, as the Changes screen does", async () => {
    const held = {
      id: "m1",
      group: "meeting-1",
      page: "Meeting. Orbit App Steerco - 2026-10-01.md",
      status: "held",
      origin: { kind: "meeting", chat: "sources/Transcript. Steerco - 2026-10-01.md" },
    };
    answers.changes_list = () => [held];
    answers.change_accept = () => ({ id: "m1", applied: true, page: held.page, flags: [], message: "Made it." });
    followUp.set([]);
    const out = (await run("changes", { action: "accept", id: "m1" })) as string;
    expect(followUp.get()).toEqual([{ note: held.page, transcript: held.origin.chat }]);
    // Accepted, the note is made: its transcript leaves the Inbox however the offer is answered.
    expect(calls.find(([c]) => c === "inbox_capture_done")![1]).toEqual({ path: held.origin.chat });
    expect(out).toMatch(
      /^Made it\. Meeting\. Orbit App Steerco - 2026-10-01 is a meeting note: Brainstead now asks the user whether to ingest it/,
    );
    // A run's: offered for each note that was made, and not for one that couldn't be.
    followUp.set([]);
    let accepted = false;
    answers.changes_list = () => [{ ...held, status: accepted ? "applied" : "held" }];
    answers.changes_accept_all = () => ((accepted = true), { done: 1, failed: [] });
    expect(await run("changes", { action: "accept_run", group: "meeting-1" })).toMatch(/^Accepted 1\. .* is a meeting note/);
    expect(followUp.get()).toHaveLength(1);
    followUp.set([]);
    accepted = false;
    answers.changes_accept_all = () => ({ done: 0, failed: ["Meeting: the page changed"] });
    expect(await run("changes", { action: "accept_run", group: "meeting-1" })).toBe("Accepted 0. Couldn't: Meeting: the page changed");
    expect(followUp.get()).toEqual([]);
  });

  it("does a run's changes the query finds, as Changes' run buttons do with its search on", async () => {
    const row = (id: string, title: string, status: string) => ({
      id,
      group: "daily-check-1",
      title,
      page: `wiki/${title}.md`,
      status,
      origin: { kind: "run", run: "daily-check-1" },
      flags: [],
    });
    answers.changes_list = () => [row("a", "Orbit App", "held"), row("b", "Lena", "held"), row("c", "Orbit App launch", "applied")];
    answers.changes_accept_all = () => ({ done: 1, failed: [] });
    answers.changes_reject_all = () => ({ done: 1, failed: [] });
    answers.changes_revert_all = () => ({ done: 1, failed: [] });
    calls.length = 0;
    await run("changes", { action: "accept_run", group: "daily-check-1", query: "orbit" });
    expect(calls.find(([c]) => c === "changes_accept_all")![1]).toEqual({ group: "daily-check-1", assistant: true, ids: ["a"] });
    await run("changes", { action: "reject_run", group: "daily-check-1", query: "lena" });
    expect(calls.find(([c]) => c === "changes_reject_all")![1]).toEqual({ group: "daily-check-1", ids: ["b"] });
    await run("changes", { action: "revert_run", group: "daily-check-1", query: "orbit" });
    expect(calls.find(([c]) => c === "changes_revert_all")![1]).toEqual({ group: "daily-check-1", ids: ["c"] });
    await expect(run("changes", { action: "reject_run", group: "daily-check-1", query: "maya" })).rejects.toThrow(/No held change/);
  });

  it("reads and sets how long Changes keeps its history", async () => {
    expect(await run("changes", { action: "history" })).toMatch(/for 90 days or up to 500 MB/);
    expect(await data("changes", { action: "history" })).toEqual({ history: { days: 90, mb: 500 } });
    expect(await run("changes", { action: "history", days: 180, mb: 1000 })).toMatch(/180 days or up to 1 GB/);
    expect(await run("changes", { action: "history" })).toMatch(/180 days/);
    await expect(run("changes", { action: "history", days: 7 })).rejects.toThrow(/days is 30, 90/);
  });

  it("retires the previous app's files, and starts the weekly review over", async () => {
    answers.switchover_status = () => ({ automated: null, skills: true, previous: true, scripts: false, claudeMd: false });
    answers.capture_status = () => ({ recent: [] });
    answers.switchover_retire = () => "Retired: .claude/skills/, CLAUDE.md updated. The folders are in the Trash.";
    settings.update({ movingOver: {}, readOnly: false });
    // The checklist as the screen lists it: what Brainstead checks, and what the user ticks.
    const list = (await run("moving_over", { action: "list" })) as string;
    expect(list).toMatch(/^Moving over from another notes app \(Settings › General\): \d+ of 8 done\./);
    expect(list).toContain("- [ ] The other app is stopped · ticked by the user: Brainstead doesn't need it running.  (otherStopped)");
    expect(list).toContain("- [x] Brainstead can write to the vault · Brainstead checks this: Read-only is off.  (writes)");
    // Retire them waits for The other app is stopped, as the screen's button does.
    await expect(run("moving_over", { action: "retire" })).rejects.toThrow(/Stop the other app first/);
    expect(calls.some(([c]) => c === "switchover_retire")).toBe(false);
    await expect(run("moving_over", { action: "tick", item: "writes" })).rejects.toThrow(/checks this itself/);
    expect(await run("moving_over", { action: "tick", item: "otherStopped" })).toBe("Ticked “The other app is stopped”.");
    expect(settings.get().movingOver).toEqual({ otherStopped: true });
    expect(await run("moving_over", { action: "retire" })).toMatch(/^Retired: \.claude\/skills\//);
    expect(calls.some(([c]) => c === "switchover_retire")).toBe(true);
    expect(await run("moving_over", { action: "untick", item: "otherStopped" })).toBe("Unticked “The other app is stopped”.");
    answers.weekly_state_read = () => ({ week: "2026-W39", step: 3, done: [0, 1, 2], log: [], notes: "x", startedAt: 1 });
    const out = (await run("weekly.start_over")) as string;
    const st = calls.find(([c]) => c === "weekly_state_write")![1]!.state as { step: number; done: number[]; notes: string };
    expect([st.step, st.done, st.notes]).toEqual([0, [], ""]);
    expect(out).toMatch(/paused on step 4 is dropped/);
  });

  it("takes the Deferred list as deferred, and scheduled as before", async () => {
    answers.tasks_all = () => [row({ scheduled: "2099-01-01" })];
    const out = (await run("tasks.list", { view: "deferred" })) as string;
    expect(out).toMatch(/^1 task in deferred:/);
    expect(await run("tasks.list", { view: "scheduled" })).toBe(out);
  });

  it("starts and stops the summaries by their names", async () => {
    answers.review_run_now = () => "run-1";
    await run("run.start", { run: "daily_summary", date: "2026-10-03" });
    await run("run.start", { run: "weekly_summary", unattended: true });
    expect(calls.filter(([c]) => c === "review_run_now").map(([, a]) => a)).toEqual([
      { kind: "daily", day: "2026-10-03", unattended: false },
      { kind: "weekly", day: null, unattended: true },
    ]);
    // Every run is told when nobody's watching, not only an ingest.
    answers.contradictions_run = () => true;
    await run("run.start", { run: "daily_check", unattended: true });
    await run("run.start", { run: "contradictions", unattended: true });
    expect(calls.filter(([c]) => c === "daily_check_run_now" || c === "contradictions_run").map(([, a]) => a)).toEqual([
      { unattended: true },
      { unattended: true },
    ]);
    await run("run.stop", { run: "weekly_summary" });
    await run("run.stop", { run: "daily_summary" });
    expect(calls.filter(([c]) => c === "reviews_stop").map(([, a]) => a)).toEqual([{ kind: "weekly" }, { kind: "daily" }]);
    await expect(run("run.start", { run: "monthly" })).rejects.toThrow(/daily_summary, weekly_summary/);
  });

  it("refuses to ingest a file an ingest can't read, as the screens offer no Ingest on it", async () => {
    answers.ingest_start = () => ["r1"];
    await expect(run("run.start", { run: "ingest", sources: ["sources/Plan.pdf", "sources/Diagram.svg"] })).rejects.toThrow(
      /can't read sources\/Diagram\.svg/,
    );
    expect(calls.some(([c]) => c === "ingest_start")).toBe(false);
    expect(await run("run.start", { run: "ingest", sources: ["sources/Plan.pdf", "sources/Whiteboard.png"] })).toMatch(/^Ingesting/);
  });

  describe("the weekly review", () => {
    const task = { path: "Me. To Do List.md", line: 11, lineText: "- [ ] Call Sam about the launch", text: "Call Sam about the launch" };
    const state = {
      week: "2099-W01",
      step: 2,
      done: [0, 1],
      log: ["Added “Book the room”"],
      notes: "A busy week.",
      startedAt: 1,
      handled: { s0: "skipped" },
    };
    const prep = {
      week: "2099-W01",
      preparedAt: "2099-01-02T12:00:00",
      model: "m",
      stamp: "",
      dropped: 0,
      suggestions: [
        { id: "s0", step: "next", text: "Done already", source: null, action: { do: "tick", task } },
        {
          id: "s1",
          step: "next",
          text: "The launch call happened",
          source: { path: "Meeting. Launch.md", quote: "called Sam" },
          action: { do: "tick", task },
        },
        { id: "s2", step: "loose", text: "Anything from the offsite?", source: null, action: null },
      ],
    };
    beforeEach(() => {
      answers.weekly_state_read = () => state;
      answers.weekprep_status = () => ({ prep, running: false, error: null });
      answers.projects_list = () => [];
      answers.tasks_all = () => [row({})];
      answers.task_toggle = edited;
    });

    it("gives the week, the progress and the suggestions still to decide, by step", async () => {
      const out = (await run("weekly.status")) as string;
      expect(out).toContain("The weekly review for 2099-W01.");
      expect(out).toContain("on step 3 of 11, Process this week's notes; 2 done (Collect loose ends, Empty the inbox)");
      expect(out).toContain("Notes: A busy week.");
      expect(out).toContain("2 still to decide, 1 accepted or skipped");
      expect(out.indexOf("Collect loose ends:")).toBeLessThan(out.indexOf("Next actions:"));
      expect(out).toContain(
        "- The launch call happened · from Meeting. Launch.md (“called Sam”) · accepting: Tick “Call Sam about the launch” off as done  (id s1)",
      );
      expect(out).not.toContain("Done already");
    });

    it("gives the week's review note once it's finished", async () => {
      answers.weekly_state_read = () => null;
      answers.weekly_review_note = () => "Me. Weekly Review - 2099-W01.md";
      const out = (await run("weekly.status")) as string;
      expect(out).toContain("Finished: the user's review of the week is in Me. Weekly Review - 2099-W01.md");
      expect(out).not.toContain("Not started");
    });

    it("accepts a suggestion with the screen's undoable action, and keeps it with the progress", async () => {
      const out = (await run("weekly.suggestion", { id: "s1", action: "accept" })) as string;
      expect(out).toMatch(/^Accepted: Ticked/);
      expect(calls.some(([c]) => c === "task_toggle")).toBe(true);
      const saved = calls.find(([c]) => c === "weekly_state_write")![1]!.state as typeof state;
      expect(saved.handled).toEqual({ s0: "skipped", s1: "accepted" });
      expect(saved.log).toEqual(["Added “Book the room”", "Ticked"]);
    });

    it("skips one, and refuses a prompt, one dealt with, or an unknown id", async () => {
      await run("weekly.suggestion", { id: "s2", action: "skip" });
      expect((calls.find(([c]) => c === "weekly_state_write")![1]!.state as typeof state).handled).toEqual({
        s0: "skipped",
        s2: "skipped",
      });
      await expect(run("weekly.suggestion", { id: "s2", action: "accept" })).rejects.toThrow(/prompt to think about/);
      await expect(run("weekly.suggestion", { id: "s0", action: "skip" })).rejects.toThrow(/skipped already/);
      await expect(run("weekly.suggestion", { id: "nope", action: "skip" })).rejects.toThrow(/No suggestion/);
    });

    it("carries unattended into an accepted link's change, and says when it's held", async () => {
      const link = { do: "link", path: "Meeting. Launch.md", phrase: "Orbit App", target: "Orbit App" };
      answers.weekprep_status = () => ({
        prep: { ...prep, suggestions: [...prep.suggestions, { id: "s3", step: "notes", text: "Link it", source: null, action: link }] },
        running: false,
        error: null,
      });
      answers.weekprep_link = () => ({ id: "c1", applied: false, page: link.path, flags: ["A check failed."], message: "Held" });
      const out = (await run("weekly.suggestion", { id: "s3", action: "accept", unattended: true })) as string;
      expect(calls.find(([c]) => c === "weekprep_link")![1]).toMatchObject({ path: link.path, unattended: true });
      expect(out).toMatch(
        /^Accepted: Held linking Orbit App in Meeting\. Launch for the user in Changes: A check failed\. Nothing is changed/,
      );
    });

    it("doesn't start a review to accept or skip one in", async () => {
      answers.weekly_state_read = () => null;
      await expect(run("weekly.suggestion", { id: "s1", action: "skip" })).rejects.toThrow(/isn't started.*weekly_step start/);
      expect(calls.some(([c]) => c === "weekly_state_write" || c === "task_toggle")).toBe(false);
    });
  });

  describe("tools for what the screens do", () => {
    const report = {
      checks: [
        {
          id: "missing-pages",
          title: "Missing pages",
          classic: true,
          items: [{ text: "[[Northwind]]", name: "Northwind", pages: ["wiki/entities/Acme.md"], safe: false }],
        },
        {
          id: "duplicates",
          title: "Possible duplicates",
          classic: false,
          items: [{ text: "Acme ~ Acme Ltd", page: "wiki/entities/Acme.md", pages: ["wiki/entities/Acme Ltd.md"], safe: false }],
        },
        {
          id: "orphans",
          title: "Orphan pages",
          classic: true,
          ignored: 2,
          items: [{ text: "Lena", page: "wiki/entities/Lena.md", safe: false }],
        },
        { id: "stale-pages", title: "Stale pages", classic: false, items: [{ text: "Maya", page: "wiki/entities/Maya.md", safe: false }] },
        {
          id: "updated",
          title: "Stale updated: dates",
          classic: true,
          items: [{ text: "Acme: updated is old", page: "wiki/entities/Acme.md", safe: true }],
        },
      ],
    };
    beforeEach(() => {
      answers.health_report = () => ({ report, history: [], ranAt: null });
      answers.files_list = () => [];
      answers.trash_list = () => [];
    });

    it("lint gives the screen's report: counted or not, safe fixes and ignored issues", async () => {
      const out = (await run("health.lint")) as string;
      expect(out).toMatch(/Orphan pages \(1; check orphans\)/);
      expect(out).toMatch(/\n\nIgnored, so not listed: Orphan pages 2\./);
      expect(out).toMatch(/Stale pages \(1, not counted/);
      expect(out).toMatch(/Acme: updated is old \[safe fix\]/);
      expect(await run("health.lint", { page: "Lena" })).toMatch(/^Knowledge health for Lena:\n\nOrphan pages \(1; check orphans\)/);
    });

    it("gives a missing link's Did you mean and In the Trash as its row does, and no Create for a page in the Trash", async () => {
      answers.files_list = () => [
        { path: "wiki/entities/Northwnd.md", layer: "wiki" },
        { path: "Templates/Northwind.md", layer: "template" },
      ];
      expect(await run("health.lint")).toContain(
        "- [[Northwind]] Did you mean Northwnd? (health_issue link_to with to: wiki/entities/Northwnd.md)",
      );
      answers.trash_list = () => [
        {
          id: "t1",
          originalRel: "wiki/entities/Northwind.md",
          basename: "Northwind.md",
          layer: "wiki",
          deletedAt: "2026-10-02T09:00",
          sizeBytes: 10,
        },
      ];
      expect(await run("health.lint")).toContain(
        "- [[Northwind]] (in the Trash since 2026-10-02: restore_from_trash with id t1 puts wiki/entities/Northwind.md back) Did you mean Northwnd?",
      );
      expect(await run("health.issue", { action: "create", item: "[[Northwind]]" })).toMatch(
        /^wiki\/entities\/Northwind\.md has been in the Trash since 2026-10-02, so there's no Create: restore_from_trash with id t1 puts it back, or health_issue link_to with to: wiki\/entities\/Northwnd\.md/,
      );
      expect(calls.some(([c]) => c === "health_create_page")).toBe(false);
    });

    it("does an issue's own buttons, and Ignore only where the screen has it", async () => {
      answers.health_create_page = () => "wiki/concepts/Northwind.md";
      await run("health.issue", { action: "create", item: "[[Northwind]]" });
      // An entity page, as the screen's Create makes.
      expect(calls.find(([c]) => c === "health_create_page")![1]).toEqual({ name: "Northwind", folder: "entities" });
      answers.links_resolve = () => ["wiki/entities/Acme.md"];
      answers.health_link_ghost = () => 1;
      expect(await run("health.issue", { action: "link_to", item: "[[Northwind]]", to: "Acme" })).toMatch(/in 1 page, each a change/);
      expect(calls.find(([c]) => c === "health_link_ghost")![1]).toEqual({
        target: "Northwind",
        to: "wiki/entities/Acme.md",
        pages: ["wiki/entities/Acme.md"],
        unattended: false,
      });
      await run("health.issue", { action: "not_duplicates", item: "Acme ~ Acme Ltd" });
      expect(calls.find(([c]) => c === "health_dismiss")![1]).toEqual({ a: "wiki/entities/Acme.md", b: "wiki/entities/Acme Ltd.md" });
      expect(await run("health.issue", { action: "not_duplicates", item: "Lena" })).toMatch(/isn't an issue/);
      expect(await run("health.ignore", { item: "[[Northwind]]" })).toMatch(/buttons of their own/);
      expect(calls.some(([c]) => c === "health_ignore")).toBe(false);
      await run("health.ignore", { item: "Lena" });
      expect(calls.find(([c]) => c === "health_ignore")![1]).toEqual({ check: "orphans", text: "Lena" });
    });

    it("tells Reshape pages and Write Current state when nobody's watching", async () => {
      answers.health_reshape = () => ({ run: "r", applied: 0, failed: [], left: 0 });
      answers.current_state_start = () => true;
      answers.current_state_status = () => ({
        running: true,
        run: "r",
        total: 1,
        done: 0,
        written: 0,
        nothing: 0,
        failed: [],
        error: null,
        model: "m",
      });
      await run("health.reshape", { unattended: true });
      await run("health.current_state", { unattended: true });
      expect(calls.find(([c]) => c === "health_reshape")![1]).toEqual({ pages: null, unattended: true });
      expect(calls.find(([c]) => c === "current_state_start")![1]).toEqual({ pages: null, limit: null, unattended: true });
    });

    it("lists contradictions, marks one and saves the report through Changes", async () => {
      const rep = {
        last: {
          started: "2026-10-05T10:00",
          finished: "2026-10-05T10:05",
          running: false,
          doing: "",
          pages: 3,
          extracted: 3,
          claims: 5,
          clashes: 1,
          judged: 1,
          contradictions: 1,
          proposed: 0,
          failed: 0,
          failures: [],
          error: null,
        },
        items: [
          {
            id: "c1",
            subject: "Orbit App",
            attribute: "launch date",
            claims: [{ page: "wiki/entities/Orbit App.md", value: "March", quote: "launches in March" }],
            verdict: {
              id: "c1",
              verdict: "contradiction",
              severity: "high",
              summary: "Two dates.",
              correct: "",
              fix: "",
              patch: null,
              judged: "",
            },
          },
        ],
      };
      answers.contradictions_report = () => rep;
      rep.items.push({
        id: "c2",
        subject: "Maya",
        attribute: "role",
        claims: [],
        verdict: { id: "c2", verdict: "compatible", severity: "low", summary: "Same role.", correct: "", fix: "", patch: null, judged: "" },
      });
      const open = (await run("contradictions")) as string;
      expect(open).toMatch(/ 1 to decide; 1 not a conflict/);
      // Every finding, as the screen lists them: the real one first, the settled after.
      expect(open).toMatch(/Orbit App · launch date \(id c1\): contradiction, high[\s\S]*Maya · role \(id c2\): compatible/);
      await run("contradictions", { action: "mark", id: "c1", as: "resolved" });
      expect(calls.find(([c]) => c === "contradictions_mark")![1]).toEqual({ id: "c1", verdict: "resolved" });
      await expect(run("contradictions", { action: "mark", id: "c1", as: "fixed" })).rejects.toThrow(/resolved/);
      let tries = 0;
      answers.changes_submit_many = () => {
        if (tries++ === 0) throw new Error("There's already a page at that path.");
        return [{ applied: true, message: "Made it.", flags: [] }];
      };
      expect(await run("contradictions", { action: "save" })).toBe("Made it.");
      const sent = calls.filter(([c]) => c === "changes_submit_many").map(([, a]) => (a!.changes as { page: string }[])[0].page);
      expect(sent[1]).toMatch(/^Contradictions - \d{4}-\d\d-\d\d-2\.md$/);
    });

    it("prepares the paused review's week, as the screen's Prepare does", async () => {
      answers.weekly_state_read = () => ({ week: "2099-W01", step: 2, done: [0, 1], log: [], notes: "", startedAt: 1 });
      expect(await run("run.start", { run: "weekly_prep" })).toMatch(/^Preparing the review of 2099-W01 \(the paused review's week\)/);
      expect(calls.find(([c]) => c === "weekprep_run")![1]).toEqual({ week: "2099-W01" });
      answers.weekly_state_read = () => ({ week: "2001-W01", step: 2, done: [], log: [], notes: "", startedAt: 1 });
      await run("run.start", { run: "weekly_prep" });
      expect(calls.filter(([c]) => c === "weekprep_run").at(-1)![1]!.week).toMatch(/^20\d\d-W\d\d$/);
      expect(calls.filter(([c]) => c === "weekprep_run").at(-1)![1]!.week).not.toBe("2001-W01");
    });

    it("goes through the weekly review: start, done, notes and finish", async () => {
      let saved: unknown = null;
      answers.weekly_state_read = () => saved;
      answers.weekly_state_write = (a) => void (saved = a!.state);
      answers.weekly_finish = () => "Me. Weekly Review - 2099-W01.md";
      await expect(run("weekly.step", { action: "done" })).rejects.toThrow(/isn't started/);
      expect(await run("weekly.step", { action: "start" })).toMatch(/step 1 of 11/);
      expect(await run("weekly.step", { action: "done" })).toMatch(/Now step 2 of 11/);
      await run("weekly.step", { action: "notes", log: "Dropped the Garden project" });
      await run("weekly.step", { action: "notes", text: "A good week." });
      expect(saved).toMatchObject({ step: 1, done: [0], log: ["Dropped the Garden project"], notes: "A good week." });
      expect(await run("weekly.step", { action: "finish" })).toMatch(/Saved the review/);
      const body = calls.find(([c]) => c === "weekly_finish")![1]!.body as string;
      expect(body).toMatch(/^2 of 11 steps done\.\n\n- Dropped the Garden project\n\nA good week\.$/);
      expect(saved).toBe(null);
    });

    it("lists, reads, renames and trashes Ask's chats", async () => {
      const c = { filename: "local:a.json", id: "a", title: "Orbit App plan", updatedAt: "2026-10-05T09:00:00" };
      answers.chats_list = () => [c];
      answers.chat_read = () => ({
        ...c,
        transcript: [
          { id: "1", role: "user", text: "Hi" },
          { id: "2", role: "tool", text: "x" },
          { id: "3", role: "assistant", text: "Hello" },
        ],
      });
      expect(await run("chats")).toMatch(/Orbit App plan · 2026-10-05 09:00 · not saved/);
      expect(await run("chats", { action: "read", chat: "orbit app plan" })).toBe(
        "Orbit App plan (2 messages):\n\nUser: Hi\n\nAssistant: Hello",
      );
      await run("chats", { action: "rename", chat: "local:a.json", title: "Launch" });
      expect(calls.find(([c]) => c === "chat_rename")![1]).toEqual({ filename: "local:a.json", title: "Launch" });
      await run("chats", { action: "trash", chat: "local:a.json" });
      expect(calls.find(([c]) => c === "chat_trash")![1]).toEqual({ filename: "local:a.json" });
    });

    it("saves a task list and shows its tasks", async () => {
      answers.settings_write = (a) => a!.settings;
      await run("task_lists", { action: "save", name: "Quick calls", view: "next", context: "@calls", effort: 15 });
      expect(await run("task_lists")).toBe("1 saved list:\n- Quick calls: next, @calls, 15 min or less");
      answers.tasks_all = () => [
        row({ contexts: ["calls"], effortMin: 10 }),
        row({ line: 3, contexts: ["calls"], effortMin: 60, text: "Long call" }),
        row({ line: 4, text: "Not a call" }),
      ];
      answers.inbox_list = () => [];
      const out = (await run("tasks.list", { list: "Quick calls" })) as string;
      expect(out).toMatch(/^1 task in next:/);
      await expect(run("tasks.list", { list: "Nope" })).rejects.toThrow(/No saved list/);
      await run("task_lists", { action: "remove", name: "Quick calls" });
      expect(await run("task_lists")).toBe("No saved lists.");
    });

    it("reads and changes the settings it's allowed, never Read-only", async () => {
      answers.settings_write = (a) => a!.settings;
      expect(await run("settings")).toMatch(/dailyCheckTime · Settings › Jobs & schedule › Daily check time: "09:00"/);
      expect(await run("settings", { action: "set", key: "dailyCheckTime", value: "03:30" })).toMatch(/is now "03:30"/);
      expect(await run("settings")).toMatch(/weeklyReview.day · Settings › Jobs & schedule › Weekly review day: "fri"/);
      await run("settings", { action: "set", key: "weeklyReview.day", value: "Thursday" });
      expect(await run("settings")).toMatch(/Weekly review day: "thu"/);
      await run("settings", { action: "set", key: "summaries.dailyTime", value: "07:00" });
      expect(settings.get().summaries?.dailyTime).toBe("07:00");
      expect(settings.get().weeklyReview).toEqual({ day: "thu", time: "16:00" });
      await expect(run("settings", { action: "set", key: "readOnly", value: false })).rejects.toThrow(/the user's/);
      await expect(run("settings", { action: "set", key: "dailyCheckTime", value: "25:00" })).rejects.toThrow(/HH:MM/);
      await expect(run("settings", { action: "set", key: "spellCheck", value: "maybe" })).rejects.toThrow(/true or false/);
      // The User scripts folder kept as the screen keeps it: no slashes at its ends, the default unset.
      await run("settings", { action: "set", key: "templateScripts", value: " /Templates/js/ " });
      expect(settings.get().templateScripts).toBe("Templates/js");
      await run("settings", { action: "set", key: "templateScripts", value: "Templates/scripts/" });
      expect(settings.get().templateScripts).toBeUndefined();
    });

    it("reads and sets a note's own look, kept in settings by path, and Use the defaults for all", async () => {
      answers.settings_write = (a) => a!.settings;
      answers.links_resolve = () => ["Idea. Orbit App.md"];
      settings.update({ docStyle: "business", docAccent: undefined, docLooks: {} });
      expect(await run("note_look")).toBe(
        "No note has its own look. The defaults (Settings › Notes › Document look): Business, Brainstead blue.",
      );
      expect(await run("note_look", { action: "set", page: "Orbit App", theme: "Editorial", colour: "green" })).toMatch(
        /^Idea\. Orbit App now looks Editorial, Green\./,
      );
      expect(settings.get().docLooks).toEqual({ "Idea. Orbit App.md": { style: "editorial", accent: "green" } });
      // A choice that matches the default isn't kept, as the Look button's.
      await run("note_look", { action: "set", page: "Orbit App", theme: "business" });
      expect(settings.get().docLooks).toEqual({ "Idea. Orbit App.md": { accent: "green" } });
      expect(await run("note_look", { page: "Orbit App" })).toMatch(/^Idea\. Orbit App looks Business, Green: its own look\./);
      expect(await run("note_look")).toContain("1 note with its own look:\n- Idea. Orbit App.md · Business, Green");
      expect(((await data("note_look")).items as Record<string, unknown>[])[0]).toEqual({
        path: "Idea. Orbit App.md",
        theme: "business",
        colour: "green",
      });
      await expect(run("note_look", { action: "set", page: "Orbit App", colour: "purple" })).rejects.toThrow(
        /colour is one of: brainstead \(Brainstead blue\)/,
      );
      await expect(run("note_look", { action: "set", page: "Orbit App" })).rejects.toThrow(/Give theme or colour/);
      await run("note_look", { action: "use_defaults", page: "Orbit App" });
      expect(settings.get().docLooks).toEqual({});
      settings.update({ docLooks: { "a.md": { style: "modern" }, "b.md": { accent: "red" } } });
      expect(await run("note_look", { action: "use_defaults_for_all" })).toBe(
        "2 notes use the defaults again, as Use the defaults for all does.",
      );
      expect(settings.get().docLooks).toEqual({});
      settings.update({ docStyle: undefined });
    });

    it("opens the window on a screen or a note", async () => {
      expect(await run("open", { screen: "knowledge_health" })).toBe("Brainstead is open on knowledge health.");
      expect(calls.find(([c]) => c === "main_show")![1]).toEqual({ screen: null });
      answers.links_resolve = () => ["wiki/entities/Orbit App.md"];
      expect(await run("open", { page: "[[Orbit App]]" })).toBe("Brainstead is open on Orbit App.");
      answers.links_resolve = () => [null];
      await expect(run("open", { page: "Nowhere" })).rejects.toThrow(/no page called Nowhere/);
    });

    it("shows an image full size, as a click on it does", async () => {
      settings.update({ vaultPath: "/v" });
      answers.asset_find = () => "images/diagram.png";
      expect(await run("open", { image: "![[diagram.png|400]]" })).toBe("Brainstead shows images/diagram.png full size.");
      expect(imageView.get()).toEqual({ url: "/v/images/diagram.png", alt: "diagram.png" });
      imageView.set(null);
      answers.asset_find = () => null;
      await expect(run("open", { image: "nowhere.png" })).rejects.toThrow(/no image called nowhere.png/);
      await expect(run("open", { image: "Orbit App" })).rejects.toThrow(/no image called Orbit App/);
    });

    it("opens a page in focus mode, which leaving the document ends", async () => {
      const off = startFocus();
      answers.links_resolve = () => ["wiki/entities/Orbit App.md"];
      expect(await run("open", { page: "Orbit App", focus: true })).toBe("Brainstead is open on Orbit App in focus mode.");
      expect(focusMode.get()).toBe(true);
      expect(await run("open", { focus: false })).toBe("Focus mode is off.");
      expect(focusMode.get()).toBe(false);
      await expect(run("open", { screen: "inbox", focus: true })).rejects.toThrow(/Focus mode shows a note/);
      focusMode.set(true);
      nav.go("inbox");
      expect(focusMode.get()).toBe(false);
      off();
    });

    it("pages every list the same way", async () => {
      answers.chats_list = () =>
        Array.from({ length: 3 }, (_, i) => ({
          filename: `local:${i}.json`,
          id: `${i}`,
          title: `Chat ${i}`,
          updatedAt: `2026-10-0${i + 1}T09:00:00`,
        }));
      const out = (await run("chats", { limit: 2 })) as string;
      expect(out.split("\n")).toEqual([
        "3 chats, newest first; 1–2 shown, offset 2 for the next:",
        "- Chat 2 · 2026-10-03 09:00 · not saved  (local:2.json)",
        "- Chat 1 · 2026-10-02 09:00 · not saved  (local:1.json)",
      ]);
      answers.projects_list = () => [];
      expect(await run("projects.list")).toBe("No projects.");
      answers.graph = () => ({
        nodes: [
          { id: "a", title: "A" },
          { id: "b", title: "B" },
        ],
        edges: [
          ["a", "b"],
          ["b", "a"],
        ],
      });
      expect(await run("graph", { limit: 1 })).toBe("2 pages; 2 links; 1–1 shown, offset 1 for the next:\n- A → B");
    });

    it("pages tasks like every list, with all of query's words, and names the defer date as edit_task does", async () => {
      answers.tasks_all = () => [
        row({ text: "Call Sam about the launch", scheduled: "2026-01-01" }),
        row({ line: 3, text: "Call Lena about the budget" }),
        row({ line: 4, text: "Email Sam the launch plan" }),
      ];
      answers.inbox_list = () => [];
      expect(((await run("tasks.list", { view: "all", limit: 1 })) as string).split("\n")[0]).toBe(
        "3 tasks in all; 1–1 shown, offset 1 for the next:",
      );
      const next = await data("tasks.list", { view: "all", limit: 1, offset: 1 });
      expect([next.offset, next.next_offset, (next.items as { text: string }[])[0].text]).toEqual([1, 2, "Call Lena about the budget"]);
      // Words, not the phrase: both of them, anywhere in the line.
      const hit = await data("tasks.list", { view: "all", query: "sam launch" });
      expect((hit.items as { id: string }[]).map((t) => t.id)).toEqual(["Me. To Do List.md:12", "Me. To Do List.md:5"]);
      expect(hit.matching).toBe(2);
      const first = (hit.items as Record<string, unknown>[])[0];
      expect([first.defer, "scheduled" in first]).toEqual(["2026-01-01", false]);
    });

    it("pages Changes as one list: held first, then made", async () => {
      const ch = (id: string, status: string) => ({
        id,
        created: "2026-10-05T09:00:00",
        origin: { kind: "chat", chat: "c1" },
        model: null,
        page: `${id}.md`,
        kind: "edit",
        title: `Change ${id}`,
        reason: "",
        status,
        decided: null,
        flags: [],
        warnings: [],
        group: "g1",
        revertable: true,
      });
      answers.changes_list = () => [ch("h1", "held"), ch("m1", "applied"), ch("h2", "held"), ch("m2", "applied")];
      const p1 = (await data("changes", { limit: 3 })) as Record<string, Record<string, unknown>>;
      const ids = (x: Record<string, unknown>) => (x.items as { id: string }[]).map((c) => c.id);
      expect([ids(p1.held), ids(p1.made), p1.held.next_offset, p1.made.total]).toEqual([["h1", "h2"], ["m1"], 3, 2]);
      const p2 = (await data("changes", { limit: 3, offset: 3 })) as Record<string, Record<string, unknown>>;
      expect([ids(p2.held), ids(p2.made), p2.made.next_offset]).toEqual([[], ["m2"], null]);
      expect(await run("changes", { limit: 3 })).toMatch(/^4 changes; 1–3 shown, offset 3 for the next: 2 held for the user, then 2 made/);
    });

    it("saves only the Tasks screen's lists, and none as no context", async () => {
      answers.settings_write = (a) => a!.settings;
      await expect(run("task_lists", { action: "save", name: "Now", view: "today" })).rejects.toThrow(/No view “today” to save/);
      await run("task_lists", { action: "save", name: "Anything", view: "deferred", context: "none" });
      expect(await run("task_lists")).toBe("1 saved list:\n- Anything: deferred");
      await run("task_lists", { action: "remove", name: "Anything" });
    });

    it("gives a task's tags, priority, status and created date, and Today's bands", async () => {
      const today = new Date().toLocaleDateString("en-CA");
      answers.tasks_all = () => [
        row({
          lineText: "- [/] Call Sam about the launch ⏫ #budget #context/calls ➕ 2026-10-01",
          status: "/",
          tags: ["budget", "context/calls"],
          contexts: ["calls"],
          created: "2026-10-01",
          due: "2000-01-01",
        }),
        row({
          line: 3,
          text: "Hear back from Lena",
          lineText: "- [ ] Hear back from Lena #waiting-for",
          tags: ["waiting-for"],
          due: today,
        }),
      ];
      answers.inbox_list = () => [];
      const items = (await data("tasks.list", { view: "today" })).items as Record<string, unknown>[];
      expect(items[0]).toMatchObject({ tags: ["budget"], priority: "high", status: "in_progress", created: "2026-10-01", band: "Overdue" });
      expect(items.map((t) => t.band)).toEqual(["Overdue", "Due today", "Waiting for"]);
      const out = (await run("tasks.list", { view: "today" })) as string;
      expect(out).toContain("Overdue:\n- [/] Call Sam about the launch · due 2000-01-01 · @calls · priority high · #budget");
      expect(out).toMatch(/Due today:\n- \[ \] Hear back from Lena .*\nWaiting for:\n- \[ \] Hear back from Lena/);
      expect(((await data("tasks.list", { view: "today", group: "project" })).items as Record<string, unknown>[])[0].band).toBeUndefined();
    });

    it("lists meeting notes being written apart from ingests, each with its id", async () => {
      answers.ingest_runs = () => [
        { id: "r1", kind: "ingest", source: "sources/Plan.pdf", status: "running", proposals: [], error: null, dropped: [] },
        { id: "r2", kind: "meeting", source: "sources/Standup transcript.vtt", status: "running", proposals: [], error: null, dropped: [] },
      ];
      answers.daily_check_status = () => ({ running: false, progress: 0 });
      answers.reviews_status = () => ({ running: [], next: {} });
      answers.weekprep_job = () => ({ running: null, last: null, next: null });
      const out = (await run("run.status")) as string;
      expect(out).toContain("Ingest running: sources/Plan.pdf (running, id r1)");
      expect(out).toContain("Meeting notes being written: sources/Standup transcript.vtt (running, id r2)");
      expect(out).not.toMatch(/Ingest running:.*Standup/);
      expect(out).toMatch(/Recent ingests: sources\/Plan\.pdf — running, 0 changes \(id r1\)$/m);
    });

    it("finds ingest runs as the Ingest runs pane's search does", async () => {
      const r = (id: string, source: string, status: string) => ({
        id,
        kind: "ingest",
        source,
        status,
        model: "claude:haiku",
        started: "2026-10-05T09:00:00",
        proposals: [],
        error: null,
        dropped: [],
      });
      answers.ingest_runs = () => [
        r("r1", "sources/Steerco notes.md", "failed"),
        r("r2", "sources/Steerco deck.pdf", "done"),
        r("r3", "sources/Plan.pdf", "failed"),
      ];
      answers.daily_check_status = () => ({ running: false, progress: 0 });
      answers.reviews_status = () => ({ running: [], next: {} });
      answers.weekprep_job = () => ({ running: null, last: null, next: null });
      const out = (await run("run.status", { query: "steerco FAILED" })) as string;
      expect(out).toMatch(/Ingests matching “steerco FAILED”: sources\/Steerco notes\.md — failed, 0 changes \(id r1\)$/m);
    });

    it("gives finished and failed meeting notes, what the checks left out, and the runs' chats", async () => {
      const run_ = (over: Record<string, unknown>) => ({ proposals: [], error: null, dropped: [], note: null, ...over });
      answers.ingest_runs = () => [
        run_({
          id: "r1",
          kind: "ingest",
          source: "sources/Plan.pdf",
          status: "done",
          proposals: ["c1"],
          dropped: [{ page: "wiki/entities/Maya.md", reason: "a quote isn't in the source" }],
        }),
        run_({
          id: "m1",
          kind: "meeting",
          source: "sources/Standup.vtt",
          status: "done",
          note: { type: "Meeting", name: "Standup", date: "2026-10-06" },
        }),
        run_({ id: "m2", kind: "meeting", source: "sources/Offsite.vtt", status: "failed", error: "No assistant answered." }),
      ];
      answers.daily_check_status = () => ({ running: false, progress: 0 });
      answers.reviews_status = () => ({
        running: [],
        next: {},
        runs: [
          {
            id: "summary-1",
            kind: "daily",
            target: "2026-10-06",
            trigger: "manual",
            startedAt: "2026-10-07T08:00:00",
            status: "done",
            file: "Me. Summaries.md",
            chat: "local:summary-1.json",
          },
        ],
      });
      answers.weekprep_job = () => ({
        running: null,
        last: { startedAt: "2026-10-06 09:00", week: "2026-W41", status: "done", count: 4, chat: "local:prep-1.json" },
        next: null,
      });
      const out = (await run("run.status")) as string;
      expect(out).toContain(
        "Recent ingests: sources/Plan.pdf — done, 1 change; left out by the checks: wiki/entities/Maya.md (a quote isn't in the source) (id r1)",
      );
      expect(out).toContain(
        "Recent meeting notes: sources/Standup.vtt — done, Meeting. Standup - 2026-10-06.md (id m1); sources/Offsite.vtt — failed (No assistant answered.) (id m2)",
      );
      expect(out).toMatch(/Weekly review preparation: last 2026-10-06 09:00 for 2026-W41 \(4 suggestions\), chat local:prep-1\.json/);
      expect(out).toMatch(/Daily summary 2026-10-06 · run now · .* · chat local:summary-1\.json$/m);
    });

    it("says where each setting is, as the screen does", async () => {
      const out = (await run("settings")) as string;
      expect(out).toContain("captureShortcut · Settings › General › Quick capture shortcut");
      expect(out).toContain("summaries.dailyEnabled · Settings › Jobs & schedule › Daily summary: false");
      expect(out).toContain("summaries.weeklyEnabled · Settings › Jobs & schedule › Weekly summary: false");
      expect(out).toContain("logDays · Settings › About › The app's logs › Keep (days: 7, 14, 30 or 90): 14");
    });

    it("gives the capture shortcut's default for the platform, as Settings does", async () => {
      const was = platform();
      try {
        setPlatform("windows");
        expect(await run("settings")).toContain(`Quick capture shortcut (as Control+Shift+Space): "Control+Shift+Space"`);
        setPlatform("macos");
        expect(await run("settings")).toContain(`Quick capture shortcut (as Control+Alt+Space): "Control+Alt+Space"`);
      } finally {
        setPlatform(was);
      }
    });

    it("lists bookmarks with Triage's untouched days, and filters saved searches by query", async () => {
      answers.bookmarks_status = () => [
        { target: "a", path: "a.md", title: "Plan", mtime: 1, days: 20, stale: true, missing: false },
        { target: "b", path: "b.md", title: "Fresh", mtime: 1, days: 2, stale: false, missing: false },
      ];
      expect(await run("bookmarks")).toBe("2 bookmarks:\n- Plan (a.md) · untouched 20 days\n- Fresh (b.md)");
      expect(((await data("bookmarks")).items as Record<string, unknown>[])[0]).toMatchObject({ days: 20, stale: true });
      // Remove by target, as Triage's Archive and Remove (a missing note's too); page only on a note that's there.
      expect(await run("bookmarks", { remove: "Gone note" })).toContain("Took the bookmark Gone note off");
      expect(calls.find(([c]) => c === "bookmark_remove")![1]).toEqual({ target: "Gone note" });
      await expect(run("bookmarks", { page: "sources/deck.pdf" })).rejects.toThrow(/isn't a note/);
      answers.doc_read = () => {
        throw new Error("not found");
      };
      await expect(run("bookmarks", { page: "Gone note.md" })).rejects.toThrow(/no note at Gone note\.md.*remove/);
      expect(calls.some(([c]) => c === "bookmark_toggle")).toBe(false);
      answers.smart_lists = () => [
        { name: "Launch", query: "soft launch +Orbit", layers: [] },
        { name: "Budget", query: "budget", layers: [] },
      ];
      expect(await run("saved_searches", { query: "orbit" })).toBe("1 saved search matching “orbit”, of 2:\n- Launch: soft launch +Orbit");
      // Each with its layers, in search's names, so running it matches the screen.
      answers.smart_lists = () => [{ name: "Launch", query: "launch", layers: ["note", "wiki", "source"] }];
      expect(await run("saved_searches")).toBe("1 saved search:\n- Launch: launch · layers notes, wiki, sources");
    });

    it("deletes a saved search", async () => {
      expect(await run("saved_searches", { name: "Launch", delete: true })).toBe("Deleted the saved search “Launch”.");
      expect(calls.find(([c]) => c === "smart_list_delete")![1]).toEqual({ name: "Launch" });
    });

    it("imports files into Sources", async () => {
      answers.sources_import = () => [
        { name: "Plan.pdf", path: "sources/Plan.pdf", error: null },
        { name: "Big.mov", path: null, error: "Over 200 MB" },
      ];
      expect(await run("sources.import", { paths: ["/tmp/Plan.pdf", "/tmp/Big.mov"] })).toBe(
        "Added to Sources: sources/Plan.pdf. Not added: Big.mov (Over 200 MB).",
      );
    });

    it("lists the Trash with its count, newest first, a page at a time", async () => {
      answers.trash_list = () =>
        Array.from({ length: 60 }, (_, i) => ({
          id: `t${i}`,
          originalRel: `Idea. ${i}.md`,
          layer: "note",
          basename: `Idea. ${i}.md`,
          deletedAt: `2026-09-${String((i % 28) + 1).padStart(2, "0")}T10:00`,
          sizeBytes: 100000,
        }));
      const out = (await run("trash")) as string;
      expect(out.split("\n")[0]).toBe("The Trash (5.7 MB), newest first: 60 items; 1–50 shown, offset 50 for the next:");
      expect(out.split("\n")).toHaveLength(51);
      expect(out.split("\n")[1]).toMatch(/deleted 2026-09-28/);
      expect(await run("trash", { query: "idea. 5", limit: 5 })).toMatch(
        /: 15 items matching “idea\. 5”, of 60; 1–5 shown, offset 5 for the next:/,
      );
      const next = (await run("trash", { offset: 50 })) as string;
      expect(next.split("\n")[0]).toMatch(/60 items; 51–60 shown:$/);
      expect(await run("trash", { offset: 70 })).toMatch(/none from offset 70/);
    });

    it("gives each listing's rows as data too, a page at a time", async () => {
      answers.trash_list = () =>
        Array.from({ length: 3 }, (_, i) => ({
          id: `t${i}`,
          originalRel: `Idea. ${i}.md`,
          layer: "note",
          basename: `Idea. ${i}.md`,
          deletedAt: `2026-09-0${i + 1}T10:00`,
          sizeBytes: 1024,
        }));
      expect(await data("trash", { limit: 2 })).toEqual({
        total: 3,
        matching: 3,
        offset: 0,
        next_offset: 2,
        bytes: 3072,
        size: "3.0 KB",
        items: [
          { id: "t2", path: "Idea. 2.md", layer: "note", deleted: "2026-09-03T10:00", bytes: 1024 },
          { id: "t1", path: "Idea. 1.md", layer: "note", deleted: "2026-09-02T10:00", bytes: 1024 },
        ],
      });
      answers.trash_list = () => [];
      expect(await data("trash")).toMatchObject({ total: 0, items: [] });
      answers.projects_list = () => [];
      expect(await data("projects.list")).toMatchObject({ total: 0, next_offset: null, items: [] });
    });

    it("says less unless detail asks for more", async () => {
      answers.projects_list = () => [
        {
          path: "Project. Orbit.md",
          name: "Orbit",
          status: "done",
          area: "Work",
          outcome: "Live",
          next: 2,
          waiting: 1,
          someday: 0,
          done: 4,
        },
      ];
      expect(await run("projects.list")).toBe("1 project:\n- Orbit · completed · 2 next  (Project. Orbit.md)");
      expect(await run("projects.list", { detail: true })).toMatch(/Work · 2 next, 1 waiting, 0 someday, 4 done · done looks like: Live/);
      expect(((await data("projects.list")).items as Record<string, unknown>[])[0]).toMatchObject({
        status: "completed",
        done_looks_like: "Live",
      });
      answers.activity = () => ({
        log: [{ date: "2026-10-01", time: "09:30", action: "ingest", title: "Orbit", description: "two pages" }],
      });
      expect(await run("activity")).toBe("1 entry in log.md, newest first:\n- 2026-10-01 ingest Orbit");
      expect(await run("activity", { detail: true })).toMatch(/2026-10-01 09:30 ingest Orbit — two pages/);
      answers.graph = () => ({
        nodes: [
          { id: "a", title: "A", layer: "wiki", type: "entity", depth: 0, degree: 3 },
          { id: "b", title: "B", layer: "wiki", type: null, depth: 1, degree: 1 },
        ],
        edges: [["a", "b"]],
      });
      expect(await run("graph")).not.toMatch(/Pages:/);
      expect(await run("graph", { detail: true })).toMatch(/Pages:\n- A · entity · wiki · 0 away · 3 links\n- B · wiki · 1 away · 1 links/);
      // The kind chips: Other wiki off leaves B and its link out.
      const hid = (await run("graph", { hide: ["Other wiki"] })) as string;
      expect(hid).toMatch(/^1 pages/);
      expect(hid).not.toMatch(/→/);
      await expect(run("graph", { hide: ["Wiki"] })).rejects.toThrow(/Graph screen's chips/);
      // A page by its name or link, resolved to its path first; two links away unless said, as the screen.
      expect(calls.filter(([c]) => c === "graph").map(([, x]) => x)).toContainEqual({ center: null, depth: 2 });
      calls.length = 0;
      answers.links_resolve = () => ["wiki/entities/Acme.md"];
      await run("graph", { page: "[[Acme]]", depth: 1 });
      expect(calls.find(([c]) => c === "links_resolve")![1]).toEqual({ targets: ["Acme"] });
      expect(calls.find(([c]) => c === "graph")![1]).toEqual({ center: "wiki/entities/Acme.md", depth: 1 });
      answers.links_resolve = () => [null];
      await expect(run("graph", { page: "Nobody" })).rejects.toThrow(/no page called Nobody/);
    });

    it("adds an image as a paste saves it, recorded in Changes with the session's origin", async () => {
      answers.image_add = () => ({
        id: "c1",
        applied: true,
        page: "images/plan.png",
        flags: [],
        message: "Added images/plan.png. Embed it in a note with ![[images/plan.png]]. Revert it in Changes.",
      });
      const origin = { kind: "chat", chat: "chat-1" };
      expect(await run("image.add", { name: "plan.png", base64: "iVBORw0K", reason: "For the note", origin })).toMatch(
        /!\[\[images\/plan\.png\]\]/,
      );
      expect(calls.find(([c]) => c === "image_add")![1]).toEqual({
        name: "plan.png",
        base64: "iVBORw0K",
        reason: "For the note",
        origin,
        model: null,
      });
      await expect(run("image.add", { name: "plan.png" })).rejects.toThrow(/name and bytes/);
    });

    it("restores from the Trash through Changes, under another path, and stops every run", async () => {
      answers.trash_list = () => [
        { id: "t1", originalRel: "Idea. One.md", layer: "note", basename: "Idea. One.md", deletedAt: "", sizeBytes: 1 },
      ];
      answers.changes_submit_many = outcomes(true);
      const origin = { kind: "chat", chat: "chat-1", trigger: "scheduled" };
      expect(await run("trash", { action: "restore", id: "t1", to: "Idea. Other.md", origin })).toBe(
        "Restored to Idea. Other.md. Revert it in Changes.",
      );
      // A change with the session's origin, so the hold rules apply; never a restore behind Changes' back.
      expect((calls.find(([c]) => c === "changes_submit_many")![1]!.changes as unknown[])[0]).toMatchObject({
        page: "Idea. Other.md",
        kind: "restore",
        instruction: { op: "restore", id: "t1" },
        origin,
      });
      expect(calls.some(([c]) => c === "trash_restore")).toBe(false);
      answers.changes_submit_many = outcomes(false);
      expect(await run("trash", { action: "restore", id: "t1" })).toMatch(/^Held for the user in Changes/);
      await expect(run("trash", { action: "restore", id: "t9" })).rejects.toThrow(/no.*id t9|id t9/i);
      await run("run.stop", { run: "find_tasks" });
      await run("run.stop", { run: "write_current_state" });
      expect(calls.some(([c]) => c === "find_stop") && calls.some(([c]) => c === "current_state_stop")).toBe(true);
      // A meeting note is stopped as the ingest queue's run of its kind, not the ingest beside it.
      answers.ingest_runs = () => [
        { id: "i1", kind: "ingest", status: "running" },
        { id: "m1", kind: "meeting", status: "running" },
      ];
      await run("run.stop", { run: "meeting_note" });
      expect(calls.filter(([c]) => c === "ingest_stop").at(-1)![1]).toEqual({ id: "m1" });
      // With none running, the next waiting one, as the screen's Stop on its row.
      answers.ingest_runs = () => [
        { id: "m2", kind: "meeting", status: "done", source: "a.vtt" },
        { id: "i2", kind: "ingest", status: "queued", source: "sources/Plan.pdf" },
      ];
      expect(await run("run.stop", { run: "ingest" })).toBe("Stopping the waiting ingest of sources/Plan.pdf (run i2); it won't start.");
      expect(calls.filter(([c]) => c === "ingest_stop").at(-1)![1]).toEqual({ id: "i2" });
      answers.ingest_runs = () => [];
      expect(await run("run.stop", { run: "meeting_note" })).toBe("No meeting note is being written or waiting.");
    });

    it("filters tasks by effort and groups them, as the Tasks screen does", async () => {
      answers.tasks_all = () => [
        row({ text: "Call Sam", effortMin: 10, project: "Project. Orbit App launch.md" }),
        row({ line: 3, text: "Write the plan", effortMin: 120, project: "Project. Orbit App launch.md" }),
        row({ line: 4, text: "Book the room", effortMin: 15 }),
      ];
      answers.inbox_list = () => [];
      expect(await run("tasks.list", { view: "all", effort: 15 })).toMatch(/^2 tasks in all:/);
      await expect(run("tasks.list", { view: "all", effort: 45 })).rejects.toThrow(/15, 30 or 60/);
      const out = (await run("tasks.list", { view: "all", group: "project" })) as string;
      expect(out.split("\n").filter((l) => !l.startsWith("- "))).toEqual(["3 tasks in all:", "Orbit App launch:", "No project:"]);
      const items = (await data("tasks.list", { view: "all", group: "project" })).items as { text: string; group: string }[];
      expect(items.map((t) => t.group)).toEqual(["Orbit App launch", "Orbit App launch", "No project"]);
      await expect(run("tasks.list", { group: "size" })).rejects.toThrow(/group is none/);
      // A saved list brings its effort and grouping.
      answers.settings_write = (a) => a!.settings;
      await run("task_lists", { action: "save", name: "Short by project", view: "next", effort: 15, group: "project" });
      expect(await run("tasks.list", { list: "Short by project" })).toMatch(/^2 tasks in next:\nOrbit App launch:\n- \[ \] Call Sam/);
      await run("task_lists", { action: "remove", name: "Short by project" });
    });

    it("suggests what Inbox items are, as Suggest does, without changing anything", async () => {
      const item: InboxItem = {
        kind: "task",
        path: "Me. To Do List.md",
        line: 13,
        text: "Call Sam about the beta",
        lineText: "- [ ] Call Sam about the beta",
        stamp: null,
      };
      answers.inbox_list = () => [item];
      answers.projects_list = () => [
        { name: "Orbit App launch", status: "active", path: "Project. Orbit App launch.md" },
        { name: "Garden", status: "someday", path: "Project. Garden.md" },
      ];
      answers.clarify_suggest = (a) =>
        (a!.items as { id: string }[]).map((i) => ({
          id: i.id,
          becomes: "next",
          text: "Call Sam about the beta",
          project: "Orbit App launch",
          context: "calls",
          effort: "15m",
          due: null,
          why: "A call to make for the launch.",
        }));
      expect(await run("inbox.list")).not.toMatch(/suggests/);
      expect(calls.some(([c]) => c === "clarify_suggest")).toBe(false);
      const out = (await run("inbox.list", { suggest: true })) as string;
      expect(out).toContain("suggests next: “Call Sam about the beta” · project Orbit App launch · @calls · effort 15m — A call to make");
      expect(calls.find(([c]) => c === "clarify_suggest")![1]).toEqual({
        items: [{ id: "task:14", kind: "task", text: "Call Sam about the beta" }],
        projects: ["Orbit App launch"],
      });
      const [first] = (await data("inbox.list", { suggest: true })).items as Record<string, Record<string, unknown>>[];
      expect(first.suggestion).toMatchObject({ becomes: "next", project: "Orbit App launch", context: "calls" });
      // Changes is only read (a transcript's held note, as its row says it).
      expect(calls.some(([c]) => c.startsWith("change") && c !== "changes_list")).toBe(false);
    });

    it("lists the Inbox in the screen's bands, so Suggest asks about the same first 20", async () => {
      const t = (n: number): InboxItem => ({
        kind: "task",
        path: "Me. To Do List.md",
        line: n,
        text: `Task ${n}`,
        lineText: `- Task ${n}`,
        stamp: null,
      });
      const thought: InboxItem = {
        kind: "thought",
        path: "Me. Scratchpad.md",
        line: 2,
        text: "Ask Maya",
        lineText: "## x",
        stamp: "2026-10-07 09:00",
      };
      const capture: InboxItem = {
        kind: "capture",
        path: "sources/Email. Hi.md",
        line: 0,
        text: "Email. Hi",
        lineText: "Email",
        stamp: "2026-10-07",
      };
      answers.inbox_list = () => [...Array.from({ length: 20 }, (_, i) => t(i + 10)), thought, capture];
      answers.projects_list = () => [];
      answers.clarify_suggest = () => [];
      const out = (await run("inbox.list", { suggest: true })) as string;
      // Captures first, then the Scratchpad, then the To Do list, as the screen's bands.
      expect(out).toMatch(/22 items to clarify:\nCaptures:\n- Email\. Hi .*\nScratchpad:\n- Ask Maya .*\nTo Do list › Other:\n- Task 10/);
      const asked = (calls.find(([c]) => c === "clarify_suggest")![1]!.items as { id: string }[]).map((i) => i.id);
      expect(asked).toHaveLength(20);
      expect(asked.slice(0, 2)).toEqual(["capture:1", "thought:3"]);
      expect(((await data("inbox.list")).items as { group: string }[])[0].group).toBe("Captures");
    });

    it("lists the recent summary runs with their change, which changes revert undoes", async () => {
      const summary = (id: string, over: Record<string, unknown> = {}) => ({
        id,
        kind: "daily",
        target: "2026-10-06",
        trigger: "manual",
        model: "claude:haiku",
        startedAt: "2026-10-07T08:00:00",
        finishedAt: "2026-10-07T08:01:00",
        status: "done",
        error: null,
        file: "Me. Summaries - 2026-10.md",
        replaced: false,
        undone: false,
        chat: null,
        ...over,
      });
      answers.ingest_runs = () => [];
      answers.daily_check_status = () => ({ running: false, progress: 0 });
      answers.weekprep_job = () => ({ running: null, last: null, next: null });
      answers.reviews_status = () => ({
        running: [],
        next: {},
        runs: [
          summary("summary-1", {}),
          summary("summary-2", { kind: "weekly", target: "2026-W40", trigger: "schedule", status: "error", error: "No model", file: null }),
          ...Array.from({ length: 8 }, (_, i) => summary(`summary-old-${i}`)),
        ],
      });
      answers.changes_list = () => [{ id: "c9", group: "summary-1", status: "applied" }];
      const out = (await run("run.status")) as string;
      expect(out).toContain(
        "- Daily summary 2026-10-06 · run now · 2026-10-07 08:00 · written to Me. Summaries - 2026-10.md · change c9 (changes revert with it undoes the summary)",
      );
      expect(out).toContain("- Weekly summary 2026-W40 · scheduled · 2026-10-07 08:00 · failed: No model");
      expect(out.split("\n").filter((l) => l.startsWith("- ") && l.includes("summary"))).toHaveLength(8);
      answers.changes_list = () => [{ id: "c9", group: "summary-1", status: "reverted" }];
      expect(await run("run.status")).toMatch(
        /Daily summary 2026-10-06 · run now · 2026-10-07 08:00 · undone, in Me\. Summaries - 2026-10\.md · change c9 \(reverted\)/,
      );
    });

    it("pins and unpins a chat, and lists which are pinned", async () => {
      const c = {
        filename: "Chat. Orbit App plan.md",
        id: "a",
        title: "Orbit App plan",
        updatedAt: "2026-10-05T09:00:00",
        state: "archived",
      };
      answers.chats_list = () => [c];
      answers.chat_read = () => ({ ...c, transcript: [] });
      await run("chats", { action: "pin", chat: "Orbit App plan" });
      expect(calls.find(([x]) => x === "chat_save")![1]!.chat).toMatchObject({ state: "pinned" });
      answers.chats_list = () => [{ ...c, state: "pinned" }];
      expect(await run("chats")).toMatch(/saved as Chat\. Orbit App plan\.md · pinned/);
      expect(((await data("chats")).items as Record<string, unknown>[])[0].pinned).toBe(true);
      expect(await run("chats", { action: "pin", chat: "Orbit App plan" })).toMatch(/pinned already/);
      expect(await run("chats", { action: "unpin", chat: "Orbit App plan" })).toBe("Unpinned “Orbit App plan”.");
      expect(calls.filter(([x]) => x === "chat_save").at(-1)![1]!.chat).toMatchObject({ state: "archived" });
    });

    it("rebuilds the index, unless it's being built already", async () => {
      answers.vault_status = () => ({ state: "ready" });
      expect(await run("index.rebuild")).toMatch(/^Rebuilding the index/);
      expect(calls.some(([c]) => c === "rebuild_index")).toBe(true);
      calls.length = 0;
      answers.vault_status = () => ({ state: "indexing" });
      expect(await run("index.rebuild")).toMatch(/being built already/);
      expect(calls.some(([c]) => c === "rebuild_index")).toBe(false);
    });

    it("fixes a name only in the files given, as ticking them does", async () => {
      const r = (file: string, action = "rewrite") => ({ file, layer: "note", inFilename: false, count: 1, lines: [], action });
      answers.fixname_plan = () => ({ rows: [r("Idea. A.md"), r("Idea. B.md"), r("Idea. C.md", "guarded")], rightPage: null });
      answers.fixname_apply = () => "Fixed the name in 1 file.";
      const args = { written_as: "Lenna", correct_spelling: "Lena", apply: true };
      await run("fix_name", { ...args, files: ["Idea. B.md"] });
      const rows = () => (calls.filter(([c]) => c === "fixname_apply").at(-1)![1]!.rows as { file: string }[]).map((x) => x.file);
      expect(rows()).toEqual(["Idea. B.md"]);
      await run("fix_name", args);
      expect(rows()).toEqual(["Idea. A.md", "Idea. B.md"]);
      await expect(run("fix_name", { ...args, files: ["Idea. C.md"] })).rejects.toThrow(
        /Not among the files.*They are: Idea\. A\.md, Idea\. B\.md/,
      );
      // Spelt the same: nothing to fix, as the screen plans nothing.
      await expect(run("fix_name", { written_as: "Lena", correct_spelling: " Lena " })).rejects.toThrow(/spelt the same/);
      // Remember is on unless false, with Where it's from; Files to leave alone go to the plan.
      const req = () => calls.filter(([c]) => c === "fixname_apply").at(-1)![1]!.req as Record<string, unknown>;
      expect(req()).toMatchObject({ skipSubstitution: false, guards: [], note: "" });
      await run("fix_name", { ...args, files_to_leave_alone: ["Lenna Park", " "], where_its_from: "confirmed in a 1-1" });
      expect(calls.filter(([c]) => c === "fixname_plan").at(-1)![1]!.req).toMatchObject({ guards: ["Lenna Park"] });
      expect(req()).toMatchObject({ skipSubstitution: false, guards: ["Lenna Park"], note: "confirmed in a 1-1" });
      await run("fix_name", { ...args, remember: false, where_its_from: "x" });
      expect(req()).toMatchObject({ skipSubstitution: true, note: "" });
    });

    it("takes Ask about each file: the notes to rewrite start unticked, and the correction is ambiguous", async () => {
      const r = (file: string, action = "rewrite") => ({ file, layer: "note", inFilename: false, count: 1, lines: [], action });
      answers.fixname_plan = () => ({ rows: [r("Idea. A.md"), r("wiki/people/Lena.md", "alias")], rightPage: "wiki/people/Lena.md" });
      answers.fixname_apply = () => "Fixed the name.";
      const args = { written_as: "Lenna", correct_spelling: "Lena", ask_about_each_file: true };
      const out = (await run("fix_name", args)) as string;
      expect(calls.find(([c]) => c === "fixname_plan")![1]!.req).toMatchObject({ ambiguous: true });
      expect(out).toMatch(/would change 1 of the 2 files it's in \(1 more to tick one by one: Ask about each file\)/);
      expect(out).toContain("- Idea. A.md: 1 line · Rewrite · unticked: read it, then give it in files");
      // Applied without files: only what's ticked to start with, the alias; with files, those too.
      const rows = () => (calls.filter(([c]) => c === "fixname_apply").at(-1)![1]!.rows as { file: string }[]).map((x) => x.file);
      await run("fix_name", { ...args, apply: true });
      expect(rows()).toEqual(["wiki/people/Lena.md"]);
      expect(calls.filter(([c]) => c === "fixname_apply").at(-1)![1]!.req).toMatchObject({ ambiguous: true });
      await run("fix_name", { ...args, apply: true, files: ["Idea. A.md"] });
      expect(rows()).toEqual(["Idea. A.md"]);
    });

    it("previews a name fix as the screen lists it, counting only what will change", async () => {
      const r = (file: string, action: string, inFilename = false) => ({ file, layer: "note", inFilename, count: 2, lines: [], action });
      answers.fixname_plan = () => ({
        rows: [
          r("Idea. A.md", "rewrite", true),
          r("wiki/people/Lena.md", "alias"),
          r("sources/Call.vtt", "leave"),
          r("Lenna Park.md", "guarded"),
        ],
        rightPage: "wiki/people/Lena.md",
      });
      const out = (await run("fix_name", { written_as: "Lenna", correct_spelling: "Lena" })) as string;
      expect(out).toMatch(/would change 2 of the 4 files it's in \(Lena has a wiki page/);
      expect(out).toContain("- Idea. A.md: 2 lines · Rewrite · In the file name: rename it separately");
      expect(out).toContain("- wiki/people/Lena.md: 2 lines · Add as alias");
      expect(out).toContain("- sources/Call.vtt: 2 lines · Left alone");
      expect(out).toContain("- Lenna Park.md: 2 lines · Guarded: someone else");
      answers.fixname_plan = () => ({ rows: [r("sources/Call.vtt", "leave")], rightPage: null });
      expect(await run("fix_name", { written_as: "Lenna", correct_spelling: "Lena" })).toMatch(
        /would change 0 of the 1 file.*\nNone of them would change/s,
      );
      expect(calls.some(([c]) => c === "fixname_apply")).toBe(false);
    });

    it("opens Meeting notes, Draft a reply and Doc check on their file", async () => {
      answers.meeting_transcripts = () => [{ path: "sources/Standup transcript.vtt", done: false, inferred: {} }];
      expect(await run("open", { screen: "meeting", transcript: "sources/Standup transcript.vtt" })).toBe(
        "Brainstead is open on meeting with sources/Standup transcript.vtt.",
      );
      await expect(run("open", { screen: "meeting", transcript: "sources/Other.vtt" })).rejects.toThrow(
        /No transcript at sources\/Other\.vtt/,
      );
      answers.links_resolve = (a) => [(a!.targets as string[])[0] === "Nowhere" ? null : "sources/Thread.eml"];
      expect(await run("open", { screen: "reply", thread: "sources/Thread.eml" })).toBe(
        "Brainstead is open on reply with sources/Thread.eml.",
      );
      expect(await run("open", { screen: "doc_check" })).toBe("Brainstead is open on doc check.");
      await expect(run("open", { screen: "doc_check", document: "Nowhere" })).rejects.toThrow(/no Nowhere in the vault/);
    });

    it("filters Activity by action and by the screen's search", async () => {
      answers.activity = () => ({
        log: [
          { date: "2026-10-02", time: "10:00", action: "trash", title: "Idea. Old", description: "" },
          { date: "2026-10-01", time: "09:30", action: "ingest", title: "Orbit", description: "two pages" },
          { date: "2026-09-30", time: "09:00", action: "ingest", title: "Hub Platform", description: "one page" },
        ],
      });
      expect(await run("activity", { action: "ingest" })).toBe(
        "2 entries in log.md, newest first:\n- 2026-10-01 ingest Orbit\n- 2026-09-30 ingest Hub Platform",
      );
      expect(await run("activity", { action: "ingest", query: "two pages" })).toMatch(
        /^1 entry in log\.md, best match first matching “two pages”, of 2:\n- 2026-10-01 ingest Orbit$/,
      );
      // While searching, Best match (the screen's default) puts every word in the title first; Latest keeps the log's order.
      expect(await run("activity", { query: "hub" })).toMatch(/best match first matching “hub”, of 3:\n- 2026-09-30 ingest Hub Platform$/);
      answers.activity = () => ({
        log: [
          { date: "2026-10-02", time: "10:00", action: "ingest", title: "Orbit", description: "about the hub" },
          { date: "2026-10-01", time: "09:30", action: "ingest", title: "Hub Platform", description: "" },
        ],
      });
      expect(await run("activity", { query: "hub" })).toMatch(/:\n- 2026-10-01 ingest Hub Platform\n- 2026-10-02 ingest Orbit$/);
      expect(await run("activity", { query: "hub", order: "latest" })).toMatch(/newest first.*:\n- 2026-10-02 ingest Orbit\n- 2026-10-01/);
      await expect(run("activity", { query: "hub", order: "oldest" })).rejects.toThrow(/order is best_match or latest/);
      await expect(run("activity", { action: "rename" })).rejects.toThrow(/Its actions: ingest\./);
    });

    it("reads and changes the newer settings: login, models, spelling, read aloud and the document look", async () => {
      answers.settings_write = (a) => a!.settings;
      let login = false;
      answers.login_item = () => login;
      answers.login_item_set = (a) => (login = a!.on as boolean);
      answers.ask_clis = () => [
        {
          cli: "claude",
          path: "/usr/local/bin/claude",
          version: "2",
          models: [
            { id: "claude:haiku", name: "Haiku" },
            { id: "claude:sonnet", name: "Sonnet" },
          ],
        },
      ];
      answers.spell_languages = () => [["en_GB", "en_US"], "en_GB"];
      answers.tts_voices = () => [{ id: "zoe", name: "Zoe", lang: "en-US", quality: "premium", default: false }];
      const all = (await run("settings")) as string;
      expect(all).toContain("openAtLogin · Settings › General › Open at login: false");
      expect(all).toMatch(
        /askModel · Settings › AI assistants › New chats use: "claude:sonnet" \(one of: claude:haiku \(Claude Code · Haiku\), claude:sonnet/,
      );
      expect(all).toContain(
        "jobModels.summaries · Settings › AI assistants › Models by job › Daily and weekly summaries: null (As new chats)",
      );
      const none = isMac ? "As macOS" : "As the system";
      expect(all).toContain(
        `spellLanguage · Settings › Notes › Spelling language: null (${none}) (one of: en_GB (British English), en_US (American English); null for ${none})`,
      );
      expect(all).toContain("docTheme · Settings › Notes › Light or dark › Documents: null (As the app)");
      await run("settings", { action: "set", key: "openAtLogin", value: true });
      expect(login).toBe(true);
      await run("settings", { action: "set", key: "jobModels.summaries", value: "Claude Code · Haiku" });
      expect(settings.get().jobModels).toEqual({ summaries: "claude:haiku" });
      await expect(run("settings", { action: "set", key: "askModel", value: "gpt-9" })).rejects.toThrow(
        /one of: claude:haiku, claude:sonnet/,
      );
      await run("settings", { action: "set", key: "jobModels.summaries", value: null });
      expect(settings.get().jobModels).toEqual({});
      // By name, as its menu lists them.
      expect(await run("settings", { action: "set", key: "spellLanguage", value: "American English" })).toMatch(/is now "en_US"/);
      await run("settings", { action: "set", key: "speechVoice", value: "zoe" });
      expect(await run("settings", { action: "set", key: "speechRate", value: 1.27 })).toMatch(/Speed \(0\.5 to 2\) is now 1\.25/);
      await expect(run("settings", { action: "set", key: "speechRate", value: 3 })).rejects.toThrow(/0\.5 to 2/);
      await run("settings", { action: "set", key: "docStyle", value: "Editorial" });
      await run("settings", { action: "set", key: "docTheme", value: "dark" });
      expect(settings.get()).toMatchObject({
        spellLanguage: "en_US",
        speechVoice: "zoe",
        speechRate: 1.25,
        docStyle: "editorial",
        docTheme: "dark",
      });
      expect(await run("settings", { action: "set", key: "docTheme", value: null })).toMatch(/Documents is now As the app/);
      expect(settings.get().docTheme).toBeUndefined();
      await expect(run("settings", { action: "set", key: "docStyle", value: null })).rejects.toThrow(/no default choice/);
      // Where macOS can't manage the login item, it isn't offered.
      answers.login_item = () => null;
      expect(await run("settings")).not.toContain("openAtLogin");
      await expect(run("settings", { action: "set", key: "openAtLogin", value: true })).rejects.toThrow(/can't be changed for this copy/);
    });

    it("gives the index, Full Disk Access, the version, the contradictions banners and a cut graph, as the screens do", async () => {
      answers.settings_read = () => ({ vaultPath: "/vault", readOnly: false });
      const stats = { files: 120, notes: 80, wiki: 30, sources: 8, templates: 2, openTasks: 14, unresolvedLinks: 3, updatedAt: 0 };
      answers.vault_status = () => ({ state: "ready", stats });
      answers.permissions = () => ({ fullDiskAccess: false, applies: true });
      answers.app_info = () => ({ version: "1.0.2", build: "abc123", dataDir: "/data/Brainstead", exe: "/x" });
      const out = (await run("status")) as string;
      expect(out).toContain("Index: 120 files indexed: 80 notes, 30 wiki, 8 sources, 2 templates, 14 open tasks, 3 unresolved links;");
      expect(out).toContain("Full Disk Access: not granted");
      expect(out).toContain(
        "Brainstead 1.0.2 (abc123); its app data folder: /data/Brainstead; its website: https://brainstead.davies.co.za/",
      );
      answers.vault_status = () => ({ state: "error", error: "The index file is damaged.", stats });
      expect(await run("status")).toContain("Index: Index failed: The index file is damaged.");
      answers.contradictions_report = () => ({
        last: {
          finished: "2026-10-06T09:00:00",
          pages: 3,
          claims: 9,
          clashes: 1,
          contradictions: 1,
          error: "The model stopped part way.",
          failed: 2,
          failures: ["Maya's role", "Orbit App's date"],
        },
        items: [],
      });
      const c = (await run("contradictions")) as string;
      expect(c).toContain(" It failed: The model stopped part way.");
      expect(c).toContain(" 2 fixes not made: Maya's role; Orbit App's date.");
      answers.graph = () => ({ nodes: [{ id: "a", title: "A" }], edges: [], truncated: true });
      expect(await run("graph")).toMatch(/^1 pages \(the nearest 400: there are more\);/);
    });

    it("sets a note's Text size, and checks and registers the capture shortcut", async () => {
      answers.settings_write = (a) => a!.settings;
      expect(await run("settings")).toContain("readSize · A note's Text size (12 to 24, px): 15");
      expect(await run("settings", { action: "set", key: "readSize", value: 18 })).toBe("A note's Text size (12 to 24, px) is now 18.");
      expect(settings.get().readSize).toBe(18);
      await expect(run("settings", { action: "set", key: "readSize", value: 30 })).rejects.toThrow(/12 to 24/);
      answers.capture_shortcut_status = () => ["Control+Shift+K", null];
      expect(await run("settings", { action: "set", key: "captureShortcut", value: "shift+control+k" })).toMatch(
        /is now "Control\+Shift\+K", registered with the system/,
      );
      expect(settings.get().captureShortcut).toBe("Control+Shift+K");
      await expect(run("settings", { action: "set", key: "captureShortcut", value: "K" })).rejects.toThrow(/one or more of Control/);
      await expect(run("settings", { action: "set", key: "captureShortcut", value: "Cmd+K" })).rejects.toThrow(/one or more of Control/);
      answers.capture_shortcut_status = () => [
        "Control+Shift+K",
        "The system wouldn't give Brainstead Super+Space; another app may use it.",
      ];
      expect(await run("settings", { action: "set", key: "captureShortcut", value: "Super+Space" })).toMatch(
        /set to "Super\+Space", but it isn't working: The system wouldn't give/,
      );
      settings.update({ readSize: undefined, captureShortcut: undefined });
    });
  });

  describe("the 2026-10-07 re-audit's screen actions", () => {
    it("rewords a task as the Tasks screen's rename does, and makes it a follow-up", async () => {
      answers.tasks_all = () => [row({ lineText: "- [ ] Call Sam about the launch 📅 2026-10-09 #work", tags: ["#work"] })];
      answers.changes_submit_many = outcomes(true);
      const out = (await run("task.edit", {
        task: "Me. To Do List.md:12",
        text: "Call Sam",
        words: "Call Lena about the launch",
        followup: true,
      })) as string;
      expect(submitted()[0].instruction).toMatchObject({ new: ["- [ ] Call Lena about the launch #followup 📅 2026-10-09 #work"] });
      expect(out).toMatch(/^Done \(reworded to “Call Lena about the launch”, follow-up\)/);
      // Taking it off again.
      calls.length = 0;
      answers.tasks_all = () => [row({ lineText: "- [ ] Call Sam about the launch #followup", tags: ["#followup"] })];
      await run("task.edit", { task: "Me. To Do List.md:12", text: "Call Sam", followup: false });
      expect(submitted()[0].instruction).toMatchObject({ new: ["- [ ] Call Sam about the launch"] });
      // Words that can't be found on the line once can't be changed simply.
      answers.tasks_all = () => [row({ lineText: "- [ ] go go", text: "go" })];
      await expect(run("task.edit", { task: "Me. To Do List.md:12", text: "go", words: "x" })).rejects.toThrow(/isn't on its line once/);
    });

    it("captures a task to the Inbox and a thought to the Scratchpad, through Changes", async () => {
      answers.changes_submit_many = outcomes(true);
      const out = (await run("inbox.capture", { kind: "task", text: "Ask  Maya about the venue @calls" })) as string;
      expect(out).toMatch(/^Added to the Inbox \(the To Do list's Other\): Ask Maya about the venue #context\/calls;/);
      await run("inbox.capture", { kind: "thought", text: "The venue\nneeds a ramp" });
      const [task, thought] = submitted();
      expect(task).toMatchObject({ page: "Me. To Do List.md", kind: "task", instruction: { op: "add_task" } });
      expect(thought).toMatchObject({ page: "Me. Scratchpad.md", instruction: { op: "add_thought", text: "The venue\nneeds a ramp" } });
      expect((thought.instruction as { stamp: string }).stamp).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
      expect(calls.some(([c]) => c === "capture")).toBe(false);
      await expect(run("inbox.capture", { kind: "note", text: "x" })).rejects.toThrow(/kind is task/);
      await expect(run("inbox.capture", { kind: "task", text: " " })).rejects.toThrow(/Give text/);
    });

    it("saves a closed chat as History's Save does, by its file or title", async () => {
      const c = { filename: "local:abc.json", id: "abc", title: "Venue options", updatedAt: "2026-10-05T09:00:00", state: "archived" };
      answers.chats_list = () => [c];
      answers.chat_read = () => ({ ...c, transcript: [{ role: "user", text: "Which venue?" }] });
      answers.chat_save = () => ({ ...c, filename: "Chat. Venue options.md" });
      expect(await run("chat.save", { chat: "venue options" })).toBe("Saved “Venue options” to the vault as Chat. Venue options.md.");
      expect(calls.find(([x]) => x === "chat_save")![1]).toMatchObject({ keep: true, chat: { id: "abc" } });
      answers.chats_list = () => [{ ...c, filename: "Chat. Venue options.md" }];
      expect(await run("chat.save", { chat: "Chat. Venue options.md" })).toMatch(/saved already/);
      await expect(run("chat.save", { chat: "Nope" })).rejects.toThrow(/No chat “Nope”/);
    });

    it("saves a doc check as a note and starts the register, through Changes", async () => {
      answers.changes_submit_many = outcomes(true);
      const entry = (version: string, status: string) => ({
        key: "pricing",
        title: "Pricing policy",
        version,
        status,
        path: `Pricing ${version}.md`,
        exists: true,
        aliases: [],
      });
      const result = {
        verdict: "Mostly aligned",
        summary: "Two gaps.",
        findings: [{ kind: "diverges", title: "Discount cap", where: "p. 2", candidate: "20%", canonical: "15%", material: true }],
        against: entry("v3", "canonical"),
        excluded: [entry("v2", "superseded")],
      };
      answers.doc_check = () => result;
      answers.canonical_register = () => ({ exists: true, problems: [], entries: [entry("v3", "canonical")] });
      const out = (await run("doccheck.run", { document: "sources/Orbit plan.pdf", save: true })) as string;
      // In the screen's words, not the internal ones.
      expect(out).toContain("Checked against Pricing policy v3 (in force).\nNot used: Pricing policy v2 (Superseded).");
      expect(out).toContain("## Conflict (material): Discount cap");
      expect(out).not.toMatch(/diverges|canonical"/);
      const path = /Saved as (.*\.md):/.exec(out)![1];
      expect(path).toMatch(/^Doc check\. Orbit plan - \d{4}-\d{2}-\d{2}\.md$/);
      expect(submitted()[0]).toMatchObject({ kind: "new", page: path, instruction: { op: "page" } });
      expect((submitted()[0].instruction as { content: string }).content).toMatch(/^# Doc check: Orbit plan\.pdf/);
      // Without save, nothing is written.
      calls.length = 0;
      await raw("doccheck.run", { document: "sources/Orbit plan.pdf" });
      expect(submitted()).toEqual([]);
      // A kept check that still stands is given again, not run again; stale or fresh, it's run.
      answers.doc_check_saved = () => ({
        candidate: "sources/Orbit plan.pdf",
        doc: "pricing",
        mode: "standard",
        at: "2026-10-10T09:00:00+02:00",
        result,
        stale: false,
      });
      calls.length = 0;
      expect(await raw("doccheck.run", { document: "sources/Orbit plan.pdf" })).toMatch(/^\(The check from 2026-10-10 09:00/);
      expect(calls.some(([c]) => c === "doc_check")).toBe(false);
      await raw("doccheck.run", { document: "sources/Orbit plan.pdf", fresh: true });
      expect(calls.some(([c]) => c === "doc_check")).toBe(true);
      answers.doc_check_saved = () => ({
        candidate: "sources/Orbit plan.pdf",
        doc: "pricing",
        mode: "standard",
        at: "2026-10-10T09:00:00+02:00",
        result,
        stale: true,
      });
      calls.length = 0;
      await raw("doccheck.run", { document: "sources/Orbit plan.pdf" });
      expect(calls.some(([c]) => c === "doc_check")).toBe(true);
      answers.doc_check_saved = () => null;
      // Compare: any two documents, A and B; unrelated ones stop at the first look.
      answers.doc_check = (x) =>
        x!.candidate === "sources/Orbit plan.pdf" && x!.doc === "sources/Lunch menu.pdf"
          ? {
              verdict: "Not comparable: they aren't about the same thing",
              summary: "A launch plan and a menu.",
              findings: [],
              against: null,
              excluded: [],
              unrelated: true,
            }
          : {
              verdict: "Mostly the same",
              summary: "One date moved.",
              findings: [
                {
                  kind: "changed",
                  title: "Launch date",
                  where: "§2 vs §2",
                  candidate: "14 November",
                  canonical: "1 November",
                  material: true,
                },
              ],
              against: null,
              excluded: [],
            };
      expect(await raw("compare.run", { document_a: "sources/Orbit plan.pdf", document_b: "sources/Lunch menu.pdf" })).toMatch(
        /Nothing was compared\.$/,
      );
      expect(await raw("compare.run", { document_a: "sources/Orbit plan.pdf", document_b: "sources/Lunch menu.pdf", save: true })).toMatch(
        /Nothing was compared, so nothing was saved as a note\.$/,
      );
      // Doc check stops the same way, and saves nothing either.
      const before = submitted().length;
      expect(
        await raw("doccheck.run", { document: "sources/Orbit plan.pdf", governing_document: "sources/Lunch menu.pdf", save: true }),
      ).toMatch(/Nothing was checked, so nothing was saved as a note\.$/);
      expect(submitted().length).toBe(before);
      const cmp = await raw("compare.run", {
        document_a: "sources/Orbit plan v2.pdf",
        document_b: "sources/Orbit plan.pdf",
        look_for: "what_changed",
      });
      expect(cmp).toContain("What changed from Orbit plan.pdf (earlier) to Orbit plan v2.pdf (newer).");
      expect(cmp).toContain("## Changed (material): Launch date");
      expect(calls.find(([c, x]) => c === "doc_check" && x!.mode === "changes")).toBeTruthy();
      await expect(run("compare.run", { document_a: "a.md", document_b: "a.md" })).rejects.toThrow(/two different/);
      answers.doc_check = () => result;
      answers.canonical_register = () => ({ exists: true, problems: [], entries: [entry("v3", "canonical"), entry("v4", "draft")] });
      expect(await run("doccheck.register")).toBe(
        "The canonical docs register (Me. Canonical Docs.md); governing_document takes a key:\n- pricing · Pricing policy v3 · In force · Pricing v3.md\n- pricing · Pricing policy v4 · Draft · Pricing v4.md",
      );
      answers.files_list = () => [];
      expect(await run("doccheck.register", { start_register: true })).toMatch(/Add a row for each version/);
      expect(submitted()[0]).toMatchObject({ page: "Me. Canonical Docs.md", kind: "new" });
      answers.files_list = () => [{ path: "Me. Canonical Docs.md" }];
      await expect(run("doccheck.register", { start_register: true })).rejects.toThrow(/there already/);
    });
  });

  describe("the 2026-10-07 re-audit's names and reads", () => {
    it("drafts a reply in the screen's tones, and a meeting note of the screen's types", async () => {
      const reply = (draft: string) => ({ answered: "n/a", verdict: "", callouts: [], draft, grounded: [], gaps: [], task: null });
      answers.draft_reply = (a) => reply(`in ${a!.tone}`);
      expect(await raw("reply.draft", { text: "Can we meet?" })).toMatch(/^Drafted reply \(Brief\), not sent.*\n\nin brief$/);
      expect(await raw("reply.draft", { text: "Can we meet?", tone: "formal" })).toMatch(/\n\nin formal$/);
      // Ask for a change: the reply as it stands and the change, revised.
      answers.draft_reply = (a) => reply(`${a!.current} → ${a!.change}`);
      expect(await raw("reply.draft", { text: "Can we meet?", current: "Yes, Tuesday.", change: "shorter" })).toMatch(
        /\n\nYes, Tuesday\. → shorter$/,
      );
      await expect(run("reply.draft", { text: "Can we meet?", change: "shorter" })).rejects.toThrow(/both current/);
      answers.draft_reply = (a) => reply(`in ${a!.tone}`);
      // The draft as the screen shows it, not the result's JSON: the verdict, each point, gaps and Add task.
      answers.draft_reply = () => ({
        ...reply("Hi Maya, Thursday works."),
        answered: "partly",
        verdict: "It gives a day but no time.",
        callouts: [{ point: "When can we meet?", status: "partly" }],
        gaps: ["The time"],
        grounded: ["Maya"],
        task: "Book the room",
      });
      expect(await raw("reply.draft", { thread: "sources/Email. Thread. Meet - 2026-10-06.md" })).toBe(
        [
          "Partly answered: It gives a day but no time.",
          "- partly: When can we meet?",
          "Drafted reply (Brief), not sent: give it to the user to paste into Outlook or Teams.",
          "",
          "Hi Maya, Thursday works.",
          "",
          "To fill in:\n- The time",
          "Drew on Maya.",
          "Its Add task, if the user wants it: capture with kind task and text “Book the room ([[Email. Thread. Meet - 2026-10-06]])”.",
        ].join("\n"),
      );
      await expect(raw("reply.draft", { text: "x", tone: "neutral" })).rejects.toThrow(/tone is brief, warm or formal/);
      answers.meeting_transcripts = () => [
        { path: "sources/Teams. Transcript. Offsite.vtt", done: null, inferred: { type: "Meeting", name: "Offsite", date: "2026-10-06" } },
      ];
      await expect(
        run("run.start", { run: "meeting_note", transcript: "sources/Teams. Transcript. Offsite.vtt", type: "Retro" }),
      ).rejects.toThrow(/type is Meeting, 1-1, Workshop, Interview/);
    });

    it("gives triage's suggestions in the screen's choices, Remove for a missing note", async () => {
      answers.bookmarks_status = () => [
        { target: "Idea. Gone", path: null, title: "Gone", mtime: null, days: null, stale: false, missing: true },
        { target: "Idea. Venue", path: "Idea. Venue.md", title: "Venue", mtime: 1, days: 20, stale: true, missing: false },
      ];
      answers.bookmarks_suggest = () => [
        {
          target: "Idea. Venue",
          decision: "promote",
          why: "It's about a place.",
          summary: "",
          page: "Northwind Hall",
          kind: "entity",
          task: null,
        },
      ];
      const out = (await run("triage.suggest", {
        items: [
          { target: "Idea. Gone", path: "Idea. Gone.md" },
          { target: "Idea. Venue", path: "Idea. Venue.md" },
        ],
      })) as string;
      expect(out).toContain("- Idea. Gone: Remove — its note is gone.");
      // How to act on each, as its button does: Make a task is capture, and both take the bookmark off.
      expect(out).toContain("Make a task is capture with kind task (to the Inbox), each then taking the bookmark off");
      expect(out).toContain("- Idea. Venue: Ingest into the wiki (the entity page Northwind Hall) — It's about a place.");
      // The model is asked only about the ones whose note is there.
      expect(calls.find(([c]) => c === "bookmarks_suggest")![1]!.items).toEqual([{ target: "Idea. Venue", path: "Idea. Venue.md" }]);
    });

    it("lists tools that start sessions in the screen's words, and needs something to match on", async () => {
      answers.automated_list = () => [];
      answers.automated_save = () => undefined;
      await run("automated", { action: "add", name: "Digest", first_message_starts_with: "Summarise the inbox" });
      expect(calls.find(([c]) => c === "automated_save")![1]!.list).toEqual([
        { label: "Digest", cwd_contains: "", opening: "Summarise the inbox" },
      ]);
      await expect(run("automated", { action: "add", name: "Digest" })).rejects.toThrow(/folder_contains or first_message_starts_with/);
    });

    it("flags projects as the Projects screen does: Stuck and Quiet", async () => {
      const p = (over: Record<string, unknown>) => ({
        name: "Orbit",
        path: "Project. Orbit.md",
        status: "active",
        area: null,
        outcome: null,
        next: 2,
        waiting: 0,
        someday: 0,
        done: 0,
        lastTouched: Date.now(),
        ...over,
      });
      answers.projects_list = () => [
        p({ next: 0 }),
        p({ name: "Hub", path: "Project. Hub.md", lastTouched: 0 }),
        p({ name: "Ok", path: "Project. Ok.md" }),
      ];
      const out = (await run("projects.list")) as string;
      expect(out).toContain("- Orbit · active · 0 next · Stuck: it has no next action  (Project. Orbit.md)");
      expect(out).toContain("- Hub · active · 2 next · Quiet: nothing has happened for two weeks  (Project. Hub.md)");
      expect(((await data("projects.list")).items as { flag: string | null }[]).map((x) => x.flag)).toEqual(["quiet", null, "stuck"]);
    });

    it("gives the judge's fix and whether it's waiting in Changes", async () => {
      const verdict = {
        id: "c1",
        verdict: "contradiction",
        severity: "high",
        summary: "Two dates.",
        correct: "wiki/entities/Orbit App.md",
        judged: "",
      };
      answers.contradictions_report = () => ({
        last: { started: "x", finished: "2026-10-05T10:05", running: false, doing: "", pages: 1, claims: 2, clashes: 1, contradictions: 1 },
        items: [
          {
            id: "c1",
            subject: "Orbit App",
            attribute: "launch date",
            claims: [],
            verdict: {
              ...verdict,
              fix: "Use 28 November.",
              patch: { page: "wiki/concepts/Launch.md", find: "March", replace: "28 November" },
            },
          },
        ],
      });
      answers.changes_list = () => [{ id: "k1", origin: { kind: "contradiction", chat: "c1" }, status: "held", group: "g" }];
      expect(await run("contradictions")).toContain(
        "  - The judge's fix: Use 28 November. (right: Orbit App) · waiting in Changes for the user (change k1)",
      );
      answers.changes_list = () => [{ id: "k1", origin: { kind: "contradiction", chat: "c1" }, status: "applied", group: "g" }];
      expect(await run("contradictions")).toContain("· made, in Changes (change k1; changes revert undoes it)");
    });

    it("lists transcripts as the screen's Show does: To do, or All with why each is done", async () => {
      answers.meeting_transcripts = () => [
        { path: "sources/Teams. Transcript. A.vtt", done: null, inferred: { type: "Meeting", name: "A", date: "2026-10-06" } },
        { path: "sources/Teams. Transcript. B.vtt", done: "linked", inferred: {} },
      ];
      answers.ingest_runs = () => [];
      expect(await run("meeting.transcripts")).toMatch(
        /^1 transcript to write up:\n- sources\/Teams\. Transcript\. A\.vtt · looks like Meeting A/,
      );
      expect(await run("meeting.transcripts", { show: "all" })).toContain("- sources/Teams. Transcript. B.vtt · done: linked from a note");
      expect(((await data("meeting.transcripts", { show: "all" })).items as { done: string | null }[]).map((x) => x.done)).toEqual([
        null,
        "linked",
      ]);
      await expect(run("meeting.transcripts", { show: "done" })).rejects.toThrow(/show is to_do or all/);
    });

    it("refuses a meeting note that exists or is being drafted, and lists both as the screen marks them", async () => {
      const t = {
        path: "sources/Teams. Transcript. Offsite.vtt",
        done: null,
        inferred: { type: "Meeting", name: "Offsite", date: "2026-10-06", exists: true, ask: ["Was this the Orbit App offsite?"] },
      };
      answers.meeting_transcripts = () => [t];
      answers.ingest_runs = () => [];
      const start = { run: "meeting_note", transcript: t.path };
      await expect(run("run.start", start)).rejects.toThrow(/Meeting\. Offsite - 2026-10-06\.md exists already/);
      expect(await run("meeting.transcripts")).toBe(
        "1 transcript to write up:\n- sources/Teams. Transcript. Offsite.vtt · looks like Meeting Offsite 2026-10-06 · note exists: Meeting. Offsite - 2026-10-06.md (start_run meeting_note with ingest_existing ingests it as it is)\n  - Was this the Orbit App offsite?",
      );
      answers.ingest_runs = () => [{ id: "m1", kind: "meeting", status: "queued", source: t.path }];
      await expect(run("run.start", { ...start, date: "2026-10-07" })).rejects.toThrow(/being drafted from .* already/);
      expect(((await data("meeting.transcripts")).items as Record<string, unknown>[])[0]).toMatchObject({
        exists: "Meeting. Offsite - 2026-10-06.md",
        ask: ["Was this the Orbit App offsite?"],
        drafted: "drafting",
      });
      answers.ingest_runs = () => [{ id: "m1", kind: "meeting", status: "done", source: t.path }];
      expect(await run("meeting.transcripts")).toContain(" · a note was drafted (see Changes)");
      expect(calls.some(([c]) => c === "ingest_queue" || c === "meeting_draft")).toBe(false);
    });

    it("ingests the note that's there as the card's Ingest the existing note does, then trashes the transcript", async () => {
      const t = {
        path: "sources/Teams. Transcript. Offsite - 2026-10-06.md",
        done: null,
        inferred: { type: "Meeting", name: "Offsite", date: "2026-10-06", exists: true, dateCheck: true, ask: [] },
      };
      answers.meeting_transcripts = () => [t];
      answers.ingest_runs = () => [];
      answers.ingest_start = () => ["r1"];
      answers.doc_read = () => ({ content: "", meta: { backlinks: [{ path: "wiki/entities/Offsite.md" }] } });
      answers.health_link_ghost = () => 1;
      answers.change_submit = () => ({ id: "c9", applied: true, page: t.path, flags: [], message: "Moved it to the Trash." });
      settings.update({ meetingIngest: true, meetingTrash: true });
      const start = { run: "meeting_note", transcript: t.path, ingest_existing: true };
      // The capture day's date is no bar: the card offers it whatever the date says.
      expect(await run("run.start", { ...start, unattended: true })).toBe(
        "Kept Meeting. Offsite - 2026-10-06.md as it is, ingesting it (run_status shows how it's going) and moved the transcript to the Trash.",
      );
      expect(calls.find(([c]) => c === "ingest_start")![1]).toEqual({ paths: ["Meeting. Offsite - 2026-10-06.md"], unattended: true });
      // Its links pointed at the note first, as every trashing of a transcript does (trashTranscript),
      // and every write recorded as the assistant's, unattended: nobody is watching.
      expect(calls.find(([c]) => c === "health_link_ghost")![1]).toMatchObject({
        target: "Teams. Transcript. Offsite - 2026-10-06",
        to: "Meeting. Offsite - 2026-10-06.md",
        unattended: true,
      });
      expect(calls.find(([c]) => c === "change_submit")![1]).toMatchObject({
        change: { page: t.path, kind: "trash", instruction: { op: "trash" }, origin: { kind: "chat", trigger: "scheduled" } },
      });
      expect(calls.some(([c]) => c === "trash_move")).toBe(false);
      // It leaves the Inbox, its note being there.
      expect(calls.find(([c]) => c === "inbox_capture_done")![1]).toEqual({ path: t.path });
      expect(calls.some(([c]) => c === "meeting_draft")).toBe(false);
      // No note at the name given: nothing to keep.
      calls.length = 0;
      await expect(run("run.start", { ...start, name: "Retro" })).rejects.toThrow(/There's no Meeting\. Retro - 2026-10-06\.md to keep/);
      // With both of Settings' switches off, it's only kept.
      settings.update({ meetingIngest: false, meetingTrash: false });
      expect(await run("run.start", start)).toMatch(/^Kept Meeting\. Offsite - 2026-10-06\.md as it is; ingesting and trashing are off/);
      expect(calls.some(([c]) => c === "ingest_start" || c === "trash_move" || c === "change_submit")).toBe(false);
      settings.update({ meetingIngest: undefined, meetingTrash: undefined });
    });

    it("trashes a transcript for trash_note with its links pointed at its note first, as the assistant's changes", async () => {
      const path = "sources/Teams. Transcript. Offsite - 2026-10-06.md";
      answers.meeting_transcripts = () => [
        { path, done: null, inferred: { type: "Meeting", name: "Offsite", date: "2026-10-06", exists: true, dateCheck: false, ask: [] } },
      ];
      answers.doc_read = () => ({ content: "", meta: { backlinks: [{ path: "wiki/entities/Offsite.md" }] } });
      answers.health_link_ghost = () => 1;
      answers.change_submit = () => ({ id: "c9", applied: true, page: path, flags: [], message: "Moved it to the Trash." });
      calls.length = 0;
      const origin = { kind: "chat", run: "s1" };
      expect(await run("note.trash_transcript", { page: path, reason: "done with it", origin })).toBe("Moved it to the Trash.");
      expect(calls.find(([c]) => c === "health_link_ghost")![1]).toMatchObject({
        to: "Meeting. Offsite - 2026-10-06.md",
        unattended: false,
      });
      expect(calls.find(([c]) => c === "change_submit")![1]).toMatchObject({
        change: { page: path, kind: "trash", reason: "done with it", origin },
      });
      expect(calls.some(([c]) => c === "trash_move")).toBe(false);
      await expect(run("note.trash_transcript", { page: "notes/Plan.md" })).rejects.toThrow(/Which transcript/);
    });

    it("makes a template's other notes beside it, and takes several choices for one question", async () => {
      answers.files_list = () => [{ path: "Templates/Trip.md" }];
      answers.doc_read = () => ({
        content:
          '<%* const who = await tp.system.multi_suggester(["Maya", "Lena", "Sam"], ["Maya", "Lena", "Sam"], false, "Who is going?"); await tp.file.create_new("Packing for " + who.join(" and "), "Packing. Offsite"); tR += "Going: " + who.join(", "); %>',
      });
      answers.user_scripts = () => [];
      answers.changes_submit_many = outcomes(true);
      const out = (await run("note.from_template", {
        template: "Trip",
        type: "Trip",
        title: "Offsite",
        answers: [["Maya", "Lena"]],
      })) as string;
      const sent = calls.find(([c]) => c === "changes_submit_many")![1]!.changes as { page: string; instruction: { content: string } }[];
      expect(sent.map((c) => [c.page, c.instruction.content])).toEqual([
        ["Trip. Offsite.md", "Going: Maya, Lena"],
        ["Packing. Offsite.md", "Packing for Maya and Lena"],
      ]);
      expect(out).toMatch(/with Packing\. Offsite\.md, which the template makes too\..*“Who is going\?” → Maya, Lena/);
      expect(calls.some(([c]) => c === "doc_create")).toBe(false);
      await expect(run("note.from_template", { template: "Trip", title: "Offsite", answers: [["Maya", "Nobody"]] })).rejects.toThrow(
        /“Nobody” isn't one of the choices for “Who is going\?”: Maya, Lena, Sam/,
      );
    });

    it("runs a template's finishing steps once its note is made, and takes Cancel for a question", async () => {
      answers.files_list = () => [{ path: "Templates/Trip.md" }];
      answers.doc_read = () => ({
        content:
          '<%* const t = await tp.system.suggester(["Bus", "Train"], ["Bus", "Train"], false, "How?"); tp.hooks.on_all_templates_executed(async () => { await tp.file.create_new("Booked", "Booking. Offsite"); }); tR += "By " + t; %>',
      });
      answers.user_scripts = () => [];
      answers.changes_submit_many = outcomes(true);
      answers.change_submit = (a) => ({
        id: "c9",
        applied: true,
        page: (a!.change as { page: string }).page,
        flags: [],
        message: "Made Booking. Offsite.",
      });
      // null is Cancel: the template carries on with no answer, as on the screen.
      const out = (await run("note.from_template", { template: "Trip", type: "Trip", title: "Offsite", answers: [null] })) as string;
      const sent = calls.find(([c]) => c === "changes_submit_many")![1]!.changes as { page: string; instruction: { content: string } }[];
      expect(sent.map((c) => c.instruction.content)).toEqual(["By null"]);
      expect(out).toContain("“How?” → (cancelled)");
      // The finishing step made its note as a change of its own, after the note.
      expect((calls.find(([c]) => c === "change_submit")![1]!.change as { page: string }).page).toBe("Booking. Offsite.md");
      expect(out).toMatch(/Its finishing steps ran\. Made Booking\. Offsite\.$/);
      // "" cancels a question with choices too; held, the finishing steps don't run.
      calls.length = 0;
      answers.changes_submit_many = outcomes(false);
      const held = (await run("note.from_template", { template: "Trip", type: "Trip", title: "Offsite", answers: [""] })) as string;
      expect(held).toContain("“How?” → (cancelled)");
      expect(held).toContain("Its finishing steps (tp.hooks) didn't run, as the note isn't made yet.");
      expect(calls.some(([c]) => c === "change_submit")).toBe(false);
    });

    it("lists the assistants found as Settings › AI assistants does, and looks again", async () => {
      answers.ask_clis = () => [
        { cli: "claude", path: "/usr/local/bin/claude", version: "2.1.0", models: [{ id: "sonnet", name: "Sonnet" }] },
        { cli: "codex", path: "/usr/local/bin/codex", version: null, models: [] },
        { cli: "copilot", path: null, version: null, models: [] },
      ];
      const out = (await run("assistants.found", { look_again: true })) as string;
      expect(calls.filter(([c]) => c === "ask_clis")).toHaveLength(1);
      expect(out).toMatch(/^Found on this computer \(Settings › AI assistants\), looked for again just now: 2 assistants\./);
      expect(out).toContain("- Claude Code · 2.1.0: 1 model · /usr/local/bin/claude");
      expect(out).toContain(`- ChatGPT (Codex): No models listed: open ${terminalApp}, run codex and sign in.`);
      expect(out).toContain("- GitHub Copilot: Not installed. How to install: https://docs.github.com/");
    });

    it("names the read-aloud highlight as Settings does", async () => {
      answers.settings_write = (a) => a!.settings;
      answers.login_item = () => null;
      expect(await run("settings")).toContain("speechHighlight · Settings › Notes › Read aloud › Highlight each word: ");
    });

    it("test-runs a template, writing nothing; reads Capture extensions and Glance", async () => {
      answers.files_list = () => [{ path: "Templates/Idea.md" }];
      answers.doc_read = () => ({ content: "## Why\n\nIt matters.\n" });
      answers.user_scripts = () => [];
      answers.changes_submit_many = outcomes(true);
      const out = (await run("note.from_template", { template: "Idea", type: "Idea", title: "Venue", test_run: true })) as string;
      expect(out).toMatch(/^Test run of Templates\/Idea\.md, nothing written: it would make Idea\. Venue\.md\./);
      expect(out).toContain("## Why");
      expect(calls.some(([c]) => c === "change_submit" || c === "changes_submit_many" || c === "doc_create")).toBe(false);
      answers.capture_status = () => ({
        folder: "/Applications/Brainstead.app/Contents/Resources/extensions",
        browsers: [{ name: "Chrome", registered: true, current: true }],
        recent: [
          { extension: "teams", what: "Chat with Lena", path: "sources/Teams. Chat. Lena.md", error: null, at: "2026-10-06T09:15:00" },
        ],
      });
      const cap = (await run("capture.status")) as string;
      expect(cap).toContain("Browsers: Chrome (knows the capture host, the one in use).");
      expect(cap).toContain("- 2026-10-06 09:15 · Teams · Chat with Lena → sources/Teams. Chat. Lena.md");
      answers.glance = () => ({
        noteTypes: [{ name: "Meeting", n: 12 }],
        noteTags: [],
        wikiTypes: [{ name: "entity", n: 3 }],
        wikiTags: [],
        mostLinked: [{ path: "wiki/entities/Orbit App.md", title: "Orbit App", layer: "wiki", links: 9 }],
      });
      const g = (await run("glance")) as string;
      expect(g).toContain("Notes by type: Meeting 12");
      expect(g).toContain("Most linked: Orbit App (wiki/entities/Orbit App.md, 9 links)");
    });
  });

  describe("the fifth audit's fixes", () => {
    it("builds a meeting note's name and date as the screen does, and refuses one without them", async () => {
      const t = (inferred: Record<string, unknown>) => ({ path: "sources/Teams. Transcript. Offsite.vtt", done: null, inferred });
      answers.ingest_runs = () => [];
      answers.meeting_draft = () => ["m1"];
      const start = { run: "meeting_note", transcript: "sources/Teams. Transcript. Offsite.vtt" };
      // No name read from it: the topic isn't taken for one, as on the screen.
      answers.meeting_transcripts = () => [t({ type: "Meeting", topic: "Offsite", date: "2026-10-06" })];
      await expect(run("run.start", start)).rejects.toThrow(/needs a name and a date/);
      // No date: not today's.
      answers.meeting_transcripts = () => [t({ type: "Meeting", name: "Offsite" })];
      await expect(run("run.start", start)).rejects.toThrow(/needs a name and a date/);
      await expect(run("run.start", { ...start, date: "6 Oct" })).rejects.toThrow(/needs a name and a date/);
      expect(calls.some(([c]) => c === "meeting_draft")).toBe(false);
      expect(await run("run.start", { ...start, date: "2026-10-06" })).toMatch(/^Drafting the Meeting note “Offsite” for 2026-10-06/);
      expect(calls.find(([c]) => c === "meeting_draft")![1]!.items).toEqual([
        [start.transcript, { type: "Meeting", name: "Offsite", date: "2026-10-06" }],
      ]);
    });

    it("refuses to ingest a wiki page or a template, as the file menu offers no Ingest on them", async () => {
      answers.ingest_start = () => ["r1"];
      for (const p of ["wiki/entities/Orbit App.md", "Templates/Meeting.md"])
        await expect(run("run.start", { run: "ingest", sources: ["sources/Plan.pdf", p] })).rejects.toThrow(/can't be ingested/);
      expect(calls.some(([c]) => c === "ingest_start")).toBe(false);
    });

    it("orders a project's Next actions, as dragging on the project's page does", async () => {
      const project = "Projects/Project. Orbit App launch.md";
      const task = (line: number, text: string, rank: number | null, tags: string[] = []) =>
        row({ path: project, line, lineText: `- [ ] ${text}`, text, rank, tags, project });
      answers.projects_list = () => [{ name: "Orbit App launch", path: project, status: "active" }];
      answers.tasks_all = () => [
        task(1, "Ask Maya", 2048),
        task(2, "Wait for Lena", 1536, ["waiting-for"]),
        task(3, "Book the room", 1024),
        task(4, "Call Lena", 3072),
      ];
      answers.change_task_line = (a) => `${a!.line} ^rank-${JSON.stringify(a!.edit)}`;
      answers.changes_submit_many = outcomes(true);
      const out = (await run("task.move", {
        task: `${project}:5`,
        text: "Call Lena",
        before: `${project}:2`,
        project: "Orbit App launch",
      })) as string;
      // In the project's order (Book the room, Ask Maya, Call Lena), ahead of Ask Maya: after Book the room.
      expect(calls.find(([c]) => c === "change_task_line")![1]!.edit).toEqual({ op: "rank", prev: 1024, next: 2048 });
      expect(out).toContain("in Orbit App launch's Next actions");
      // A waiting task isn't in the project's Next actions.
      await expect(
        run("task.move", { task: `${project}:5`, text: "Call Lena", after: `${project}:3`, project: "Orbit App launch" }),
      ).rejects.toThrow(/aren't both in Orbit App launch's Next actions/);
    });

    it("checks a new note's folder and date with a template too", async () => {
      answers.files_list = () => [{ path: "Templates/Idea.md" }];
      answers.doc_read = () => ({ content: "# Idea\n" });
      answers.changes_submit_many = outcomes(true);
      // The server checks before it asks the app (src-tauri/mcp/src/tests.rs); the app still makes it.
      expect(await run("note.from_template", { template: "Idea", type: "Idea", title: "Venue" })).toMatch(/Changed Idea\. Venue\.md/);
    });

    it("starts the weekly review over and prepares its suggestions, as Start over does", async () => {
      answers.weekly_state_read = () => null;
      answers.weekprep_ensure = () => true;
      const out = (await run("weekly.start_over")) as string;
      expect(calls.find(([c]) => c === "weekprep_ensure")![1]!.week).toMatch(/^\d{4}-W\d{2}$/);
      expect(out).toMatch(/Its suggestions are being prepared/);
    });

    it("checks against the register's first governing document when none is named, as the picker starts", async () => {
      const entry = (key: string, version: string, status: string) => ({
        key,
        title: `${key} policy`,
        version,
        status,
        path: `${key} ${version}.md`,
        exists: true,
        aliases: [],
      });
      answers.canonical_register = () => ({
        exists: true,
        problems: [],
        entries: [entry("pricing", "v2", "superseded"), entry("brand", "v1", "canonical"), entry("pricing", "v3", "canonical")],
      });
      answers.doc_check = () => ({
        verdict: "Aligned",
        summary: "",
        findings: [],
        against: entry("pricing", "v3", "canonical"),
        excluded: [],
      });
      await run("doccheck.run", { document: "sources/Orbit plan.pdf" });
      expect(calls.find(([c]) => c === "doc_check")![1]).toMatchObject({ doc: "pricing", mode: "standard" });
      answers.canonical_register = () => ({ exists: true, problems: [], entries: [] });
      await expect(run("doccheck.run", { document: "sources/Orbit plan.pdf" })).rejects.toThrow(/Add a governing document/);
    });

    it("opens Graph around a page", async () => {
      answers.links_resolve = () => ["wiki/entities/Orbit App.md"];
      expect(await run("open", { screen: "graph", page: "Orbit App" })).toBe("Brainstead is open on Graph, around Orbit App.");
    });

    it("saves a search only as Save search allows: not a query saved already, and the screen's layers", async () => {
      answers.smart_lists = () => [{ name: "Launch", query: "soft launch", layers: [] }];
      await expect(run("saved_searches", { name: "Again", query: "soft launch" })).rejects.toThrow(/saved already, as “Launch”/);
      await expect(run("saved_searches", { name: "T", query: "budget", layers: ["templates"] })).rejects.toThrow(/notes, wiki and sources/);
      await run("saved_searches", { name: "Budget", query: "budget", layers: ["notes", "wiki", "sources"] });
      await run("saved_searches", { name: "Budget wiki", query: "budget wiki", layers: ["wiki"] });
      expect(calls.filter(([c]) => c === "smart_list_save").map(([, a]) => a!.layers)).toEqual([[], ["wiki"]]);
    });

    it("gives a day's log entries beside its files, and searches as the screen does", async () => {
      answers.activity = () => ({
        log: [
          { date: "2026-10-02", time: "10:00", action: "ingest", title: "Orbit", description: "two pages" },
          { date: "2026-10-02", time: "09:00", action: "trash", title: "Idea. Old", description: "" },
          { date: "2026-10-01", time: "09:30", action: "ingest", title: "Hub Platform", description: "" },
        ],
      });
      answers.activity_day = () => [{ path: "wiki/entities/Orbit App.md" }];
      expect(await run("activity", { date: "2026-10-02" })).toBe(
        "3 files changed and entries in log.md on 2026-10-02:\n- wiki/entities/Orbit App.md\n- 2026-10-02 ingest Orbit\n- 2026-10-02 trash Idea. Old",
      );
      // The query narrows the entries, not the files; the date isn't searched, as on the screen.
      expect(await run("activity", { date: "2026-10-02", query: "pages" })).toMatch(
        /:\n- wiki\/entities\/Orbit App\.md\n- 2026-10-02 ingest Orbit$/,
      );
      expect(await run("activity", { query: "2026-10-01" })).toMatch(/^0 entries/);
    });

    it("won't save a contradictions report while a check runs", async () => {
      answers.contradictions_report = () => ({ last: { finished: "2026-10-05T10:05", running: true, failures: [] }, items: [] });
      await expect(run("contradictions", { action: "save" })).rejects.toThrow(/check is running/);
    });

    it("says ⌘Z undoes an accepted suggestion, as Found's toast does", async () => {
      answers.find_decide = () => "Added “Book the room” to Orbit App launch. Revert it in Changes.";
      expect(await run("suggestions", { action: "accept", id: "s1" })).toBe(
        `Added “Book the room” to Orbit App launch. ${keyLabel("⌘Z")} in the app undoes it (Undo, as Found's toast offers), and it's in Changes too.`,
      );
      expect(calls.find(([c]) => c === "find_decide")![1]).toMatchObject({ unattended: false });
    });

    it("carries unattended into an accepted suggestion's change, and says when it's held", async () => {
      answers.find_decide = () => "Held for the user in Changes: Add “Book the room”. A check failed.";
      expect(await run("suggestions", { action: "accept", id: "s1", unattended: true })).toBe(
        "Held for the user in Changes: Add “Book the room”. A check failed. Nothing is changed until they accept it.",
      );
      expect(calls.find(([c]) => c === "find_decide")![1]).toMatchObject({ id: "s1", unattended: true });
    });

    it("names Mark complete for a completed project", async () => {
      answers.projects_list = () => [{ name: "Orbit App launch", path: "Project. Orbit App launch.md", status: "active" }];
      answers.changes_submit_many = outcomes(true);
      expect(await run("project.update", { project: "Orbit App launch", status: "completed" })).toMatch(
        /^Set status Completed \(Mark complete\) for Orbit App launch\./,
      );
    });

    it("refuses Only in the menu bar while Show in the menu bar is off", async () => {
      settings.update({ menuBar: false });
      await expect(run("settings", { action: "set", key: "menuBarOnly", value: true })).rejects.toThrow(/Show in the menu bar/);
      settings.update({ menuBar: true });
    });

    it("reads a source's Provenance card", async () => {
      answers.source_provenance = () => ({
        sha256: "ab12",
        size: 2048,
        mtime: Date.parse("2026-10-01T10:00:00Z"),
        chunks: 4,
        citers: [
          {
            path: "wiki/entities/Orbit App.md",
            title: "Orbit App",
            listed: true,
            passages: [{ line: 3, anchor: "page=2", context: "Launch is in May [[Orbit plan.pdf#page=2]] ^q1" }],
          },
        ],
        lastRun: { id: "r1", finished: "2026-10-02T10:00:00Z", status: "done", model: "claude:sonnet" },
      });
      const out = (await run("source.provenance", { source: "sources/Orbit plan.pdf" })) as string;
      expect(out).toMatch(/^sources\/Orbit plan\.pdf: Ingested 2 Oct 2026 \(run r1, sonnet\)\./);
      expect(out).toContain("Text: 4 passages indexed, searchable · SHA-256 ab12.");
      expect(out).toContain(
        "Cited by 1 page, 1 passage:\n- Orbit App (wiki/entities/Orbit App.md): 1 passage\n  - “Launch is in May” at “page=2”",
      );
      answers.source_provenance = () => null;
      await expect(run("source.provenance", { source: "sources/Gone.pdf" })).rejects.toThrow(/no source at/);
    });

    it("gives Knowledge health's summary and trend", async () => {
      const report = {
        checks: [
          { id: "orphans", title: "Orphan pages", classic: true, items: [{ text: "Lena", page: "wiki/entities/Lena.md", safe: false }] },
        ],
      };
      answers.health_report = () => ({
        report,
        history: [
          ["2026-10-01", { total: 4, decisions: 3 }],
          ["2026-10-06", { total: 1, decisions: 1 }],
        ],
        ranAt: null,
      });
      expect(await run("health.lint")).toMatch(
        /^Knowledge health:\nNeed a decision: 1; nothing safe to fix\. Trend: 1 Oct: 3 → 6 Oct: 1 \(1 Oct to 6 Oct\. Highest: 3\.\)/,
      );
      answers.health_report = () => ({ report, history: [], ranAt: null });
      expect(await run("health.lint")).toContain("The trend shows after a second day.");
    });

    it("gives AI assistants' Terminal commands and the skills Brainstead does now", async () => {
      answers.ask_clis = () => [{ cli: "claude", path: "/usr/local/bin/claude", version: "2.1.0", models: [] }];
      answers.app_info = () => ({ version: "1.0.2", build: "1", dataDir: "/tmp/x", exe: "/Applications/Brainstead.app/brainstead" });
      answers.skills_list = () => [
        { name: "triage", description: "", source: "vault", opens: "triage" },
        { name: "notes", description: "", source: "vault" },
      ];
      const out = (await run("assistants.found", { look_again: true })) as string;
      expect(out).toContain('- Claude Code: claude mcp add -s user brainstead -- "/Applications/Brainstead.app/brainstead" --mcp');
      expect(out).toContain("Skills Brainstead does now");
      expect(out).toContain("- /triage: Bookmarks triage");
      expect(out).not.toContain("/notes");
    });
  });

  it("refuses an action it doesn't know", async () => {
    await expect(run("nope")).rejects.toThrow(/doesn't know the action/);
  });
});
