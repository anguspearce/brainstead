// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Pane widths: every pane and table column a screen splits itself into (the sidebar, a screen's
// left list, its right-hand panel, every table's columns) can be dragged wider or narrower by the
// divider beside it (src/PaneResizer.tsx). Each has a key here with its range and the CSS custom
// property its screen's grid reads (src/styles.css); a width the user set is kept per machine in app
// settings (`paneWidths`, by key), applied before the first paint, and clamped to its range and to
// the room the screen has, so a pane can never cover another: the flexible pane keeps MIN_FLEX at
// least (more on a screen whose flexible column holds more, usePanes' minFlex), and gives way before
// that; a width an assistant sets (the settings tool) is kept within the range only, and the screen
// fits it to its room as it draws. Nothing stored means the stylesheet's default, which narrows at
// its own breakpoints (container queries on the main area, so a wide sidebar counts) and, where it
// still doesn't fit, gives way before the stored ones (fitPanes). Settings › General › Appearance ›
// Reset pane widths clears them all.
// Each pane is of a type (a screen's panes, or a table's columns) that Settings can lock
// (`resizeLocks`, by type): a locked type has no dividers and its widths can't be set, but the
// widths kept still apply and Reset pane widths still clears them.

import { Settings } from "./api";
import { settings } from "./store";

/** What a divider resizes, each with its own lock: a screen's panes (the sidebar too) or a table's
 *  columns. A further type is a further key of `resizeLocks`; a lock not there is off. */
export type ResizeType = keyof NonNullable<Settings["resizeLocks"]>;

/** Each type's lock as Settings › General › Appearance › Pane widths names it. */
export const RESIZE_LOCKS: Record<ResizeType, { label: string; what: string }> = {
  panes: { label: "Lock pane widths", what: "Pane widths" },
  columns: { label: "Lock column widths", what: "Column widths" },
};

/** Which side of its divider the pane is on: a "left" pane grows as the divider moves right. A
 *  table's column is "left" before the table's flexible column (its divider at its right edge) and
 *  "right" after it (its divider at its left edge), so the divider always follows the pointer. */
export type PaneSide = "left" | "right";

export interface PaneSpec {
  /** The custom property the screen's grid-template-columns reads. */
  cssVar: string;
  /** The stylesheet's width on a wide window; it narrows at the stylesheet's breakpoints. */
  def: number;
  min: number;
  max: number;
  side: PaneSide;
  /** Which lock stops it being resized. */
  type: ResizeType;
  /** What the pane holds, for the divider's label ("Resize the task lists"). */
  label: string;
  /** A table column's heading, as its table draws it ("Was in"); the table takes it from here. */
  heading?: string;
}

/** Arrow keys move a divider this far; with Shift, PANE_STEP_BIG. */
export const PANE_STEP = 8;
export const PANE_STEP_BIG = 40;
/** The least the flexible pane keeps when the fixed ones around it are wide, where a screen asks
 *  for no more (usePanes' minFlex: Settings' cards, a project's detail and the like hold more). */
export const MIN_FLEX = 240;

const left = (cssVar: string, label: string, def: number, min: number, max: number, type: ResizeType = "panes"): PaneSpec => ({
  cssVar,
  label,
  def,
  min,
  max,
  side: "left",
  type,
});
const right = (cssVar: string, label: string, def: number, min: number, max: number): PaneSpec => ({
  cssVar,
  label,
  def,
  min,
  max,
  side: "right",
  type: "panes",
});
/** A table's column headed `heading`, on `side` of the table's flexible column's divider. */
const col = (cssVar: string, heading: string, def: number, min: number, max: number, side: PaneSide): PaneSpec => ({
  ...left(cssVar, `the ${heading} column`, def, min, max, "columns"),
  side,
  heading,
});

/** The range every list table's column has. */
const COLUMN = { min: 48, max: 480 };
/** A list table's column (src/FileList.tsx): after the table's Title, as every one is, so side
 *  "right". Its heading and width in the table's column list are listColumn(key), from here. */
const listCol = (key: string, heading: string, def: number): PaneSpec =>
  col(`--col-${key}`, heading, def, COLUMN.min, Math.max(COLUMN.max, def), "right");

/** The screens' panes, by key. A screen's left list is --pane-left or --pane-list and its right-hand
 *  panel --pane-side, set on the screen's root (the .body element) so the stylesheet's breakpoints
 *  stay the default. The tables' columns (the Trash, Activity's log, Doc check's register) are on
 *  their tables, `<table>.<column>`, and so are the list tables' (Notes, Wiki, Sources, Templates,
 *  by the column's key in its table); each table has one flexible column, with no key, that takes
 *  the rest and gives way as the others are dragged. All are here, so the settings tool knows every
 *  one before its screen is shown. */
