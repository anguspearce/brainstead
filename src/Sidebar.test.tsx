// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ready } from "./App";
import { ask, blankChat } from "./askState";
import { place } from "./nav";
import { Sidebar, statusLine, summed } from "./Sidebar";
import { EMPTY_STATUS, vaultStatus } from "./state";
import { settings } from "./store";
import { tasksFailed } from "./taskModel";

afterEach(cleanup);

describe("Sidebar", () => {
  it("shows every section's screens and the index counts, and navigates", () => {
    vaultStatus.set({
      ...EMPTY_STATUS,
      state: "ready",
      stats: { ...EMPTY_STATUS.stats, files: 2210, notes: 1561, wiki: 381, updatedAt: Date.now() },
    });
    render(<Sidebar onSearch={() => {}} />);
    for (const l of [
      "Today",
      "Weekly review",
      "Inbox",
      "Tasks",
      "Projects",
      "Changes",
      "Ask",
      "Search",
      "Notes",
      "Wiki",
      "Sources",
      "Draft reply",
      "Meeting notes",
      "Compare",
      "Vault",
      "Settings",
    ]) {
      // Search exactly: the search box above is "Search or jump to…".
      expect(screen.getByRole("button", { name: new RegExp(l === "Search" ? "^Search$" : `^${l}(?! health)`) })).toBeTruthy();
    }
    expect(screen.getByRole("button", { name: /^Notes/ }).textContent).toContain("1,561");
    expect(screen.getByRole("status").textContent).toBe("2,210 files · just now");
    fireEvent.click(screen.getByRole("button", { name: /^Wiki(?! health)/ }));
    expect(place.get().place.screen).toBe("wiki");
    expect(screen.getByRole("button", { name: /^Wiki(?! health)/ }).getAttribute("aria-current")).toBe("page");
  });

  it("shows Read-only while the vault is, which opens Settings › Vault", () => {
    settings.update({ readOnly: false });
    render(<Sidebar onSearch={() => {}} />);
    expect(screen.queryByRole("button", { name: "Read-only" })).toBeNull();
    cleanup();
    settings.update({ readOnly: true });
    render(<Sidebar onSearch={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Read-only" }));
    expect(place.get().place).toMatchObject({ screen: "settings", pane: "vault" });
    expect(settings.get().readOnly).toBe(true);
    settings.update({ readOnly: false });
  });

  it("shows a warning, not 0, when the tasks couldn't be loaded", () => {
    tasksFailed.set("disk error");
    render(<Sidebar onSearch={() => {}} />);
    expect(screen.getByRole("button", { name: /^Today/ }).getAttribute("title")).toContain("Couldn't load the tasks");
    expect(screen.getByRole("button", { name: /^Tasks/ }).querySelector(".n.err")).toBeTruthy();
    tasksFailed.set(null);
  });

  it("counts the open chats with an answer you haven't seen on Ask", () => {
    const chat = (unread: boolean) => ({ ...blankChat("claude:sonnet"), unread });
    ask.set({ chats: [chat(true), chat(false), chat(true)], active: null });
    render(<Sidebar onSearch={() => {}} />);
    const badge = screen.getByRole("button", { name: /^Ask/ }).querySelector(".n");
    expect(badge?.textContent).toBe("2");
    expect(badge?.classList.contains("hot")).toBe(true);
    ask.set({ chats: [], active: null });
  });

  it("lists the assistant's tools under Ask, each with its own row", () => {
    render(<Sidebar onSearch={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /^Draft reply/ }));
    expect(place.get().place.screen).toBe("reply");
    expect(screen.getByRole("button", { name: /^Draft reply/ }).getAttribute("aria-current")).toBe("page");
  });

  it("folds Wiki health, Activity, Trash, Templates and Graph into Vault, whose badge sums theirs while it's shut", () => {
    settings.update({ folded: {} });
    render(<Sidebar onSearch={() => {}} />);
    // Shut to begin with.
    const fold = screen.getByRole("button", { name: /^Vault/ });
    expect(fold.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("button", { name: /^Wiki health/ })).toBeNull();
    fireEvent.click(fold);
    expect(settings.get().folded?.vault).toBe(false);
    for (const l of ["Wiki health", "Activity", "Trash", "Templates", "Graph"])
      expect(screen.getByRole("button", { name: new RegExp(`^${l}`) })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /^Graph/ }));
    expect(place.get().place.screen).toBe("graph");
    fireEvent.click(screen.getByRole("button", { name: /^Vault/ }));
    expect(screen.queryByRole("button", { name: /^Graph/ })).toBeNull();
    // Shut on one of its screens, the fold is lit for it.
    expect(screen.getByRole("button", { name: /^Vault/ }).classList.contains("on")).toBe(true);
  });

  it("adds up the badges a shut fold hides", () => {
    expect(summed([undefined, { n: 2 }, { n: 3, hot: true }])).toMatchObject({ n: 5, hot: true, bad: false });
    expect(summed([undefined])).toBeUndefined();
    expect(summed([{ n: 0, busy: "Checking" }])?.busy).toBe("Checking");
  });

  it("folds a section", () => {
    render(<Sidebar onSearch={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /^Library/ }));
    expect(screen.queryByRole("button", { name: /^Sources/ })).toBeNull();
    settings.update({ folded: {} });
  });
});

describe("status line", () => {
  it("says what the index is doing", () => {
    expect(statusLine({ ...EMPTY_STATUS, state: "indexing" }, 0).tone).toBe("busy");
    expect(statusLine({ ...EMPTY_STATUS, state: "error", error: "no access" }, 0)).toMatchObject({ tone: "bad", tip: "no access" });
    expect(statusLine({ ...EMPTY_STATUS, state: "ready", stats: { ...EMPTY_STATUS.stats, files: 3, updatedAt: 0 } }, 0).text).toBe(
      "3 files · indexed",
    );
  });
});

describe("first run", () => {
  it("asks for Full Disk Access, then the vault", () => {
    const s = { vaultPath: null, skippedFullDiskAccess: false };
    expect(ready(s, false, true)).toBe(false);
    expect(ready({ ...s, skippedFullDiskAccess: true, vaultPath: "/v" }, false, true)).toBe(true);
    expect(ready({ ...s, vaultPath: "/v" }, true, true)).toBe(true);
    expect(ready({ ...s, vaultPath: "/v" }, false, false)).toBe(true);
    expect(ready(s, true, true)).toBe(false);
  });
});
