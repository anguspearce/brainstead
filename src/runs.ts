// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What's running, for the menu-bar window and the main window alike (D-20261120-03): each run with
// what it's doing, how far it has got and its Stop, read as run_status reads them. The main window
// also notices a run ending, keeps the last few that ended this session and says so in a toast.

import { api, CurrentStateRun, DailyCheckStatus, FindState, IngestRun, ReviewsStatus, WeekPrepJob } from "./api";
import { pageName } from "./knowledge";
import type { Place } from "./nav";
import { Store } from "./store";

export interface Running {
  key: string;
  label: string;
  /** The run in a word or two, for the sidebar's line: "Daily check", "Ingest". */
  short: string;
  doing: string;
  /** 0 to 1, or null when there's no measure. */
  progress: number | null;
  stop: () => Promise<unknown>;
  /** An ingest waiting its turn in the queue: the lists count these rather than list them. */
  queued?: boolean;
  /** Where the run and what it made are: its pane, screen or list. */
  place: Place;
  /** How it ended, for a run the window keeps itself (an Update…), read once it's gone. */
  end?: () => Pick<Finished, "text" | "failed" | "place" | "open">;
}

/** What's going, as run_status lists it (src/mcpActions.ts): each run with its Stop. */
export interface RunStates {
  ingests: IngestRun[];
  dailyCheck: DailyCheckStatus;
  reviews: ReviewsStatus;
  contradictions: boolean;
  weekprep: WeekPrepJob | null;
  find: FindState | null;
  currentState: CurrentStateRun | null;
}

const JOBS: Place = { screen: "settings", pane: "jobs" };
const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

export function runningOf({ ingests, dailyCheck, reviews, contradictions, weekprep, find, currentState }: RunStates): Running[] {
  const out: Running[] = [];
  // Meeting notes are runs in the ingest queue of their own kind.
  for (const r of ingests.filter((x) => x.status === "running" || x.status === "queued")) {
    const steps = r.steps.length;
    const done = r.steps.filter((s) => s.status === "done" || s.status === "skipped").length;
    const meeting = r.kind === "meeting";
    out.push({
      key: `ingest:${r.id}`,
      label: meeting ? `Writing a meeting note from ${pageName(r.source)}` : `Ingesting ${pageName(r.source)}`,
      short: meeting ? "Meeting note" : "Ingest",
      doing: r.status === "queued" ? "Waiting its turn" : (r.steps.find((s) => s.status === "running")?.name ?? "Starting"),
      progress: steps ? done / steps : null,
      stop: () => api.ingestStop(r.id),
      queued: r.status === "queued",
      place: { screen: "sources" },
    });
  }
  if (dailyCheck.running)
    out.push({
      key: "daily-check",
      label: "Running the daily check",
      short: "Daily check",
      doing: dailyCheck.doing,
      progress: dailyCheck.progress,
      stop: () => api.dailyCheckStop(),
      place: JOBS,
    });
  for (const k of reviews.running)
    out.push({
      key: `review:${k}`,
      label: `Writing the ${k} summary`,
      short: `${cap(k)} summary`,
      doing: "The model is writing it",
      progress: null,
      stop: () => api.reviewsStop(k),
      place: JOBS,
    });
  if (contradictions)
    out.push({
      key: "contradictions",
      label: "Checking for contradictions",
      short: "Contradictions",
      doing: "Reading the wiki",
      progress: null,
      stop: () => api.contradictionsStop(),
      place: { screen: "contradictions" },
    });
  if (weekprep?.running)
    out.push({
      key: "weekprep",
      label: "Preparing the weekly review",
      short: "Weekly review prep",
      doing: `Suggestions for ${weekprep.running}`,
      progress: null,
      stop: () => api.weekprepStop(),
      place: { screen: "weekly" },
    });
  const f = find?.run;
  if (f?.status === "running")
    out.push({
      key: "find",
      label: "Finding tasks and projects",
      short: "Find tasks",
      doing: `${f.done} of ${f.batches} batches, ${f.found} found`,
      progress: f.batches ? f.done / f.batches : null,
      stop: () => api.findStop(),
      place: JOBS,
    });
  if (currentState?.running)
    out.push({
      key: "current-state",
      label: "Writing Current state",
      short: "Current state",
      doing: `${currentState.done} of ${currentState.total} pages`,
      progress: currentState.total ? currentState.done / currentState.total : null,
      stop: () => api.currentStateStop(),
      place: { screen: "health" },
    });
  return out;
}

/** The runs a list shows, each with its row, and how many more wait their turn (one line, "3 more
 *  waiting their turn", rather than a row each). */
export function shownRuns(running: Running[]): { shown: Running[]; queued: number } {
  return { shown: running.filter((r) => !r.queued), queued: running.filter((r) => r.queued).length };
}

/** Every run's state, each read on its own so one that fails reads as not running. */
export async function loadRunStates(): Promise<RunStates & { contra: Awaited<ReturnType<typeof api.contradictionsReport>> | null }> {
  const [ingests, dailyCheck, reviews, contra, weekprep, find, currentState] = await Promise.all([
    api.ingestRuns().catch(() => [] as IngestRun[]),
    api.dailyCheckStatus().catch(() => ({ running: false }) as DailyCheckStatus),
    api.reviewsStatus().catch(() => ({ runs: [], next: { daily: null, weekly: null, brief: null }, running: [] }) as ReviewsStatus),
    api.contradictionsReport().catch(() => null),
    api.weekprepJob().catch(() => null),
    api.findStatus().catch(() => null),
    api.currentStateStatus().catch(() => null),
  ]);
  return { ingests, dailyCheck, reviews, contradictions: !!contra?.last?.running, weekprep, find, currentState, contra };
}

