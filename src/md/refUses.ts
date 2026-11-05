// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Where a note's or a project's reference keys and links are written, gathered from its properties,
// its text and its tasks (src/md/refs.ts finds them). Each is listed once with where it appears: the
// project's Tickets and links card, a note's References card and the MCP tools read the same lists.
// None of it exists until a Reference links pattern is set: without one the lists are empty.

import type { ProjectRow, TaskRow } from "../api";
import { projectTasks } from "../gtd";
import { taskRefs } from "./inline";
import { bodyRefs, propertyValues } from "./noteRefs";
import { collectRefs, type Ref, type RefPattern } from "./refs";

/** Where some links were written: a label, and for a plural one the noun to count (`task`, `tasks`).
 *  `values` are the links of each value (a property, a task), so a link in two values of one place
 *  counts twice. */
export interface RefSource {
  where: string;
  plural?: string;
  values: readonly (readonly Ref[])[];
}

/** A key or link with the places it is written. `n` counts the values it is in. */
export interface RefUse {
  key: string;
  url: string;
  kind: "ref" | "link" | "url";
  from: { where: string; plural?: string; n: number }[];
}

/** One place as the cards say it: the label, or with a plural noun the count (`2 tasks`). */
export const fromLabel = (f: RefUse["from"][number]) => (f.plural ? `${f.n} ${f.n === 1 ? f.where : f.plural}` : f.where);

/** Each distinct address the sources hold once, in the order first found. */
export function gatherRefs(sources: readonly RefSource[]): RefUse[] {
  const out = new Map<string, RefUse>();
  for (const s of sources)
    for (const v of s.values)
      for (const r of v) {
        const use = out.get(r.url) ?? { key: r.key, url: r.url, kind: r.kind, from: [] };
        out.set(r.url, use);
        const at = use.from.find((f) => f.where === s.where);
        if (at) at.n++;
        else use.from.push({ where: s.where, plural: s.plural, n: 1 });
      }
  return [...out.values()];
}

/** The references in an open note: its properties, its text and its open tasks. Nothing with no
 *  pattern set. */
export function noteRefs(md: string, tasks: readonly TaskRow[], patterns: readonly RefPattern[]): RefUse[] {
  if (!patterns.length) return [];
  return gatherRefs([
    { where: "property", values: propertyValues(md).map((v) => collectRefs(v, { patterns })) },
    { where: "text", values: [bodyRefs(md, patterns)] },
    { where: "task", plural: "tasks", values: tasks.filter((t) => !t.done).map((t) => taskRefs(t.text, patterns)) },
  ]);
}

/** The references of a project: its note's and its documents' properties, and its open tasks by the
 *  list they are on. Every task is in `tasks`; the project's own are picked from it. Nothing with no
 *  pattern set. */
export function projectRefs(p: ProjectRow, tasks: readonly TaskRow[], patterns: readonly RefPattern[]): RefUse[] {
  if (!patterns.length) return [];
  const t = projectTasks([...tasks], p.path);
  const props = (texts: readonly string[] | undefined) => (texts ?? []).map((v) => collectRefs(v, { patterns }));
  const list = (rows: TaskRow[]) => rows.map((x) => taskRefs(x.text, patterns));
  return gatherRefs([
    { where: "Project note", values: props(p.propertyText) },
    ...(p.documents ?? []).map((d) => ({ where: d.title, values: props(d.propertyText) })),
    { where: "Next action", values: list(t.next) },
    { where: "Waiting for", values: list(t.waiting) },
    { where: "Someday / maybe", values: list(t.someday) },
  ]);
}
