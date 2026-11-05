// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Task lines changed as the Tasks plugin changes them: completing a recurring task (its next
// occurrence on the line above, Getting Started/Recurring Tasks there), deleting on completion
// (🏁 delete), cancelling, reopening, starting, and priorities. Rust writes the result in one
// atomic edit (`task_replace`), undoable with ⌘Z.

import moment, { Moment } from "moment";
import { RRule } from "rrule";
import { DATE_EMOJI, DateField, parseTask, PRIORITY_EMOJI, PriorityName, splitQuote, statusOf } from "./fields";

const DAY = "YYYY-MM-DD";
const valid = (d: string | undefined): d is string => !!d && moment(d, DAY, true).isValid();
const FIELD_EMOJI = /[🔺⏫🔼🔽⏬📅📆🗓⏳⌛🛫➕✅❌🔁🏁⛔🆔]/u;

/** The line with its status symbol changed. */
const withStatus = (line: string, sym: string) => line.replace(/^(\s*(?:[-*+]|\d+[.)])\s+\[)[^\]](\])/, `$1${sym}$2`);

/** Removes one kind of date (`✅ 2026-10-01`) wherever it is. */
const withoutDate = (line: string, k: DateField) =>
  line.replace(new RegExp(`\\s*(?:${DATE_EMOJI[k].join("|")})\\uFE0F? *\\d{4}-\\d{2}-\\d{2}`, "gu"), "");

/** Adds ` <text>` before a trailing `^rank-N` or block id, else at the end. */
function append(line: string, text: string): string {
  const m = /(\s+\^[\w-]+)+\s*$/.exec(line);
  return m ? `${line.slice(0, m.index).trimEnd()} ${text}${m[0]}` : `${line.trimEnd()} ${text}`;
}

/** The day the next occurrence is based on: due, else scheduled, else start. */
function reference(dates: Partial<Record<DateField, string>>): DateField | null {
  return (["due", "scheduled", "start"] as DateField[]).find((k) => valid(dates[k])) ?? null;
}

const utc = (m: Moment) => new Date(Date.UTC(m.year(), m.month(), m.date()));
const fromUtc = (d: Date) => moment([d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()]);

/** The day after `base` that `rule` (without "when done") next falls on, or null if it can't be read. */
export function nextDay(rule: string, base: Moment): Moment | null {
  let opts;
  try {
    opts = RRule.parseText(rule);
  } catch {
    return null;
  }
  if (!opts || opts.freq === undefined) return null;
  const specific =
    opts.bymonthday != null || opts.byweekday != null || opts.bymonth != null || opts.bysetpos != null || opts.byyearday != null;
  // "every month" and "every year" have no opinion on the day: add months, and move earlier
  // when the day doesn't exist (31 October → 30 November), as the plugin does.
  if (!specific && (opts.freq === RRule.MONTHLY || opts.freq === RRule.YEARLY)) {
    return base.clone().add(opts.interval ?? 1, opts.freq === RRule.MONTHLY ? "months" : "years");
  }
  const r = new RRule({ ...opts, dtstart: utc(base) });
  const next = r.after(utc(base), false);
  return next ? fromUtc(next) : null;
}

/** The next occurrence of a recurring task, as a fresh to-do line, or null when it isn't one. */
export function nextOccurrence(line: string, today: Moment): string | null {
  const f = parseTask(line);
  if (!f?.recurrence) return null;
  const whenDone = /\s+when done$/i.test(f.recurrence);
  const rule = f.recurrence.replace(/\s+when done$/i, "");
  const ref = reference(f.dates);
  // A fresh to-do: the status a done task goes back to.
  let next = withStatus(line, statusOf("x").nextSymbol);
  next = withoutDate(withoutDate(next, "done"), "cancelled")
    .replace(/\s*\^rank-\d+\s*$/, "")
    .replace(/\s+\^[a-zA-Z0-9-]+\s*$/, "");
  if (ref) {
    const refDay = moment(f.dates[ref], DAY, true);
    const nextRef = nextDay(rule, whenDone ? today.clone().startOf("day") : refDay);
    if (!nextRef) return null;
    // Every date keeps its distance from the reference day.
    for (const k of ["due", "scheduled", "start"] as DateField[]) {
      const d = f.dates[k];
      if (!valid(d)) continue;
      const shifted = nextRef.clone().add(moment(d, DAY).diff(refDay, "days"), "days").format(DAY);
      next = next.replace(new RegExp(`((?:${DATE_EMOJI[k].join("|")})\\uFE0F? *)${d}`, "u"), `$1${shifted}`);
    }
  } else if (!RRule.parseText(rule)) return null;
  return next.trimEnd();
}

export interface LineChange {
  lines: string[];
  label: string;
}

