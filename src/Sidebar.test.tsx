// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ready } from "./App";
import { ask, blankChat } from "./askState";
import { place } from "./nav";
import { Sidebar, statusLine } from "./Sidebar";
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
      "Templates",
      "Graph",
      "Tools",
      "Knowledge health",
      "Activity",
      "Trash",
      "Settings",
    ]) {
      // Search exactly: the search box above is "Search or jump to…".
      expect(screen.getByRole("button", { name: new RegExp(l === "Search" ? "^Search$" : `^${l}`) })).toBeTruthy();
    }
    expect(screen.getByRole("button", { name: /^Notes/ }).textContent).toContain("1,561");
    expect(screen.getByRole("status").textContent).toBe("2,210 files · just now");
    fireEvent.click(screen.getByRole("button", { name: /^Wiki/ }));
    expect(place.get().place.screen).toBe("wiki");
    expect(screen.getByRole("button", { name: /^Wiki/ }).getAttribute("aria-current")).toBe("page");
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

  it("opens Draft reply, Meeting notes and Compare from the Tools menu", () => {
    render(<Sidebar onSearch={() => {}} />);
    expect(screen.queryByRole("menuitem", { name: /^Draft reply/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /^Tools/ }));
    expect(screen.getByRole("menuitem", { name: /^Meeting notes/ })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: /^Compare/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("menuitem", { name: /^Draft reply/ }));
    expect(place.get().place.screen).toBe("reply");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("button", { name: /^Tools/ }).classList.contains("on")).toBe(true);
  });

  it("folds a section", () => {
    render(<Sidebar onSearch={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /^Library/ }));
    expect(screen.queryByRole("button", { name: /^Graph/ })).toBeNull();
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
