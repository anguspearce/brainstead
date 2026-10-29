// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// ⌘K: find a file, go anywhere or run a command (a sibling app's Palette pattern). Files come
// from the index's search, so the same syntax works here as on the Search page.

import { findRun } from "./Found";
import { startWeekly } from "./Weekly";
import { openFixName } from "./FixName";
import { startNewProject } from "./Projects";
import { shortcutsOpen } from "./Shortcuts";
import { GUIDES } from "./help";
import { openHelp } from "./help/HelpDrawer";
import { askAboutNote } from "./Ask";
import { captureStamp, prepareCapture } from "./Capture";
import { toast } from "./Toast";
import { reportEditError } from "./taskModel";
import { newChat, send } from "./askState";
import { openBuilderAt } from "./editor/queryAssist";
import { activeEditor } from "./editor/queryBuilderStore";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, Layer } from "./api";
import { useAllTasks } from "./taskModel";
import { taskLabel } from "./md/TaskBlock";
import { Icon } from "./icons";
import { Snippet } from "./FileList";
import { nav, openDoc, place } from "./nav";
import { openRename, trashFile } from "./notes/actions";
import { openNewNote } from "./notes/dialogs";
import { useSearchOf } from "./Search";
import { DAILY, FOOTER, HOTKEYS, hotkey, LIBRARY, TOOLS } from "./Sidebar";

/** What the Tools entries are found by besides their names (what ⌘K once called them). */
const TOOL_WORDS: Partial<Record<string, string>> = {
  reply: "draft a reply email teams message respond",
  meeting: "meeting note from a transcript teams 1-1 write up",
  doccheck: "doc check check a document canonical register version in force feedback operating model align",
};
import { applyTheme, settings } from "./store";
import { LAYER_LABEL, useDebounced } from "./ui";
import { fileManager, isMod, keyLabel } from "./platform";

export interface Command {
  id: string;
  /** A search hit's passage, shown under it. */
  hit?: import("./api").SearchHit;
  group: string;
  icon: string;
  label: string;
  /** More words it answers to. */
  keywords?: string;
  kbd?: string;
  run: () => void;
}

/** Each screen's key: ⌥⌘1 to ⌥⌘0 for the screens used most (Sidebar's HOTKEYS), and Settings' own. */
const KBD: Partial<Record<string, string>> = {
  ...Object.fromEntries(HOTKEYS.map((s) => [s, hotkey(s)!])),
  settings: "⌘,",
};

