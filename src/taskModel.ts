// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The task lists (the previous app's To Do note, as views), and every change to a task: tick, dates,
// order, undo. Rust makes the change; this side reports it with an Undo button, or says why it
// was refused.

import moment from "moment";
import { useEffect, useState } from "react";
import { api, Edited, EditError, InboxItem, TaskDateKind, TaskRow } from "./api";
import { inboxTaskKeys } from "./gtd";
import { nav } from "./nav";
import { localToday } from "./md/taskQuery";
import { useVaultVersion } from "./state";
import { Store, useStore } from "./store";
import { cancelled, completion, inQuote, reopened, started, waitingToggled, withPriority } from "./tasksq/edits";
import type { PriorityName } from "./tasksq/fields";
import { toast } from "./Toast";

export type ViewId = "next" | "followups" | "waiting" | "scheduled" | "someday" | "done-this-week" | "done-last-week";

const has = (t: TaskRow, tag: string) => t.tags.some((g) => g === tag || g.startsWith(`${tag}/`));
const parked = (t: TaskRow) => has(t, "followup") || has(t, "waiting-for") || has(t, "someday-maybe");

/** Monday-start week, as the previous app counts "done this week". */
export function weekOf(today: string, offset = 0): [string, string] {
  const d = new Date(+today.slice(0, 4), +today.slice(5, 7) - 1, +today.slice(8, 10));
  const start = new Date(d);
  start.setDate(d.getDate() - ((d.getDay() + 6) % 7) + offset * 7);
  const end = new Date(start);
  end.setDate(start.getDate() + 7);
  return [localToday(start), localToday(end)];
}

export interface View {
  id: ViewId;
  label: string;
  icon: string;
  /** Rows can be dragged into a new order (written as ^rank-N). */
  manual: boolean;
  /** `inbox`: the Inbox tasks still to clarify (`inboxTaskKeys`), which Next actions leaves out. */
  filter: (t: TaskRow, today: string, inbox: Set<string>) => boolean;
  sort?: (a: TaskRow, b: TaskRow) => number;
  /** The tag a task added from this list gets, so it shows here; none sends it to the Inbox. */
  adds?: string;
}

const byDoneDesc = (a: TaskRow, b: TaskRow) => (b.doneOn ?? "").localeCompare(a.doneOn ?? "");
const inWeek = (t: TaskRow, w: [string, string]) => !!t.doneOn && t.doneOn >= w[0] && t.doneOn < w[1];

/** The lists of the previous app's To Do note, plus Deferred. */
export const VIEWS: View[] = [
  {
    id: "next",
    label: "Next actions",
    icon: "tasks",
    manual: true,
    // A deferred task stays off until its day, and an Inbox task until it's clarified.
    filter: (t, today, inbox) => !t.done && !parked(t) && !(t.scheduled && t.scheduled > today) && !inbox.has(`${t.path}:${t.line}`),
  },
  { id: "followups", label: "Follow-ups", icon: "ask", manual: true, filter: (t) => !t.done && has(t, "followup"), adds: "#followup" },
  {
    id: "waiting",
    label: "Waiting for",
    icon: "waiting",
    manual: true,
    filter: (t) => !t.done && has(t, "waiting-for"),
    adds: "#waiting-for",
  },
  {
    id: "scheduled",
    label: "Deferred",
    icon: "calendar",
    manual: false,
    filter: (t) => !t.done && !!t.scheduled,
    sort: (a, b) => (a.scheduled ?? "").localeCompare(b.scheduled ?? ""),
  },
  {
    id: "someday",
    label: "Someday / maybe",
    icon: "someday",
    manual: true,
    filter: (t) => !t.done && has(t, "someday-maybe"),
    adds: "#someday-maybe",
  },
  {
    id: "done-this-week",
    label: "Done this week",
    icon: "check",
    manual: false,
    filter: (t, d) => t.done && inWeek(t, weekOf(d)),
    sort: byDoneDesc,
  },
  {
    id: "done-last-week",
    label: "Done last week",
    icon: "check",
    manual: false,
    filter: (t, d) => t.done && inWeek(t, weekOf(d, -1)),
    sort: byDoneDesc,
  },
];

