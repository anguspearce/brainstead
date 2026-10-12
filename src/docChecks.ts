// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// This session's Doc checks, kept current from their events from the start (the sidebar shows one
// going, and one that ended while you were elsewhere): a check ending says so in a toast, with Open.

import { api, CheckRun } from "./api";
import { nav, place } from "./nav";
import { Store } from "./store";
import { toast } from "./Toast";

export const docChecks = new Store<CheckRun[]>([]);
/** Checks that ended while Doc check wasn't showing, and whether one failed: the sidebar's badge. */
export const docChecksUnseen = new Store<{ n: number; bad: boolean }>({ n: 0, bad: false });

let started = false;
export function startDocChecks() {
  if (started) return;
  started = true;
  void api
    .docCheckRuns()
    .then((l) => docChecks.set(l))
    .catch(() => {});
  void api
    .onDocCheckChanged((x) => {
      docChecks.set([x, ...docChecks.get().filter((y) => y.id !== x.id)]);
      if (x.status !== "done" && x.status !== "failed") return;
      const showing = place.get().place.screen;
      if (showing !== "doccheck" && showing !== "compare") {
        const u = docChecksUnseen.get();
        docChecksUnseen.set({ n: u.n + 1, bad: u.bad || x.status === "failed" });
      }
      const cmp = x.mode === "differences" || x.mode === "changes";
      const open = {
        label: "Open",
        run: () => nav.go({ screen: cmp ? "compare" : "doccheck", path: x.candidate.startsWith("/") ? undefined : x.candidate }),
      };
      const what = cmp ? "Compare" : "Doc check";
      if (x.status === "done") toast(`${what} done: ${x.result?.verdict ?? ""}`, open, "ok");
      else toast(`${what} failed: ${x.error ?? ""}`, open, "bad");
    })
    .catch(() => {});
}

/** Doc check or Compare is showing: what ended there is seen. */
export const seenDocChecks = () => docChecksUnseen.set({ n: 0, bad: false });
