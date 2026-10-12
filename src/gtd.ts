// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// GTD on top of the task lines (stage 6): projects, contexts, effort and the Inbox. A project is a
// `Project. <name>.md` note with `status`, `area` and `outcome` properties; a context is a
// `#context/<name>` tag, shown as @name; effort is a Dataview field, `[effort:: 15m]`. Rust reads
// them into the index (TaskRow.project/contexts/effort) and makes every change
// (src-tauri/src/gtd.rs); this side has the formats, shorthand, flags and the screens' data.

import { useEffect, useState } from "react";
import { api, InboxItem, ProjectRow, TaskRow } from "./api";
import { useVaultVersion } from "./state";
import { settings, Store, useStore } from "./store";

export const PROJECT_PREFIX = "Project. ";
export type ProjectStatus = "active" | "on-hold" | "someday" | "done";
export const STATUS_LABEL: Record<ProjectStatus, string> = {
  active: "Active",
  "on-hold": "On hold",
  someday: "Someday",
  done: "Completed",
};

/** A project's file stem, for a `[[link]]` to it. */
export const projectStem = (name: string) => `${PROJECT_PREFIX}${name}`;
/** The name a project note's path gives it. */
export const projectName = (path: string) =>
  (path.split("/").pop() ?? path).replace(/\.md$/i, "").replace(new RegExp(`^${PROJECT_PREFIX.replace(".", "\\.")}`), "");

/** Minutes in an effort as written: 15m, 1h, 1h30m, 90m, 2d (a day is 8 hours). */
export function effortMinutes(s: string): number | null {
  const t = s.trim().toLowerCase().replace(/\s+/g, "");
  if (/^\d+$/.test(t)) return +t;
  const m = /^(?:(\d+(?:\.\d+)?)d)?(?:(\d+(?:\.\d+)?)h)?(?:(\d+)m(?:in)?)?$/.exec(t);
  if (!m || (!m[1] && !m[2] && !m[3])) return null;
  return Math.round((+(m[1] ?? 0) * 8 + +(m[2] ?? 0)) * 60 + +(m[3] ?? 0));
}

/** "15 min", "1 h 30", "2 d". */
export function fmtEffort(min: number): string {
  if (min < 60) return `${min} min`;
  if (min % 480 === 0) return `${min / 480} d`;
  const h = Math.floor(min / 60);
  const m = min % 60;
  return m ? `${h} h ${m}` : `${h} h`;
}

