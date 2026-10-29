// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The MCP server's workspace action and a project's folders, through the window (src/mcpActions.ts).

import { beforeEach, describe, expect, it, vi } from "vitest";

type Call = [string, Record<string, unknown> | undefined];
const calls: Call[] = [];
let answers: Record<string, (a: Record<string, unknown> | undefined) => unknown> = {};
vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (cmd: string, a?: Record<string, unknown>) => {
    calls.push([cmd, a]);
    const f = answers[cmd];
    return f ? f(a) : undefined;
  },
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/path", () => ({ homeDir: async () => "/Users/maya" }));

import { foldersIn, Listing, runAction } from "./mcpActions";
import type { Folder, SessionInfo } from "./workspaceApi";
import { workspace } from "./workspaceState";

const raw = (args: Record<string, unknown>) => runAction({ id: "1", action: "workspace", args });
const run = async (args: Record<string, unknown>) => {
  const r = await raw(args);
  return r && typeof r === "object" && "structured" in r ? (r as Listing).text : r;
};
const called = (cmd: string) => calls.filter(([c]) => c === cmd).map(([, a]) => a);

const session = (over: Partial<SessionInfo> = {}): SessionInfo => ({
  id: "t1",
  folder: "/Users/maya/repos/orbit-app",
  name: "orbit-app",
  program: "claude",
  state: "waiting",
  started: 0,
  filesTouched: [],
  unseen: true,
  external: false,
  ...over,
});

const folders: Folder[] = [
  { path: "/Users/maya/repos", name: "repos", kind: "root", pinned: false, insideVault: false },
  {
    path: "/Users/maya/repos/orbit-app",
    name: "orbit-app",
    kind: "repo",
    root: "/Users/maya/repos",
    branch: "main",
    dirty: true,
    pinned: false,
    project: { path: "Project. Orbit App launch.md", name: "Orbit App launch" },
    insideVault: false,
  },
];

