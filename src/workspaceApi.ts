// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Workspace's calls into Rust (src-tauri/src/workspace.rs, terminal.rs) and its events.

import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

/** The project a folder belongs to: its note's path and name ("Project. Orbit App.md", "Orbit App"). */
export interface ProjectLink {
  path: string;
  name: string;
}

/** A place to start a session: a root such as ~/repos, a git repo under one, or any other folder. */
export interface Folder {
  path: string;
  name: string;
  kind: "root" | "repo" | "folder";
  /** The root a repo was found under. */
  root?: string;
  branch?: string;
  /** Uncommitted changes in a repo. */
  dirty?: boolean;
  pinned: boolean;
  /** When a session last started here, ms since the epoch. */
  recent?: number;
  project?: ProjectLink;
  /** In the vault (or the vault itself): sessions can't start there. */
  insideVault: boolean;
}

/** A Claude Code session from ~/.claude/projects whose folder is this one. */
export interface PastSession {
  id: string;
  folder: string;
  started: number;
  lastActive: number;
  firstMessage: string;
  program: "claude";
}

export type Program = "claude" | "codex" | "shell";

export interface StartRequest {
  folder: string;
  program: Program;
  /** The CLI's own session id to resume. */
  resume?: string;
  /** A first message, for claude and codex. */
  prompt?: string;
  model?: string;
  /** Start in a git worktree of that name (claude --worktree). */
  worktree?: string;
}

/** working: answering or running a tool; waiting: asking you something; idle: finished its turn. */
export type SessionState = "starting" | "working" | "waiting" | "idle" | "exited";

export interface SessionInfo {
  id: string;
  folder: string;
  name: string;
  program: Program;
  title?: string;
  state: SessionState;
  exitCode?: number;
  started: number;
  claudeSessionId?: string;
  project?: ProjectLink;
  filesTouched: string[];
  /** Finished or waiting since you last looked at it. */
  unseen: boolean;
  /** Running in a terminal outside Brainstead, seen through its hooks: state only, no output. */
  external: boolean;
}

/** A piece of a session's output: base64 bytes, and how many bytes the session wrote before them. */
export interface OutputChunk {
  id: string;
  data: string;
  offset: number;
}

/** What Rust kept of a session's output: the tail, base64, and the total byte count it ends at. */
export interface OutputTail {
  data: string;
  end: number;
}

export const workspaceApi = {
  folders: () => invoke<Folder[]>("workspace_folders"),
  addRoot: (path: string) => invoke<void>("workspace_add_root", { path }),
  removeRoot: (path: string) => invoke<void>("workspace_remove_root", { path }),
  pin: (path: string, pinned: boolean) => invoke<void>("workspace_pin", { path, pinned }),
  pastSessions: (folder: string) => invoke<PastSession[]>("workspace_past_sessions", { folder }),
  start: (req: StartRequest) => invoke<SessionInfo>("term_start", { req }),
  write: (id: string, data: string) => invoke<void>("term_write", { id, data }),
  resize: (id: string, cols: number, rows: number) => invoke<void>("term_resize", { id, cols, rows }),
  stop: (id: string) => invoke<void>("term_stop", { id }),
  close: (id: string) => invoke<void>("term_close", { id }),
  sessions: () => invoke<SessionInfo[]>("term_sessions"),
  /** The last 512 KB of a session's output, base64, and the count of all its bytes it ends at. */
  buffer: (id: string) => invoke<OutputTail>("term_buffer", { id }),
  seen: (id: string) => invoke<void>("term_seen", { id }),
  externalCommand: (folder: string, program: Program, resume?: string) =>
    invoke<string>("term_external_command", { folder, program, resume: resume ?? null }),
  openExternal: (folder: string, program: Program, resume?: string) =>
    invoke<void>("term_open_external", { folder, program, resume: resume ?? null }),
  /** A session's output as it comes. */
  onOutput: (f: (o: OutputChunk) => void): Promise<UnlistenFn> => listen<OutputChunk>("term-output", (e) => f(e.payload)),
  onState: (f: (s: SessionInfo) => void): Promise<UnlistenFn> => listen<SessionInfo>("term-state", (e) => f(e.payload)),
  onExit: (f: (o: { id: string; code: number | null }) => void): Promise<UnlistenFn> =>
    listen<{ id: string; code: number | null }>("term-exit", (e) => f(e.payload)),
};
