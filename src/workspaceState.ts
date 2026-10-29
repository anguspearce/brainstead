// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Workspace's state: the folders sessions start in (roots and their repos, pinned, recent),
// the terminal sessions running this run of the app, the folder and session showing, and each
// folder's past Claude Code sessions. Rust runs the sessions (src-tauri/src/terminal.rs) and keeps
// the folder settings (src-tauri/src/workspace.rs); this follows its events and hands each
// session's output to its terminal on the screen (src/Workspace.tsx) while one is open.

import { api, Settings } from "./api";
import { place } from "./nav";
import { isMac } from "./platform";
import { settings, Store } from "./store";
import { Folder, PastSession, Program, SessionInfo, StartRequest, workspaceApi } from "./workspaceApi";

export const PROGRAM_LABEL: Record<Program, string> = { claude: "Claude Code", codex: "Codex", shell: "Shell" };

/** What Wrap up types into a Claude Code session: Brainstead's MCP prompt, then Enter. */
export const WRAP_UP = "/mcp__brainstead__wrap_up\r";

export interface WorkspaceState {
  /** Every folder Rust lists; null until the first read. */
  folders: Folder[] | null;
  /** Why the folders couldn't be read. */
  error: string | null;
  sessions: SessionInfo[];
  /** The folder showing, by path. */
  folder: string | null;
  /** The session tab showing, by id; null shows the folder. */
  active: string | null;
  /** Each folder's past Claude Code sessions, newest first, once read. */
  past: Record<string, PastSession[]>;
}

export const workspace = new Store<WorkspaceState>({ folders: null, error: null, sessions: [], folder: null, active: null, past: {} });

const patch = (p: Partial<WorkspaceState>) => workspace.set({ ...workspace.get(), ...p });

// --- pure rules (tested) ---------------------------------------------------------------------

/** A session's change merged in: replaced by id, or added at the end. */
export function mergeSession(list: SessionInfo[], s: SessionInfo): SessionInfo[] {
  const i = list.findIndex((x) => x.id === s.id);
  if (i < 0) return [...list, s];
  const next = [...list];
  next[i] = s;
  return next;
}

/** A session's end: exited with its code. */
export function exitSession(list: SessionInfo[], id: string, code: number | null): SessionInfo[] {
  return list.map((s) => (s.id === id ? { ...s, state: "exited", exitCode: code ?? undefined } : s));
}

/** How many sessions are asking you something: the sidebar's count. */
export const waitingCount = (list: SessionInfo[]) => list.filter((s) => s.state === "waiting").length;

/** A session's state in words, for its tab's tooltip and the MCP tool. */
/** Whether a session's state is followed: Claude Code's hooks tell Brainstead when it works, waits
 *  and finishes. A shell or Codex has no hooks, so it's only running or ended. */
export const followsState = (s: SessionInfo) => s.program === "claude";

export function stateLabel(s: SessionInfo): string {
  if (!followsState(s) && s.state !== "exited") return "Running";
  switch (s.state) {
    case "starting":
      return "Starting";
    case "working":
      return "Working";
    case "waiting":
      return "Waiting for you";
    case "idle":
      return s.unseen ? "Finished, not seen yet" : "Finished its turn";
    case "exited":
      return s.exitCode === undefined || s.exitCode === null ? "Ended" : `Ended with code ${s.exitCode}`;
  }
}

/** The left column's groups: each root with its repos, then pinned folders, then recent ones
 *  (newest first, not those already pinned). A repo shows under its root and, when pinned, in
 *  Pinned as well. */
export function groupFolders(folders: Folder[]): {
  roots: { root: Folder; repos: Folder[] }[];
  pinned: Folder[];
  recent: Folder[];
} {
  const byName = (a: Folder, b: Folder) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
  const roots = folders
    .filter((f) => f.kind === "root")
    .map((root) => ({ root, repos: folders.filter((f) => f.kind === "repo" && f.root === root.path).sort(byName) }));
  const pinned = folders.filter((f) => f.pinned).sort(byName);
  const recent = folders
    .filter((f) => f.recent && !f.pinned)
    .sort((a, b) => (b.recent ?? 0) - (a.recent ?? 0))
    .slice(0, 20);
  return { roots, pinned, recent };
}

