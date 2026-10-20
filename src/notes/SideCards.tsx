// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Cards beside a note: its outline, its open tasks, and for a 1-1, the last one with that person
// and what's still open with them.

import { useEffect, useMemo, useState } from "react";
import { api, FileSummary, TaskRow } from "../api";
import { Icon } from "../icons";
import { taskLabel } from "../md/TaskBlock";
import { openDoc } from "../nav";
import { useVaultVersion } from "../state";
import { useStore } from "../store";
import { retryTasks, tasksFailed, toggleTask, useAllTasks } from "../taskModel";
import { LayerTag } from "../ui";

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
