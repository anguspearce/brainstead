// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Changes (what assistants and runs changed, and what's held for you) and Knowledge health's last
// report, loaded once and reloaded when Rust says they changed: changes arrive from the MCP server
// and from runs in the background, and the checks rerun after the index changes.

import { api, ChangeOrigin, ChangeRow, FileSummary, HealthFix, HealthView, LintItem, LintReport, TrashEntry, VaultChanges } from "./api";
import { Store } from "./store";

export const changes = new Store<ChangeRow[] | null>(null);
export const health = new Store<HealthView | null>(null);

let started = false;

export function reloadChanges() {
  return api
    .changesList()
    .then((r) => changes.set(r))
    .catch(() => changes.set(changes.get() ?? []));
}

export function reloadHealth(fresh = false) {
  return api
    .healthReport(fresh)
    .then((r) => health.set(r))
    .catch(() => {});
}

/** Starts listening; safe to call from every screen that needs them. */
export function startKnowledge() {
  if (started) return;
  started = true;
  void reloadChanges();
  void reloadHealth();
  // Outside Tauri (tests) there are no events to listen to.
  api.onChangesChanged(() => void reloadChanges()).catch(() => {});
  api.onHealthChanged(() => void reloadHealth()).catch(() => {});
}

/** The changes waiting for you. */
export const held = (rows: ChangeRow[]) => rows.filter((c) => c.status === "held");

/** Where a change came from, for its row: "Ask · Sonnet", "Knowledge health", "Daily summary 2026-10-02". */
export function originLabel(c: ChangeRow): string {
  if (c.origin.kind === "lint") return c.origin.label === "Missing page" || !c.origin.label ? "Knowledge health" : c.origin.label;
  if (c.origin.kind === "review") return c.origin.label ? reviewLabel(c.origin.label) : "Scheduled run";
  if (c.origin.label) return c.origin.label;
  return c.origin.chat ? "Ask" : "An assistant";
}

/** What searching Changes looks in: its title, page (and where it moved to), origin and model. The
 *  screen's search box and list_changes' query both match on this. */
export const changeText = (c: ChangeRow) => [c.title, c.page, c.to, originLabel(c), c.model].filter(Boolean).join(" ");

/** Has every word of `q` in its search text (an empty query matches everything). */
export function changeMatches(c: ChangeRow, q: string): boolean {
  const text = changeText(c).toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => text.includes(w));
}

/** A run's changes in one state that a search shows (all of them with no search): what Changes'
 *  Accept all, Reject all and Revert all act on, and the changes tool's runs given a query. */
export function runChanges(rows: ChangeRow[], group: string, status: ChangeRow["status"], q = ""): ChangeRow[] {
  return rows.filter((c) => c.group === group && c.status === status && changeMatches(c, q));
}

/** A run's name in Changes: "Daily check", "Ingest of Steerco", "Ask", "Terminal session". */
export function runLabel(c: ChangeRow): string {
  if (c.origin.run?.startsWith("daily-check-")) return "Daily check";
  if (c.origin.kind === "contradiction") return "Contradiction check";
  return originLabel(c);
}

/** Changes in runs, the runs newest first, each run's changes newest first. */
export function byRun(rows: ChangeRow[]): { group: string; label: string; rows: ChangeRow[] }[] {
  const m = new Map<string, ChangeRow[]>();
  for (const c of rows) m.set(c.group, [...(m.get(c.group) ?? []), c]);
  return [...m].map(([group, rs]) => ({ group, label: runLabel(rs[rs.length - 1]), rows: rs }));
}

/** A new page made at once, as an agent change (in Changes, revertable): "File this answer", a
 *  bookmark promoted to a wiki page, a note from a template. */
export function makePage(page: string, content: string, title: string, origin: ChangeOrigin) {
  return api.changeSubmit({ page, kind: "new", title, instruction: { op: "page", content }, origin });
}

/** `daily-summary 2026-10-02` → `Daily summary 2026-10-02`; `weekly-summary 2026-W40` → `Weekly summary 2026-W40`. */
function reviewLabel(detail: string): string {
  const m = /^(daily|weekly)-summary\s+(.*)$/.exec(detail);
  return m ? `${m[1] === "daily" ? "Daily summary" : "Weekly summary"} ${m[2]}` : detail;
}

/** A page's name from its path. */
export const pageName = (path: string) => (path.split("/").pop() ?? path).replace(/\.md$/, "");

