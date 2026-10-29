// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Workspace: your own Claude Code, Codex or shell sessions in your repos and folders, each a
// real terminal (xterm.js) in a tab. On the left the folders (roots and their repos, pinned,
// recent); a folder shows its sessions, its past Claude Code sessions and New session; a session
// shows its terminal beside its project, the files it changed and Wrap up. The sessions can't
// change the vault except through Brainstead's tools (src-tauri/src/terminal.rs's guard). The
// state is in src/workspaceState.ts.

import "@xterm/xterm/css/xterm.css";
import { useEffect, useMemo, useRef, useState } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { ITerminalOptions, Terminal } from "@xterm/xterm";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "./api";
import { projectTasks, useProjects } from "./gtd";
import { Icon } from "./icons";
import { nav, openDoc } from "./nav";
import { dragRegion, terminalApp } from "./platform";
import { settings, useStore } from "./store";
import { reportEditError, useAllTasks } from "./taskModel";
import { toast } from "./Toast";
import { AskButton, askLink, HistButtons } from "./TopBar";
import { HelpButton } from "./help/HelpDrawer";
import { ago, Seg } from "./ui";
import { Folder, PastSession, Program, SessionInfo, workspaceApi } from "./workspaceApi";
import {
  addRoot,
  attachOutput,
  canType,
  closeSession,
  decodeOutput,
  followsState,
  groupFolders,
  inScene,
  inVault,
  pastEnd,
  pinFolder,
  PROGRAM_LABEL,
  refreshFolders,
  removeRoot,
  restartSession,
  sceneOutput,
  selectFolder,
  selectSession,
  startSession,
  stateLabel,
  stopSession,
  terminalKeepsKey,
  tilde,
  workspace,
  wrapUp,
} from "./workspaceState";

const VAULT_REASON = "That folder is in your vault. The vault changes only through Brainstead, so use Ask there.";

