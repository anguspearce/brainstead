// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A `tasks` block in the Tasks plugin's full query language (src/tasksq/), answered from every
// task in the vault and worked like the Tasks screen: tick, right-click for dates, status and
// priority, drag to reorder when the block sorts by rank. Groups, layout options, `explain` and
// errors as the plugin shows them, and a link to the language's documentation.

import moment from "moment";
import { useEffect, useState } from "react";
import { api, TaskRow } from "../api";
import { parseQuery, Query, Result, runQuery } from "../tasksq/query";
import { QTask, toQTask } from "../tasksq/task";
import { withoutFields } from "../taskFields";
import { TaskDisplay, TaskList } from "../TaskList";
import { useVaultVersion } from "../state";
import { QueryDocsLink } from "./queryDocs";
import { SCRIPTS_OFF } from "./scripts";
import { localToday } from "./taskQuery";

/** The task's text without the fields and tags shown beside it as chips. */
export function taskLabel(text: string): string {
  return withoutFields(text)
    .replace(/\s*[[(]effort::[^\])]*[\])]/gi, "")
    .replace(/\s*\[\[Project\. [^\]]+\]\]/g, "")
    .replace(/(^|\s)#[A-Za-z][\w/-]*/g, "")
    .replace(/\s+\^[\w-]+\s*$/, "")
    .trim();
}

// Every task, fetched once per change in the vault and shared by the blocks on screen. A failed
// fetch isn't kept, so the next draw asks again.
let allTasks: { version: number; rows: Promise<TaskRow[]> } | null = null;
export function tasksAt(version: number) {
  if (!allTasks || allTasks.version !== version) {
    const entry = { version, rows: api.tasksAll() };
    allTasks = entry;
    entry.rows.catch(() => {
      if (allTasks === entry) allTasks = null;
    });
  }
  return allTasks.rows;
}
// The note's properties, for query file defaults (`TQ_*`) and query.file.property(): for this
// version of the vault only, and not when reading the note failed.
const props = new Map<string, Promise<Record<string, unknown> | null>>();
let propsAt = -1;
export function propsOf(path: string, version: number) {
  if (propsAt !== version) {
    props.clear();
    propsAt = version;
  }
  const k = path;
  if (!props.has(k)) {
    const p = path ? api.docRead(path).then((d) => d.meta.frontmatter) : Promise.resolve(null);
    props.set(k, p);
    p.catch(() => {
      if (props.get(k) === p) props.delete(k);
    });
  }
  return props.get(k)!.catch(() => null);
}

/** A `tasks` block (`dataview` blocks are src/md/Dataview.tsx). `path` is the note it's in (empty
 *  for text that isn't a note); `filter/sort/group by function` run only when `scripts` says so. */
export function TaskBlock({ src, path, scripts }: { src: string; path: string; scripts: boolean }) {
  const version = useVaultVersion();
  return <TasksQueryBlock src={src} path={path} scripts={scripts} version={version} />;
}

function TasksQueryBlock({ src, path, scripts, version }: { src: string; path: string; scripts: boolean; version: number }) {
  const [state, setState] = useState<{ result: Result; query: Query; rows: Map<QTask, TaskRow> } | { error: string } | null>(null);
  useEffect(() => {
    let live = true;
    void Promise.all([tasksAt(version), propsOf(path, version)])
      .then(([rows, frontmatter]) => {
        if (!live) return;
        const today = moment(localToday(), "YYYY-MM-DD");
        const file = { path, frontmatter, scripts };
        const query = parseQuery(src, file, today);
        const byTask = new Map<QTask, TaskRow>();
        const all: QTask[] = [];
        for (const r of rows) {
          const t = toQTask(r, today);
          if (t) {
            all.push(t);
            byTask.set(t, r);
          }
        }
        setState({ result: runQuery(query, { today, all, file }), query, rows: byTask });
      })
      .catch((e) => live && setState({ error: String(e) }));
    return () => {
      live = false;
    };
  }, [src, path, scripts, version]);

  const head = (
    <div className="tqhead">
      <span className="eyebrow">tasks</span>
      <code title={src}>{src.replace(/\s+/g, " ").trim()}</code>
      <QueryDocsLink lang="tasks" />
    </div>
  );
  // The count, when there's one to show (the docs link is in the header).
  const foot = (extra?: React.ReactNode) => (extra ? <div className="tqfoot">{extra}</div> : null);
  if (!state)
    return (
      <div className="tq">
        {head}
        <div className="tqmsg faint">Loading…</div>
      </div>
    );
  if (!("error" in state) && state.result.error === SCRIPTS_OFF)
    return (
      <div className="tq">
        {head}
        <div className="tqmsg faint">{SCRIPTS_OFF}</div>
      </div>
    );
  if ("error" in state || state.result.error)
    return (
      <div className="tq">
        {head}
        <pre className="tqmsg err tqerr">{"error" in state ? state.error : state.result.error}</pre>
        {foot()}
      </div>
    );
  const { result, query, rows } = state;
  const shown = query.shown;
  const display: TaskDisplay = {
    hidden: new Set(ALL_ELEMENTS.filter((e) => !(shown as Set<string>).has(e))),
    short: query.shortMode,
    urgency: shown.has("urgency"),
  };
  const manual = !result.groups.some((g) => g.headings.length) && /^sort by (rank|manual)\b/i.test(query.sorters[0]?.source ?? "");
  const count =
    result.shown === result.total ? `${result.total} task${result.total === 1 ? "" : "s"}` : `${result.shown} of ${result.total} tasks`;
  let prev: string[] = [];
  return (
    <div className="tq">
      {head}
      {result.explanation && <pre className="tqexplain">{result.explanation}</pre>}
      {result.total === 0 ? (
        <div className="tqmsg faint">No matching tasks.</div>
      ) : (
        result.groups.map((g, i) => {
          // A heading only where it changes from the group before.
          const from = g.headings.findIndex((h, k) => h !== prev[k]);
          const heads = from < 0 ? [] : g.headings.slice(from).map((h, k) => ({ h, level: from + k }));
          prev = g.headings;
          const leaf = g.headings.length - 1;
          return (
            <div key={i} className="tqgroup">
              {heads.map(({ h, level }) => (
                <div key={level} className={`tqh l${Math.min(level, 2)}`}>
                  {h || "\u00a0"}
                  {level === leaf && shown.has("group count") && <span className="faint"> {g.tasks.length}</span>}
                </div>
              ))}
              <TaskList rows={g.tasks.map((t) => rows.get(t)!)} manual={manual} display={display} />
            </div>
          );
        })
      )}
      {foot(shown.has("task count") && result.total > 0 ? <span className="faint">{count}</span> : null)}
    </div>
  );
}

const ALL_ELEMENTS = [
  "id",
  "depends on",
  "priority",
  "cancelled date",
  "created date",
  "start date",
  "scheduled date",
  "due date",
  "done date",
  "recurrence rule",
  "on completion",
  "tags",
  "backlink",
];
