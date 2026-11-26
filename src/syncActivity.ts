// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Activity › Sync & ingest (D-20261119-01): the sync's passes (src-tauri/src/sync/log.rs) and the
// ingest runs, as one timeline, newest first. The screen, the menu-bar window's line and the
// sync_activity tool all read it from here, so they can't disagree.

import type { IngestRun, SyncLog, SyncPass } from "./api";
import { localToday } from "./md/taskQuery";

export type Service = "teams" | "outlook" | "slack" | "other";
export type FeedFilter = "all" | Exclude<Service, "other"> | "failed";
export type FeedStatus = "ok" | "nothing" | "trashed" | "running" | "failed" | "blocked" | "stopped";

export interface FeedRow {
  key: string;
  /** Local, YYYY-MM-DDTHH:MM:SS. */
  at: string;
  kind: "sync" | "ingest";
  status: FeedStatus;
  services: Service[];
  /** One line: what happened. */
  title: string;
  /** The files it names, by what they are ("New from Teams", "Changed"), to open. */
  groups: [label: string, paths: string[]][];
  /** More lines: what went wrong, what was left out. */
  detail: string[];
  /** Every file in `groups`, in order. */
  paths: string[];
}

/** A group as a line of text: "New from Teams: Teams. Chat. Maya - 2026-11-19". */
export const groupLine = ([label, paths]: [string, string[]]) => `${label}: ${paths.map(nameOf).join(", ")}`;

export const FEED_FILTERS: [FeedFilter, string, string][] = [
  ["all", "All", "Every sync pass and ingest"],
  ["teams", "Teams", "Teams chats and transcripts"],
  ["outlook", "Outlook", "Outlook mail"],
  ["slack", "Slack", "Slack conversations"],
  ["failed", "Problems", "Failed ingests and sync passes with a problem or that couldn't run"],
];

/** Which service a source came from, by its name. */
export function serviceOf(path: string): Service {
  const name = path.split("/").pop() ?? path;
  if (name.startsWith("Teams. ")) return "teams";
  if (name.startsWith("Email. ")) return "outlook";
  if (name.startsWith("Slack. ")) return "slack";
  return "other";
}

const SERVICE_NAME: Record<Service, string> = { teams: "Teams", outlook: "Outlook", slack: "Slack", other: "Other" };
export const nameOf = (path: string) => (path.split("/").pop() ?? path).replace(/\.md$/, "");
const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const hm = (at: string) => at.slice(11, 16);

function services(paths: string[]): Service[] {
  return [...new Set(paths.map(serviceOf))].sort((a, b) => Object.keys(SERVICE_NAME).indexOf(a) - Object.keys(SERVICE_NAME).indexOf(b));
}

function passRow(p: SyncPass): FeedRow {
  const all = [...p.added, ...p.toppedUp];
  const status: FeedStatus = p.blocked ? "blocked" : p.problems.length && !all.length ? "failed" : "ok";
  const parts = [p.added.length && `${p.added.length} new`, p.toppedUp.length && `${p.toppedUp.length} topped up`].filter(Boolean);
  const again = p.repeats ? ` (${plural(p.repeats + 1, "time")}, last at ${hm(p.last ?? p.at)})` : "";
  const title = p.blocked
    ? `Sync couldn't run${again}`
    : all.length
      ? `${p.backfill ? "Fetched again" : "Synced"}: ${parts.join(", ")}`
      : `Sync had a problem${again}`;
  const byService = (list: string[], how: string): [string, string[]][] =>
    services(list).map((s) => [`${how} from ${SERVICE_NAME[s]}`, list.filter((x) => serviceOf(x) === s)]);
  const groups = [...byService(p.added, "New"), ...byService(p.toppedUp, "Topped up")];
  return {
    key: `sync:${p.at}`,
    at: p.at,
    kind: "sync",
    status,
    services: services(all),
    title,
    groups,
    detail: [
      ...(p.blocked ? [p.blocked] : []),
      ...p.problems.map((x) => `Problem: ${x}`),
      ...(p.backfill && p.added.length ? ["Fetched again, so it waits in the Inbox rather than being ingested."] : []),
    ],
    paths: groups.flatMap(([, ps]) => ps),
  };
}