export function commands(): Command[] {
  // Every sidebar entry in the sidebar's order, the Tools menu's included (under their own names, found by what they do too).
  const screens: Command[] = [...DAILY, ...LIBRARY, ...TOOLS, ...FOOTER].map((i) => ({
    id: `go:${i.screen}`,
    group: "Go to",
    icon: i.icon,
    label: i.label,
    keywords: TOOL_WORDS[i.screen],
    kbd: KBD[i.screen],
    run: () => nav.go(i.screen),
  }));
  const theme = (t: "light" | "dark" | "system") => () => {
    settings.update({ theme: t });
    applyTheme(t);
  };
  const here = place.get().place;
  const doc: Command[] =
    here.screen === "doc" && here.path
      ? [
          {
            id: "rename",
            group: "Do",
            icon: "text",
            label: "Rename this note",
            keywords: "move links",
            kbd: "⇧⌘R",
            run: () => openRename(here.path!),
          },
          {
            id: "query",
            group: "Do",
            icon: "tasks",
            label: "Insert a query…",
            keywords: "tasks dataview table list build",
            run: () => {
              const v = activeEditor();
              if (v) openBuilderAt(v, v.state.selection.main.head);
            },
          },
          {
            id: "ask-this",
            group: "Do",
            icon: "ask",
            label: "Ask about this note",
            keywords: "chat assistant ai",
            run: () => askAboutNote(here.path!),
          },
          {
            id: "trash-this",
            group: "Do",
            icon: "trash",
            label: "Move this note to the Trash",
            keywords: "delete remove",
            run: () => void trashFile(here.path!),
          },
        ]
      : [];
  return [
    ...screens,
    { id: "new", group: "Do", icon: "plus", label: "New note", keywords: "create template", kbd: "⌘N", run: () => openNewNote() },
    {
      id: "weekly",
      group: "Do",
      icon: "calendar",
      label: "Start the weekly review",
      keywords: "gtd review week",
      run: startWeekly,
    },
    { id: "newproject", group: "Do", icon: "project", label: "New project", keywords: "create project", run: () => startNewProject() },
    {
      id: "find",
      group: "Do",
      icon: "zap",
      label: "Find tasks and projects in my notes",
      keywords: "suggest scan recommend gtd",
      run: () => {
        void findRun();
        nav.go("tasks");
      },
    },
    {
      id: "shortcuts",
      group: "Go to",
      icon: "key",
      label: "Keyboard shortcuts",
      keywords: "keys hotkeys help",
      run: () => shortcutsOpen.set(true),
    },
    {
      id: "help",
      group: "Go to",
      icon: "help",
      label: "Help for this screen",
      keywords: "help how guide manual",
      kbd: "?",
      run: () => openHelp({}),
    },
    ...GUIDES.map((g) => ({
      id: `guide:${g.id}`,
      group: "Go to",
      icon: "help",
      label: `Getting started: ${g.title}`,
      keywords: "help guide walkthrough tutorial",
      run: () => openHelp({ guide: g.id, step: 0 }),
    })),
    {
      id: "clarify",
      group: "Do",
      icon: "inbox",
      label: "Clarify the inbox",
      keywords: "gtd process scratchpad",
      run: () => nav.go("inbox"),
    },
    ...doc,
    {
      id: "vault",
      group: "Do",
      icon: "folder",
      label: "Vault settings",
      keywords: "folder excluded read-only",
      run: () => nav.go({ screen: "settings", pane: "vault" }),
    },
    {
      id: "fixname",
      group: "Do",
      icon: "text",
      label: "Fix a name everywhere…",
      keywords: "misspelt wrong name correct substitution",
      run: () => openFixName(),
    },
    {
      id: "triage",
      group: "Do",
      icon: "pin",
      label: "Triage bookmarks",
      keywords: "bookmarks read later clean",
      run: () => nav.go("triage"),
    },
    {
      id: "compare",
      group: "Do",
      icon: "shield",
      label: "Compare two documents…",
      keywords: "diff differences versions changed side by side",
      run: () => nav.go("compare"),
    },
    {
      id: "rebuild",
      group: "Do",
      icon: "refresh",
      label: "Rebuild index",
      keywords: "reindex",
      run: () => {
        toast("Rebuilding the index: every file in the vault is read again");
        void api.rebuildIndex();
      },
    },
    {
      id: "reveal",
      group: "Do",
      icon: "finder",
      label: `Show vault in ${fileManager}`,
      keywords: "reveal open folder",
      run: () => void api.reveal("").catch(() => {}),
    },
    {
      id: "perm",
      group: "Do",
      icon: "shield",
      label: "Permissions",
      keywords: "full disk access privacy",
      run: () => nav.go({ screen: "settings", pane: "permissions" }),
    },
    { id: "light", group: "Appearance", icon: "sun", label: "Light appearance", keywords: "theme", run: theme("light") },
    { id: "dark", group: "Appearance", icon: "moon", label: "Dark appearance", keywords: "theme", run: theme("dark") },
    {
      id: "system",
      group: "Appearance",
      icon: "settings",
      label: "Match the system appearance",
      keywords: "theme auto",
      run: theme("system"),
    },
  ];
}

/** Commands whose label or keywords contain every word typed, labels starting with it first. */
/** ⌘K's "Capture … as a task": onto the To Do list, as Quick capture does. */
async function captureTask(text: string) {
  const t = prepareCapture("task", text);
  if (!t) return;
  try {
    toast(await api.capture("task", t, captureStamp()), undefined, "ok");
  } catch (e) {
    reportEditError(e);
  }
}

/** What ⌘K searches, Tab by Tab. */
const PALETTE_LAYERS: [string, Layer[]][] = [
  ["All", []],
  ["Notes", ["note"]],
  ["Wiki", ["wiki"]],
  ["Sources", ["source"]],
];

export function match(all: Command[], q: string): Command[] {
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return all.filter((c) => c.group === "Go to");
  const scored = all
    .map((c) => {
      const label = c.label.toLowerCase();
      const hay = `${label} ${c.keywords ?? ""}`.toLowerCase();
      if (!words.every((w) => hay.includes(w))) return null;
      return { c, score: label.startsWith(words[0]) ? 0 : label.includes(words[0]) ? 1 : 2 };
    })
    .filter((x): x is { c: Command; score: number } => !!x);
  return scored.sort((a, b) => a.score - b.score).map((x) => x.c);
}

