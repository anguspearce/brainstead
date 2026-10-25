// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Where the user is, and back/forward through where they've been (⌘[ and ⌘]), as in a sibling
// app's place history. The history is kept in settings, so a restart opens where the user was.

import { askEdit } from "./notes/dialogs";
import { settings, Store, useStore } from "./store";

export type Screen =
  | "today"
  | "inbox"
  | "tasks"
  | "projects"
  | "review"
  | "ask"
  | "notes"
  | "wiki"
  | "sources"
  | "templates"
  | "graph"
  | "health"
  | "activity"
  | "trash"
  | "settings"
  | "doc"
  | "search"
  | "weekly"
  | "contradictions"
  | "meeting"
  | "triage"
  | "reply"
  | "doccheck"
  | "compare";

export type SettingsPane = "general" | "notes" | "vault" | "assistants" | "jobs" | "capture" | "sync" | "permissions" | "about";

export interface Place {
  screen: Screen;
  pane?: SettingsPane;
  /** The document open in "doc". */
  path?: string;
  /** A search: the query on "search", the words to highlight on "doc". */
  q?: string;
  /** A heading or block to scroll to in "doc". */
  anchor?: string;
  /** Lines of the document to highlight in "doc", scrolled to the first, as their text (trimmed). */
  lines?: string[];
  /** The screen's own choices here (search text, sort, tab, filter, selection): useViewState. */
  view?: Record<string, unknown>;
}

export const samePlace = (a: Place, b: Place) =>
  a.screen === b.screen &&
  (a.pane ?? null) === (b.pane ?? null) &&
  (a.path ?? null) === (b.path ?? null) &&
  (a.q ?? null) === (b.q ?? null) &&
  (a.anchor ?? null) === (b.anchor ?? null) &&
  (a.lines ?? []).join("\n") === (b.lines ?? []).join("\n");

/** Opens a vault file in Edit (a project's Edit button); one that can't be edited opens in View. */
export const editDoc = (path: string) => {
  askEdit(path);
  openDoc(path);
};

/** Opens a vault file. */
export const openDoc = (path: string, extra: { q?: string; anchor?: string; lines?: string[] } = {}) =>
  nav.go({ screen: "doc", path, ...extra });

/** Back/forward history. Going somewhere new drops anything ahead; going where you are does nothing. */
export class History {
  stack: Place[];
  i: number;
  constructor(
    start: Place,
    private max = 100,
  ) {
    this.stack = [start];
    this.i = 0;
  }
  get current(): Place {
    return this.stack[this.i];
  }
  get canBack() {
    return this.i > 0;
  }
  get canForward() {
    return this.i < this.stack.length - 1;
  }
  go(p: Place): boolean {
    if (samePlace(p, this.current)) return false;
    this.stack = [...this.stack.slice(0, this.i + 1), p].slice(-this.max);
    this.i = this.stack.length - 1;
    return true;
  }
  /** In place of the current entry, for a step that corrects where the user already is. */
  replace(p: Place) {
    this.stack = [...this.stack.slice(0, this.i), p, ...this.stack.slice(this.i + 1)];
  }
  move(d: -1 | 1): boolean {
    const j = this.i + d;
    if (j < 0 || j >= this.stack.length) return false;
    this.i = j;
    return true;
  }
}

const hist = new History({ screen: "today" });
/** The current place; changes on every move. */
export const place = new Store<{ place: Place; canBack: boolean; canForward: boolean }>({
  place: hist.current,
  canBack: false,
  canForward: false,
});

/** The places kept across restarts. */
const KEPT = 50;
const publish = () => {
  place.set({ place: hist.current, canBack: hist.canBack, canForward: hist.canForward });
  const from = Math.max(0, hist.stack.length - KEPT);
  settings.update({ places: { stack: hist.stack.slice(from), i: hist.i - from } });
};

/** Back where the last run left off (main.tsx, at start). A screen scene sets its own place after. */
export function restorePlaces(saved: { stack?: Place[]; i?: number } | undefined) {
  const stack = (saved?.stack ?? []).filter((p) => p && typeof p.screen === "string");
  if (!stack.length) return;
  hist.stack = stack;
  hist.i = Math.min(Math.max(0, saved?.i ?? stack.length - 1), stack.length - 1);
  place.set({ place: hist.current, canBack: hist.canBack, canForward: hist.canForward });
  return hist.current;
}

/** Asked before leaving the current place (an editor with unsaved changes): return true to hold
 *  the move, and call `proceed` later to make it. */
type LeaveGuard = (proceed: () => void) => boolean;
let guard: LeaveGuard | null = null;
export function setLeaveGuard(g: LeaveGuard | null) {
  guard = g;
}
const guarded = (move: () => void) => {
  if (guard && guard(move)) return;
  move();
};

/** Runs `f` once nothing holds the current place: at once, or after the unsaved-changes question. */
export const whenFree = (f: () => void) => guarded(f);

export const nav = {
  go: (p: Place | Screen) => {
    const to = typeof p === "string" ? { screen: p } : p;
    if (samePlace(to, hist.current)) return;
    // Another heading in the same document isn't leaving it.
    const same = to.screen === "doc" && hist.current.screen === "doc" && to.path === hist.current.path;
    const go = () => {
      if (hist.go(to)) publish();
    };
    if (same) go();
    else guarded(go);
  },
  replace: (p: Place) => {
    // The screen's own choices stay unless the new place brings its own.
    hist.replace(p.view || p.screen !== hist.current.screen ? p : { ...p, view: hist.current.view });
    publish();
  },
  back: () => {
    if (hist.canBack)
      guarded(() => {
        if (hist.move(-1)) publish();
      });
  },
  forward: () => {
    if (hist.canForward)
      guarded(() => {
        if (hist.move(1)) publish();
      });
  },
};

/**
 * A screen's choice (what's typed in its search box, its sort, tab, filter or selection), kept on
 * the history entry, so back and forward return to it and a restart keeps it. A fresh visit to the
 * screen starts from the last choice made there (`viewMemory`), else `initial`; with `remember`
 * false it starts from `initial` (or what the place brings), and the choice lives only on the entry.
 */
export function useViewState<T>(key: string, initial: T, remember = true): [T, (v: T | ((prev: T) => T)) => void] {
  useStore(place);
  useStore(settings);
  const read = (): T => {
    const view = hist.current.view;
    const memory = settings.get().viewMemory;
    return (view && key in view ? view[key] : remember && memory && key in memory ? memory[key] : initial) as T;
  };
  const set = (x: T | ((prev: T) => T)) => {
    const next = typeof x === "function" ? (x as (prev: T) => T)(read()) : x;
    const cur = hist.current;
    hist.replace({ ...cur, view: { ...cur.view, [key]: next } });
    if (remember) settings.update({ viewMemory: { ...settings.get().viewMemory, [key]: next } });
    publish();
  };
  return [read(), set];
}
