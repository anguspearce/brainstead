// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null), convertFileSrc: (p: string) => p }));

import { settings } from "./store";
import { MoreMenu, ReadOnlyBanner, TopBar } from "./TopBar";
import { Seg, Squeeze } from "./ui";

afterEach(cleanup);

describe("MoreMenu", () => {
  it("lists the screen's less used actions, runs one and closes", () => {
    const check = vi.fn();
    render(
      <MoreMenu
        title="Check now"
        items={[
          { label: "Check now", icon: "refresh", title: "Run the checks again", run: check },
          { label: "Busy", icon: "info", title: "Not now", run: () => {}, disabled: true },
        ]}
      />,
    );
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "More" }));
    expect((screen.getByRole("menuitem", { name: "Busy" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("menuitem", { name: "Check now" }));
    expect(check).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("MoreMenu's button", () => {
  it("closes the menu when it's clicked again", async () => {
    render(<MoreMenu title="More" items={[{ label: "Check now", icon: "refresh", title: "Run the checks", run: () => {} }]} />);
    const more = screen.getByRole("button", { name: "More" });
    fireEvent.click(more);
    expect(screen.getByRole("menu")).toBeTruthy();
    // The click-outside listener is armed after a tick.
    await new Promise((r) => setTimeout(r, 0));
    fireEvent.mouseDown(more);
    fireEvent.click(more);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("ReadOnlyBanner", () => {
  it("says what read-only blocks, and turns it off", () => {
    settings.update({ readOnly: true });
    render(<ReadOnlyBanner blocks="nothing here can be accepted or reverted" />);
    expect(screen.getByRole("status").textContent).toContain("The vault is read-only, so nothing here can be accepted or reverted.");
    fireEvent.click(screen.getByRole("button", { name: "Turn off read-only" }));
    expect(settings.get().readOnly).toBe(false);
  });
});

describe("a narrow top bar", () => {
  const css = readFileSync("src/styles.css", "utf8");

  // jsdom lays nothing out, so these read the rules themselves; the narrow scenes show the result.
  it("truncates the subtitle, then the title, with an ellipsis, and never paints them under the controls", () => {
    expect(css).toMatch(/\.top \.crumbs \{[^}]*min-width: 48px;\s*overflow: hidden;/);
    expect(css).toMatch(/\.top \.crumbs b \{\s*flex: none;\s*max-width: 100%;\s*min-width: 0;[^}]*text-overflow: ellipsis;/);
    expect(css).toMatch(/\.top \.crumbs \.sub \{\s*flex: 0 1 auto;\s*min-width: 0;[^}]*text-overflow: ellipsis;/);
  });

  it("on Windows, backs the window's buttons with the bar's own colour", () => {
    expect(css).toMatch(/\.winctl \{[^}]*background: var\(--ground\);/);
  });

  it("condenses the bar's controls on the bar's own width", () => {
    expect(css).toMatch(/\.top \{[^}]*container: top \/ inline-size;/);
    expect(css).toMatch(/@container top \(width <= 800px\) \{\s*\.top \.segd \{\s*display: none;/);
    expect(css).toMatch(/@container top \(width <= 680px\) \{[^@]*\.top \.segsel \{\s*display: block;/);
  });

  it("draws the subtitle after the title, its dot from CSS", () => {
    const { container } = render(<TopBar title="Trash" sub="2 items" />);
    expect(container.querySelector(".crumbs")?.textContent).toBe("Trash2 items");
    expect(container.querySelector(".crumbs .sub")?.textContent).toBe("2 items");
  });

  it("gives a segmented control in the bar a drop-down twin with the same choices, and none elsewhere", () => {
    const pick = vi.fn();
    const seg = (
      <Seg
        label="Show"
        value="all"
        onChange={pick}
        options={[
          ["all", "All", "Everything"],
          ["note", "Notes", "Only the notes", " · 1"],
        ]}
      />
    );
    const { container, unmount } = render(<TopBar title="Trash">{seg}</TopBar>);
    expect(screen.getByRole("radio", { name: "Notes · 1" }).querySelector(".segd")?.textContent).toBe(" · 1");
    const twin = screen.getByRole("combobox", { name: "Show" }) as HTMLSelectElement;
    expect(twin.classList.contains("segsel")).toBe(true);
    expect(Array.from(twin.options).map((o) => o.textContent)).toEqual(["All", "Notes"]);
    fireEvent.change(twin, { target: { value: "note" } });
    expect(pick).toHaveBeenCalledWith("note");
    unmount();
    render(seg);
    expect(container.querySelector("select")).toBeNull();
    expect(screen.queryByRole("combobox")).toBeNull();
  });

  it("gives words a span per wording, each classed with the widths it serves", () => {
    const { container } = render(<Squeeze full="Empty the Trash · frees 93 B" tight="Empty the Trash" />);
    expect(Array.from(container.querySelectorAll("span")).map((s) => [s.className, s.textContent])).toEqual([
      ["sq sq-w", "Empty the Trash · frees 93 B"],
      ["sq sq-t sq-n", "Empty the Trash"],
    ]);
    const { container: c2 } = render(<Squeeze full="New project" narrow="New" />);
    expect(Array.from(c2.querySelectorAll("span")).map((s) => s.className)).toEqual(["sq sq-w sq-t", "sq sq-n"]);
  });
});