/** Runs of results in one group, in order, with where each starts in the list. */
export function sections(items: Command[]): { group: string; from: number; rows: Command[] }[] {
  const out: { group: string; from: number; rows: Command[] }[] = [];
  items.forEach((c, k) => {
    const last = out[out.length - 1];
    if (last && last.group === c.group) last.rows.push(c);
    else out.push({ group: c.group, from: k, rows: [c] });
  });
  return out;
}

/** Where ↑ ↓ ← → move from row `k`, by the layout as drawn: a run of Go to screens sits in two
 *  columns, where ↑ ↓ stay in a column and ← → cross; any other row is one line. A run is one
 *  section of `sections`, so Go to rows apart in the list (results sorted between them) never pair. */
export function paletteMove(items: Command[], k: number, key: "up" | "down" | "left" | "right"): number {
  const sec = sections(items).find((x) => k >= x.from && k < x.from + x.rows.length);
  if (!sec) return k;
  const end = sec.from + sec.rows.length;
  const grid = sec.group === "Go to" && sec.rows.length > 1;
  const clamp = (x: number) => Math.max(0, Math.min(items.length - 1, x));
  if (key === "left" || key === "right") {
    if (!grid) return k;
    const p = k - sec.from;
    // Across a row of two: right from the left column, left from the right one.
    const to = key === "right" && p % 2 === 0 ? k + 1 : key === "left" && p % 2 === 1 ? k - 1 : k;
    return to < end ? to : k;
  }
  const d = key === "down" ? 1 : -1;
  if (!grid) return clamp(k + d);
  const to = k + 2 * d;
  if (to >= sec.from && to < end) return to;
  // Out of the grid: to the row after it, or the one before.
  return clamp(d > 0 ? end : sec.from - 1);
}