/** Checks that are worth a look but aren't problems: not counted in the sidebar. */
/** Checks whose issues have their own buttons, so they get no Ignore. */
export const OWN_ACTIONS = new Set(["missing-pages", "duplicates", "unreferenced-images", "uningested-sources", "page-shape"]);

export const ADVISORY = new Set(["stale-pages", "uncited-claims", "no-current-state"]);

/** The note a missing link most likely meant: one whose name is the link's but for a slip or two
 *  ("meting" for "meeting"), as when a note was renamed after the link was written. Null when
 *  none is that close, or two are. */
export function nearName(name: string, paths: string[]): string | null {
  const want = linkStem(name).toLowerCase();
  if (want.length < 4) return null;
  const most = Math.min(3, Math.max(1, Math.floor(want.length * 0.08)));
  let best: string | null = null;
  let bestD = most + 1;
  let ties = 0;
  for (const p of paths) {
    const stem = (p.split("/").pop() ?? p).replace(/\.md$/i, "").toLowerCase();
    if (Math.abs(stem.length - want.length) > most || stem === want) continue;
    const d = distance(want, stem, most);
    if (d < bestD) {
      best = p;
      bestD = d;
      ties = 0;
    } else if (d === bestD) ties++;
  }
  return best && !ties ? best : null;
}

/** The meeting note made from a transcript a link names: the same title and day, the transcript's
 *  "Part 1" or "Part 2" counting as the one meeting, as Make a meeting note names it (Meeting, 1-1,
 *  Workshop or Interview). Null when it isn't a transcript's name or there's no such note. */
export function noteFromTranscript(name: string, paths: string[]): string | null {
  const m = /^Teams\. Transcript\. (.+?)(?: - Part \d+)? - (\d{4}-\d{2}-\d{2})$/i.exec(linkStem(name));
  if (!m) return null;
  const [, title, date] = m;
  const want = title.toLowerCase();
  const hits = paths.filter((p) => {
    const n = /^(?:Meeting|1-1|Workshop|Interview)\. (.+) - (\d{4}-\d{2}-\d{2})\.md$/.exec(p.split("/").pop() ?? "");
    return !!n && n[2] === date && (n[1].toLowerCase() === want || distance(n[1].toLowerCase(), want, 2) <= 2);
  });
  return hits.length === 1 ? hits[0] : null;
}

/** A link's note name: its folder and `.md` dropped, as a link written `[[wiki/entities/Maya.md]]` names Maya. */
const linkStem = (name: string) => (name.trim().split("/").pop() ?? "").replace(/\.md$/i, "");

/** The Trash's files by name (lower case, no `.md`), for a missing link that names one. */
export const trashByName = (entries: TrashEntry[]) => new Map(entries.map((e) => [linkStem(e.basename).toLowerCase(), e]));

/** What Knowledge health offers for a missing link (a missing page, or a source a page cites that
 *  isn't there): the file it names if that's in the Trash, and Did you mean, the note it most likely
 *  meant (the meeting note a trashed transcript became, else one a slip away). The screen's rows and
 *  lint and health_issue all use this. */
export type GhostHelp = { name: string; inTrash: TrashEntry | null; near: string | null };
export function ghostHelp(check: string, i: LintItem, paths: string[], trashed: Map<string, TrashEntry>): GhostHelp | null {
  if ((check !== "missing-pages" && check !== "broken-sources") || !i.name) return null;
  return {
    name: i.name,
    inTrash: trashed.get(linkStem(i.name).toLowerCase()) ?? null,
    near: noteFromTranscript(i.name, paths) ?? nearName(i.name, paths),
  };
}

/** The files a missing link may have meant: every note and page, templates left out. */
export const ghostPaths = (files: FileSummary[]) => files.filter((x) => x.layer !== "template").map((x) => x.path);

/** Every missing link's help in a report, by check and item text (`missing-pages:…`): worked out
 *  once per report and file list rather than per row on every render. */
export function ghostHelps(r: LintReport | null, paths: string[], trashed: Map<string, TrashEntry>): Map<string, GhostHelp> {
  const out = new Map<string, GhostHelp>();
  for (const c of r?.checks ?? [])
    for (const i of c.items) {
      const g = ghostHelp(c.id, i, paths, trashed);
      if (g) out.set(`${c.id}:${i.text}`, g);
    }
  return out;
}

/** Whether a vault change can have changed which files there are (one added, removed or renamed, or
 *  a reindex), rather than only what's in them, as an autosave does: `known` is the file list last read. */
