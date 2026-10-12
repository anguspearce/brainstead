// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// This session's Doc checks, kept current from their events from the start (the sidebar shows one
// going, and one that ended while you were elsewhere): a check ending says so in a toast, with Open.

import { api, CheckRun } from "./api";
import { nav, place, Place } from "./nav";
import { Store } from "./store";
import { toast } from "./Toast";

export const docChecks = new Store<CheckRun[]>([]);
/** Checks that ended while Doc check wasn't showing, and whether one failed: the sidebar's badge. */
export const docChecksUnseen = new Store<{ n: number; bad: boolean }>({ n: 0, bad: false });

/**
 * A run's copy merged into the list, in its place (a new one first). Copies can arrive out of order
 * (doc_check_start's answer after the run's first events), so one never moves a run back: a running
 * copy doesn't replace a run that has ended (done, failed or stopped), nor one with more steps.
 */
export function mergeRun(list: CheckRun[], x: CheckRun): CheckRun[] {
  const i = list.findIndex((y) => y.id === x.id);
  if (i < 0) return [x, ...list];
  const had = list[i];
  if (x.status === "running" && (had.status !== "running" || had.steps.length > x.steps.length)) return list;
  return list.map((y, j) => (j === i ? x : y));
}

/** Where a run's Open goes: its screen with both documents, the mode and the governing document chosen. */
export function runPlace(x: CheckRun): Place {
  if (x.mode === "differences" || x.mode === "changes")
    return { screen: "compare", view: { "compare.a": x.candidate, "compare.b": x.doc, "compare.mode": x.mode } };
  // Feedback mode keys without the governing document: the one chosen there stays.
  return {
    screen: "doccheck",
    view: { "doccheck.candidate": x.candidate, "doccheck.mode": x.mode, ...(x.doc ? { "doccheck.doc": x.doc } : {}) },
  };
}

/** Opens a run's screen with its choices; on that screen already, it changes them there. */
const openRun = (x: CheckRun) => {
  const to = runPlace(x);
  const here = place.get().place;
  if (here.screen === to.screen) nav.replace({ ...to, view: { ...here.view, ...to.view } });
  else nav.go(to);
};

let started = false;
export function startDocChecks() {
  if (started) return;
  started = true;
  void api
    .docCheckRuns()
    .then((l) => docChecks.set(l.reduceRight(mergeRun, docChecks.get())))
    .catch(() => {});
  void api
    .onDocCheckChanged((x) => {
      const list = docChecks.get();
      const next = mergeRun(list, x);
      if (next === list) return;
      docChecks.set(next);
      if (x.status !== "done" && x.status !== "failed") return;
      const showing = place.get().place.screen;
      if (showing !== "doccheck" && showing !== "compare") {
        const u = docChecksUnseen.get();
        docChecksUnseen.set({ n: u.n + 1, bad: u.bad || x.status === "failed" });
      }
      const cmp = x.mode === "differences" || x.mode === "changes";
      const open = { label: "Open", run: () => openRun(x) };
      const what = cmp ? "Compare" : "Doc check";
      if (x.status === "done") toast(`${what} done: ${x.result?.verdict ?? ""}`, open, "ok");
      else toast(`${what} failed: ${x.error ?? ""}`, open, "bad");
    })
    .catch(() => {});
}

/** Doc check or Compare is showing: what ended there is seen. */
export const seenDocChecks = () => docChecksUnseen.set({ n: 0, bad: false });