/** A context's name from what's typed: "@Calls" or "calls" → "calls". */
export const contextName = (s: string) =>
  s
    .trim()
    .replace(/^@|^#?context\//i, "")
    .replace(/\s+/g, "-")
    .toLowerCase();

const CONTEXT_WORD = /(^|[\s(])@([\p{L}\p{N}_/-]*[\p{L}_-][\p{L}\p{N}_/-]*)/gu;
const EFFORT_WORD = /(^|[\s(])effort:(\S+)/gi;

/** Capture's GTD shorthand: `@calls` becomes `#context/calls`, `effort:15m` becomes `[effort:: 15m]`.
 *  An email address (`a@b`) isn't a context; an effort that isn't one is left as typed. */
export function applyGtdShorthand(text: string): string {
  return text
    .replace(CONTEXT_WORD, (_m, pre: string, name: string) => `${pre}#context/${name.toLowerCase()}`)
    .replace(EFFORT_WORD, (m, pre: string, e: string) => (effortMinutes(e) === null ? m : `${pre}[effort:: ${e.toLowerCase()}]`));
}

/** The link a task carries to its project. */
export const projectLink = (name: string) => `[[${projectStem(name)}]]`;

/** Why a project needs a look: no next action, or nothing has happened for two weeks. */
export function projectFlag(p: ProjectRow, now = Date.now()): "none" | "quiet" | null {
  if (p.status !== "active") return null;
  if (p.next === 0) return "none";
  if (now - p.lastTouched > 14 * 86400000) return "quiet";
  return null;
}

/** The open tasks of a project, split the way the Projects screen shows them. */
export function projectTasks(all: TaskRow[], path: string) {
  const mine = all.filter((t) => t.project === path);
  const has = (t: TaskRow, tag: string) => t.tags.some((g) => g === tag || g.startsWith(`${tag}/`));
  const open = mine.filter((t) => !t.done);
  return {
    // In their dragged order (`^rank-N`, as the To Do list's), unranked ones after in file order.
    next: open
      .filter((t) => !has(t, "waiting-for") && !has(t, "someday-maybe"))
      .map((t, i) => [t, i] as const)
      .sort(([a, i], [b, j]) => (a.rank ?? Infinity) - (b.rank ?? Infinity) || i - j)
      .map(([t]) => t),
    waiting: open.filter((t) => has(t, "waiting-for")),
    someday: open.filter((t) => has(t, "someday-maybe")),
    done: mine.filter((t) => t.done),
  };
}

/** Every context used on an open task, most used first. */
export function contextsInUse(all: TaskRow[]): string[] {
  const n = new Map<string, number>();
  for (const t of all) if (!t.done) for (const c of t.contexts ?? []) n.set(c, (n.get(c) ?? 0) + 1);
  return [...n].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([c]) => c);
}

function useLoaded<T>(load: () => Promise<T>, also = 0): T | null {
  const [v, setV] = useState<T | null>(null);
  const ver = useVaultVersion();
  useEffect(() => {
    let live = true;
    load()
      .then((r) => live && setV(r))
      .catch(() => live && setV(null));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ver, also]);
  return v;
}

/** Every project, reloaded when the vault changes. */
export const useProjects = () => useLoaded<ProjectRow[]>(() => api.projectsList());

/** The projects under an area, whatever their status: what renaming the area changes, on the
 *  Projects screen and in rename_area. Areas match as the screen groups them, exactly. */
export const areaProjects = (projects: ProjectRow[], area: string) => projects.filter((p) => (p.area ?? "") === area && area !== "");

// ---- the Projects screen's order (D-20261012-01): kept in settings, never in the vault.

/** `items` in the saved order (by `key`), those not in it after, A–Z by `label`. */
export function inSavedOrder<T>(items: T[], key: (t: T) => string, label: (t: T) => string, saved: string[] = []): T[] {
  const at = new Map(saved.map((k, i) => [k, i]));
  return [...items].sort((a, b) => {
    const x = at.get(key(a));
    const y = at.get(key(b));
    if (x !== undefined && y !== undefined) return x - y;
    if (x !== undefined) return -1;
    if (y !== undefined) return 1;
    return label(a).localeCompare(label(b));
  });
}

/** The areas in the screen's order: as dragged, then A–Z, no area ("") last. */
export function orderedAreas(areas: string[], saved = settings.get().projectOrder?.areas): string[] {
  const named = [...new Set(areas.filter(Boolean))];
  const out = inSavedOrder(
    named,
    (a) => a,
    (a) => a,
    saved,
  );
  return areas.includes("") ? [...out, ""] : out;
}

/** Projects in the screen's order: by area as `orderedAreas`, then as dragged within it, then A–Z. */
export function orderedProjects(ps: ProjectRow[], order = settings.get().projectOrder): ProjectRow[] {
  const areas = orderedAreas(
    ps.map((p) => p.area ?? ""),
    order?.areas,
  );
  const inArea = (a: string) =>
    inSavedOrder(
      ps.filter((p) => (p.area ?? "") === a),
      (p) => p.path,
      (p) => p.name,
      order?.projects,
    );
  return areas.flatMap(inArea);
}

/** `full` with `item` moved to just before `before`, or just after `after` when there's no `before`
 *  (the end of what's shown, which may not be the end of `full`). */
export function movedTo(full: string[], item: string, after: string | null, before: string | null): string[] {
  const rest = full.filter((k) => k !== item);
  const i = before !== null ? rest.indexOf(before) : after !== null ? rest.indexOf(after) + 1 : -1;
  return i < 0 ? [...rest, item] : [...rest.slice(0, i), item, ...rest.slice(i)];
}

/** Saves `item`'s new place among the areas (`kind` "areas") or projects (paths): between `after`
 *  and `before`, as they're shown. Every project and area is in `all`, so their order holds across
 *  the status tabs. */
export function moveInProjectOrder(
  all: ProjectRow[],
  kind: "areas" | "projects",
  item: string,
  after: string | null,
  before: string | null,
) {
  const order = settings.get().projectOrder ?? {};
  const full =
    kind === "areas"
      ? orderedAreas(
          all.map((p) => p.area ?? ""),
          order.areas,
        ).filter(Boolean)
      : orderedProjects(all, order).map((p) => p.path);
  settings.update({ projectOrder: { ...order, [kind]: movedTo(full, item, after, before) } });
}

/** A renamed area or project note keeps its place; renamed onto one that's there, the two merge
 *  where that one was. An area's old name keeps its place too, just after the new one, so undoing
 *  the rename (⌘Z, the toast's Undo or Revert in Changes, none of which tells the screens) finds
 *  it where it was; a project note's undo is a rename back, which `fileMoved` follows. `areas` are
 *  the areas there before the rename: a new name that's only left over from an earlier rename is
 *  no area to merge with. Names no area has any more go at the next drag (`moveInProjectOrder`). */
export function renameInProjectOrder(kind: "areas" | "projects", from: string, to: string, areas: string[] = []) {
  const order = settings.get().projectOrder;
  const list = order?.[kind];
  if (!list?.includes(from) || from === to) return;
  if (kind === "areas" && areas.includes(to)) return;
  const next =
    kind === "areas"
      ? list.filter((k) => k !== to).flatMap((k) => (k === from ? [to, from] : [k]))
      : list.includes(to)
        ? list.filter((k) => k !== from)
        : list.map((k) => (k === from ? to : k));
  settings.update({ projectOrder: { ...order, [kind]: next } });
}

/** The Inbox: Scratchpad blocks and the To Do list's `#### Other` tasks. */
/** Counts captures marked done: that's kept in Brainstead's app data, not the vault, so no vault
 *  change reloads the Inbox for it. */
const capturesDone = new Store(0);

/** The Inbox, reloaded when the vault changes or a capture is marked done. */
export const useInbox = () => useLoaded<InboxItem[]>(() => api.inboxList(), useStore(capturesDone));

/** Takes a capture out of the Inbox, its file kept in Sources; every Inbox shown reloads. */
export const captureDone = (path: string) => api.inboxCaptureDone(path).then(() => capturesDone.set(capturesDone.get() + 1));

/** An Inbox item still to clarify. A To Do `#### Other` task is, while it's open and has no
 *  project, context or GTD tag; Next actions leaves those out too (`inboxTaskKeys`). */
export function unclarified(i: InboxItem): boolean {
  if (i.kind === "thought" || i.kind === "capture") return true;
  const l = i.lineText;
  if (/^\s*[-*+]\s+\[[^ ]\]/.test(l)) return false;
  return !/#context\/|\[\[Project\. |#(waiting-for|someday-maybe|followup)\b/.test(l);
}

/** The To Do `#### Other` tasks still to clarify, as `path:line` (as `taskKey` in TaskList). */
export const inboxTaskKeys = (items: InboxItem[] | null | undefined): Set<string> =>
  new Set((items ?? []).filter((i) => i.kind === "task" && unclarified(i)).map((i) => `${i.path}:${i.line}`));
