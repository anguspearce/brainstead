// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Tasks screen's grouping (by project, context, due date or priority), its priority sort and
// its effort filter.

import moment from "moment";
import type { TaskRow } from "./api";
import { projectName } from "./gtd";
import type { PriorityName } from "./tasksq/fields";
import { priorityOf } from "./taskFields";

export type GroupBy = "none" | "project" | "context" | "due" | "priority";
/** The order of a list: `manual` is its own (dragged) order, `priority` puts the most important first. */
export type SortBy = "manual" | "priority";

/** Every grouping, as the Tasks screen's Group by and the assistants' `group` name them. */
export const GROUP_NAMES: GroupBy[] = ["none", "project", "context", "due", "priority"];
export const SORT_NAMES: SortBy[] = ["manual", "priority"];

/** Most important first. A task with no priority comes after Low and Lowest (the Tasks plugin's own
 *  order puts it between Medium and Low). */
const PRIORITY_ORDER: PriorityName[] = ["highest", "high", "medium", "low", "lowest", "none"];
const PRIORITY_LABEL: Record<PriorityName, string> = {
  highest: "Highest",
  high: "High",
  medium: "Medium",
  low: "Low",
  lowest: "Lowest",
  none: "No priority",
};

/** Rows by priority, highest first and unprioritised last; rows of one priority keep the order they
 *  came in (the list's own, manual or not). Returns a new array. */
export function sortTasks(rows: TaskRow[], by: SortBy): TaskRow[] {
  // Anything but a known sort (a stale remembered value) keeps the list's own order.
  if (by !== "priority") return rows;
  const key = new Map(rows.map((t) => [t, PRIORITY_ORDER.indexOf(priorityOf(t))]));
  return [...rows].sort((a, b) => key.get(a)! - key.get(b)!);
}

export interface TaskGroup {
  key: string;
  label: string;
  rows: TaskRow[];
}

const DUE_ORDER = ["overdue", "today", "week", "later", "none"] as const;
const DUE_LABEL: Record<(typeof DUE_ORDER)[number], string> = {
  overdue: "Overdue",
  today: "Today",
  week: "This week",
  later: "Later",
  none: "No due date",
};

function dueBucket(t: TaskRow, today: string): (typeof DUE_ORDER)[number] {
  if (!t.due) return "none";
  if (t.due < today) return "overdue";
  if (t.due === today) return "today";
  const end = moment(today, "YYYY-MM-DD").isoWeekday(7).format("YYYY-MM-DD");
  return t.due <= end ? "week" : "later";
}

/** Rows in groups, each keeping the rows' own order. Projects and contexts A–Z with the "none" one last; due dates and priorities in their own order. */
export function groupTasks(rows: TaskRow[], by: GroupBy, today: string): TaskGroup[] {
  if (by === "none") return [{ key: "", label: "", rows }];
  const groups = new Map<string, TaskGroup>();
  const add = (key: string, label: string, t: TaskRow) => {
    const g = groups.get(key) ?? { key, label, rows: [] };
    g.rows.push(t);
    groups.set(key, g);
  };
  for (const t of rows) {
    if (by === "priority") {
      const p = priorityOf(t);
      add(p, PRIORITY_LABEL[p], t);
    } else if (by === "project") add(t.project ?? "", t.project ? projectName(t.project) : "No project", t);
    else if (by === "context") {
      const c = t.contexts?.[0];
      add(c ?? "", c ? `@${c}` : "No context", t);
    } else {
      const b = dueBucket(t, today);
      add(b, DUE_LABEL[b], t);
    }
  }
  const out = [...groups.values()];
  if (by === "priority")
    return out.sort((a, b) => PRIORITY_ORDER.indexOf(a.key as PriorityName) - PRIORITY_ORDER.indexOf(b.key as PriorityName));
  if (by === "due") return out.sort((a, b) => DUE_ORDER.indexOf(a.key as never) - DUE_ORDER.indexOf(b.key as never));
  return out.sort((a, b) => ((a.key === "") !== (b.key === "") ? (a.key === "" ? 1 : -1) : a.label.localeCompare(b.label)));
}

/** The effort filter's choices, in minutes ("" for any). */
export const EFFORT_LIMITS: [string, string][] = [
  ["", "Any effort"],
  ["15", "15 min or less"],
  ["30", "30 min or less"],
  ["60", "1 hour or less"],
];

export const withinEffort = (t: TaskRow, limit: string) => !limit || (t.effortMin != null && t.effortMin <= Number(limit));

const fold = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

/** Whether a task matches the Tasks search: every word of `q` somewhere in its text, note, heading,
 *  project, tags or contexts, ignoring case and accents. */
export function matchesSearch(t: TaskRow, q: string): boolean {
  const words = fold(q).split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const hay = fold(
    [t.text, t.title, t.heading, t.project && projectName(t.project), ...t.tags, ...(t.contexts ?? []).map((c) => `@${c}`)]
      .filter(Boolean)
      .join(" "),
  );
  return words.every((w) => hay.includes(w.replace(/^#/, "")));
}