/** A view's rows. Pass the Inbox (`useInbox`) for Next actions, so unclarified tasks stay out. */
export function viewRows(all: TaskRow[], v: View, today: string, inbox?: InboxItem[] | null): TaskRow[] {
  const held = inboxTaskKeys(inbox);
  const r = all.filter((t) => v.filter(t, today, held));
  return v.sort ? r.sort(v.sort) : r;
}

/** Deferred to a day after `today`: left off Today, its counts and its Waiting band (D-20261006-02). */
export const deferredPast = (t: TaskRow, today: string) => !!t.scheduled && t.scheduled > today;

/** Today: overdue, due today, and deferred until today or earlier (the previous app's GTD counts).
 *  A task deferred past today is left out even when it's due or overdue: deferring it is saying
 *  "not today" (D-20261006-02). */
export function todayRows(all: TaskRow[], today: string): { overdue: TaskRow[]; due: TaskRow[]; scheduled: TaskRow[]; soon: TaskRow[] } {
  const open = all.filter((t) => !t.done && !deferredPast(t, today));
  const overdue = open.filter((t) => t.due && t.due < today).sort((a, b) => a.due!.localeCompare(b.due!));
  const due = open.filter((t) => t.due === today);
  const scheduled = open.filter((t) => t.scheduled && t.scheduled <= today && !(t.due && t.due <= today));
  // Coming up this week: due in the next seven days, soonest first.
  const week = moment(today, "YYYY-MM-DD").add(7, "days").format("YYYY-MM-DD");
  const soon = open.filter((t) => t.due && t.due > today && t.due <= week).sort((a, b) => a.due!.localeCompare(b.due!));
  return { overdue, due, scheduled, soon };
}

/** Why the tasks couldn't be read, or null. Screens show it with Retry rather than an empty list. */
export const tasksFailed = new Store<string | null>(null);
const retries = new Store(0);
/** Reads the tasks again, after a failed load. */
export const retryTasks = () => retries.set(retries.get() + 1);

/** Every task in the notes, in manual order (unranked first), reloaded when the vault changes.
 *  A failed read gives an empty list and sets `tasksFailed`. */
export function useAllTasks(): TaskRow[] | null {
  const [rows, setRows] = useState<TaskRow[] | null>(null);
  const v = useVaultVersion();
  const r = useStore(retries);
  useEffect(() => {
    let live = true;
    api
      .tasksQuery({ status: "any", sort: "manual" })
      .then((t) => {
        if (!live) return;
        tasksFailed.set(null);
        setRows(t);
      })
      .catch((e) => {
        if (!live) return;
        tasksFailed.set(String(e));
        setRows([]);
      });
    return () => {
      live = false;
    };
  }, [v, r]);
  return rows;
}

const isEditError = (e: unknown): e is EditError => typeof e === "object" && !!e && "code" in e && "message" in e;

/** Says why an edit didn't happen; read-only offers the way to Settings. */
export function reportEditError(e: unknown) {
  if (isEditError(e)) {
    if (e.code === "read-only")
      toast(e.message, { label: "Open Settings", run: () => nav.go({ screen: "settings", pane: "vault" }) }, "bad");
    else toast(e.message, undefined, "bad");
  } else toast(String(e), undefined, "bad");
}

export const undoAction = { label: "Undo", kbd: "⌘Z", run: () => void undoLast() };

type TaskRef = Pick<TaskRow, "path" | "line" | "lineText">;

/** Ticks or unticks. A recurring task gets its next occurrence above it, and `🏁 delete` takes the
 *  done line away, as the Tasks plugin does (src/tasksq/edits.ts); anything else is a plain tick. */
export async function toggleTask(t: TaskRef, done: boolean) {
  try {
    const r = await toggleTaskEdit(t, done);
    if (r.undo) toast(r.undo, undoAction, "ok");
  } catch (e) {
    reportEditError(e);
  }
}

/** The tick itself, for callers that report it their own way (src/mcpActions.ts). */
export function toggleTaskEdit(t: TaskRef, done: boolean): Promise<Edited> {
  const c = done ? inQuote(t.lineText, (l) => completion(l, moment(localToday(), "YYYY-MM-DD"))) : null;
  return c ? api.taskReplace(t, c.lines, c.label) : api.taskToggle(t as TaskRow, done, localToday());
}

export type TaskChange = "cancel" | "reopen" | "start" | "waiting" | "delete" | { priority: PriorityName };