/** `f`'s change made to the task inside its quote or callout markers (`> `), each line it writes
 *  getting them back. */
export function inQuote<C extends LineChange | null>(line: string, f: (body: string) => C): C {
  const [pre, body] = splitQuote(line);
  const c = f(body);
  return (c && { ...c, lines: c.lines.map((l) => pre + l) }) as C;
}

/** Ticking a task: null for a plain tick (Rust's `task_toggle`), else the lines to write: the
 *  next occurrence of a recurring task above the done one, and 🏁 delete taking the done one away. */
export function completion(line: string, today: Moment): LineChange | null {
  const f = parseTask(line);
  if (!f || (!f.recurrence && f.onCompletion !== "delete")) return null;
  const day = today.format(DAY);
  const done = append(withoutDate(withStatus(line, "x"), "done"), `✅ ${day}`);
  const next = nextOccurrence(line, today);
  const lines = [...(next ? [next] : []), ...(f.onCompletion === "delete" ? [] : [done])];
  return { lines, label: f.onCompletion === "delete" && !next ? "Deleted" : "Ticked" };
}

export function cancelled(line: string, today: Moment): LineChange {
  return {
    lines: [append(withoutDate(withoutDate(withStatus(line, "-"), "done"), "cancelled"), `❌ ${today.format(DAY)}`)],
    label: "Cancelled",
  };
}

export function reopened(line: string): LineChange {
  return { lines: [withoutDate(withoutDate(withStatus(line, " "), "done"), "cancelled")], label: "Reopened" };
}

export function started(line: string): LineChange {
  return { lines: [withoutDate(withoutDate(withStatus(line, "/"), "done"), "cancelled")], label: "Started" };
}

/** `text` with `f` applied to the parts outside `code` spans, which are left as written. */
const outsideCode = (text: string, f: (part: string) => string) =>
  text
    .split(/(`[^`\n]*`)/)
    .map((part, i) => (i % 2 ? part : f(part)))
    .join("");

/** The index of the first field emoji outside `code` spans, or -1. */
function fieldAt(text: string): number {
  let at = 0;
  for (const [i, part] of text.split(/(`[^`\n]*`)/).entries()) {
    const m = i % 2 ? -1 : part.search(FIELD_EMOJI);
    if (m >= 0) return at + m;
    at += part.length;
  }
  return -1;
}

/** The line with its priority set (or cleared with "none"), placed before the other fields. A
 *  priority emoji inside a `code` span is text, not a priority, and stays. */
export function withPriority(line: string, p: PriorityName): LineChange {
  let l = outsideCode(line, (part) => part.replace(/\s*[🔺⏫🔼🔽⏬]️?/gu, ""));
  if (p !== "none") {
    const emoji = Object.entries(PRIORITY_EMOJI).find(([, v]) => v === p)![0];
    const head = /^(\s*(?:[-*+]|\d+[.)])\s+\[[^\]]\]\s+)/.exec(l)![0];
    const body = l.slice(head.length);
    const at = fieldAt(body);
    const tail = /(\s+\^[\w-]+)+\s*$/.exec(body);
    const cut = at >= 0 ? at : tail ? tail.index : body.length;
    l = `${head}${body.slice(0, cut).trimEnd()} ${emoji}${cut < body.length ? ` ${body.slice(cut).trimStart()}` : ""}`;
  }
  return { lines: [l], label: p === "none" ? "Cleared the priority of" : "Set the priority of" };
}

/** Adds `#waiting-for` after the task's text (before its fields), or takes it out. */
export function waitingToggled(line: string): LineChange {
  return tagToggled(line, "waiting-for", "Waiting for", "No longer waiting for");
}

/** Adds `#tag` after the task's text (before its fields), or takes it out: `on` and `off` are the
 *  undo labels. Someday / maybe, from the weekly review's suggestions, uses it too. */
export function tagToggled(line: string, tag: string, on: string, off: string): LineChange {
  const t = tag.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // The tag itself, not one under it (#waiting-for/acme).
  if (new RegExp(`(^|\\s)#${t}(?![\\w/-])`).test(line)) {
    return { lines: [line.replace(new RegExp(`\\s*#${t}(?![\\w/-])`), "")], label: off };
  }
  const head = /^(\s*(?:[-*+]|\d+[.)])\s+\[[^\]]\]\s+)/.exec(line)![0];
  const body = line.slice(head.length);
  const at = body.search(FIELD_EMOJI);
  const tail = /(\s+\^[\w-]+)+\s*$/.exec(body);
  const cut = at >= 0 ? at : tail ? tail.index : body.length;
  const rest = body.slice(cut).trimStart();
  return { lines: [`${head}${body.slice(0, cut).trimEnd()} #${tag}${rest ? ` ${rest}` : ""}`], label: on };
}