/** The home folder, for writing paths `~/…`. */
function useHome(): string | null {
  const [home, setHome] = useState<string | null>(inScene() ? "/Users/maya" : null);
  useEffect(() => {
    if (inScene()) return;
    let live = true;
    void import("@tauri-apps/api/path")
      .then((p) => p.homeDir())
      .then((h) => live && setHome(h))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  return home;
}

export function WorkspaceScreen() {
  const s = useStore(workspace);
  const home = useHome();
  useEffect(() => {
    void refreshFolders().catch(() => {});
  }, []);
  const active = s.sessions.find((x) => x.id === s.active) ?? null;
  const folder = s.folders?.find((f) => f.path === s.folder) ?? null;
  // Seeing a session reads it: one that finishes while the window is closed stays unseen till you're back.
  useEffect(() => {
    if (!active?.unseen) return;
    const id = active.id;
    const read = () => {
      if (document.visibilityState !== "hidden" && document.hasFocus()) selectSession(id);
    };
    read();
    window.addEventListener("focus", read);
    document.addEventListener("visibilitychange", read);
    return () => {
      window.removeEventListener("focus", read);
      document.removeEventListener("visibilitychange", read);
    };
  }, [active?.id, active?.unseen]);
  const where = active ? { name: active.name, path: active.folder, project: active.project } : folder;
  return (
    <main className="main workspace">
      <div className="top drag" {...dragRegion()}>
        <HistButtons />
        <div className="crumbs">
          <b>Workspace</b>
        </div>
        <div className="asktabs" role="tablist" aria-label="Sessions">
          {s.sessions.map((x) => (
            <SessionTab key={x.id} s={x} on={x.id === s.active} />
          ))}
        </div>
        <div className="sp" />
        <AskButton
          ask={
            where
              ? {
                  about: "this folder",
                  prompt: `About the folder ${tilde(where.path, home)}${where.project ? ` and ${askLink(where.project.path)}` : ""}: `,
                }
              : { about: "the Workspace", prompt: "About the Workspace: " }
          }
        />
        <HelpButton />
      </div>
      <div className="body wsplit">
        <FolderColumn home={home} />
        <div className="wsmain">
          {/* Every terminal stays alive while another tab shows: hidden, not closed. */}
          {s.sessions
            .filter((x) => !x.external)
            .map((x) => (
              <TerminalView key={x.id} s={x} shown={x.id === s.active} />
            ))}
          {active?.external && <ExternalView s={active} />}
          {!active && (folder ? <FolderView f={folder} home={home} /> : <NoFolder hasFolders={!!s.folders?.length} />)}
        </div>
        {active && <SidePanel s={active} />}
      </div>
    </main>
  );
}

/** A session's tab: its folder, and a dot for how it stands. */
function SessionTab({ s, on }: { s: SessionInfo; on: boolean }) {
  const label = s.title ? `${s.name}: ${s.title}` : s.name;
  return (
    <span className={`asktab ${on ? "on" : ""}`}>
      <button
        type="button"
        role="tab"
        aria-selected={on}
        onClick={() => selectSession(s.id)}
        title={`${PROGRAM_LABEL[s.program]} in ${s.folder}${s.title ? `: ${s.title}` : ""}. ${stateLabel(s)}.`}
      >
        <StateDot s={s} />
        <span className="t">{label}</span>
      </button>
      <button
        type="button"
        className="ibtn xs"
        aria-label={`Close ${s.name}`}
        title={s.state === "exited" || s.external ? "Close this tab" : "Stop this session and close its tab"}
        onClick={() => void closeSession(s.id).catch(reportEditError)}
      >
        <Icon name="x" size={11} />
      </button>
    </span>
  );
}

/** Working: a spinner; waiting for you: the accent dot; finished and not seen: a dot; ended: a cross.
 *  A shell or Codex session, which has no hooks to say how it stands, shows a quiet running ring. */
function StateDot({ s }: { s: SessionInfo }) {
  if (s.state === "exited") return <Icon name="stop" size={11} className="faint" />;
  if (!followsState(s)) return <span className="wslive" aria-label={stateLabel(s)} />;
  if (s.state === "working" || s.state === "starting") return <span className="spin" aria-label={stateLabel(s)} />;
  if (s.state === "waiting") return <span className="dot" aria-label={stateLabel(s)} />;
  if (s.state === "idle" && s.unseen) return <span className="dot idle" aria-label={stateLabel(s)} />;
  return null;
}

// --- the folders -----------------------------------------------------------------------------

function FolderColumn({ home }: { home: string | null }) {
  const s = useStore(workspace);
  const g = useMemo(() => groupFolders(s.folders ?? []), [s.folders]);
  const add = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false, title: "Add a folder to the Workspace" });
    if (typeof picked === "string") await addRoot(picked).catch(reportEditError);
  };
  const empty = s.folders !== null && !g.roots.length && !g.pinned.length && !g.recent.length;
  return (
    <nav className="tagtree wsfolders" aria-label="Folders">
      {s.error && <p className="err small pad">{s.error}</p>}
      {empty && (
        <div className="wsempty">
          <p className="faint">Add the folder your repos are in, and each repo shows here to start a session in.</p>
          {home && (
            <button
              type="button"
              className="btn pri"
              title={`List the git repos in ${home}/repos`}
              onClick={() => void addRoot(`${home.replace(/[\\/]+$/, "")}/repos`).catch(reportEditError)}
            >
              <Icon name="plus" size={14} />
              Add ~/repos
            </button>
          )}
        </div>
      )}
      {g.roots.map(({ root, repos }) => (
        <div key={root.path} className="wsroot">
          <div className="sect static wsrhead">
            <span className="ell" title={root.path}>
              {tilde(root.path, home)}
            </span>
            <button
              type="button"
              className="ibtn xs"
              aria-label={`Remove ${root.name}`}
              title={`Stop listing the repos in ${tilde(root.path, home)}; nothing on disk changes`}
              onClick={() => void removeRoot(root.path).catch(reportEditError)}
            >
              <Icon name="x" size={11} />
            </button>
          </div>
          <FolderRow f={root} home={home} />
          {repos.map((f) => (
            <FolderRow key={f.path} f={f} home={home} indent />
          ))}
        </div>
      ))}
      {!!g.pinned.length && (
        <>
          <div className="sect static">Pinned</div>
          {g.pinned.map((f) => (
            <FolderRow key={f.path} f={f} home={home} />
          ))}
        </>
      )}
      {!!g.recent.length && (
        <>
          <div className="sect static">Recent</div>
          {g.recent.map((f) => (
            <FolderRow key={f.path} f={f} home={home} />
          ))}
        </>
      )}
      <button type="button" className="tnode wsadd" title="Pick a folder; its git repos are listed here" onClick={() => void add()}>
        <Icon name="plus" size={14} />
        Add folder…
      </button>
    </nav>
  );
}

