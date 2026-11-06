// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Every field a task line can carry, as Brainstead shows it: never as the file's emoji, always
// as a line icon (src/icons.tsx) with a label and a tooltip. The dates are in src/taskDates.ts;
// this adds priority, recurrence, the cancelled date, ids, dependencies and on-completion, and
// splits task text into words and fields for the task lists, View and Edit.

import { TASK_DATES, TaskDate } from "./taskDates";
import { PRIORITY_EMOJI, type PriorityName } from "./tasksq/fields";

export type FieldId = "priority" | "recurrence" | "cancelled" | "id" | "dependsOn" | "onCompletion";

export interface FieldDef {
  id: FieldId | TaskDate["id"];
  icon: string;
  label: string;
  explain: string;
}

export const PRIORITY_NAMES: Record<string, string> = { "🔺": "Highest", "⏫": "High", "🔼": "Medium", "🔽": "Low", "⏬": "Lowest" };

export const TASK_FIELDS: Record<FieldId, FieldDef> = {
  priority: {
    id: "priority",
    icon: "priority",
    label: "Priority",
    explain: "How important it is: Highest, High, Medium, Low or Lowest. Higher priorities sort first and raise its urgency.",
  },
  recurrence: {
    id: "recurrence",
    icon: "repeat",
    label: "Repeats",
    explain:
      "When it's ticked, the next one is added above it, with its dates moved on by this rule ('when done' counts from the day it's done).",
  },
  cancelled: {
    id: "cancelled",
    icon: "cancelled",
    label: "Cancelled",
    explain: "The day it was cancelled; it's off every list of things to do.",
  },
  id: { id: "id", icon: "task-id", label: "Id", explain: "This task's id, for other tasks that wait on it." },
  dependsOn: {
    id: "dependsOn",
    icon: "depends-on",
    label: "Waits on",
    explain: "The ids of tasks that must be done first; until then it's blocked.",
  },
  onCompletion: {
    id: "onCompletion",
    icon: "on-completion",
    label: "When done",
    explain: "What happens to it when it's ticked: 'delete' takes the line away, 'keep' leaves it.",
  },
};

export type FieldPart = { field: FieldDef; value: string; emoji: string };
export type TextPart = string | FieldPart;

