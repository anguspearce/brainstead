// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A project's member notes: those whose `Project: [[Project. <name>]]` line names it (the index
// works out which, core/src/projects.rs). The project page lists them by type, and the Notes
// screen can hide the working ones or group every note by its project. A tracker's Current state
// comes from the index (core pageshape), so the rule for finding it lives in one place.

import type { FileSummary, ProjectDoc } from "./api";

/** The types the project page's Documents card groups by, in this order; the rest go under Other. */
export const DOC_TYPES = ["Tracker", "Plan", "Findings", "Review", "Ticket", "Prompt"] as const;

/** Member notes that are a project's working papers, which the Notes screen can hide. Findings and
 *  Reviews are results worth finding, so they stay. */
export const WORKING_TYPES: readonly string[] = ["Plan", "Tracker", "Ticket", "Prompt"];

/** Whether a note is one of a project's working notes: a Plan, Tracker, Ticket or Prompt that
 *  belongs to a project. */
export const isWorkingNote = (r: Pick<FileSummary, "path" | "type" | "project">): boolean =>
  !!r.project && r.project !== r.path && !!r.type && WORKING_TYPES.includes(r.type);

/** The member notes in the Documents card's groups, in `DOC_TYPES` order then Other, each by title;
 *  empty groups left out. */
export function documentGroups(docs: ProjectDoc[]): { type: string; docs: ProjectDoc[] }[] {
  const known = new Set<string>(DOC_TYPES);
  const by = new Map<string, ProjectDoc[]>();
  for (const d of docs) {
    const t = d.type && known.has(d.type) ? d.type : "Other";
    by.set(t, [...(by.get(t) ?? []), d]);
  }
  return [...DOC_TYPES, "Other"]
    .filter((t) => by.has(t))
    .map((type) => ({ type, docs: by.get(type)!.sort((a, b) => a.title.localeCompare(b.title)) }));
}
