// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Cards beside a note: its outline, its open tasks, and for a 1-1, the last one with that person
// and what's still open with them.

import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, FileSummary, TaskRow } from "../api";
import { Icon } from "../icons";
import { taskLabel, tasksAt } from "../md/TaskBlock";
import { fromLabel, noteRefs, type RefUse } from "../md/refUses";
import { type RefPattern, useRefPatterns } from "../md/refs";
import { openDoc } from "../nav";
import { useVaultVersion } from "../state";
import { useStore } from "../store";
import { retryTasks, tasksFailed, toggleTask, useAllTasks } from "../taskModel";
import { LayerTag, useDebounced } from "../ui";

/** The note's headings, outside code blocks and frontmatter, as (level, text). */
export function outline(md: string): [number, string][] {
  const out: [number, string][] = [];
  let fence = false;
  const lines = md.split(/\r?\n/);
  let i = 0;
  if (lines[0]?.trim() === "---") {
    i = lines.findIndex((l, k) => k > 0 && l.trim() === "---") + 1;
  }
  for (; i < lines.length; i++) {
    const l = lines[i];
    if (/^\s*(```|~~~)/.test(l)) fence = !fence;
    if (fence) continue;
    const m = /^(#{1,6})\s+(.+?)\s*#*\s*$/.exec(l);
    if (m) out.push([m[1].length, m[2].trim()]);
  }
  return out;
}

export function OutlineCard({ path, md }: { path: string; md: string }) {
  const hs = useMemo(() => outline(md), [md]);
  if (hs.length < 2) return null;
  const top = Math.min(...hs.map(([l]) => l));
  return (
    <section className="card side-card">
      <div className="eyebrow">Outline</div>
      {hs.map(([l, t], i) => (
        <button
          key={i}
          type="button"
          className="blink outline"
          style={{ marginLeft: (l - top) * 12 }}
          title="Jump to this heading"
          onClick={() => openDoc(path, { anchor: t })}
        >
          <span className="ell">{t}</span>
        </button>
      ))}
    </section>
  );
}

/** How many links show in a references card before Show all. */
const REFS_SHOWN = 12;

/** A card of the links a note or project holds (reference keys, `[label](url)` links and URLs), each
 *  once, with where it is written. Each opens in the browser. Nothing when there are none. */
export function RefsCard({ title, uses }: { title: string; uses: RefUse[] }) {
  const [all, setAll] = useState(false);
  if (!uses.length) return null;
  return (
    <section className="card side-card">
      <div className="eyebrow">
        {title} · {uses.length}
      </div>
      {(all ? uses : uses.slice(0, REFS_SHOWN)).map((u) => (
        <div key={u.url} className="refrow">
          <a
            className="tlinkword ext"
            href={u.url}
            title={u.url}
            onClick={(e) => {
              e.preventDefault();
              void openUrl(u.url);
            }}
          >
            {u.key}
          </a>
          <span className="faint small">{u.from.map(fromLabel).join(", ")}</span>
        </div>
      ))}
      {uses.length > REFS_SHOWN && (
        <button
          type="button"
          className="btn sm ghost"
          title={all ? "Show the first few again" : "Show every link"}
          onClick={() => setAll(!all)}
        >
          {all ? "Show fewer" : `Show all ${uses.length}`}
        </button>
      )}
    </section>
  );
}

/** The open note's References: the keys and links in its properties, text and open tasks. Only
 *  with a Reference links pattern set. */
export function ReferencesCard({ path, md }: { path: string; md: string }) {
  const patterns = useRefPatterns();
  // Keyed by the note, so what is read of one note is never shown beside another.
  return patterns.length ? <NoteReferences key={path} path={path} md={md} patterns={patterns} /> : null;
}

function NoteReferences({ path, md, patterns }: { path: string; md: string; patterns: RefPattern[] }) {
  const v = useVaultVersion();
  // The tasks every task block on the screen shares, not a query of this card's own.
  const [tasks, setTasks] = useState<TaskRow[]>([]);
  useEffect(() => {
    let live = true;
    tasksAt(v)
      .then((all) => live && setTasks(all.filter((t) => t.path === path)))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [v, path]);
  // The text is read again a moment after typing stops, not on every key.
  const text = useDebounced(md, 400);
  const uses = useMemo(() => noteRefs(text, tasks, patterns), [text, tasks, patterns]);
  return <RefsCard title="References" uses={uses} />;
}

/** Says the tasks couldn't be read, with Retry, where a card would otherwise show none. */
function TasksFailed({ what }: { what: string }) {
  return (
    <p className="faint small">
      Couldn&apos;t load {what}.{" "}
      <button type="button" className="blink" title="Read the tasks again" onClick={retryTasks}>
        Retry
      </button>
    </p>
  );
}

export function NoteTasksCard({ path }: { path: string }) {
  const all = useAllTasks();
  const failed = useStore(tasksFailed);
  const open = useMemo(() => (all ?? []).filter((t) => t.path === path && !t.done), [all, path]);
  if (failed)
    return (
      <section className="card side-card">
        <div className="eyebrow">Open tasks in this note</div>
        <TasksFailed what="the tasks" />
      </section>
    );
  if (!open.length) return null;
  return (
    <section className="card side-card">
      <div className="eyebrow">Open tasks in this note · {open.length}</div>
      {open.slice(0, 12).map((t) => (
        <TaskLine key={`${t.line}`} t={t} />
      ))}
    </section>
  );
}

function TaskLine({ t }: { t: TaskRow }) {
  return (
    <div className="ntask">
      <button
        type="button"
        className="cb"
        role="checkbox"
        aria-checked={false}
        aria-label={`Tick ${taskLabel(t.text)}`}
        title="Mark this task done"
        onClick={() => void toggleTask(t, true)}
      />
      <span className="ell" title={taskLabel(t.text)}>
        {taskLabel(t.text)}
      </span>
    </div>
  );
}

/** "1-1. Maya - 2026-09-23.md" → ["Maya", "2026-09-23"]. */
export function oneOnOne(path: string): [string, string] | null {
  const m = /(?:^|\/)1-1\. (.+?) - (\d{4}-\d{2}-\d{2})\.md$/.exec(path);
  return m ? [m[1], m[2]] : null;
}

/** On a 1-1: the one before with the same person, and the follow-ups still open with them. */
export function LastTimeCard({ path }: { path: string }) {
  const who = oneOnOne(path);
  const v = useVaultVersion();
  const all = useAllTasks();
  const failed = useStore(tasksFailed);
  const [notes, setNotes] = useState<FileSummary[]>([]);
  useEffect(() => {
    if (!who) return;
    let live = true;
    void api
      .filesList("note")
      .then((f) => live && setNotes(f))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [path, v]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!who) return null;
  const [name, date] = who;
  const before = notes
    .map((n) => [n, oneOnOne(n.path)] as const)
    .filter(([, w]) => w && w[0].toLowerCase() === name.toLowerCase() && w[1] < date)
    .sort((a, b) => b[1]![1].localeCompare(a[1]![1]))[0]?.[0];
  const first = name.split(/\s+/)[0].toLowerCase();
  const followups = (all ?? []).filter((t) => !t.done && t.tags.some((g) => g.toLowerCase() === `followup/${first}`));
  if (!before && !followups.length && !failed) return null;
  return (
    <section className="card side-card">
      <div className="eyebrow">Last time with {name}</div>
      {before ? (
        <button type="button" className="blink" title={`Open the previous 1-1 with ${name}`} onClick={() => openDoc(before.path)}>
          <span className="lbt">
            <LayerTag layer="note" />
            <span className="ell">{before.title}</span>
          </span>
        </button>
      ) : (
        <p className="faint small">The first 1-1 with {name} here.</p>
      )}
      {failed && <TasksFailed what="the open follow-ups" />}
      {!failed && followups.length > 0 && (
        <>
          <div className="eyebrow sub">
            <Icon name="tasks" size={11} /> Open follow-ups · {followups.length}
          </div>
          {followups.slice(0, 8).map((t) => (
            <TaskLine key={`${t.path}:${t.line}`} t={t} />
          ))}
        </>
      )}
    </section>
  );
}