/** A path with the home folder written `~`, as the screen shows it. */
export function tilde(path: string, home: string | null): string {
  if (!home) return path;
  const h = home.replace(/[\\/]+$/, "");
  if (path === h) return "~";
  return path.startsWith(`${h}/`) || path.startsWith(`${h}\\`) ? `~${path.slice(h.length)}` : path;
}

/** `~/repos` and the like made absolute, for a path the user or an assistant typed. */
export function expandHome(path: string, home: string | null): string {
  const p = path.trim();
  if (!home || !(p === "~" || p.startsWith("~/") || p.startsWith("~\\"))) return p;
  return `${home.replace(/[\\/]+$/, "")}${p.slice(1)}`;
}

/** A vault file's vault-relative path, when an absolute path is inside the vault; else null. */
export function inVault(path: string, vault: string | null): string | null {
  if (!vault) return null;
  const v = vault.replace(/[\\/]+$/, "");
  if (!path.startsWith(`${v}/`) && !path.startsWith(`${v}\\`)) return null;
  return path.slice(v.length + 1).replace(/\\/g, "/");
}

/** Output as Rust sends it (base64) as the bytes xterm writes raw. */
export function decodeOutput(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/**
 * The part of a chunk of output (`bytes`, which follow `offset` bytes the session wrote before them)
 * that comes after `end`, the byte count a redraw from Rust's buffer ended at: all of it when it's
 * newer, none when the redraw had it already, and the rest when it straddles `end`.
 */
export function pastEnd(bytes: Uint8Array, offset: number, end: number): Uint8Array {
  if (offset >= end) return bytes;
  if (offset + bytes.length <= end) return bytes.subarray(0, 0);
  return bytes.subarray(end - offset);
}

/**
 * Whether a key pressed in a terminal belongs to it rather than to the app: every key except the
 * app's own command chords (⌘ on macOS; Ctrl+Shift elsewhere, since Ctrl alone is the terminal's).
 * So `?`, Esc and Ctrl+C go to the program, and ⌘K still opens the palette.
 */
export function terminalKeepsKey(e: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; altKey: boolean }, mac = isMac): boolean {
  if (mac) return !e.metaKey;
  return !(e.ctrlKey && e.shiftKey && !e.altKey);
}

/** Wrap up and the other Claude Code buttons work in a Claude Code session that's still running here. */
export const canType = (s: SessionInfo) => !s.external && s.state !== "exited";

// --- the store's life ------------------------------------------------------------------------

/** Output for a session's terminal on the screen; while none is open, Rust's buffer keeps it. */
const sinks = new Map<string, (bytes: Uint8Array, offset: number) => void>();

/** Gives a session's output to `f`, with the byte count before each chunk, until the returned
 *  function is called. */
export function attachOutput(id: string, f: (bytes: Uint8Array, offset: number) => void): () => void {
  sinks.set(id, f);
  return () => {
    if (sinks.get(id) === f) sinks.delete(id);
  };
}

/** Set by the screenshot scene: the sample folders and sessions stand, and nothing is asked of Rust. */
let scene = false;
let started = false;

/** Follows Rust's sessions from app start (the sidebar's count), once. */
export function startWorkspace() {
  if (started) return;
  started = true;
  if (scene) return;
  void workspaceApi
    .sessions()
    .then((l) => patch({ sessions: l.reduce(mergeSession, workspace.get().sessions) }))
    .catch(() => {});
  void workspaceApi
    .onState((s) => {
      // A session you're looking at is seen as soon as it changes.
      const shown = seeing(s.id);
      patch({ sessions: mergeSession(workspace.get().sessions, shown ? { ...s, unseen: false } : s) });
      if (shown && s.unseen) void workspaceApi.seen(s.id).catch(() => {});
    })
    .catch(() => {});
  void workspaceApi.onExit(({ id, code }) => patch({ sessions: exitSession(workspace.get().sessions, id, code) })).catch(() => {});
  void workspaceApi
    .onOutput(({ id, data, offset }) => {
      const f = sinks.get(id);
      if (f) f(decodeOutput(data), offset);
    })
    .catch(() => {});
}