function FolderRow({ f, home, indent }: { f: Folder; home: string | null; indent?: boolean }) {
  const s = useStore(workspace);
  const on = !s.active && s.folder === f.path;
  const waiting = s.sessions.some((x) => x.folder === f.path && x.state === "waiting");
  const running = s.sessions.some((x) => x.folder === f.path && x.state !== "exited");
  const tip = f.insideVault
    ? `${tilde(f.path, home)}. ${VAULT_REASON}`
    : `${tilde(f.path, home)}${f.branch ? ` on ${f.branch}${f.dirty ? ", with uncommitted changes" : ""}` : ""}${f.project ? `. Project: ${f.project.name}` : ""}`;
  return (
    <button
      type="button"
      className={`tnode wsrow ${on ? "on" : ""} ${indent ? "ind" : ""} ${f.insideVault ? "gone" : ""}`}
      aria-current={on ? "true" : undefined}
      title={tip}
      onClick={() => selectFolder(f.path)}
    >
      <Icon name={f.kind === "repo" ? "folder" : f.kind === "root" ? "project" : "folder"} size={14} />
      <span className="ell wsname">{f.kind === "root" ? "This folder" : f.name}</span>
      {f.dirty && <span className="wsdirty" aria-label="Uncommitted changes" />}
      {f.project && <span className="faint ell wsproj">{f.project.name}</span>}
      {waiting ? (
        <span className="dot" aria-label="A session is waiting for you" />
      ) : (
        running && <span className="wslive" aria-label="A session is running" />
      )}
    </button>
  );
}

function NoFolder({ hasFolders }: { hasFolders: boolean }) {
  return (
    <div className="body center">
      <div className="soon">
        <Icon name="terminal" size={28} />
        <h1 className="h2">Workspace</h1>
        <p className="muted">
          {hasFolders
            ? "Pick a folder on the left to start Claude Code, Codex or a shell there, or to pick up a past session."
            : "Your own Claude Code, Codex and shell sessions, in your repos. They can't change the vault except through Brainstead, so every change is in Changes with Revert."}
        </p>
      </div>
    </div>
  );
}

/** A folder: its sessions here, New session, and its past Claude Code sessions to resume. */
function FolderView({ f, home }: { f: Folder; home: string | null }) {
  const s = useStore(workspace);
  const live = s.sessions.filter((x) => x.folder === f.path);
  const past = s.past[f.path];
  return (
    <div className="body wsfolder">
      <div className="wshead">
        <div className="grow">
          <h1 className="h2">{f.kind === "root" ? tilde(f.path, home) : f.name}</h1>
          <div className="faint small row">
            <span className="mono">{tilde(f.path, home)}</span>
            {f.branch && (
              <span className="chip">
                {f.branch}
                {f.dirty ? " · uncommitted changes" : ""}
              </span>
            )}
            {f.project && (
              <button
                type="button"
                className="chip green"
                title={`Open ${f.project.name} on Projects`}
                onClick={() => nav.go({ screen: "projects", path: f.project!.path })}
              >
                <Icon name="project" size={12} />
                {f.project.name}
              </button>
            )}
          </div>
        </div>
        <button
          type="button"
          className="btn"
          title={f.pinned ? "Take this folder out of Pinned" : "Keep this folder at the top, under Pinned"}
          onClick={() => void pinFolder(f.path, !f.pinned).catch(reportEditError)}
        >
          <Icon name="pin" size={14} />
          {f.pinned ? "Unpin" : "Pin"}
        </button>
      </div>
      {f.insideVault ? (
        <div className="robanner" role="status">
          <Icon name="lock" size={14} />
          <span className="grow">{VAULT_REASON}</span>
          <button
            type="button"
            className="btn sm"
            title="Go to Ask, which works on the vault through Brainstead"
            onClick={() => nav.go("ask")}
          >
            Go to Ask
          </button>
        </div>
      ) : (
        <NewSession f={f} />
      )}
      {!!live.length && (
        <section>
          <h2 className="h3">Sessions here</h2>
          <div className="card wslist">
            {live.map((x) => (
              <button key={x.id} type="button" className="wsitem" title="Show this session" onClick={() => selectSession(x.id)}>
                <StateDot s={x} />
                <span className="grow ell">{x.title ?? PROGRAM_LABEL[x.program]}</span>
                <span className="faint small">
                  {PROGRAM_LABEL[x.program]} · {stateLabel(x)}
                </span>
              </button>
            ))}
          </div>
        </section>
      )}
      {!f.insideVault && (
        <section>
          <h2 className="h3">Past Claude Code sessions</h2>
          {!past ? (
            <p className="faint small">Looking…</p>
          ) : !past.length ? (
            <p className="faint small">None in this folder yet.</p>
          ) : (
            <div className="card wslist">
              {past.map((p) => (
                <PastRow key={p.id} p={p} />
              ))}
            </div>
          )}
        </section>
      )}
    </div>
  );
}

