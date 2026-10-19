// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Quick capture: one box, Task or Thought (Tab switches). A task goes under "#### Other" in the
// To Do list and a thought to the Scratchpad, as the previous app writes them. `due: mon` and
// `defer: +3d` become dates; `due:` alone opens a date picker; [[ and # complete from the vault.

import { useEffect, useRef, useState } from "react";
import { api, type FileSummary } from "./api";
import { applyGtdShorthand } from "./gtd";
import { Icon, Mark } from "./icons";
import { DatePicker } from "./DatePicker";
import { applyShorthand, pendingShorthand, shorthandEmoji, ShorthandKind } from "./md/dates";
import { applyTheme, settings } from "./store";
import { localToday } from "./md/taskQuery";
import { reportEditError } from "./taskModel";
import { toast } from "./Toast";

export type CaptureKind = "task" | "thought";

/** "YYYY-MM-DD HH:MM" in local time, the heading of a captured thought. */
export function captureStamp(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${localToday(d)} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** What a capture writes, before it's sent; a task's `@context` and `effort:` become their formats too: shorthand turned into dates, a task on one line. */
export function prepareCapture(kind: CaptureKind, text: string, today = localToday()): string {
  const t = applyShorthand(text, today);
  return kind === "task" ? applyGtdShorthand(t).replace(/\s+/g, " ").trim() : t.trim();
}

interface Suggestion {
  label: string;
  insert: string;
}

/** `[[partial` or `#partial` right before the caret. */
function trigger(before: string): { kind: "link" | "tag"; q: string; start: number } | null {
  const l = /\[\[([^\]\n]*)$/.exec(before);
  if (l) return { kind: "link", q: l[1], start: before.length - l[0].length };
  const t = /(^|\s)#([\w/-]*)$/.exec(before);
  if (t) return { kind: "tag", q: t[2], start: before.length - t[2].length - 1 };
  return null;
}

/** The vault's files for `[[` and `#`, fetched at most every few seconds while typing. */
let fileCache: { at: number; files: Promise<FileSummary[]> } | null = null;
function vaultFiles(): Promise<FileSummary[]> {
  if (!fileCache || Date.now() - fileCache.at > 5000) {
    fileCache = { at: Date.now(), files: api.filesList(null) };
    fileCache.files.catch(() => (fileCache = null));
  }
  return fileCache.files;
}

/** Notes for `[[q`, by name: names starting with `q` first, then a word in the name starting with it,
 *  then any name containing it; newest first within each. With nothing typed yet, the newest notes. */
export function linkMatches(files: FileSummary[], q: string, limit = 8): FileSummary[] {
  const want = q.trim().toLowerCase();
  const name = (f: FileSummary) =>
    f.path
      .split("/")
      .pop()!
      .replace(/\.(md|txt)$/i, "");
  const rank = (f: FileSummary) => {
    if (!want) return 0;
    const n = name(f).toLowerCase();
    const t = f.title.toLowerCase();
    if (n.startsWith(want) || t.startsWith(want)) return 0;
    if (new RegExp(`(^|[^\\p{L}\\p{N}])${want.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`, "u").test(`${n} ${t}`)) return 1;
    if (n.includes(want) || t.includes(want)) return 2;
    return -1;
  };
  return files
    .filter((f) => f.layer !== "template")
    .map((f) => ({ f, r: rank(f) }))
    .filter((x) => x.r >= 0)
    .sort((a, b) => a.r - b.r || b.f.mtime - a.f.mtime)
    .slice(0, limit)
    .map((x) => x.f);
}

export async function suggest(kind: "link" | "tag", q: string): Promise<Suggestion[]> {
  if (kind === "link") {
    return linkMatches(await vaultFiles(), q).map((f) => ({
      label: f.title,
      insert: `[[${f.path
        .replace(/\.(md|txt)$/i, "")
        .split("/")
        .pop()}]] `,
    }));
  }
  return tagMatches(await vaultFiles(), q).map((t) => ({ label: `#${t}`, insert: `#${t} ` }));
}

/** Tags for `#q`: ones starting with `q` first, then ones with a part after a `/` starting with it
 *  (`#calls` finds `context/calls`), most used first within each. */
export function tagMatches(files: FileSummary[], q: string, limit = 8): string[] {
  const want = q.toLowerCase();
  const counts = new Map<string, number>();
  for (const f of files) for (const t of f.tags) counts.set(t, (counts.get(t) ?? 0) + 1);
  const rank = (t: string) => {
    const l = t.toLowerCase();
    if (l.startsWith(want)) return 0;
    if (l.split("/").some((part) => part.startsWith(want))) return 1;
    return -1;
  };
  return [...counts]
    .map(([t, n]) => ({ t, n, r: rank(t) }))
    .filter((x) => x.r >= 0)
    .sort((a, b) => a.r - b.r || b.n - a.n || a.t.localeCompare(b.t))
    .slice(0, limit)
    .map((x) => x.t);
}

/** Every tag in the vault, most used first, for tag pickers. */
export async function vaultTags(): Promise<string[]> {
  return tagMatches(await vaultFiles(), "", 500);
}

export function CaptureBox({
  autoFocus,
  onDone,
  compact,
  narrow,
  adds,
}: {
  autoFocus?: boolean;
  onDone?: (saved: boolean) => void;
  compact?: boolean;
  /** In the menu-bar window: a shorter prompt and footer. */
  narrow?: boolean;
  /** Tags a new task gets unless it has them already (the Tasks list it's added from), space-separated. */
  adds?: string;
}) {
  const [kind, setKind] = useState<CaptureKind>("task");
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [sugg, setSugg] = useState<{ items: Suggestion[]; start: number; i: number } | null>(null);
  const [picker, setPicker] = useState<ShorthandKind | null>(null);
  const [focused, setFocused] = useState(false);
  const box = useRef<HTMLTextAreaElement>(null);
  /** The latest suggestion request: an answer to an earlier one, or one that comes after the
   *  text was sent or the list put away, is dropped. */
  const req = useRef(0);
  // The capture window is sized for the box alone: it grows while the date picker is open.
  useEffect(() => {
    if (compact) return;
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow, LogicalSize }) => getCurrentWindow().setSize(new LogicalSize(700, picker ? 560 : 230)))
      .catch(() => {});
  }, [picker, compact]);
  useEffect(() => {
    if (autoFocus) box.current?.focus();
  }, [autoFocus]);

  const update = (v: string, caret: number) => {
    setText(v);
    const before = v.slice(0, caret);
    setPicker(pendingShorthand(before));
    const n = ++req.current;
    const tr = trigger(before);
    if (!tr) return setSugg(null);
    suggest(tr.kind, tr.q)
      .then((items) => n === req.current && setSugg(items.length ? { items, start: tr.start, i: 0 } : null))
      .catch(() => n === req.current && setSugg(null));
  };

  const accept = (s: Suggestion) => {
    const el = box.current!;
    const caret = el.selectionStart;
    const v = text.slice(0, sugg!.start) + s.insert + text.slice(caret);
    req.current++;
    setText(v);
    setSugg(null);
    requestAnimationFrame(() => {
      const at = sugg!.start + s.insert.length;
      el.setSelectionRange(at, at);
      el.focus();
    });
  };

  const placeDate = (d: string) => {
    const el = box.current!;
    const caret = el.selectionStart;
    const before = text.slice(0, caret).replace(/(due|defer|start|created):$/i, (_m, k: string) => `${shorthandEmoji(k)} ${d}`);
    setText(before + text.slice(caret));
    setPicker(null);
    requestAnimationFrame(() => el.focus());
  };

  const save = async () => {
    const typed = prepareCapture(kind, text);
    if (!typed || busy) return;
    const t = kind === "task" && adds ? [typed, ...adds.split(" ").filter((g) => g && !typed.split(/\s+/).includes(g))].join(" ") : typed;
    req.current++;
    setSugg(null);
    setBusy(true);
    try {
      const msg = await api.capture(kind, t, captureStamp());
      setText("");
      toast(msg, undefined, "ok");
      onDone?.(true);
    } catch (e) {
      reportEditError(e);
    } finally {
      setBusy(false);
    }
  };

  const key = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (sugg) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        const d = e.key === "ArrowDown" ? 1 : -1;
        setSugg({ ...sugg, i: (sugg.i + d + sugg.items.length) % sugg.items.length });
        return e.preventDefault();
      }
      if (e.key === "Enter" || e.key === "Tab") {
        accept(sugg.items[sugg.i]);
        return e.preventDefault();
      }
      if (e.key === "Escape") {
        req.current++;
        setSugg(null);
        return e.preventDefault();
      }
    }
    if (e.key === "Tab") {
      setKind((k) => (k === "task" ? "thought" : "task"));
      return e.preventDefault();
    }
    // Enter saves; ⇧↩ is a new line in a thought.
    if (e.key === "Enter" && !(e.shiftKey && kind === "thought")) {
      void save();
      return e.preventDefault();
    }
    if (e.key === "Escape") {
      onDone?.(false);
      return e.preventDefault();
    }
  };

  return (
    <div className={`capture ${compact ? "compact" : ""}`}>
      <div className="crow">
        {compact ? <Icon name="plus" style={{ color: "var(--accent)" }} /> : <Mark size={22} />}
        <textarea
          ref={box}
          rows={1}
          value={text}
          onChange={(e) => update(e.target.value, e.target.selectionStart)}
          onKeyDown={key}
          placeholder={
            narrow
              ? kind === "task"
                ? "Capture a task"
                : "Capture a thought"
              : kind === "task"
                ? adds
                  ? `Capture a task; it gets ${adds}`
                  : "Capture a task — [[ to link, # to tag, due: mon"
                : "Capture a thought for the Scratchpad"
          }
          aria-label="Capture"
          spellCheck
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
        />
      </div>
      {picker && (
        <div className="cpick">
          <DatePicker
            value={null}
            label={PICK_LABEL[picker]}
            onPick={placeDate}
            onCancel={() => {
              setPicker(null);
              box.current?.focus();
            }}
          />
        </div>
      )}
      {kind === "task" && focused && !text.trim() && !picker && <TaskHint />}
      {sugg && (
        <ul className="csugg" role="listbox">
          {sugg.items.map((s, i) => (
            <li key={s.label}>
              <button
                type="button"
                role="option"
                aria-selected={i === sugg.i}
                className={i === sugg.i ? "on" : ""}
                onMouseDown={(e) => e.preventDefault()}
                title={`Insert ${s.label}`}
                onClick={() => accept(s)}
              >
                {s.label}
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="cfoot">
        <div className="seg" role="radiogroup" aria-label="Capture as">
          {(["task", "thought"] as CaptureKind[]).map((k) => (
            <button
              key={k}
              type="button"
              role="radio"
              aria-checked={kind === k}
              className={kind === k ? "on" : ""}
              title={
                k === "task"
                  ? "Capture as a task on the To Do list (Tab switches)"
                  : "Capture as a thought in the Scratchpad (Tab switches)"
              }
              onClick={() => setKind(k)}
            >
              {k === "task" ? "Task" : "Thought"}
            </button>
          ))}
        </div>
        <span className="sp" />
        <span className="faint kbdhint">
          <span className="kbd">↩</span>
          {kind === "task" ? "To Do list" : "Scratchpad"}
        </span>
      </div>
    </div>
  );
}

const PICK_LABEL: Record<ShorthandKind, string> = { due: "Due date", defer: "Defer until", start: "Start date", created: "Created date" };

/** What a task can carry, shown while the task box is empty. */
export function TaskHint() {
  return (
    <div className="chint" aria-hidden>
      <span>
        <span className="kbd">due:</span> deadline
      </span>
      <span>
        <span className="kbd">defer:</span> hide until
      </span>
      <span>
        <span className="kbd">start:</span> can start
      </span>
      <span>
        <span className="kbd">created:</span> written
      </span>
      <span className="faint">then a word: tomorrow, fri, +3d, 5 oct</span>
      <span>
        <span className="kbd">#followup</span> <span className="kbd">#waiting-for</span> <span className="kbd">#someday-maybe</span>
      </span>
    </div>
  );
}

/** The floating capture window the global shortcut opens. */
export function CaptureWindow() {
  const [n, setN] = useState(0);
  useEffect(() => {
    let off: (() => void) | undefined;
    // The window lives for the whole run, so settings changed in the main window (the theme) are
    // read again each time it opens.
    const opened = () => {
      setN((x) => x + 1);
      api
        .settingsRead()
        .then((s) => {
          settings.loadFrom(s);
          applyTheme(s.theme);
        })
        .catch(() => {});
    };
    void import("@tauri-apps/api/event").then(({ listen }) => listen("capture-open", opened).then((u) => (off = u)));
    return () => off?.();
  }, []);
  return (
    <div className="capwin">
      <CaptureBox key={n} autoFocus onDone={() => void api.captureClose()} />
    </div>
  );
}
