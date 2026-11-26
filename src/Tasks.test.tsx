// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A grouped Tasks list folds: each group from its band, every group from the top bar, and what's
// folded is remembered for each grouping.

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const rows = vi.hoisted(() => ({ list: [] as unknown[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => (cmd === "tasks_query" ? rows.list : [])),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { TaskRow } from "./api";
import { nav } from "./nav";
import { settings } from "./store";
import { TasksScreen } from "./Tasks";

const t = (text: string, line: number, o: Partial<TaskRow> = {}): TaskRow => ({
  path: "Me. To Do List.md",
  title: "Me. To Do List",
  line,
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
  ...o,
});

afterEach(() => {
  cleanup();
  settings.update({ viewMemory: {} });
});

// A band's fold button, apart from the project chips on the rows under it.
const band = (name: RegExp) => screen.getAllByRole("button", { name }).find((b) => b.closest(".tb-band"))!;
const shown = (text: string) => screen.queryByText(text) !== null;

async function grouped() {
  rows.list = [
    t("Book the venue", 1, { project: "Project. Orbit App launch.md" }),
    t("Draft the invite", 2, { project: "Project. Acme renewal.md" }),
    t("Water the plants", 3),
  ];
  nav.go("tasks");
  render(<TasksScreen />);
  await act(async () => {});
  fireEvent.click(screen.getByRole("radio", { name: "Project" }));
}

describe("a grouped Tasks list", () => {
  it("folds a group from its band and keeps the others open", async () => {
    await grouped();
    expect(band(/Orbit App launch/).getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(band(/Orbit App launch/));
    expect(band(/Orbit App launch/).getAttribute("aria-expanded")).toBe("false");
    expect(shown("Book the venue")).toBe(false);
    expect(shown("Draft the invite")).toBe(true);
    // Remembered, for this grouping alone.
    expect(settings.get().viewMemory?.["tasks:folded:project"]).toEqual(["Project. Orbit App launch.md"]);
    fireEvent.click(band(/Orbit App launch/));
    expect(shown("Book the venue")).toBe(true);
  });

  it("folds and opens every group from the top bar", async () => {
    await grouped();
    fireEvent.click(screen.getByRole("button", { name: "Collapse all" }));
    for (const text of ["Book the venue", "Draft the invite", "Water the plants"]) expect(shown(text)).toBe(false);
    expect(band(/No project/).getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(screen.getByRole("button", { name: "Expand all" }));
    for (const text of ["Book the venue", "Draft the invite", "Water the plants"]) expect(shown(text)).toBe(true);
    // One group left open is enough for the button to offer Collapse all again.
    fireEvent.click(band(/No project/));
    expect(screen.queryByRole("button", { name: "Collapse all" })).not.toBeNull();
  });

  it("keeps each grouping's folds apart, and across a visit", async () => {
    await grouped();
    fireEvent.click(band(/Orbit App launch/));
    fireEvent.click(screen.getByRole("radio", { name: "Due" }));
    expect(band(/No due date/).getAttribute("aria-expanded")).toBe("true");
    expect(shown("Book the venue")).toBe(true);
    fireEvent.click(screen.getByRole("radio", { name: "Project" }));
    expect(shown("Book the venue")).toBe(false);
    cleanup();
    nav.go("today");
    nav.go("tasks");
    render(<TasksScreen />);
    await act(async () => {});
    expect(band(/Orbit App launch/).getAttribute("aria-expanded")).toBe("false");
    expect(shown("Book the venue")).toBe(false);
    expect(shown("Draft the invite")).toBe(true);
  });

  it("leaves folded the groups a search hides when collapsing all", async () => {
    await grouped();
    fireEvent.click(band(/Acme renewal/));
    fireEvent.change(document.querySelector<HTMLInputElement>(".searchbox input")!, { target: { value: "venue" } });
    await waitFor(() => expect(screen.queryAllByRole("button", { name: /No project/ })).toHaveLength(0));
    fireEvent.click(screen.getByRole("button", { name: "Collapse all" }));
    fireEvent.change(document.querySelector<HTMLInputElement>(".searchbox input")!, { target: { value: "" } });
    await waitFor(() => expect(shown("Water the plants")).toBe(true));
    expect(band(/Acme renewal/).getAttribute("aria-expanded")).toBe("false");
    expect(band(/Orbit App launch/).getAttribute("aria-expanded")).toBe("false");
    expect(band(/No project/).getAttribute("aria-expanded")).toBe("true");
  });

  it("has no fold button on an ungrouped list", async () => {
    await grouped();
    fireEvent.click(screen.getByRole("radio", { name: "None" }));
    expect(screen.queryByRole("button", { name: /Collapse all|Expand all/ })).toBeNull();
    expect(shown("Book the venue")).toBe(true);
  });
});