function PastRow({ p }: { p: PastSession }) {
  return (
    <div className="wsitem">
      <span className="grow ell" title={p.firstMessage}>
        {p.firstMessage || "(no message)"}
      </span>
      <span className="faint small">{ago(p.lastActive)}</span>
      <button
        type="button"
        className="btn sm"
        title="Pick this session up here, where it left off (claude --resume)"
        onClick={() => void startSession({ folder: p.folder, program: "claude", resume: p.id }).catch(reportEditError)}
      >
        Resume
      </button>
      <button
        type="button"
        className="btn sm ghost"
        title={`Pick this session up in ${terminalApp}, guarded the same way`}
        onClick={() => void workspaceApi.openExternal(p.folder, "claude", p.id).catch(reportEditError)}
      >
        Resume in your terminal
      </button>
    </div>
  );
}

function NewSession({ f }: { f: Folder }) {
  const [program, setProgram] = useState<Program>("claude");
  const [prompt, setPrompt] = useState("");
  const [worktree, setWorktree] = useState("");
  const [busy, setBusy] = useState(false);
  const start = () => {
    setBusy(true);
    startSession({
      folder: f.path,
      program,
      prompt: program !== "shell" && prompt.trim() ? prompt.trim() : undefined,
      worktree: program === "claude" && worktree.trim() ? worktree.trim() : undefined,
    })
      .then(() => {
        setPrompt("");
        setWorktree("");
      })
      .catch(reportEditError)
      .finally(() => setBusy(false));
  };
  return (
    <section className="card wsnew">
      <div className="row">
        <h2 className="h3 grow">New session</h2>
        <Seg
          label="Program"
          value={program}
          onChange={setProgram}
          options={[
            ["claude", "Claude Code", "Claude Code, signed in with your own account"],
            ["codex", "Codex", "Codex, signed in with your own account"],
            ["shell", "Shell", "Your shell, to type commands yourself"],
          ]}
        />
      </div>
      {program !== "shell" && (
        <label className="inp">
          <input
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && !busy && start()}
            placeholder="First message (optional)"
            aria-label="First message"
          />
        </label>
      )}
      {program === "claude" && (
        <label className="inp">
          <input
            value={worktree}
            onChange={(e) => setWorktree(e.target.value)}
            placeholder="Work in a new git worktree of this name (optional)"
            aria-label="Worktree name"
          />
        </label>
      )}
      <div className="row">
        <button type="button" className="btn pri" disabled={busy} title={`Start ${PROGRAM_LABEL[program]} in ${f.name}`} onClick={start}>
          {busy ? <span className="spin" /> : <Icon name="play" size={14} />}
          Start
        </button>
        <button
          type="button"
          className="btn ghost"
          title={`Start ${PROGRAM_LABEL[program]} in ${terminalApp} instead, guarded the same way`}
          onClick={() => void workspaceApi.openExternal(f.path, program).catch(reportEditError)}
        >
          Open in your terminal
        </button>
      </div>
    </section>
  );
}

// --- a session -------------------------------------------------------------------------------

/** The terminal's colours and font, from the design tokens as they are now (light or dark). */
function termTheme(): Pick<ITerminalOptions, "theme" | "fontFamily"> {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string) => css.getPropertyValue(name).trim();
  return {
    fontFamily: v("--mono") || "monospace",
    theme: {
      background: v("--surface"),
      foreground: v("--ink"),
      cursor: v("--accent"),
      cursorAccent: v("--surface"),
      selectionBackground: v("--accent-soft"),
      selectionForeground: v("--ink"),
      black: v("--ink2"),
      brightBlack: v("--ink3"),
      red: v("--red"),
      brightRed: v("--red"),
      green: v("--green"),
      brightGreen: v("--green"),
      yellow: v("--amber"),
      brightYellow: v("--amber"),
      blue: v("--accent-ink"),
      brightBlue: v("--accent-ink"),
      magenta: v("--pie-6"),
      brightMagenta: v("--pie-6"),
      cyan: v("--l-wiki"),
      brightCyan: v("--l-wiki"),
      white: v("--line2"),
      brightWhite: v("--ink"),
    },
  };
}

/** One session's terminal: made when the screen opens, redrawn from what Rust kept, and kept (hidden)
 *  while another tab shows. */