/** The window is in front of you and showing that session. */
function seeing(id: string) {
  return (
    place.get().place.screen === "workspace" &&
    workspace.get().active === id &&
    document.visibilityState !== "hidden" &&
    document.hasFocus()
  );
}

/** Rust keeps the folder settings: the window takes its copy, so its next save doesn't put back
 *  the old ones. */
async function adoptSettings() {
  if (scene) return;
  const s = await api.settingsRead().catch(() => null);
  if (!s) return;
  const keys: (keyof Settings)[] = ["workspaceRoots", "workspacePinned", "workspaceRecent", "workspaceShell"];
  const cur = settings.get();
  if (keys.every((k) => JSON.stringify(cur[k]) === JSON.stringify(s[k]))) return;
  settings.update(Object.fromEntries(keys.map((k) => [k, s[k]])) as Partial<Settings>);
}

/** Reads the folders again: roots' repos, branches, linked projects. */
export async function refreshFolders(): Promise<Folder[]> {
  if (scene) return workspace.get().folders ?? [];
  try {
    const folders = await workspaceApi.folders();
    patch({ folders, error: null });
    return folders;
  } catch (e) {
    patch({ error: String(e), folders: workspace.get().folders ?? [] });
    throw e;
  }
}

/** A folder whose git repos are listed. */
export async function addRoot(path: string) {
  await workspaceApi.addRoot(path);
  await adoptSettings();
  await refreshFolders();
}

export async function removeRoot(path: string) {
  await workspaceApi.removeRoot(path);
  await adoptSettings();
  await refreshFolders();
}

export async function pinFolder(path: string, pinned: boolean) {
  await workspaceApi.pin(path, pinned);
  await adoptSettings();
  await refreshFolders();
}

/** Shows a folder, and reads its past Claude Code sessions. */
export function selectFolder(path: string) {
  patch({ folder: path, active: null });
  void loadPast(path).catch(() => {});
}

export async function loadPast(folder: string): Promise<PastSession[]> {
  if (scene) return workspace.get().past[folder] ?? [];
  const list = await workspaceApi.pastSessions(folder);
  patch({ past: { ...workspace.get().past, [folder]: list } });
  return list;
}

/** Shows a session's tab, which sees it. */
export function selectSession(id: string | null) {
  const s = workspace.get().sessions.find((x) => x.id === id);
  patch({
    active: id,
    folder: s?.folder ?? workspace.get().folder,
    sessions: workspace.get().sessions.map((x) => (x.id === id ? { ...x, unseen: false } : x)),
  });
  if (s?.unseen && !scene) void workspaceApi.seen(s.id).catch(() => {});
}

/** Starts a session and shows its tab. Rust refuses a folder in the vault or one that's gone. */
export async function startSession(req: StartRequest): Promise<SessionInfo> {
  const s = await workspaceApi.start(req);
  patch({ sessions: mergeSession(workspace.get().sessions, s), active: s.id, folder: s.folder });
  void adoptSettings();
  void refreshFolders().catch(() => {});
  return s;
}

export async function stopSession(id: string) {
  await workspaceApi.stop(id);
}

/** Closes a session's tab: stopped first if it's still running. */
export async function closeSession(id: string) {
  await workspaceApi.close(id);
  const st = workspace.get();
  const sessions = st.sessions.filter((s) => s.id !== id);
  patch({ sessions, active: st.active === id ? null : st.active });
}

/** An ended session started again in its place: a Claude Code one picks up where it left off. */
export async function restartSession(id: string): Promise<SessionInfo> {
  const old = workspace.get().sessions.find((s) => s.id === id);
  if (!old) throw new Error("That session isn't open any more.");
  const s = await startSession({
    folder: old.folder,
    program: old.program,
    resume: old.program === "claude" ? old.claudeSessionId : undefined,
  });
  await workspaceApi.close(id).catch(() => {});
  patch({ sessions: workspace.get().sessions.filter((x) => x.id !== id) });
  return s;
}

/** Types text into a session and presses Enter. */
export async function sendText(id: string, text: string) {
  await workspaceApi.write(id, `${text}\r`);
}

