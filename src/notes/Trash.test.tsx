// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { TrashEntry } from "../api";
import { byFile, renamedPath } from "./Rename";

const entries: TrashEntry[] = [
  {
    id: "a",
    originalRel: "Meeting. Old - 2026-09-24.md",
    layer: "note",
    basename: "Meeting. Old - 2026-09-24.md",
    deletedAt: "2026-10-02T08:40:00Z",
    sizeBytes: 6000,
  },
  { id: "b", originalRel: "wiki/entities/Dup.md", layer: "wiki", basename: "Dup.md", deletedAt: "2026-09-28T10:00:00Z", sizeBytes: 4000 },
];

const api = vi.hoisted(() => ({
  trashList: vi.fn(),
  trashRestore: vi.fn(),
  trashDelete: vi.fn(),
  trashEmpty: vi.fn(),
  trashQuickLook: vi.fn(),
  previewAvailable: vi.fn(),
}));
vi.mock(import("../api"), async (orig) => ({ ...(await orig()), api: api as never }));

const { TrashScreen, deletedWhen } = await import("./Trash");
const { previewView } = await import("../Preview");
const { setPlatform } = await import("../platform");

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("Trash", () => {
  it("previews the one ticked file on Space, even with its tick box just clicked (Windows), but not on a Mac", async () => {
    api.trashList.mockResolvedValue(entries);
    api.previewAvailable.mockResolvedValue(true);
    api.trashQuickLook.mockResolvedValue(true);
    try {
      setPlatform("windows");
      render(<TrashScreen />);
      const tick = await screen.findByLabelText("Select Dup.md");
      fireEvent.pointerDown(tick);
      fireEvent.click(tick);
      tick.focus();
      // Taken from the tick box: its default prevented, so the browser doesn't also untick it on keyup.
      expect(fireEvent.keyDown(tick, { key: " " })).toBe(false);
      await waitFor(() => expect(previewView.get()?.name).toBe("Dup.md"));
      expect(tick.getAttribute("aria-checked")).toBe("true");
      previewView.set(null);
      cleanup();
      setPlatform("macos");
      render(<TrashScreen />);
      const again = await screen.findByLabelText("Select Dup.md");
      fireEvent.pointerDown(again);
      fireEvent.click(again);
      again.focus();
      fireEvent.keyDown(again, { key: " " });
      await new Promise((r) => setTimeout(r, 5));
      expect(api.trashQuickLook).not.toHaveBeenCalled();
    } finally {
      setPlatform("linux");
      previewView.set(null);
    }
  });

  it("searches by name and the folder it was in", async () => {
    api.trashList.mockResolvedValue(entries);
    render(<TrashScreen />);
    await screen.findByText("Dup");
    const box = screen.getByLabelText("Search the Trash");
    fireEvent.change(box, { target: { value: "entities" } });
    expect(screen.queryByText("Meeting. Old - 2026-09-24")).toBeNull();
    expect(screen.getByText("Dup")).toBeTruthy();
    fireEvent.change(box, { target: { value: "meeting old" } });
    expect(screen.getByText("Meeting. Old - 2026-09-24")).toBeTruthy();
    expect(screen.queryByText("Dup")).toBeNull();
    fireEvent.change(box, { target: { value: "zzz" } });
    expect(screen.getByText(/Nothing in the Trash matches/)).toBeTruthy();
  });

  it("lists what's in .trash, filters by layer and restores", async () => {
    api.trashList.mockResolvedValue(entries);
    api.trashRestore.mockResolvedValue("Meeting. Old - 2026-09-24.md");
    render(<TrashScreen />);
    expect(await screen.findByText("Meeting. Old - 2026-09-24")).toBeTruthy();
    expect(screen.getByText("wiki/entities/")).toBeTruthy();
    fireEvent.click(screen.getByRole("radio", { name: "Wikis · 1" }));
    expect(screen.queryByText("Meeting. Old - 2026-09-24")).toBeNull();
    fireEvent.click(screen.getByRole("radio", { name: "All" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Restore" })[0]);
    await waitFor(() => expect(api.trashRestore).toHaveBeenCalledWith("a", null));
  });

  it("offers Restore as when the original path is taken", async () => {
    api.trashList.mockResolvedValue(entries);
    api.trashRestore
      .mockRejectedValueOnce({ code: "exists", message: "taken" })
      .mockResolvedValueOnce("Meeting. Old - 2026-09-24 (restored).md");
    render(<TrashScreen />);
    fireEvent.click((await screen.findAllByRole("button", { name: "Restore" }))[0]);
    expect(await screen.findByText(/a file now exists at the original path/)).toBeTruthy();
    fireEvent.click(screen.getAllByRole("button", { name: "Restore as…" })[0]);
    const input = screen.getByRole("textbox", { name: "Vault path" }) as HTMLInputElement;
    expect(input.value).toBe("Meeting. Old - 2026-09-24 (restored).md");
    fireEvent.submit(input.closest("form")!);
    await waitFor(() => expect(api.trashRestore).toHaveBeenLastCalledWith("a", "Meeting. Old - 2026-09-24 (restored).md"));
  });

  it("asks before deleting forever or emptying", async () => {
    api.trashList.mockResolvedValue(entries);
    api.trashEmpty.mockResolvedValue(2);
    api.trashDelete.mockResolvedValue(undefined);
    render(<TrashScreen />);
    fireEvent.click(await screen.findByRole("button", { name: /Empty the Trash/ }));
    expect(api.trashEmpty).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("dialog").querySelector(".danger-pri")!);
    await waitFor(() => expect(api.trashEmpty).toHaveBeenCalled());
    fireEvent.click(screen.getByRole("checkbox", { name: "Select Dup.md" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete forever" }));
    fireEvent.click(screen.getByRole("dialog").querySelector(".danger-pri")!);
    await waitFor(() => expect(api.trashDelete).toHaveBeenCalledWith("b"));
  });

  it("says when", () => {
    const now = new Date(2026, 9, 2, 12, 0);
    expect(deletedWhen(new Date(2026, 9, 2, 8, 40).toISOString(), now)).toBe("Today 08:40");
    expect(deletedWhen(new Date(2026, 9, 1).toISOString(), now)).toBe("Yesterday");
    expect(deletedWhen(new Date(2026, 8, 20).toISOString(), now)).toBe("20 Sept");
  });
});

describe("rename", () => {
  it("keeps the folder and extension", () => {
    expect(
      renamedPath("Victor/Workshop. Role Framework - 2026-07-14.md", {
        type: "Workshop",
        title: "Role Framework kickoff",
        date: "2026-07-14",
      }),
    ).toBe("Victor/Workshop. Role Framework kickoff - 2026-07-14.md");
    expect(renamedPath("a.txt", { type: "", title: "b", date: "" })).toBe("b.txt");
    expect(renamedPath("images/x.png", { type: "", title: "y.png", date: "" })).toBe("images/y.png");
    expect(renamedPath("a.md", { type: "T", title: " ", date: "" })).toBe("");
  });

  it("groups the plan by file", () => {
    const c = (path: string, line: number) => ({ path, line, before: "[[a]]", after: "[[b]]" });
    expect(
      byFile({ from: "a.md", to: "b.md", changes: [c("x.md", 1), c("y.md", 2), c("x.md", 5)] }).map(([f, cs]) => [f, cs.length]),
    ).toEqual([
      ["x.md", 2],
      ["y.md", 1],
    ]);
  });

  it("narrows its columns' defaults to the room, so a narrow table keeps its buttons", async () => {
    // A 1040px window with the sidebar at 400px leaves the table about 600px: the checkbox, the
    // buttons, the padding and the gaps take 322, and the columns' defaults another 492.
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(600);
    api.trashList.mockResolvedValue(entries);
    render(<TrashScreen />);
    await screen.findByText("Dup");
    const table = screen.getByRole("table", { name: "Trash" });
    const col = (v: string) => parseFloat(table.style.getPropertyValue(v));
    const cols = ["--col-type", "--col-folder", "--col-deleted", "--col-size"].map(col);
    expect(cols).toEqual([60, 80, 70, 50]);
    expect(322 + cols.reduce((a, b) => a + b, 0)).toBeLessThanOrEqual(600);
    vi.restoreAllMocks();
  });
});