function ingestRow(r: IngestRun): FeedRow {
  const source = nameOf(r.source);
  const meeting = r.kind === "meeting";
  const verb = meeting ? "Meeting note from" : r.kind === "actions" ? "Actions and decisions from" : "Ingested";
  const running = r.status === "running" || r.status === "queued";
  const status: FeedStatus = running
    ? "running"
    : r.status === "failed"
      ? "failed"
      : r.status === "stopped"
        ? "stopped"
        : r.trashed
          ? "trashed"
          : r.pages.length
            ? "ok"
            : "nothing";
  const what =
    status === "running"
      ? r.status === "queued"
        ? "waiting its turn"
        : (r.steps.find((s) => s.status === "running")?.name ?? "starting").toLowerCase()
      : status === "failed"
        ? "failed"
        : status === "stopped"
          ? "stopped"
          : status === "trashed"
            ? "nothing in it, moved to the Trash"
            : status === "nothing"
              ? "nothing for the wiki"
              : `${plural(r.pages.length, "page")} changed`;
  return {
    key: `ingest:${r.id}`,
    at: r.started,
    kind: "ingest",
    status,
    services: [serviceOf(r.source)],
    title: `${verb} ${source}: ${what}`,
    groups: [["Source", [r.source]], ...(r.pages.length ? [["Changed", r.pages] as [string, string[]]] : [])],
    detail: [
      ...(r.error && status === "failed" ? [r.error] : []),
      ...r.dropped.map((d) => `Left out ${nameOf(d.page)}: ${d.reason}`),
      ...(r.trigger === "scheduled" ? ["Started by the daily check or an unattended assistant."] : []),
    ],
    paths: [r.source, ...r.pages],
  };
}

const failing = (r: FeedRow) => r.status === "failed" || r.status === "blocked" || r.detail.some((d) => d.startsWith("Problem: "));

/** The timeline, newest first, kept to `filter`. */
export function syncFeed(log: SyncLog, runs: IngestRun[], filter: FeedFilter = "all"): FeedRow[] {
  const rows = [...log.passes.map(passRow), ...runs.map(ingestRow)].sort((a, b) => b.at.localeCompare(a.at) || a.key.localeCompare(b.key));
  if (filter === "all") return rows;
  if (filter === "failed") return rows.filter(failing);
  return rows.filter((r) => r.services.includes(filter));
}

/** How long ago, in words: "just now", "4 min ago", "2 h ago", or the day. */
export function ago(at: string, now = new Date()): string {
  const t = new Date(at).getTime();
  if (Number.isNaN(t)) return at;
  const min = Math.floor((now.getTime() - t) / 60000);
  if (min < 1) return "just now";
  if (min < 60) return `${min} min ago`;
  if (min < 24 * 60) return `${Math.floor(min / 60)} h ago`;
  return `on ${at.slice(0, 10)}`;
}

/** The menu-bar window's line: "Synced 4 min ago · 3 in today · 2 ingesting · 1 problem today"; null
 *  when nothing has synced or been ingested. `problems` is its last part alone ("1 problem today",
 *  or ""), the one part Activity shows in amber: the rest is only informational. */
export function syncLine(log: SyncLog, runs: IngestRun[], now = new Date()): { text: string; problem: boolean; problems: string } | null {
  if (!log.lastPass && !runs.length) return null;
  const today = localToday(now);
  const rows = syncFeed(log, runs);
  const inToday = log.passes.filter((p) => p.at.startsWith(today)).reduce((n, p) => n + p.added.length, 0);
  const ingesting = runs.filter((r) => r.status === "running" || r.status === "queued").length;
  // A run of repeats counts when its last pass was today.
  const lastAt = (r: FeedRow) => (r.kind === "sync" ? (log.passes.find((p) => p.at === r.at)?.last ?? r.at) : r.at);
  const problems = rows.filter((r) => failing(r) && lastAt(r).startsWith(today)).length;
  const bad = problems ? `${plural(problems, "problem")} today` : "";
  const parts = [
    log.lastPass ? `Synced ${ago(log.lastPass, now)}` : "Not synced yet",
    inToday ? `${inToday} in today` : "",
    ingesting ? `${ingesting} ingesting` : "",
    bad,
  ].filter(Boolean);
  return { text: parts.join(" · "), problem: problems > 0, problems: bad };
}