const DATE_EMOJI_ALT: Record<string, string> = { "📆": "📅", "🗓": "📅", "⌛": "⏳" };
/** Any field: its emoji, then its value. Dates first, so a priority's emoji never eats them. */
export const FIELD_RE =
  /(📅|📆|🗓|⏳|⌛|🛫|➕|✅|❌)️? *(\d{4}-\d{2}-\d{2})|([🔺⏫🔼🔽⏬])️?|🔁️? ?([a-zA-Z0-9, !]+?)(?=\s*(?:[📅📆🗓⏳⌛🛫➕✅❌🔺⏫🔼🔽⏬🏁⛔🆔#^]|$))|🏁️? *([a-zA-Z]+)|⛔️? *([a-zA-Z0-9-_]+(?: *, *[a-zA-Z0-9-_]+)*)|🆔️? *([a-zA-Z0-9-_]+)/gu;

function partOf(m: RegExpMatchArray): FieldPart | null {
  if (m[1]) {
    const e = DATE_EMOJI_ALT[m[1]] ?? m[1];
    if (e === "❌") return { field: TASK_FIELDS.cancelled, value: m[2], emoji: m[1] };
    const d = TASK_DATES.find((x) => x.emoji === e);
    return d ? { field: d, value: m[2], emoji: m[1] } : null;
  }
  if (m[3]) return { field: TASK_FIELDS.priority, value: PRIORITY_NAMES[m[3]], emoji: m[3] };
  if (m[4]) return { field: TASK_FIELDS.recurrence, value: m[4].trim(), emoji: "🔁" };
  if (m[5]) return { field: TASK_FIELDS.onCompletion, value: m[5], emoji: "🏁" };
  if (m[6]) return { field: TASK_FIELDS.dependsOn, value: m[6].replace(/\s+/g, ""), emoji: "⛔" };
  if (m[7]) return { field: TASK_FIELDS.id, value: m[7], emoji: "🆔" };
  return null;
}

/** Task text as words and fields, `code` spans left alone. */
export function splitTaskFields(text: string): TextPart[] {
  const out: TextPart[] = [];
  const push = (s: string) => {
    if (!s) return;
    if (typeof out[out.length - 1] === "string") out[out.length - 1] += s;
    else out.push(s);
  };
  for (const [i, part] of text.split(/(`[^`\n]*`)/).entries()) {
    if (i % 2) {
      push(part);
      continue;
    }
    let last = 0;
    for (const m of part.matchAll(FIELD_RE)) {
      const p = partOf(m);
      if (!p) continue;
      push(part.slice(last, m.index));
      out.push(p);
      last = m.index! + m[0].length;
    }
    push(part.slice(last));
  }
  return out;
}

/** The fields of task text, in order. */
export const taskFieldsOf = (text: string) => splitTaskFields(text).filter((p): p is FieldPart => typeof p !== "string");

/** A task's priority as its row's chip shows it: the first priority emoji anywhere in its text
 *  outside code spans, not only at the end of the line (capture writes `Call Sam ⏫ [effort:: 15m]`).
 *  The one reader for the Tasks screen's sort and grouping and the assistants' list_tasks, so they
 *  agree with the chip. The `tasks` query blocks keep the Tasks plugin's trailing-fields reading. */
export function priorityOf(t: { text: string }): PriorityName {
  const p = taskFieldsOf(t.text).find((x) => x.field.id === "priority");
  return (p && PRIORITY_EMOJI[p.emoji]) || "none";
}

/** Task text with every field taken out (they're shown as chips). */
export const withoutFields = (text: string) =>
  splitTaskFields(text)
    .filter((p): p is string => typeof p === "string")
    .join("")
    .replace(/\s{2,}/g, " ")
    .trim();

/** A field's chip text: a date as given by `fmt`, the rest as written. */
export function fieldValue(p: FieldPart, fmtDate: (d: string) => string): string {
  if (/^\d{4}-\d{2}-\d{2}$/.test(p.value)) return fmtDate(p.value);
  if (p.field.id === "dependsOn") return p.value.split(",").join(", ");
  return p.value;
}

/** Text with its task fields as words, for a tooltip, which can't draw their icons: "✅ 2026-10-04" → "Done 2026-10-04". */
export const fieldsAsWords = (text: string) =>
  splitTaskFields(text)
    .map((p) => (typeof p === "string" ? p : `${p.field.label} ${p.value}`))
    .join("");

/** A field's tooltip: what it is and what it means. */
export const fieldTip = (p: FieldPart, shown: string) =>
  `${p.field.label}: ${shown}${/^\d{4}-\d{2}-\d{2}$/.test(p.value) ? ` · ${p.value}` : ""}\n${p.field.explain}`;

/** The icon for a field: a low priority points down. */
export const fieldIcon = (p: FieldPart) => (p.field.id === "priority" && /low/i.test(p.value) ? "priority-low" : p.field.icon);

/** Every emoji a task field is written with, for tests that nothing shows them. */
export const FIELD_EMOJI = ["📅", "📆", "🗓", "⏳", "⌛", "🛫", "➕", "✅", "❌", "🔺", "⏫", "🔼", "🔽", "⏬", "🔁", "🏁", "⛔", "🆔"];

/** The field a Tasks-format emoji stands for, on its own (in prose or code, not a task's field). */
export function fieldOfEmoji(emoji: string): FieldDef | null {
  const e = emoji.replace(/\uFE0F/g, "");
  const alt = DATE_EMOJI_ALT[e] ?? e;
  if (alt === "❌") return TASK_FIELDS.cancelled;
  const d = TASK_DATES.find((x) => x.emoji === alt);
  if (d) return d;
  if (PRIORITY_NAMES[e]) return TASK_FIELDS.priority;
  return (
    (
      { "🔁": TASK_FIELDS.recurrence, "🏁": TASK_FIELDS.onCompletion, "⛔": TASK_FIELDS.dependsOn, "🆔": TASK_FIELDS.id } as Record<
        string,
        FieldDef
      >
    )[e] ?? null
  );
}

/** Every Tasks-format emoji, with a variation selector or not. */
export const FIELD_EMOJI_RE = new RegExp(`(?:${FIELD_EMOJI.join("|")})\uFE0F?`, "gu");

/** A field by id (dates included), for chips drawn from data attributes. */
export function fieldById(id: string): FieldDef | null {
  return (TASK_FIELDS as Record<string, FieldDef>)[id] ?? TASK_DATES.find((d) => d.id === id) ?? null;
}

/** What a chip shows: its icon, its text and its tooltip. */
export function chipOf(id: string, value: string, fmtDate: (d: string) => string): { icon: string; text: string; tip: string } | null {
  const field = fieldById(id);
  if (!field) return null;
  const p: FieldPart = { field, value, emoji: "" };
  const text = fieldValue(p, fmtDate);
  return { icon: fieldIcon(p), text, tip: fieldTip(p, text) };
}
