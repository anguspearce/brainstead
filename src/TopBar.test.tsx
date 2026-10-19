// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null), convertFileSrc: (p: string) => p }));

import { settings } from "./store";
import { MoreMenu, ReadOnlyBanner } from "./TopBar";

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

describe("ReadOnlyBanner", () => {
  it("says what read-only blocks, and turns it off", () => {
    settings.update({ readOnly: true });
    render(<ReadOnlyBanner blocks="nothing here can be accepted or reverted" />);
    expect(screen.getByRole("status").textContent).toContain("The vault is read-only, so nothing here can be accepted or reverted.");
    fireEvent.click(screen.getByRole("button", { name: "Turn off read-only" }));
    expect(settings.get().readOnly).toBe(false);
  });
});