/** A run that ended while the window was open. */
export interface Finished {
  key: string;
  /** "Daily check finished: 115 pages read", "Ingest of Roadmap failed". */
  text: string;
  short: string;
  at: number;
  failed: boolean;
  place: Place;
  /** Opens what it made, where a place alone can't (an Update…'s chat in Ask). */
  open?: () => void;
}

/** How a run that has gone from the running list ended, from what the states now say. */
export function endOf(r: Running, s: RunStates, at: number): Finished {
  const base = { key: `${r.key}@${at}`, short: r.short, at, place: r.place, failed: false };
  if (r.end) return { ...base, ...r.end() };
  if (r.key.startsWith("ingest:")) {
    const run = s.ingests.find((x) => `ingest:${x.id}` === r.key);
    const what =
      r.short === "Meeting note" ? `Meeting note from ${pageName(run?.source ?? "")}` : `Ingest of ${pageName(run?.source ?? "")}`;
    const st = run?.status ?? "done";
    if (st === "failed") return { ...base, failed: true, text: `${what} failed` };
    if (st === "stopped") return { ...base, text: `${what} stopped` };
    const n = run?.proposals.length ?? 0;
    return { ...base, text: `${what} finished${n ? `: ${n} change${n === 1 ? "" : "s"}` : ""}`, place: n ? { screen: "review" } : r.place };
  }
  if (r.key === "daily-check") {
    const first = s.dailyCheck.summary?.split(";")[0]?.trim();
    return { ...base, text: `Daily check finished${first ? `: ${first}` : ""}` };
  }
  return { ...base, text: `${r.short} finished` };
}

/** How many ended runs the main window keeps. */
export const FINISHED_KEPT = 8;

export interface RunsNow {
  running: Running[];
  finished: Finished[];
}

/** The main window's view of the runs (watchRuns keeps it fresh). */
export const runsNow = new Store<RunsNow>({ running: [], finished: [] });

/** A screenshot scene's sample runs (`runs`): kept as they are, and the list opened for "open". */
export const sceneRuns = new Store<"going" | "open" | null>(null);

/** A scene's sample: two runs going, ingests waiting, and a run that ended. */
export function sampleRuns(): RunsNow {
  const stop = async () => {};
  const ingest = (id: string, name: string, queued: boolean): Running => ({
    key: `ingest:${id}`,
    label: `Ingesting ${name}`,
    short: "Ingest",
    doing: queued ? "Waiting its turn" : "Draft the changes",
    progress: queued ? 0 : 0.5,
    stop,
    queued,
    place: { screen: "sources" },
  });
  return {
    running: [
      {
        key: "daily-check",
        label: "Running the daily check",
        short: "Daily check",
        doing: "Judging: batch 1 of 8",
        progress: 0.62,
        stop,
        place: JOBS,
      },
      ingest("1", "Email. Thread. Re Orbit App launch date", false),
      ingest("2", "Teams. Chat. Launch crew", true),
      ingest("3", "Email. Steerco minutes", true),
    ],
    finished: [
      {
        key: "contradictions@1",
        text: "Contradictions finished",
        short: "Contradictions",
        at: Date.now() - 4 * 60_000,
        failed: false,
        place: { screen: "contradictions" },
      },
    ],
  };
}

/** Runs the window keeps itself rather than Rust (src/updates.ts's Update… runs). */
export const windowRuns = new Store<Running[]>([]);

/** The next state from the last one and fresh states: runs gone from the list become finished. */
export function nextRuns(prev: RunsNow, s: RunStates, at: number, own: Running[] = []): { now: RunsNow; ended: Finished[] } {
  const running = [...runningOf(s), ...own];
  const keys = new Set(running.map((r) => r.key));
  const ended = prev.running.filter((r) => !keys.has(r.key)).map((r) => endOf(r, s, at));
  return { now: { running, finished: [...ended, ...prev.finished].slice(0, FINISHED_KEPT) }, ended };
}

/** Keeps runsNow fresh (the main window, from App): on every run's event, and polled (often while one is going). `onEnd` hears
 *  each run that ends. Returns the stop. */
export function watchRuns(onEnd: (f: Finished) => void): () => void {
  let live = true;
  let timer = 0;
  let busy = false;
  let last: RunStates | null = null;
  const settle = (s: RunStates) => {
    const { now, ended } = nextRuns(runsNow.get(), s, Date.now(), windowRuns.get());
    runsNow.set(now);
    ended.forEach(onEnd);
  };
  // The window's own runs change without Rust saying so: settled at once on the last states.
  const offOwn = windowRuns.subscribe(() => {
    if (last && !sceneRuns.get()) settle(last);
  });
  const refresh = async () => {
    if (busy) return;
    busy = true;
    try {
      const s = await loadRunStates();
      if (!live) return;
      // A scene's sample runs stay as they are.
      if (sceneRuns.get()) return;
      last = s;
      settle(s);
    } finally {
      busy = false;
    }
    if (!live) return;
    window.clearTimeout(timer);
    timer = window.setTimeout(() => void refresh(), runsNow.get().running.length ? 2000 : 20000);
  };
  void refresh();
  const offs = [
    api.onIngestChanged(() => void refresh()),
    api.onDailyCheckChanged(() => void refresh()),
    api.onReviewsChanged(() => void refresh()),
    api.onContradictionsChanged(() => void refresh()),
    api.onWeekprepChanged(() => void refresh()),
    api.onFindChanged(() => void refresh()),
    api.onCurrentStateChanged(() => void refresh()),
  ];
  return () => {
    live = false;
    offOwn();
    window.clearTimeout(timer);
    offs.forEach((o) => void o.then((f) => f()));
  };
}