export function Palette({ onClose }: { onClose: () => void }) {
  const [q, setQ] = useState("");
  const [i, setI] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const all = useMemo(() => commands(), []);
  // Commands match as you type; the vault is searched once typing pauses.
  const fq = useDebounced(q, 150);
  // Tab steps through what's searched: everything, then each layer.
  const [layer, setLayer] = useState(0);
  const searched = useSearchOf(fq, PALETTE_LAYERS[layer][1], 8);
  const found = searched?.r ?? null;
  // The file results answer an earlier query while typing runs ahead of the search.
  const stale = q.trim().length >= 2 && searched?.q !== q.trim();
  const [enterWanted, setEnterWanted] = useState(false);
  const tasks = useAllTasks();
  const items = useMemo(() => {
    const cmds = match(all, q);
    if (q.trim().length < 2) return cmds;
    const files: Command[] = (found?.hits ?? []).map((h) => ({
      id: `file:${h.file.path}`,
      group: "Files",
      icon: { note: "note", wiki: "wiki", source: "source", template: "template" }[h.file.layer],
      label: h.file.title,
      keywords: LAYER_LABEL[h.file.layer],
      hit: h,
      run: () => openDoc(h.file.path, { q: q.trim() }),
    }));
    const all2: Command = {
      id: "search",
      group: "Search",
      icon: "search",
      label: `Search everything for “${q.trim()}”`,
      kbd: "⌘↩",
      run: () => nav.go({ screen: "search", q: q.trim() }),
    };
    const askIt: Command = {
      id: "ask",
      group: "Search",
      icon: "ask",
      label: `Ask “${q.trim()}”`,
      keywords: "chat assistant ai",
      run: () => {
        send(newChat(), q.trim());
        nav.go("ask");
      },
    };
    const taskIt: Command = {
      id: "capture-task",
      group: "Search",
      icon: "plus",
      label: `Capture “${q.trim()}” as a task`,
      keywords: "todo add",
      run: () => void captureTask(q.trim()),
    };
    // Open tasks whose text has every word typed (not when one layer is chosen).
    const words = q.toLowerCase().split(/\s+/).filter(Boolean);
    const taskRows: Command[] =
      layer === 0
        ? (tasks ?? [])
            .filter((t) => !t.done && words.every((w) => t.text.toLowerCase().includes(w)))
            .slice(0, 5)
            .map((t) => ({
              id: `task:${t.path}:${t.line}`,
              group: "Tasks",
              icon: "tasks",
              label: taskLabel(t.text),
              keywords: t.title,
              run: () => openDoc(t.path, t.heading ? { anchor: t.heading } : {}),
            }))
        : [];
    return layer === 0 ? [...files, ...taskRows, ...cmds, all2, askIt, taskIt] : [...files, all2];
  }, [all, q, found, tasks, layer]);
  useEffect(() => {
    input.current?.focus();
  }, []);
  useEffect(() => {
    setI(0);
  }, [q]);
  const run = (c: Command | undefined) => {
    if (!c) return;
    onClose();
    c.run();
  };
  // ↩ pressed before the search caught up: acts on the first result for what's typed, once it's in.
  useEffect(() => {
    if (enterWanted && !stale) {
      setEnterWanted(false);
      run(items[0]);
    }
  }, [enterWanted, stale, items]); // eslint-disable-line react-hooks/exhaustive-deps
  const key = (e: React.KeyboardEvent) => {
    if (e.key === "Tab") {
      setLayer((x) => (x + (e.shiftKey ? PALETTE_LAYERS.length - 1 : 1)) % PALETTE_LAYERS.length);
      setI(0);
      e.preventDefault();
    } else if (e.key === "ArrowDown") {
      setI((x) => paletteMove(items, x, "down"));
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      setI((x) => paletteMove(items, x, "up"));
      e.preventDefault();
    } else if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && !q && items[i]?.group === "Go to") {
      // Across the Go to columns; with something typed the arrows move the caret instead.
      setI(paletteMove(items, i, e.key === "ArrowRight" ? "right" : "left"));
      e.preventDefault();
    } else if (e.key === "Enter") {
      if (!isMod(e) && stale && i === 0) setEnterWanted(true);
      else run(isMod(e) ? items.find((c) => c.id === "search") : items[i]);
      e.preventDefault();
    } else if (e.key === "Escape") {
      onClose();
      e.preventDefault();
    }
  };
  return (
    <div
      className="scrim upper"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div role="dialog" aria-label="Search or jump to" className="dialog palette">
        <label className="pq">
          <Icon name="search" size={18} />
          <input
            ref={input}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={key}
            placeholder="Find a note, go to a screen or run a command"
            aria-label="Go to or run"
            spellCheck={false}
            autoCorrect="off"
          />
          {q && (
            <button
              type="button"
              className="ibtn xs"
              aria-label="Clear search"
              title="Clear what you typed"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                setQ("");
                input.current?.focus();
              }}
            >
              <Icon name="x" size={11} />
            </button>
          )}
          <span className="kbd">esc</span>
        </label>
        <div className="players" role="radiogroup" aria-label="Search in">
          {PALETTE_LAYERS.map(([label], k) => (
            <button
              key={label}
              type="button"
              role="radio"
              aria-checked={k === layer}
              title={k === 0 ? "Search notes, wiki and sources (Tab to switch)" : `Search only ${label.toLowerCase()} (Tab to switch)`}
              className={`chip f ${k === layer ? "on" : ""}`}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => setLayer(k)}
            >
              {label}
            </button>
          ))}
          <span className="faint small">Tab to switch</span>
        </div>
        <div className="plist" role="listbox" aria-label="Results">
          {items.length === 0 && <div className="pempty">Nothing matches “{q}”.</div>}
          {sections(items).map(({ group, from, rows }) => (
            <div key={group + from} role="group" aria-label={group}>
              <div className="pg">{group}</div>
              <div className={group === "Go to" ? "pgoto" : undefined}>
                {rows.map((c, j) => {
                  const k = from + j;
                  return (
                    <button
                      key={c.id}
                      type="button"
                      role="option"
                      aria-selected={k === i}
                      title={keyLabel(
                        `${c.group === "Files" ? `Open ${c.label}` : c.group === "Tasks" ? `Open the note with this task, ${c.keywords}` : c.label}${c.kbd ? ` (${c.kbd})` : ""}`,
                      )}
                      className={`pr ${k === i ? "on" : ""}`}
                      onMouseEnter={() => setI(k)}
                      onClick={() => run(c)}
                    >
                      <Icon name={c.icon} />
                      <span className="t">
                        <span className="ell">{c.label}</span>
                        {c.hit && <Snippet hit={c.hit} />}
                      </span>
                      {c.kbd && <span className="kbd">{keyLabel(c.kbd)}</span>}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
        <div className="pfoot">
          <span>
            <span className="kbd">↑↓←→</span> move
          </span>
          <span>
            <span className="kbd">{keyLabel("↩")}</span> open
          </span>
          <span>
            <span className="kbd">{keyLabel("⌘↩")}</span> search everything
          </span>
        </div>
      </div>
    </div>
  );
}
