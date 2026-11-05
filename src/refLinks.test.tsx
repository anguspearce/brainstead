// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Reference links outside the task rows: the Settings rows that set them, the References and
// Tickets and links cards, and a Dataview value.

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn(async (..._a: unknown[]): Promise<unknown> => []));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
const openUrl = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl, openPath: vi.fn() }));

import { RefLinksSettings } from "./RefLinksSettings";
import { ReferencesCard, RefsCard } from "./notes/SideCards";
import { PropValue } from "./Doc";
import { DvText } from "./md/dataview/DvText";
import { gatherRefs } from "./md/refUses";
import { collectRefs } from "./md/refs";
import { settings } from "./store";

const ORBIT = { prefix: "ORBIT-", url: "https://tickets.example.com/browse/{key}" };

afterEach(() => {
  cleanup();
  openUrl.mockClear();
  settings.update({ referenceLinks: [] });
});

describe("Settings › Notes › Reference links", () => {
  it("adds, edits and removes a row, showing what a sample key opens", () => {
    const { container } = render(<RefLinksSettings />);
    expect(screen.queryByLabelText("Prefix, row 1")).toBeNull();
    fireEvent.click(screen.getByText("Add a prefix"));
    expect(settings.get().referenceLinks).toEqual([{ prefix: "", url: "" }]);
    fireEvent.change(screen.getByLabelText("Prefix, row 1"), { target: { value: "ORBIT-" } });
    fireEvent.change(screen.getByLabelText("Address, row 1"), { target: { value: "ftp://nope" } });
    expect(screen.getByText(/must start with http/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Address, row 1"), { target: { value: ORBIT.url } });
    expect(settings.get().referenceLinks).toEqual([ORBIT]);
    expect(container.querySelector(".reflink-sample")?.textContent).toBe("ORBIT-12 opens https://tickets.example.com/browse/ORBIT-12");
    fireEvent.click(screen.getByLabelText("Remove ORBIT-"));
    expect(settings.get().referenceLinks).toEqual([]);
  });

  it("marks a repeated prefix as the rule that ignores it does, and names each Remove button", () => {
    settings.update({
      referenceLinks: [
        ORBIT,
        { prefix: "HELP-", url: "https://help.example.com/{num}" },
        { prefix: "ORBIT-", url: "https://other.example.com/{key}" },
      ],
    });
    const { container } = render(<RefLinksSettings />);
    const samples = [...container.querySelectorAll(".reflink-sample")].map((e) => e.textContent);
    expect(samples[2]).toBe("An earlier row has this prefix already.");
    expect(samples[0]).toContain("opens");
    expect([...container.querySelectorAll("button")].map((b) => b.getAttribute("aria-label")).filter(Boolean)).toEqual([
      "Remove ORBIT-",
      "Remove HELP-",
      "Remove ORBIT-",
    ]);
    // A row keeps what was typed in it when one above it is removed.
    fireEvent.change(screen.getByLabelText("Address, row 2"), { target: { value: "https://help.example.com/x/{num}" } });
    fireEvent.click(screen.getAllByLabelText("Remove ORBIT-")[0]);
    expect((screen.getByLabelText("Address, row 1") as HTMLInputElement).value).toBe("https://help.example.com/x/{num}");
  });
});

describe("the References and Tickets and links cards", () => {
  const uses = gatherRefs([
    { where: "Project note", values: [collectRefs("ORBIT-9", { patterns: [ORBIT] })] },
    { where: "task", plural: "tasks", values: ["ORBIT-9 first", "then ORBIT-9 again"].map((v) => collectRefs(v, { patterns: [ORBIT] })) },
  ]);

  it("list each key once with where it is written, and open it in the browser", () => {
    render(<RefsCard title="Tickets and links" uses={uses} />);
    expect(screen.getByText("Tickets and links · 1")).toBeTruthy();
    expect(screen.getByText("Project note, 2 tasks")).toBeTruthy();
    fireEvent.click(screen.getByText("ORBIT-9"));
    expect(openUrl).toHaveBeenCalledWith("https://tickets.example.com/browse/ORBIT-9");
  });

  it("are not drawn with nothing to list", () => {
    const { container } = render(<RefsCard title="References" uses={[]} />);
    expect(container.firstChild).toBeNull();
  });
});

describe("with no pattern set", () => {
  it("draws no References card and asks for no tasks", () => {
    const { container } = render(<ReferencesCard path="Note.md" md={"A https://example.com/x link and ORBIT-1."} />);
    expect(container.firstChild).toBeNull();
    expect(invoke.mock.calls.filter((c) => c[0] === "tasks_all")).toHaveLength(0);
  });

  it("shows nothing of the note it came from after the next note opens", async () => {
    settings.update({ referenceLinks: [ORBIT] });
    const { rerender } = render(<ReferencesCard path="A.md" md="See ORBIT-1." />);
    expect(await screen.findByText("ORBIT-1")).toBeTruthy();
    rerender(<ReferencesCard path="B.md" md="See ORBIT-2." />);
    // At once, not after the debounce: the first note's key is gone.
    expect(screen.queryByText("ORBIT-1")).toBeNull();
    expect(await screen.findByText("ORBIT-2")).toBeTruthy();
    // And a note with none shows no card at all, not the last note's.
    rerender(<ReferencesCard path="C.md" md="Nothing here." />);
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByText(/References/)).toBeNull();
  });

  it("draws the card once a pattern is set", async () => {
    settings.update({ referenceLinks: [ORBIT] });
    render(<ReferencesCard path="Note.md" md={"A https://example.com/x link and ORBIT-1."} />);
    expect(await screen.findByText("References · 2")).toBeTruthy();
  });

  it("shows a URL property as written, now a link, and a key as plain text", () => {
    const { container } = render(
      <>
        <PropValue v="https://example.com/some/page" />
        <PropValue v={["https://example.com/a", "ORBIT-12", "plain"]} />
      </>,
    );
    const a = [...container.querySelectorAll<HTMLAnchorElement>("a.propurl")];
    expect(a.map((x) => x.textContent)).toEqual(["https://example.com/some/page", "https://example.com/a"]);
    expect(container.querySelector("button")).toBeNull();
    expect(container.textContent).toContain("ORBIT-12");
    fireEvent.click(a[0]);
    expect(openUrl).toHaveBeenLastCalledWith("https://example.com/some/page");
  });

  it("makes a key a link chip only once a pattern is set, and never for tags", () => {
    settings.update({ referenceLinks: [ORBIT] });
    const { container } = render(
      <>
        <PropValue v={["ORBIT-12"]} />
        <PropValue v={["ORBIT-13"]} tags />
        <PropValue v={["ORBIT-14"]} label />
      </>,
    );
    expect([...container.querySelectorAll("button.chip.link")].map((b) => b.textContent)).toEqual(["ORBIT-12"]);
  });
});

describe("a Dataview value", () => {
  it("links a configured key and leaves other text alone", () => {
    settings.update({ referenceLinks: [ORBIT] });
    const { container } = render(<DvText text="Blocked by ORBIT-12 and `ORBIT-13` or XORBIT-14" />);
    const a = container.querySelector("a.mdref")!;
    expect(a.textContent).toBe("ORBIT-12");
    fireEvent.click(a);
    expect(openUrl).toHaveBeenCalledWith("https://tickets.example.com/browse/ORBIT-12");
    expect(container.querySelectorAll("a").length).toBe(1);
    expect(container.textContent).toBe("Blocked by ORBIT-12 and ORBIT-13 or XORBIT-14");
  });
});