export const PANES = {
  sidebar: left("--pane-sidebar", "the sidebar", 232, 180, 400),
  "tasks.left": left("--pane-left", "the task lists", 260, 180, 480),
  "tasks.side": right("--pane-side", "the task's pane", 340, 260, 640),
  "inbox.left": left("--pane-left", "the Inbox list", 260, 200, 520),
  "wiki.left": left("--pane-left", "the wiki's tags", 260, 180, 480),
  "search.left": left("--pane-left", "the saved searches", 260, 180, 480),
  "settings.left": left("--pane-left", "the Settings panes", 260, 180, 400),
  "weekly.left": left("--pane-left", "the review's steps", 260, 180, 480),
  "weekly.side": right("--pane-side", "the review's side column", 340, 260, 640),
  "meeting.left": left("--pane-left", "the meeting list", 260, 180, 480),
  "meeting.side": right("--pane-side", "the meeting's pane", 360, 260, 640),
  "projects.list": left("--pane-list", "the project list", 400, 260, 720),
  "changes.list": left("--pane-list", "the list of changes", 400, 260, 720),
  "today.side": right("--pane-side", "the right rail", 320, 240, 560),
  "health.side": right("--pane-side", "the side column", 380, 260, 640),
  "activity.side": right("--pane-side", "the side column", 380, 260, 640),
  "sources.side": right("--pane-side", "the ingest runs", 360, 260, 640),
  "graph.side": right("--pane-side", "the graph's pane", 300, 220, 560),
  "note.side": right("--pane-side", "the note's Linked from pane", 280, 200, 480),
  "project.side": right("--pane-side", "the project's side column", 280, 200, 480),
  "contradictions.left": left("--pane-left", "the list of contradictions", 340, 240, 640),
  "compare.left": left("--pane-left", "the verdict", 300, 220, 560),
  "doccheck.left": left("--pane-left", "the verdict", 300, 220, 560),
  "trash.type": col("--col-type", "Type", 90, 60, 200, "right"),
  "trash.folder": col("--col-folder", "Was in", 220, 80, 480, "right"),
  "trash.deleted": col("--col-deleted", "Deleted", 110, 70, 240, "right"),
  "trash.size": col("--col-size", "Size", 72, 50, 160, "right"),
  "activity.when": col("--col-when", "When", 150, 80, 320, "left"),
  "activity.time": col("--col-time", "Time", 64, 48, 160, "left"),
  "activity.action": col("--col-action", "Action", 120, 72, 320, "right"),
  "register.version": col("--col-version", "Version", 100, 60, 240, "right"),
  "register.status": col("--col-status", "Status", 110, 72, 240, "right"),
  "register.file": col("--col-file", "File", 200, 80, 480, "right"),
  "notes.type": listCol("type", "Type", 110),
  "notes.date": listCol("date", "Date", 96),
  "notes.mtime": listCol("mtime", "Modified", 110),
  "notes.size": listCol("size", "Size", 76),
  "wiki.type": listCol("type", "Type", 110),
  "wiki.sources": listCol("sources", "Sources", 72),
  "wiki.health": listCol("health", "Health", 96),
  "wiki.tags": listCol("tags", "Tags", 200),
  "wiki.mtime": listCol("mtime", "Modified", 110),
  "sources.kind": listCol("kind", "Kind", 64),
  "sources.ingested": listCol("ingested", "Status", 88),
  "sources.path": listCol("path", "Folder", 90),
  "sources.mtime": listCol("mtime", "Modified", 110),
  "sources.size": listCol("size", "Size", 76),
  "templates.mtime": listCol("mtime", "Modified", 110),
  "templates.size": listCol("size", "Size", 76),
} satisfies Record<string, PaneSpec>;

export type PaneKey = keyof typeof PANES;

/** The tables src/FileList.tsx draws, whose columns are `<table>.<column>` in PANES. */
export type ListTable = "notes" | "wiki" | "sources" | "templates";

/** A list table's resizable column's heading and flex basis ("0 0 110px", from its default here):
 *  the table's column list takes them from this, so the two can't differ. */
export const listColumn = (key: PaneKey) => ({ label: PANES[key].heading ?? "", width: `0 0 ${PANES[key].def}px` });

/** Every pane and table column, by key. */
export function paneSpecs(): Record<string, PaneSpec> {
  return PANES;
}

/** The screen or table a key's first part is, as the sidebar and the screens' titles name it. */
const PANE_WHERE: Record<string, string> = {
  tasks: "Tasks",
  inbox: "Inbox",
  wiki: "Wiki",
  search: "Search",
  settings: "Settings",
  weekly: "Weekly review",
  meeting: "Meeting notes",
  projects: "Projects",
  changes: "Changes",
  today: "Today",
  health: "Knowledge health",
  activity: "Activity",
  sources: "Sources",
  graph: "Graph",
  note: "A note",
  project: "A project",
  contradictions: "Contradictions",
  compare: "Compare",
  doccheck: "Doc check",
  trash: "Trash",
  register: "Doc check's register",
  notes: "Notes",
  templates: "Templates",
};

