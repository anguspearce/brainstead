// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Delete project's dialog: what it says happens to the project's tasks in other notes.

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn(async (..._a: unknown[]): Promise<unknown> => []));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { ProjectRow, TaskRow } from "./api";
import { projectTasks } from "./gtd";
import { DeleteProject } from "./Projects";

const p: ProjectRow = {
  name: "Orbit",
  path: "Project. Orbit.md",
  status: "active",
  area: null,
  outcome: null,
  next: 0,
  waiting: 0,
  someday: 0,
  done: 0,
  lastTouched: 0,
  links: [],
  documents: [],
};

const t = (path: string, text: string, projectFrom: TaskRow["projectFrom"]): TaskRow => ({
  path,
  title: path.replace(/\.md$/, ""),
  line: 0,
  lineText: `- [ ] ${text}`,
  text,
  done: false,
  due: null,
  scheduled: null,
  start: null,
  doneOn: null,
  created: null,
  rank: null,
  tags: [],
  heading: null,
  project: p.path,
  projectFrom,
});

afterEach(cleanup);

describe("Delete project", () => {
  it("tells tasks that link the project from tasks in its documents", () => {
    const tasks = projectTasks(
      [
        t(p.path, "In the project note", "note"),
        t("Meeting. Steerco.md", "Links it", "line"),
        t("Tracker. Orbit.md", "In the tracker", "member"),
        t("Tracker. Orbit.md", "Also in the tracker", "member"),
      ],
      p.path,
    );
    render(<DeleteProject p={p} tasks={tasks} onClose={() => {}} />);
    expect(screen.getByText(/with the 1 task written in it/)).toBeTruthy();
    expect(screen.getByText(/^1 task in another note links it;/)).toBeTruthy();
    expect(screen.getByText(/^2 tasks in its documents \(notes whose Project: line names it\)/)).toBeTruthy();
  });

  it("says nothing of other notes when every task is in the project note", () => {
    render(<DeleteProject p={p} tasks={projectTasks([t(p.path, "Only here", "note")], p.path)} onClose={() => {}} />);
    expect(screen.queryByText(/link it/)).toBeNull();
    expect(screen.queryByText(/its documents/)).toBeNull();
  });
});
