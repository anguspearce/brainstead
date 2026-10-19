// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Activity screen's state apart from drawing: the heatmap's weeks and shades, the log's
// action names and filters.

import type { ActivityDay, LogEntry } from "./api";

/** A YYYY-MM-DD date `n` days after `d` (local, noon, so daylight saving can't move it a day). */
export function addDays(d: string, n: number): string {
  const [y, m, day] = d.split("-").map(Number);
  const t = new Date(y, m - 1, day + n, 12);
  return iso(t);
}

export const iso = (t: Date) => `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, "0")}-${String(t.getDate()).padStart(2, "0")}`;

/** Monday 0 … Sunday 6. */
const weekday = (d: string) => (new Date(`${d}T12:00:00`).getDay() + 6) % 7;

export interface Cell {
  date: string;
  total: number;
  day: ActivityDay | null;
  /** 0 (nothing) to 4 (the busiest days). */
  level: number;
  /** After today: drawn empty. */
  future: boolean;
}

export interface Grid {
  /** Columns of seven days, Monday first, oldest week first. */
  weeks: Cell[][];
  /** A month's short name over the week its first day is in, with the year on January and on
   * the first month shown. */
  months: { week: number; label: string; year?: string }[];
}

const total = (d: ActivityDay) => d.notes + d.wiki + d.sources;

/** The shade for `n` changes when the busiest day had `max`: quarters of the range. */
export function level(n: number, max: number): number {
  if (n <= 0 || max <= 0) return 0;
  return Math.min(4, Math.max(1, Math.ceil((n / max) * 4)));
}

/**
 * Every week from the earliest change (at least `minWeeks` back) to this week. The scale ignores
 * the busiest 2% of days, so one bulk change (a sync, a rename across the vault) doesn't wash out
 * the rest.
 */
export function buildGrid(days: ActivityDay[], today: string, minWeeks = 26): Grid {
  const byDate = new Map(days.map((d) => [d.date, d]));
  const end = addDays(today, 6 - weekday(today));
  const earliest = days.length ? days[0].date : today;
  let start = addDays(end, -(minWeeks * 7 - 1));
  if (earliest < start) start = addDays(earliest, -weekday(earliest));
  const totals = days.map(total).sort((a, b) => a - b);
  const max = totals.length ? totals[Math.min(totals.length - 1, Math.floor(totals.length * 0.98))] : 0;
  const weeks: Cell[][] = [];
  const months: Grid["months"] = [];
  for (let d = start, w = 0; d <= end; w++) {
    const col: Cell[] = [];
    for (let i = 0; i < 7; i++, d = addDays(d, 1)) {
      const day = byDate.get(d) ?? null;
      const n = day ? total(day) : 0;
      col.push({ date: d, total: n, day, level: level(n, max), future: d > today });
    }
    const first = col.find((c) => c.date.endsWith("-01"));
    if (first) {
      const year = !months.length || first.date.slice(5, 7) === "01" ? first.date.slice(0, 4) : undefined;
      months.push({ week: w, label: MONTH.format(noon(first.date)), year });
    }
    weeks.push(col);
  }
  return { weeks, months };
}

// Intl formatters are slow to make and fast to use: made once.
const LONG = new Intl.DateTimeFormat("en-GB", { weekday: "short", day: "numeric", month: "short", year: "numeric" });
const MONTH = new Intl.DateTimeFormat("en-GB", { month: "short" });
const noon = (d: string) => new Date(`${d}T12:00:00`);

/** "Tue, 29 Sep 2026". */
export const fmtDate = (d: string) => LONG.format(noon(d));

/** What a day's cell says on hover. */
export function cellTip(c: Cell): string {
  const when = fmtDate(c.date);
  if (!c.day || !c.total) return `${when}: nothing changed`;
  const parts = [
    [c.day.notes, "note"],
    [c.day.wiki, "wiki page"],
    [c.day.sources, "source"],
  ]
    .filter(([n]) => (n as number) > 0)
    .map(([n, w]) => `${n} ${w}${n === 1 ? "" : "s"}`);
  return `${when}: ${parts.join(", ")}`;
}

const ACTIONS: Record<string, string> = {
  create: "Created",
  update: "Updated",
  ingest: "Ingested",
  review: "Summary",
  "lint-fix": "Health fix",
  lint: "Health check",
  "fix-name": "Name fixed",
  index: "Index",
  rename: "Renamed",
  trash: "Trashed",
  delete: "Deleted",
  query: "Query",
};

export function actionLabel(a: string): string {
  return ACTIONS[a] ?? (a.charAt(0).toUpperCase() + a.slice(1)).replace(/-/g, " ");
}

/** Actions in the log, the most used first. */
export function actionsIn(log: LogEntry[]): string[] {
  const n = new Map<string, number>();
  for (const e of log) n.set(e.action, (n.get(e.action) ?? 0) + 1);
  return [...n.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([a]) => a);
}

/** The entries matching an action, a day and every word of a search. */
export function filterLog(log: LogEntry[], f: { action: string | null; day: string | null; q: string }): LogEntry[] {
  const words = f.q.toLowerCase().split(/\s+/).filter(Boolean);
  return log.filter(
    (e) =>
      (!f.action || e.action === f.action) &&
      (!f.day || e.date === f.day) &&
      words.every((w) => `${e.title}\n${e.description}\n${actionLabel(e.action)}`.toLowerCase().includes(w)),
  );
}

/** A log title as a link target: `[[Page|alias]]` → `Page`, else the title as it is. */
export function linkTarget(title: string): string {
  const m = title.match(/^\[\[([^\]|#]+)(?:[#|][^\]]*)?\]\]$/);
  return (m ? m[1] : title).trim();
}

const words = (q: string) => q.toLowerCase().split(/\s+/).filter(Boolean);

/**
 * Search results with the best matches first: every word in the title, then some, then the
 * description only; newest first within each.
 */
export function rankLog(log: LogEntry[], q: string): LogEntry[] {
  const ws = words(q);
  const score = (e: LogEntry) => {
    const t = e.title.toLowerCase();
    const inTitle = ws.filter((w) => t.includes(w)).length;
    return (inTitle === ws.length ? 100 : 0) + inTitle * 10 + (t.startsWith(ws[0] ?? "\0") ? 5 : 0);
  };
  return log
    .map((e, i) => ({ e, i, s: score(e) }))
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .map((x) => x.e);
}

/** `text` cut into pieces, the ones matching a word of `q` marked (case ignored). */
export function marks(text: string, q: string): { text: string; hit: boolean }[] {
  const ws = words(q).filter((w) => w.length >= 2);
  if (!ws.length || !text) return [{ text, hit: false }];
  const re = new RegExp(`(${ws.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`, "gi");
  return text
    .split(re)
    .filter((p) => p !== "")
    .map((p) => ({ text: p, hit: ws.includes(p.toLowerCase()) }));
}