function TerminalView({ s, shown }: { s: SessionInfo; shown: boolean }) {
  const host = useRef<HTMLDivElement>(null);
  const term = useRef<{ t: Terminal; fit: FitAddon } | null>(null);
  const id = s.id;
  useEffect(() => {
    const el = host.current!;
    const t = new Terminal({ fontSize: 12.5, cursorBlink: true, allowProposedApi: false, scrollback: 10_000, ...termTheme() });
    const fit = new FitAddon();
    t.loadAddon(fit);
    t.loadAddon(new WebLinksAddon((_e, uri) => void openUrl(uri).catch(() => {})));
    t.open(el);
    term.current = { t, fit };
    // Output that arrives while the kept output is read waits, then follows it: only the bytes
    // after the ones the redraw wrote (`end`), so nothing is drawn twice.
    let live = true;
    let ready = false;
    let end = 0;
    const early: [Uint8Array, number][] = [];
    const put = (b: Uint8Array, offset: number) => {
      const rest = pastEnd(b, offset, end);
      if (rest.length) t.write(rest);
    };
    const detach = attachOutput(id, (b, offset) => (ready ? put(b, offset) : early.push([b, offset])));
    const redraw = inScene()
      ? Promise.resolve(sceneOutput.get(id) ?? "").then((x) => live && t.write(x))
      : workspaceApi.buffer(id).then((b) => {
          if (!live) return;
          if (b.data) t.write(decodeOutput(b.data));
          end = b.end;
        });
    void redraw
      .catch(() => {})
      .finally(() => {
        if (!live) return;
        ready = true;
        early.splice(0).forEach(([b, offset]) => put(b, offset));
      });
    const typed = t.onData((d) => {
      if (!inScene()) void workspaceApi.write(id, d).catch(() => {});
    });
    // Keys typed in the terminal are the program's, not the app's single-key shortcuts (`?`, Esc).
    const keys = (e: KeyboardEvent) => {
      if (terminalKeepsKey(e)) e.stopPropagation();
    };
    el.addEventListener("keydown", keys);
    // The colours follow light and dark.
    const retheme = () => {
      t.options.theme = termTheme().theme;
    };
    const watch = new MutationObserver(retheme);
    watch.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    const media = window.matchMedia?.("(prefers-color-scheme: dark)");
    media?.addEventListener?.("change", retheme);
    // Fitted to its pane, and Rust told the new size.
    let last = "";
    const resize = () => {
      if (!el.offsetWidth || !el.offsetHeight) return;
      try {
        fit.fit();
      } catch {
        return;
      }
      const size = `${t.cols}x${t.rows}`;
      if (size === last) return;
      last = size;
      if (!inScene()) void workspaceApi.resize(id, t.cols, t.rows).catch(() => {});
    };
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    resize();
    return () => {
      live = false;
      ro.disconnect();
      watch.disconnect();
      media?.removeEventListener?.("change", retheme);
      el.removeEventListener("keydown", keys);
      typed.dispose();
      detach();
      t.dispose();
      term.current = null;
    };
  }, [id]);
  useEffect(() => {
    if (!shown || !term.current) return;
    try {
      term.current.fit.fit();
    } catch {
      // Not laid out yet: the resize observer fits it once it is.
    }
    term.current.t.focus();
  }, [shown]);
  return (
    <div className="wsterm" hidden={!shown}>
      {s.state === "exited" && (
        <div className="robanner" role="status">
          <Icon name="stop" size={14} />
          <span className="grow">
            {stateLabel(s)}.{" "}
            {s.program === "claude" && s.claudeSessionId
              ? "Restart picks it up where it left off."
              : "Restart starts it again in the same folder."}
          </span>
          <button
            type="button"
            className="btn sm"
            title="Start this session again in its place"
            onClick={() => void restartSession(s.id).catch(reportEditError)}
          >
            <Icon name="refresh" size={13} />
            Restart
          </button>
        </div>
      )}
      <div className="wsxterm" ref={host} />
    </div>
  );
}

/** A session in a terminal outside Brainstead: its state only, from its hooks. */
function ExternalView({ s }: { s: SessionInfo }) {
  return (
    <div className="body center">
      <div className="soon">
        <Icon name="terminal" size={28} />
        <h1 className="h2">{s.title ?? s.name}</h1>
        <p className="muted">
          This session runs in a terminal outside Brainstead. You see how it stands and the files it changed here; type into it there.
        </p>
        <span className="chip">{stateLabel(s)}</span>
      </div>
    </div>
  );
}