describe("the workspace action", () => {
  beforeEach(() => {
    calls.length = 0;
    answers = { settings_read: () => ({}) };
    workspace.set({ folders: null, error: null, sessions: [], folder: null, active: null, past: {} });
  });

  it("lists folders in the screen's order, with their project and branch", async () => {
    answers.workspace_folders = () => folders;
    const text = await run({ do: "folders" });
    expect(text).toMatch(/^2 folders:\n- repos · root/);
    expect(text).toMatch(/orbit-app · repo · main \(uncommitted changes\) · project Orbit App launch/);
    const items = ((await raw({ do: "folders" })) as Listing).structured.items as Record<string, unknown>[];
    expect(items[1]).toMatchObject({ name: "orbit-app", dirty: true, project: { name: "Orbit App launch" } });
  });

  it("lists sessions with their state, and past sessions of a folder", async () => {
    answers.term_sessions = () => [session({ title: "Move the banner" })];
    expect(await run({ do: "sessions" })).toMatch(/- t1 · Claude Code in orbit-app · Waiting for you · “Move the banner”/);
    answers.workspace_past_sessions = () => [
      { id: "p1", folder: "/Users/maya/repos/orbit-app", started: 0, lastActive: 0, firstMessage: "Tidy the copy", program: "claude" },
    ];
    expect(await run({ do: "past_sessions", folder: "~/repos/orbit-app" })).toMatch(/p1 .*“Tidy the copy”/);
    // ~ is the home folder.
    expect(called("workspace_past_sessions")[0]).toEqual({ folder: "/Users/maya/repos/orbit-app" });
  });

  it("adds, removes, pins and unpins folders as the screen does", async () => {
    answers.workspace_folders = () => folders;
    await run({ do: "add_root", path: "~/repos" });
    expect(called("workspace_add_root")[0]).toEqual({ path: "/Users/maya/repos" });
    await run({ do: "pin", path: "/Users/maya/repos/orbit-app" });
    await run({ do: "unpin", path: "/Users/maya/repos/orbit-app" });
    expect(called("workspace_pin")).toEqual([
      { path: "/Users/maya/repos/orbit-app", pinned: true },
      { path: "/Users/maya/repos/orbit-app", pinned: false },
    ]);
    await run({ do: "remove_root", path: "/Users/maya/repos" });
    expect(called("workspace_remove_root")).toHaveLength(1);
    await expect(run({ do: "pin" })).rejects.toThrow(/Give the folder's path/);
  });

  it("starts a session, but never in a session nobody is watching", async () => {
    answers.term_start = (a) => session({ folder: (a!.req as { folder: string }).folder, state: "starting" });
    expect(await run({ do: "start_session", folder: "~/repos/orbit-app", program: "claude", prompt: "Fix the banner" })).toMatch(
      /Started Claude Code in \/Users\/maya\/repos\/orbit-app as session t1/,
    );
    expect(called("term_start")[0]).toEqual({
      req: { folder: "/Users/maya/repos/orbit-app", program: "claude", resume: undefined, prompt: "Fix the banner" },
    });
    // The screen shows its tab.
    expect(workspace.get().active).toBe("t1");
    calls.length = 0;
    await expect(run({ do: "start_session", folder: "~/repos/orbit-app", unattended: true })).rejects.toThrow(/nobody is watching/);
    await expect(run({ do: "start_session", folder: "~/repos/orbit-app", program: "bash" })).rejects.toThrow(/claude or codex/);
    expect(called("term_start")).toHaveLength(0);
  });

  it("never starts a shell, which isn't guarded, but starts Codex", async () => {
    answers.term_start = (a) => session({ program: (a!.req as { program: "codex" }).program, state: "starting" });
    await expect(run({ do: "start_session", folder: "~/repos/orbit-app", program: "shell" })).rejects.toThrow(
      "A shell session isn't guarded, so assistants can't start one or type into one. Start it yourself in the Workspace.",
    );
    expect(called("term_start")).toHaveLength(0);
    expect(await run({ do: "start_session", folder: "~/repos/orbit-app", program: "codex", prompt: "Fix it" })).toMatch(/Started Codex/);
    expect(called("term_start")[0]).toMatchObject({ req: { program: "codex", prompt: "Fix it" } });
  });

  it("never types into a shell session or one in an outside terminal", async () => {
    answers.term_sessions = () => [
      session({ id: "sh", program: "shell" }),
      session({ id: "ex", external: true }),
      session({ id: "cx", program: "codex" }),
    ];
    await expect(run({ do: "send", id: "sh", text: "rm -rf ~" })).rejects.toThrow(/A shell session isn't guarded/);
    await expect(run({ do: "send", id: "ex", text: "hi" })).rejects.toThrow(/outside Brainstead/);
    expect(called("term_write")).toHaveLength(0);
    expect(await run({ do: "send", id: "cx", text: "go on" })).toMatch(/Typed into session cx/);
  });

  it("types into a session and presses Enter, and stops one", async () => {
    answers.term_sessions = () => [session(), session({ id: "t2", state: "exited" }), session({ id: "t3", external: true })];
    expect(await run({ do: "send", id: "t1", text: "/mcp__brainstead__wrap_up" })).toMatch(/Typed into session t1/);
    expect(called("term_write")[0]).toEqual({ id: "t1", data: "/mcp__brainstead__wrap_up\r" });
    await expect(run({ do: "send", id: "t2", text: "hi" })).rejects.toThrow(/has ended/);
    await expect(run({ do: "send", id: "t3", text: "hi" })).rejects.toThrow(/outside Brainstead/);
    await expect(run({ do: "send", id: "t1", text: "hi", unattended: true })).rejects.toThrow(/nobody is watching/);
    await expect(run({ do: "send", id: "nope", text: "hi" })).rejects.toThrow(/no session nope/);
    expect(await run({ do: "stop_session", id: "t1" })).toMatch(/Stopped session t1/);
    expect(called("term_stop")).toEqual([{ id: "t1" }]);
    expect(await run({ do: "stop_session", id: "t2" })).toMatch(/ended already/);
  });

  it("refuses what it doesn't do", async () => {
    await expect(run({ do: "open" })).rejects.toThrow(/do is folders, sessions/);
  });
});

describe("a project's folders", () => {
  beforeEach(() => {
    calls.length = 0;
    answers = {};
  });

  it("reads a list, or one string, without blanks or repeats", () => {
    expect(foldersIn(["~/repos/orbit-app", " orbit-app ", "", "~/repos/orbit-app"])).toEqual(["~/repos/orbit-app", "orbit-app"]);
    expect(foldersIn("hub-platform")).toEqual(["hub-platform"]);
    expect(foldersIn(null)).toEqual([]);
    expect(() => foldersIn([3])).toThrow(/list of folder paths/);
  });

  it("sets and clears them through Changes, and passes them to a new project", async () => {
    answers.projects_list = () => [{ name: "Orbit App launch", path: "Project. Orbit App launch.md", status: "active" }];
    answers.changes_submit_many = (a) =>
      (a!.changes as { page: string }[]).map((c) => ({ id: "c", applied: true, page: c.page, flags: [], message: "Changed." }));
    await runAction({ id: "1", action: "project.update", args: { project: "Orbit App launch", folders: ["~/repos/orbit-app"] } });
    await runAction({ id: "1", action: "project.update", args: { project: "Orbit App launch", folders: [] } });
    const sets = called("changes_submit_many").map((a) => (a!.changes as { instruction: unknown }[])[0].instruction);
    expect(sets).toEqual([
      { op: "properties", set: [["folders", ["~/repos/orbit-app"]]] },
      { op: "properties", set: [["folders", null]] },
    ]);
    answers.change_project_note = (a) => ({ path: `Project. ${a!.name}.md`, content: "---\nstatus: active\n---\n" });
    await runAction({ id: "1", action: "project.create", args: { name: "Hub platform", folders: ["hub-platform"] } });
    expect(called("change_project_note")[0]).toMatchObject({ name: "Hub platform", folders: ["hub-platform"] });
  });
});
