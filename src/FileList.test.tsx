// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => ({ hits: [], total: 0, ms: 0 })), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { FileList } from "./FileList";
import { nav, restorePlaces } from "./nav";
import type { FileSummary } from "./api";
import { settings } from "./store";
import { listWidth } from "./panes";

afterEach(cleanup);
beforeEach(() => settings.update({ viewMemory: {}, searchOrder: {} }));
Element.prototype.scrollIntoView = () => {};

const row = (title: string, mtime: number) => ({ path: `${title}.md`, title, mtime, layer: "note", tags: [] }) as unknown as FileSummary;
const rows = [row("Banana", 2), row("Apple", 1), row("Cherry", 3)];
const list = () =>
  render(
    <FileList
      rows={rows}
      columns={[
        { key: "title", label: "Title", value: (r: FileSummary) => r.title, render: (r: FileSummary) => r.title },
        { key: "mtime", label: "Modified", value: (r: FileSummary) => r.mtime, render: (r: FileSummary) => String(r.mtime) },
      ]}
      layers={["note"]}
      noun={["note", "notes"]}
      table="notes"
      defaultSort={["mtime", -1]}
      placeholder="Filter"
    />,
  );
const order = () => screen.getAllByText(/^(Apple|Banana|Cherry)$/).map((e) => e.textContent);

describe("the search order switch", () => {
  it("shows the latest first while searching, and is kept", async () => {
    restorePlaces({ stack: [{ screen: "notes" }], i: 0 });
    list();
    // Only while a search is typed.
    expect(screen.queryByRole("radio", { name: "Latest" })).toBeNull();
    fireEvent.change(screen.getByPlaceholderText("Filter"), { target: { value: "an" } });
    await act(() => new Promise((r) => setTimeout(r, 400)));
    fireEvent.click(screen.getByRole("radio", { name: "Latest" }));
    expect(settings.get().searchOrder).toEqual({ notes: "latest" });
    expect(screen.getByRole("radio", { name: "Latest" }).getAttribute("aria-checked")).toBe("true");
  });
});

describe("a column sort during a search", () => {
  it("is kept for a fresh visit with the search still in the box", async () => {
    restorePlaces({ stack: [{ screen: "notes" }], i: 0 });
    const v = list();
    fireEvent.change(screen.getByPlaceholderText("Filter"), { target: { value: "an" } });
    await act(() => new Promise((r) => setTimeout(r, 400)));
    fireEvent.click(screen.getByRole("columnheader", { name: /Title/ }));
    v.unmount();
    act(() => nav.go("tasks"));
    act(() => nav.go("notes"));
    list();
    await act(() => new Promise((r) => setTimeout(r, 400)));
    expect(screen.getByRole("columnheader", { name: /Title/ }).getAttribute("aria-sort")).toBe("ascending");
  });
});

describe("a list's sort", () => {
  it("survives a search and a fresh visit", async () => {
    restorePlaces({ stack: [{ screen: "notes" }], i: 0 });
    const v = list();
    expect(order()).toEqual(["Cherry", "Banana", "Apple"]);
    fireEvent.click(screen.getByRole("columnheader", { name: /Title/ }));
    expect(order()).toEqual(["Apple", "Banana", "Cherry"]);
    const box = screen.getByPlaceholderText("Filter");
    fireEvent.change(box, { target: { value: "an" } });
    await act(() => new Promise((r) => setTimeout(r, 400)));
    fireEvent.change(box, { target: { value: "" } });
    await act(() => new Promise((r) => setTimeout(r, 400)));
    expect(order()).toEqual(["Apple", "Banana", "Cherry"]);
    v.unmount();
    act(() => nav.go("tasks"));
    act(() => nav.go("notes"));
    list();
    expect(order()).toEqual(["Apple", "Banana", "Cherry"]);
  });
});

describe("a list's columns", () => {
  it("are kept by the table's name, not its noun, and a column after Title widens as its left edge goes left", () => {
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(1200);
    settings.update({ paneWidths: undefined, resizeLocks: undefined });
    render(
      <FileList
        rows={rows}
        columns={[
          { key: "title", label: "Title", width: "3", value: (r: FileSummary) => r.title },
          { key: "tags", label: "Tags", width: listWidth("wiki.tags"), value: () => "" },
          { key: "act", label: "", width: "0 0 150px", value: () => "" },
        ]}
        layers={["wiki"]}
        noun={["wiki page", "wiki pages"]}
        table="wiki"
        defaultSort={["title", 1]}
        placeholder="Filter"
      />,
    );
    // Title takes the rest and the button column keeps its width: Tags alone has a divider.
    const [tags, ...others] = screen.getAllByRole("separator");
    expect(others).toEqual([]);
    expect(tags.getAttribute("aria-label")).toBe("Resize the Tags column");
    expect(tags.dataset.edge).toBe("start");
    fireEvent.keyDown(tags, { key: "ArrowLeft" });
    expect(settings.get().paneWidths).toEqual({ "wiki.tags": 208 });
    vi.restoreAllMocks();
  });
});