export function fileSetChanged(known: ReadonlySet<string> | null, c: VaultChanges | null): boolean {
  return !known || !c || c.removed.length > 0 || c.changed.some((p) => !known.has(p));
}

/** Edit distance, given up past `most` (a quick no). */
function distance(a: string, b: string, most: number): number {
  let prev = Array.from({ length: b.length + 1 }, (_, j) => j);
  for (let i = 1; i <= a.length; i++) {
    const row = [i];
    let low = i;
    for (let j = 1; j <= b.length; j++) {
      row[j] = Math.min(prev[j] + 1, row[j - 1] + 1, prev[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
      low = Math.min(low, row[j]);
    }
    if (low > most) return most + 1;
    prev = row;
  }
  return prev[b.length];
}

/** The checks with Ignore all (knowledge.rs IGNORE_ALL, kept the same): advisory, with no fix of their own. */
export const IGNORE_ALL = new Set(["stale-pages", "uncited-claims"]);

/** Issues that need you: everything Brainstead can't fix on its own. */
export function decisions(r: LintReport | null | undefined): number {
  if (!r) return 0;
  return r.checks.filter((c) => !ADVISORY.has(c.id)).reduce((n, c) => n + c.items.filter((i) => !i.safe).length, 0);
}

/** Every safe fix in the report, as health_fix takes them. */
export function safeFixes(r: LintReport | null | undefined): HealthFix[] {
  if (!r) return [];
  // Reshaping pages is its own action (Reshape pages), recorded in Changes rather than undone with ⌘Z.
  return r.checks.filter((c) => c.id !== "page-shape").flatMap((c) => c.items.filter((i) => i.safe && i.page).map((i) => fixOf(c.id, i)));
}

export const fixOf = (check: string, i: LintItem): HealthFix => ({ check, page: i.page!, name: i.name, detail: i.detail });

/** The trend line's points: one per day with a count, oldest first, scaled into w × h. */
export function trendPoints(history: [string, { decisions: number }][], w: number, h: number): string {
  if (history.length < 2) return "";
  const max = Math.max(1, ...history.map(([, d]) => d.decisions));
  return history
    .map(([, d], i) => `${((i / (history.length - 1)) * w).toFixed(1)},${(h - 4 - (d.decisions / max) * (h - 8)).toFixed(1)}`)
    .join(" ");
}

/** Whether the trend says anything: two days or more, and a day that needed a decision. */
export function trendShown(history: [string, { decisions: number }][]): boolean {
  return history.length >= 2 && history.some(([, d]) => d.decisions > 0);
}

const dayLabel = (day: string) => {
  const d = new Date(`${day}T00:00:00`);
  return Number.isNaN(d.getTime()) ? day : d.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
};

/** The trend's end labels and its tooltip. */
export function trendLabels(history: [string, { decisions: number }][]): { from: string; to: string; tip: string } {
  const [first, d0] = history[0];
  const [last, d1] = history[history.length - 1];
  const max = Math.max(...history.map(([, d]) => d.decisions));
  return {
    from: `${dayLabel(first)}: ${d0.decisions}`,
    to: `${dayLabel(last)}: ${d1.decisions}`,
    tip: `Issues needing a decision each day, ${dayLabel(first)} to ${dayLabel(last)}. Highest: ${max}.`,
  };
}

/** Claims with no citation by page, in the order the check gives them. */
export function claimsByPage(items: LintItem[]): { page: string; items: LintItem[] }[] {
  const out = new Map<string, LintItem[]>();
  for (const i of items) {
    const p = i.page ?? "";
    out.set(p, [...(out.get(p) ?? []), i]);
  }
  return [...out].map(([page, items]) => ({ page, items }));
}

/** What Ask is asked to do about the issues that need judgement. */
export function fixWithAskPrompt(r: LintReport): string {
  const lines: string[] = [];
  for (const c of r.checks) {
    if (ADVISORY.has(c.id)) continue;
    const open = c.items.filter((i) => !i.safe);
    if (!open.length) continue;
    lines.push(`${c.title}:`);
    for (const i of open.slice(0, 15)) lines.push(`- ${i.text}`);
  }
  return [
    "Knowledge health found these issues in the wiki. For each one you can fix from the vault, make the fix with edit_page (one change per page; quote the source for any fact). Say which ones need me to decide.",
    "",
    ...lines,
  ].join("\n");
}
