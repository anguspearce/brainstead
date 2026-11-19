// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Changes › All (D-20261119-04): what Changes records (each change, with Revert) and the vault's
// log.md in one history, newest first. Applying a change writes its log.md line, so a line that
// stands for a change Changes still has is left out: it's shown once, as the change. What's left
// of log.md is what Changes doesn't hold: lines older than its history, lines from another
// computer, the previous app or a terminal, and Brainstead's own writes that aren't changes (the
// weekly review, Fix name, safe fixes, the catalogue). The screen and list_changes share this.

import type { ChangeRow, LogEntry } from "./api";
import { actionLabel, filterLog } from "./activityModel";
import { byRun, changeMatches } from "./knowledge";

/** The log.md action a change's line has (src-tauri/src/changes.rs `log_line`, kept beside it). */
export function changeAction(c: ChangeRow): string {
  if (c.origin.kind === "ingest") return "ingest";
  if (c.origin.kind === "review") return "review";
  if (c.origin.kind === "lint") return "lint-fix";
  return c.kind === "new" ? "create" : "update";
}

/** A change's day, YYYY-MM-DD (its time is local, as log.md's are). */
export const changeDay = (c: ChangeRow) => c.created.slice(0, 10);

/** Minutes since the epoch of a local "YYYY-MM-DDTHH:MM[:SS]" or a log entry's day and time. */
const minutes = (s: string) => {
  const t = new Date(s.length === 10 ? `${s}T00:00` : s.slice(0, 16)).getTime();
  return Number.isNaN(t) ? NaN : Math.floor(t / 60_000);
};
const entryMinutes = (e: LogEntry) => (e.time ? minutes(`${e.date}T${e.time}`) : NaN);
const stem = (p: string) => (p.split("/").pop() ?? p).replace(/\.md$/, "");

/** Whether a log.md line is the one `c` wrote when it was made: the same action, within two
 *  minutes of when it was made (or accepted), and for a page's own line the same page. */
export function writtenBy(e: LogEntry, c: ChangeRow): boolean {
  if (c.status !== "applied" && c.status !== "reverted") return false;
  if (e.action !== changeAction(c)) return false;
  const at = entryMinutes(e);
  if (Number.isNaN(at)) return false;
  const near = [c.created, c.decided].some((t) => t && Math.abs(minutes(t) - at) <= 2);
  if (!near) return false;
  if (e.action === "create" || e.action === "update") return e.title === stem(c.page) || (!!c.to && e.title === stem(c.to));
  return true;
}

/** log.md's lines that aren't a change Changes still has, newest first as log.md is. */
export function logOnly(log: LogEntry[], rows: ChangeRow[]): LogEntry[] {
  // Changes by day, so each line is checked against that day's (and its neighbours') only.
  const byDay = new Map<string, ChangeRow[]>();
  for (const c of rows)
    for (const d of [c.created, c.decided ?? ""].filter(Boolean)) {
      const k = d.slice(0, 10);
      const list = byDay.get(k) ?? [];
      if (!list.includes(c)) list.push(c);
      byDay.set(k, list);
    }
  return log.filter((e) => !(byDay.get(e.date) ?? []).some((c) => writtenBy(e, c)));
}

/** All's filters: a type (a log.md action, as its chips name it), a day and the search. */
export interface AllFilter {
  type: string | null;
  day: string | null;
  q: string;
}

export const changeShown = (c: ChangeRow, f: AllFilter) =>
  (!f.type || changeAction(c) === f.type) && (!f.day || changeDay(c) === f.day) && changeMatches(c, f.q);

export const entryShown = (e: LogEntry, f: AllFilter) => filterLog([e], { action: f.type, day: f.day, q: f.q }).length > 0;

/** The types All's chips offer: every action among the changes and the log, the most used first. */
export function typesIn(rows: ChangeRow[], log: LogEntry[]): string[] {
  const n = new Map<string, number>();
  for (const a of [...rows.map(changeAction), ...log.map((e) => e.action)]) n.set(a, (n.get(a) ?? 0) + 1);
  return [...n].sort((a, b) => b[1] - a[1] || actionLabel(a[0]).localeCompare(actionLabel(b[0]))).map(([a]) => a);
}

/** One line of All: a run of changes, or a log.md line that isn't one. */
export type AllItem =
  { kind: "run"; at: string; run: { group: string; label: string; rows: ChangeRow[] } } | { kind: "log"; at: string; entry: LogEntry };

/** All, newest first: the runs of the changes given (newest change first) and the log lines given,
 *  each placed by when it happened; a line with no time goes after its day's timed ones. */
export function allTimeline(rows: ChangeRow[], log: LogEntry[]): AllItem[] {
  const runs: AllItem[] = byRun(rows).map((run) => ({ kind: "run", at: run.rows[0].created.slice(0, 16), run }));
  const lines: AllItem[] = log.map((entry) => ({ kind: "log", at: entry.time ? `${entry.date}T${entry.time}` : `${entry.date}T`, entry }));
  // Stable: log.md's own order is kept among lines at the same time.
  return [...runs, ...lines]
    .map((x, i) => ({ x, i }))
    .sort((a, b) => (a.x.at < b.x.at ? 1 : a.x.at > b.x.at ? -1 : a.i - b.i))
    .map(({ x }) => x);
}