/** Beside a session: its project and next actions, the files it changed, and what to do with it. */
function SidePanel({ s }: { s: SessionInfo }) {
  const projects = useProjects();
  const all = useAllTasks();
  const vault = useStore(settings).vaultPath;
  const p = s.project ? projects?.find((x) => x.path === s.project!.path) : undefined;
  const next = useMemo(() => (s.project && all ? projectTasks(all, s.project.path).next.slice(0, 5) : []), [all, s.project]);
  const copy = (text: string, what: string) =>
    void api
      .copyText(text)
      .then(() => toast(`Copied ${what}`, undefined, "ok"))
      .catch(reportEditError);
  const resume = s.program === "claude" ? s.claudeSessionId : undefined;
  const running = s.state !== "exited" && !s.external;
  return (
    <aside className="wsside" aria-label="This session">
      <section>
        <h2 className="h3">Project</h2>
        {s.project ? (
          <>
            <button
              type="button"
              className="tnode"
              title={`Open ${s.project.name} on Projects`}
              onClick={() => nav.go({ screen: "projects", path: s.project!.path })}
            >
              <Icon name="project" size={14} />
              <span className="ell">{s.project.name}</span>
              {p && <span className="n">{p.next} next</span>}
            </button>
            {!!next.length && (
              <ul className="wsnext">
                {next.map((t) => (
                  <li key={`${t.path}:${t.line}`} className="ell" title={t.text}>
                    {t.text.replace(/#[^\s#]+|\^[\w-]+/g, "").trim()}
                  </li>
                ))}
              </ul>
            )}
          </>
        ) : (
          <p className="faint small">
            This folder isn&apos;t linked to a project. Add it to a project&apos;s folders on Projects, or name the project like the folder.
          </p>
        )}
      </section>
      <section>
        <h2 className="h3">Files changed</h2>
        {!s.filesTouched.length ? (
          <p className="faint small">None yet.</p>
        ) : (
          s.filesTouched.map((f) => {
            const rel = inVault(f, vault);
            return (
              <button
                key={f}
                type="button"
                className="tnode"
                title={rel ? `Open ${rel}` : `Copy the path: ${f}`}
                onClick={() => (rel ? openDoc(rel) : copy(f, "the path"))}
              >
                <Icon name={rel ? "note" : "copy"} size={14} />
                <span className="ell">{f.split(/[\\/]/).pop()}</span>
              </button>
            );
          })
        )}
      </section>
      <section className="wsacts">
        {s.program === "claude" && (
          // Disabled buttons get no pointer events in WebKit, so the tooltip goes on a wrapper.
          <span
            title={
              canType(s)
                ? "Ask Claude Code to tick off the tasks done, add new ones and record findings and decisions in the vault, through Brainstead"
                : "Wrap up types into a session running here; this one has ended"
            }
          >
            <button type="button" className="btn" disabled={!canType(s)} onClick={() => void wrapUp(s.id).catch(reportEditError)}>
              <Icon name="check" size={14} />
              Wrap up
            </button>
          </span>
        )}
        {!s.external && (
          <button
            type="button"
            className="btn"
            title={
              running && resume
                ? `Move this session to ${terminalApp}: it stops here and carries on there`
                : `Open ${PROGRAM_LABEL[s.program]} in this folder in ${terminalApp}`
            }
            onClick={() =>
              void (async () => {
                if (running && resume) await stopSession(s.id);
                await workspaceApi.openExternal(s.folder, s.program, resume);
              })().catch(reportEditError)
            }
          >
            <Icon name="terminal" size={14} />
            Open in your terminal
          </button>
        )}
        <button
          type="button"
          className="btn"
          title="Copy the command that starts this in another terminal, guarded the same way"
          onClick={() =>
            void workspaceApi
              .externalCommand(s.folder, s.program, resume)
              .then((c) => copy(c, "the command"))
              .catch(reportEditError)
          }
        >
          <Icon name="copy" size={14} />
          Copy command
        </button>
        {running && (
          <button
            type="button"
            className="btn"
            title="End this session's program"
            onClick={() => void stopSession(s.id).catch(reportEditError)}
          >
            <Icon name="stop" size={14} />
            Stop
          </button>
        )}
        {s.state === "exited" && !s.external && (
          <button
            type="button"
            className="btn"
            title="Start this session again in its place"
            onClick={() => void restartSession(s.id).catch(reportEditError)}
          >
            <Icon name="refresh" size={14} />
            Restart
          </button>
        )}
      </section>
    </aside>
  );
}