/** A pane in the screens' own words, for the settings tool: "Trash › the Was in column". */
export function paneName(key: string): string {
  const where = PANE_WHERE[key.split(".")[0]];
  const label = paneSpecs()[key]?.label ?? key;
  return where && key.includes(".") ? `${where} › ${label}` : label;
}

/** A stored pane width, if any and a finite number. */
export function storedPaneWidth(widths: Record<string, number> | undefined, key: string): number | undefined {
  const v = widths?.[key];
  return typeof v === "number" && Number.isFinite(v) ? v : undefined;
}

/** `px` within the pane's range and within `room`: what the screen has left for it once the other
 *  fixed panes, the gaps and the flexible pane's least width (MIN_FLEX) are taken. The range's
 *  minimum wins over the room, so a tiny window never collapses a pane to nothing. Nonsense (a
 *  hand-edited settings.json) gives the default. */
export function clampPaneWidth(spec: PaneSpec, px: number | undefined, room = Infinity): number {
  const n = Math.round(Number(px));
  const want = Number.isFinite(n) ? n : spec.def;
  const max = Math.max(spec.min, Math.min(spec.max, room));
  return Math.max(spec.min, Math.min(max, want));
}

/** The room a screen's fixed panes share on a root `width` wide (the grid's content box), once
 *  `fixed` (gaps, and columns that never move) and the flexible pane's least width (`minFlex`, the
 *  screen's reserve, MIN_FLEX where it names none) are taken. */
export function paneRoom(width: number, fixed = 0, minFlex = MIN_FLEX): number {
  return width - fixed - minFlex;
}

/** What each of a screen's panes shows when `room` px are left for them all (the root's width less
 *  its gaps, the columns that never move and the flexible pane's least width): its `want` (the stored
 *  width, else the stylesheet's) within its range. Where they don't all fit, the panes nobody set
 *  give way first, then the set ones, each in proportion to how far it is above its minimum, and
 *  never below it: a tiny window keeps every pane at its least and the flexible one gets what's left. */
export function fitPanes(panes: { spec: PaneSpec; want: number; stored: boolean }[], room: number): number[] {
  const shown = panes.map((p) => clampPaneWidth(p.spec, p.want));
  let over = shown.reduce((a, b) => a + b, 0) - room;
  for (const stored of [false, true]) {
    if (over <= 0) break;
    const give = panes.flatMap((p, i) => (p.stored === stored ? [i] : []));
    const slack = give.reduce((n, i) => n + shown[i] - panes[i].spec.min, 0);
    if (slack <= 0) continue;
    const take = Math.min(over, slack);
    for (const i of give) shown[i] = Math.max(panes[i].spec.min, Math.floor(shown[i] - (take * (shown[i] - panes[i].spec.min)) / slack));
    over -= take;
  }
  return shown;
}

/** Whether a type of pane is locked against resizing (Settings › General › Appearance). */
export function resizeLocked(type: ResizeType, locks = settings.get().resizeLocks): boolean {
  return locks?.[type] === true;
}

/** Turns a type's lock on or off, as its Settings checkbox does: a lock turned off is left out, so
 *  no locks at all leaves no key in settings.json. */
export function setResizeLock(type: ResizeType, on: boolean) {
  const next = { ...settings.get().resizeLocks, [type]: on };
  if (!on) delete next[type];
  settings.update({ resizeLocks: Object.keys(next).length ? next : undefined });
}

/** What a change to a locked pane's width is told: what's locked and which setting unlocks it. */
export function lockedMessage(key: string, type: ResizeType): string {
  const { label, what } = RESIZE_LOCKS[type];
  return `${what} are locked, so ${key} can't be resized: turn off ${label} (resizeLocks.${type}) in Settings › General › Appearance › Pane widths first.`;
}

function unlocked(key: string, spec: PaneSpec) {
  if (resizeLocked(spec.type)) throw new Error(lockedMessage(key, spec.type));
}

/** Keeps a pane's width (clamped), as a drag's release or an arrow key does, and returns it.
 *  Refused while the pane's type is locked. */
export function setPaneWidth(key: string, px: number, room = Infinity): number {
  const spec = paneSpecs()[key];
  if (!spec) throw new Error(`No pane “${key}”.`);
  unlocked(key, spec);
  const v = clampPaneWidth(spec, px, room);
  settings.update({ paneWidths: { ...settings.get().paneWidths, [key]: v } });
  return v;
}

/** Puts one pane back to the stylesheet's width (a double-click or Enter on its divider); refused
 *  while its type is locked. */
export function resetPaneWidth(key: string) {
  const spec = paneSpecs()[key];
  if (spec) unlocked(key, spec);
  const w = { ...settings.get().paneWidths };
  delete w[key];
  settings.update({ paneWidths: Object.keys(w).length ? w : undefined });
}

/** Settings › General › Appearance › Reset pane widths: every pane back to the stylesheet's, locked
 *  or not (a lock keeps widths from being dragged, not from being put back). */
export function resetPaneWidths() {
  settings.update({ paneWidths: undefined });
}