/** Cancel, reopen, start, or set a priority: one line rewritten, undoable. */
export async function changeTask(t: TaskRef, how: TaskChange) {
  try {
    const r = await changeTaskEdit(t, how);
    if (r.undo) toast(r.undo, undoAction, "ok");
  } catch (e) {
    reportEditError(e);
  }
}

/** The change itself, for callers that report it their own way. */
export function changeTaskEdit(t: TaskRef, how: TaskChange): Promise<Edited> {
  const today = moment(localToday(), "YYYY-MM-DD");
  const c = inQuote(t.lineText, (l) =>
    how === "cancel"
      ? cancelled(l, today)
      : how === "reopen"
        ? reopened(l)
        : how === "start"
          ? started(l)
          : how === "waiting"
            ? waitingToggled(l)
            : how === "delete"
              ? { lines: [], label: "Deleted" }
              : withPriority(l, how.priority),
  );
  return api.taskReplace(t, c.lines, c.label);
}

/** Stage 6: a task's contexts, effort or project, one line rewritten, undoable. */
export async function setTaskGtd(t: TaskRef, change: { contexts: string[] } | { effort: string | null } | { project: string | null }) {
  try {
    const r =
      "contexts" in change
        ? await api.taskSetContexts(t, change.contexts)
        : "effort" in change
          ? await api.taskSetEffort(t, change.effort)
          : await api.taskSetProject(t, change.project);
    if (r.undo) toast(r.undo, undoAction, "ok");
  } catch (e) {
    reportEditError(e);
  }
}

/** The task's line with its words changed and its dates, fields and tags kept; null when the
 *  words aren't on the line exactly once, so they can't be changed simply. */
export function retitled(lineText: string, words: string, to: string): string | null {
  // After the bullet and checkbox, so the words never match inside them.
  const head = /^\s*(?:>\s*)*(?:[-*+]|\d+[.)])\s+\[.\]\s*/.exec(lineText)?.[0].length ?? 0;
  const at = words ? lineText.indexOf(words, head) : -1;
  if (at < 0 || lineText.indexOf(words, at + 1) >= 0 || !to.trim()) return null;
  return lineText.slice(0, at) + to.trim() + lineText.slice(at + words.length);
}

/** Changes a task's words, undoable. */
export async function retitleTask(t: TaskRef, words: string, to: string) {
  const line = retitled(t.lineText, words, to);
  if (!line || to.trim() === words) return;
  try {
    const r = await api.taskReplace(t, [line], "Renamed");
    if (r.undo) toast(r.undo, undoAction, "ok");
  } catch (e) {
    reportEditError(e);
  }
}

export async function setTaskDate(t: TaskRow, kind: TaskDateKind, date: string | null) {
  try {
    const r = await api.taskSetDate(t, kind, date);
    if (r.undo) toast(r.undo, undoAction, "ok");
  } catch (e) {
    reportEditError(e);
  }
}

/** Where `moved` goes in `order`: the ranks of its nearest ranked neighbours, or null when the
 *  whole list is to be renumbered, as some rows have no rank yet (src/mcpActions.ts's move_task
 *  places a task by the same rule). */
export function rankBetween(order: TaskRow[], moved: TaskRow): { prev: number | null; next: number | null } | null {
  if (order.some((t) => t.rank === null)) return null;
  const i = order.indexOf(moved);
  const prev =
    order
      .slice(0, i)
      .reverse()
      .find((t) => t.rank !== null)?.rank ?? null;
  const next = order.slice(i + 1).find((t) => t.rank !== null)?.rank ?? null;
  return { prev, next };
}

/** Puts `moved` between the rows that are now above and below it in `order` (the list after the
 *  drop): its nearest ranked neighbours, or the whole list renumbered when there's no room or some
 *  rows have no rank yet (the previous app's TaskQueryBlock). False (and said) when that couldn't
 *  be written. */
export async function moveTask(order: TaskRow[], moved: TaskRow): Promise<boolean> {
  const gap = rankBetween(order, moved);
  try {
    if (gap && (await api.taskRank(moved, gap.prev, gap.next))) return true;
    await api.tasksRankAll(order);
    return true;
  } catch (e) {
    reportEditError(e);
    return false;
  }
}

export async function undoLast() {
  try {
    const r = await api.undo();
    toast(r ?? "Nothing to undo.");
  } catch (e) {
    reportEditError(e);
  }
}