/** Wrap up: Brainstead's wrap_up prompt typed into a Claude Code session. */
export async function wrapUp(id: string) {
  await workspaceApi.write(id, WRAP_UP);
}

// --- the screenshot scene --------------------------------------------------------------------

/** The `workspace` scene's sample output, by session id. */
export const sceneOutput = new Map<string, string>();

/** The scene's folders under ~/repos and two sessions, one waiting for you (tools/screenshots). */
export function workspaceScene(home = "/Users/maya") {
  scene = true;
  const root = `${home}/repos`;
  const orbit = { path: "Project. Orbit App launch.md", name: "Orbit App launch" };
  const hub = { path: "Project. Hub migration.md", name: "Hub migration" };
  const now = Date.now();
  const folders: Folder[] = [
    { path: root, name: "repos", kind: "root", pinned: false, insideVault: false },
    {
      path: `${root}/orbit-app`,
      name: "orbit-app",
      kind: "repo",
      root,
      branch: "launch-date",
      dirty: true,
      pinned: true,
      recent: now - 600_000,
      project: orbit,
      insideVault: false,
    },
    {
      path: `${root}/hub-platform`,
      name: "hub-platform",
      kind: "repo",
      root,
      branch: "main",
      pinned: false,
      recent: now - 3_600_000,
      project: hub,
      insideVault: false,
    },
    { path: `${root}/design-notes`, name: "design-notes", kind: "repo", root, branch: "main", pinned: false, insideVault: false },
    { path: `${home}/Notes`, name: "Notes", kind: "folder", pinned: false, recent: now - 86_400_000, insideVault: true },
  ];
  const sessions: SessionInfo[] = [
    {
      id: "s1",
      folder: `${root}/orbit-app`,
      name: "orbit-app",
      program: "claude",
      title: "Move the launch banner to 28 November",
      state: "waiting",
      started: now - 900_000,
      claudeSessionId: "c1",
      project: orbit,
      filesTouched: [`${root}/orbit-app/src/banner.tsx`, `${root}/orbit-app/src/dates.ts`],
      unseen: true,
      external: false,
    },
    {
      id: "s2",
      folder: `${root}/hub-platform`,
      name: "hub-platform",
      program: "claude",
      title: "Why does the nightly import time out?",
      state: "working",
      started: now - 300_000,
      claudeSessionId: "c2",
      project: hub,
      filesTouched: [],
      unseen: false,
      external: false,
    },
  ];
  const past: PastSession[] = [
    {
      id: "p1",
      folder: `${root}/orbit-app`,
      started: now - 2 * 86_400_000,
      lastActive: now - 2 * 86_400_000 + 1_800_000,
      firstMessage: "Add the steerco's new date to the release checklist",
      program: "claude",
    },
    {
      id: "p2",
      folder: `${root}/orbit-app`,
      started: now - 5 * 86_400_000,
      lastActive: now - 5 * 86_400_000 + 900_000,
      firstMessage: "Tidy the onboarding copy Lena flagged",
      program: "claude",
    },
  ];
  sceneOutput.set(
    "s1",
    [
      "\x1b[1m> Move the launch banner to 28 November\x1b[0m",
      "",
      "\x1b[2m● Read src/banner.tsx\x1b[0m",
      "\x1b[2m● Edit src/banner.tsx, src/dates.ts\x1b[0m",
      "",
      "The banner now reads 28 November, and the countdown uses the same date.",
      "",
      "\x1b[1mDo you want me to run the tests?\x1b[0m",
      "  \x1b[36m❯ 1. Yes\x1b[0m",
      "    2. No, I'll run them myself",
      "",
    ].join("\r\n"),
  );
  sceneOutput.set("s2", "\x1b[1m> Why does the nightly import time out?\x1b[0m\r\n\r\n\x1b[2m● Searching for import jobs…\x1b[0m\r\n");
  workspace.set({ folders, error: null, sessions, folder: `${root}/orbit-app`, active: "s1", past: { [`${root}/orbit-app`]: past } });
}

/** True while the screenshot scene's sample stands in for Rust. */
export const inScene = () => scene;
