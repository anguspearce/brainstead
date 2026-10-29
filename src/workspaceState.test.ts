// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, vi } from "vitest";

const calls: [string, Record<string, unknown> | undefined][] = [];
let answers: Record<string, (a: Record<string, unknown> | undefined) => unknown> = {};
vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (cmd: string, a?: Record<string, unknown>) => {
    calls.push([cmd, a]);
    const f = answers[cmd];
    return f ? f(a) : undefined;
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import type { Folder, SessionInfo } from "./workspaceApi";
import {
  canType,
  closeSession,
  decodeOutput,
  exitSession,
  expandHome,
  groupFolders,
  inVault,
  mergeSession,
  pastEnd,
  restartSession,
  selectSession,
  stateLabel,
  terminalKeepsKey,
  tilde,
  waitingCount,
  workspace,
  wrapUp,
} from "./workspaceState";

const session = (over: Partial<SessionInfo> = {}): SessionInfo => ({
  id: "t1",
  folder: "/Users/maya/repos/orbit-app",
  name: "orbit-app",
  program: "claude",
  state: "working",
  started: 0,
  filesTouched: [],
  unseen: false,
  external: false,
  ...over,
});

const folder = (over: Partial<Folder>): Folder => ({ path: "/x", name: "x", kind: "folder", pinned: false, insideVault: false, ...over });

describe("the Workspace's rules", () => {
  it("merges a session's changes and its end", () => {
    const a = mergeSession([], session());
    expect(a).toHaveLength(1);
    const b = mergeSession(a, session({ state: "waiting" }));
    expect(b).toHaveLength(1);
    expect(b[0].state).toBe("waiting");
    expect(mergeSession(b, session({ id: "t2" })).map((s) => s.id)).toEqual(["t1", "t2"]);
    expect(exitSession(b, "t1", 1)[0]).toMatchObject({ state: "exited", exitCode: 1 });
    expect(stateLabel(exitSession(b, "t1", 1)[0])).toBe("Ended with code 1");
    expect(stateLabel(exitSession(b, "t1", null)[0])).toBe("Ended");
    // A shell or Codex has no hooks: it's running till it ends, never "Starting" for ever.
    expect(stateLabel(session({ program: "shell", state: "starting" }))).toBe("Running");
    expect(stateLabel(session({ program: "codex", state: "starting" }))).toBe("Running");
    expect(stateLabel(session({ program: "shell", state: "exited", exitCode: 0 }))).toBe("Ended with code 0");
    expect(stateLabel(session({ state: "starting" }))).toBe("Starting");
  });

  it("counts the sessions waiting for you, for the sidebar", () => {
    expect(waitingCount([session(), session({ id: "2", state: "waiting" }), session({ id: "3", state: "idle", unseen: true })])).toBe(1);
  });

  it("groups roots with their repos, then pinned, then recent newest first", () => {
    const g = groupFolders([
      folder({ path: "/r", name: "r", kind: "root" }),
      folder({ path: "/r/zed", name: "zed", kind: "repo", root: "/r" }),
      folder({ path: "/r/Alpha", name: "Alpha", kind: "repo", root: "/r", pinned: true }),
      folder({ path: "/old", name: "old", recent: 1 }),
      folder({ path: "/new", name: "new", recent: 5 }),
      folder({ path: "/pinned-recent", name: "pinned-recent", recent: 9, pinned: true }),
    ]);
    expect(g.roots.map((r) => [r.root.name, r.repos.map((x) => x.name)])).toEqual([["r", ["Alpha", "zed"]]]);
    expect(g.pinned.map((f) => f.name)).toEqual(["Alpha", "pinned-recent"]);
    expect(g.recent.map((f) => f.name)).toEqual(["new", "old"]);
  });

  it("writes the home folder ~, and reads it back", () => {
    expect(tilde("/Users/maya/repos/orbit-app", "/Users/maya")).toBe("~/repos/orbit-app");
    expect(tilde("/Users/maya", "/Users/maya/")).toBe("~");
    expect(tilde("/Users/mayank/x", "/Users/maya")).toBe("/Users/mayank/x");
    expect(tilde("C:\\Users\\maya\\repos", "C:\\Users\\maya")).toBe("~\\repos");
    expect(expandHome("~/repos", "/Users/maya")).toBe("/Users/maya/repos");
    expect(expandHome(" /opt/x ", "/Users/maya")).toBe("/opt/x");
    expect(expandHome("~/repos", null)).toBe("~/repos");
  });

  it("finds a changed file in the vault", () => {
    expect(inVault("/Users/maya/Notes/Project. Orbit App launch.md", "/Users/maya/Notes")).toBe("Project. Orbit App launch.md");
    expect(inVault("C:\\Notes\\wiki\\a.md", "C:\\Notes")).toBe("wiki/a.md");
    expect(inVault("/Users/maya/Notes2/a.md", "/Users/maya/Notes")).toBeNull();
    expect(inVault("/a.md", null)).toBeNull();
  });

  it("decodes output to the bytes the terminal writes", () => {
    expect([...decodeOutput(btoa("hi\r\n"))]).toEqual([104, 105, 13, 10]);
    // A multibyte character stays bytes, for the terminal to put together.
    expect([...decodeOutput("4pyT")]).toEqual([0xe2, 0x9c, 0x93]);
  });

  it("writes only the output after what a redraw drew", () => {
    const b = new Uint8Array([1, 2, 3, 4]);
    // Bytes 10 to 13, after a redraw that ended at 10, 14, 12 and 20.
    expect([...pastEnd(b, 10, 10)]).toEqual([1, 2, 3, 4]);
    expect([...pastEnd(b, 10, 14)]).toEqual([]);
    expect([...pastEnd(b, 10, 12)]).toEqual([3, 4]);
    expect([...pastEnd(b, 10, 20)]).toEqual([]);
    expect([...pastEnd(b, 30, 20)]).toEqual([1, 2, 3, 4]);
  });

  it("leaves keys to the terminal, except the app's command chords", () => {
    const k = (o: Partial<KeyboardEvent>) => ({ metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...o });
    expect(terminalKeepsKey(k({}), true)).toBe(true);
    expect(terminalKeepsKey(k({ ctrlKey: true }), true)).toBe(true);
    expect(terminalKeepsKey(k({ metaKey: true }), true)).toBe(false);
    expect(terminalKeepsKey(k({ ctrlKey: true }), false)).toBe(true);
    expect(terminalKeepsKey(k({ ctrlKey: true, shiftKey: true }), false)).toBe(false);
  });

  it("types only into a session running here", () => {
    expect(canType(session())).toBe(true);
    expect(canType(session({ state: "exited" }))).toBe(false);
    expect(canType(session({ external: true }))).toBe(false);
  });
});

describe("the Workspace's store", () => {
  beforeEach(() => {
    calls.length = 0;
    answers = {};
    workspace.set({ folders: null, error: null, sessions: [], folder: null, active: null, past: {} });
  });

  it("shows a session's tab, which sees it", () => {
    workspace.set({ ...workspace.get(), sessions: [session({ unseen: true, state: "idle" })] });
    selectSession("t1");
    expect(workspace.get()).toMatchObject({ active: "t1", folder: "/Users/maya/repos/orbit-app" });
    expect(workspace.get().sessions[0].unseen).toBe(false);
    expect(calls).toContainEqual(["term_seen", { id: "t1" }]);
  });

  it("wraps up with Brainstead's prompt and Enter", async () => {
    await wrapUp("t1");
    expect(calls).toEqual([["term_write", { id: "t1", data: "/mcp__brainstead__wrap_up\r" }]]);
  });

  it("restarts an ended Claude Code session where it left off, in its place", async () => {
    workspace.set({ ...workspace.get(), sessions: [session({ state: "exited", claudeSessionId: "c1" })], active: "t1" });
    answers.term_start = () => session({ id: "t9", state: "starting" });
    answers.settings_read = () => ({});
    answers.workspace_folders = () => [];
    await restartSession("t1");
    expect(calls.find(([c]) => c === "term_start")![1]).toEqual({
      req: { folder: "/Users/maya/repos/orbit-app", program: "claude", resume: "c1" },
    });
    expect(calls).toContainEqual(["term_close", { id: "t1" }]);
    expect(workspace.get().sessions.map((s) => s.id)).toEqual(["t9"]);
    expect(workspace.get().active).toBe("t9");
  });

  it("closes a tab", async () => {
    workspace.set({ ...workspace.get(), sessions: [session(), session({ id: "t2" })], active: "t1" });
    await closeSession("t1");
    expect(workspace.get().sessions.map((s) => s.id)).toEqual(["t2"]);
    expect(workspace.get().active).toBeNull();
  });
});
