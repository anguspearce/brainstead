// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Tasks: the previous app's To Do lists as views, each task live from the notes it's written in.

import { FoundTasks } from "./Found";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { CaptureBox } from "./Capture";
import { EFFORT_LIMITS, GroupBy, groupTasks, matchesSearch, withinEffort } from "./taskGroups";
import { fieldIcon, fieldValue, taskFieldsOf } from "./taskFields";
import type { TaskDateKind, TaskRow } from "./api";
import { DateField } from "./DatePicker";
import { contextsInUse, useInbox } from "./gtd";
import { Icon } from "./icons";
import { fmtShortDate } from "./md/dates";
import { taskLabel } from "./md/TaskBlock";
import { localToday } from "./md/taskQuery";
import { nav, openDoc, useViewState } from "./nav";
import { askWith } from "./askState";
import { InfoTip, lineDate, TaskAttrs, taskKey, TaskList, TasksFailed, TaskText, useTaskListKeys } from "./TaskList";
import { taskDate, TaskDateId } from "./taskDates";
import { changeTask, retitled, retitleTask, setTaskDate, tasksFailed, toggleTask, useAllTasks, viewRows, VIEWS, ViewId } from "./taskModel";
import { askLink, askQuote, TopBar } from "./TopBar";
import { fmtCount, Popover, SearchBox, Seg, TableBand, useDebounced } from "./ui";
import { settings, useStore } from "./store";
import type { SavedTaskList } from "./api";
import { keyLabel } from "./platform";

/** Each grouping's band icon, and what its count's tooltip says. */
const GROUP_ICON: Record<GroupBy, string> = { none: "", project: "project", context: "tag", due: taskDate("due").icon };
const groupTip = (by: GroupBy, label: string) =>
  by === "project"
    ? label === "No project"
      ? "with no project"
      : "in this project"
    : by === "context"
      ? label === "No context"
        ? "with no context"
        : "in this context"
      : label === "No due date"
        ? "with no due date"
        : label === "Overdue"
          ? "overdue"
          : `due ${label.toLowerCase()}`;

export function TasksScreen() {
  const all = useAllTasks();
  const inbox = useInbox();
  const [viewId, setViewId] = useViewState("tasks:view", "next");
  const [sel, setSel] = useViewState<string | null>("tasks:sel", null);
  const today = localToday();
  const view = VIEWS.find((v) => v.id === viewId) ?? VIEWS[0];
  // The search narrows every list, so the counts on the left say where the matches are.
  const [q, setQ] = useViewState("tasks:q", "");
  const dq = useDebounced(q).trim();
  const counts = useMemo(
    () => new Map(VIEWS.map((v) => [v.id, all ? viewRows(all, v, today, inbox).filter((t) => matchesSearch(t, dq)).length : 0])),
    [all, inbox, today, dq],
  );
  const [ctx, setCtx] = useViewState("tasks:context", "");
  const [effort, setEffort] = useViewState("tasks:effort", "");
  const [groupBy, setGroupBy] = useViewState<GroupBy>("tasks:group", "none");
  const [adding, setAdding] = useState<DOMRect | null>(null);
  const contexts = useMemo(() => (all ? contextsInUse(all) : []), [all]);
  const rows = useMemo(
    () =>
      (all ? viewRows(all, view, today, inbox) : []).filter(
        (t) => (!ctx || (t.contexts ?? []).includes(ctx)) && withinEffort(t, effort) && matchesSearch(t, dq),
      ),
    [all, inbox, view, today, ctx, effort, dq],
  );
  const groups = useMemo(() => groupTasks(rows, groupBy, today), [rows, groupBy, today]);
  // Saved lists: a view with its context, effort and grouping, by name (settings `taskLists`).
  const saved = useStore(settings).taskLists ?? [];
  const savedOn = saved.find((l) => l.view === view.id && l.context === ctx && l.effort === effort && l.group === groupBy)?.name ?? null;
  const applySaved = (l: SavedTaskList) => {
    setViewId(l.view as ViewId);
    setCtx(l.context);
    setEffort(l.effort);
    setGroupBy(l.group as GroupBy);
  };
  const [naming, setNaming] = useState<{ at: DOMRect; name: string } | null>(null);
  const saveList = (name: string) => {
    if (!name.trim()) return;
    const l: SavedTaskList = { name: name.trim(), view: view.id, context: ctx, effort, group: groupBy };
    settings.update({ taskLists: [...saved.filter((x) => x.name !== l.name), l] });
    setNaming(null);
  };
  const selected = rows.find((t) => taskKey(t) === sel) ?? null;
  // The selection is a line in a note. When lines above it come or go (an edit elsewhere), it
  // follows its task: the same text in the same note, nearest to where it was.
  const selText = useRef<{ key: string | null; text: string | null }>({ key: null, text: null });
  useEffect(() => {
    const { key, text: was } = selText.current;
    selText.current = { key: sel, text: selected?.lineText ?? null };
    // Only when the rows changed under the same selection, not when another task was chosen.
    if (!was || !sel || key !== sel || selected?.lineText === was) return;
    const at = Number(sel.slice(sel.lastIndexOf(":") + 1));
    const path = sel.slice(0, sel.lastIndexOf(":"));
    const moved = rows
      .filter((t) => t.path === path && t.lineText === was)
      .sort((a, b) => Math.abs(a.line - at) - Math.abs(b.line - at))[0];
    if (moved) {
      selText.current = { key: taskKey(moved), text: moved.lineText };
      setSel(taskKey(moved));
    }
  }, [rows, selected, sel, setSel]);
  const failed = useStore(tasksFailed);
  // A new task takes the list's tag and context, so it shows where you are; with none it goes to the Inbox.
  const adds = [view.adds, ctx && `#context/${ctx}`].filter(Boolean).join(" ");
  const search = useRef<HTMLInputElement>(null);
  useTaskListKeys(search);
  return (
    <main className="main">
      <TopBar
        title={view.label}
        sub={all && !failed ? (rows.length === 1 ? "1 task" : `${fmtCount(rows.length)} tasks`) : undefined}
        ask={
          selected
            ? {
                about: `the task ${askQuote(taskLabel(selected.text))}`,
                prompt: `About the task “${taskLabel(selected.text)}” in ${askLink(selected.path)}: `,
              }
            : { about: `your ${view.label.toLowerCase()}`, prompt: `About my ${view.label.toLowerCase()}: ` }
        }
      >
        <span className="seglabel faint">Group by</span>
        <Seg<GroupBy>
          label="Group by"
          value={groupBy}
          onChange={setGroupBy}
          options={[
            ["none", "None", "No groups: one list, in its own order (drag to reorder where the list allows)"],
            ["project", "Project"],
            ["context", "Context"],
            ["due", "Due"],
          ]}
        />
        <button
          type="button"
          className="btn pri"
          title={
            adds
              ? `A task onto the To Do list, with ${adds}${view.adds || view.id === "next" ? `, so it shows in ${view.label}` : ""}`
              : "A task into the Inbox (the To Do list, under Other), to clarify there"
          }
          onClick={(e) => setAdding(e.currentTarget.getBoundingClientRect())}
        >
          <Icon name="plus" size={14} />
          New task
        </button>
      </TopBar>
      <div className="body tasks3">
        <nav className="tagtree" aria-label="Task lists">
          {VIEWS.map((v) => (
            <button
              key={v.id}
              type="button"
              className={`tnode ${v.id === view.id && !savedOn ? "on" : ""}`}
              title={`Show ${v.label.toLowerCase()}`}
              onClick={() => setViewId(v.id as ViewId)}
            >
              <Icon name={v.icon} size={13} />
              <span className="ell">{v.label}</span>
              <span className="n">{fmtCount(counts.get(v.id) ?? 0)}</span>
            </button>
          ))}
          {saved.length > 0 && <div className="sect static">Saved</div>}
          {saved.map((l) => (
            <div key={l.name} className={`tnode saved ${savedOn === l.name ? "on" : ""}`}>
              <button type="button" className="grow ell" title={`Show the saved list “${l.name}”`} onClick={() => applySaved(l)}>
                <Icon name="tag" size={13} />
                {l.name}
              </button>
              <button
                type="button"
                className="ibtn xs"
                aria-label={`Forget “${l.name}”`}
                title={`Forget the saved list “${l.name}”; its tasks stay as they are`}
                onClick={() => settings.update({ taskLists: saved.filter((x) => x.name !== l.name) })}
              >
                <Icon name="x" size={11} />
              </button>
            </div>
          ))}
          {(ctx || effort || groupBy !== "none") && !savedOn && (
            <button
              type="button"
              className="tnode faint"
              title="Save this view, with its context, effort and grouping, as a named list"
              onClick={(e) =>
                setNaming({ at: e.currentTarget.getBoundingClientRect(), name: [ctx && `@${ctx}`, view.label].filter(Boolean).join(" · ") })
              }
            >
              <Icon name="plus" size={13} />
              <span className="ell">Save this list…</span>
            </button>
          )}
        </nav>
        <div className="tpage">
          {view.id === "next" && <FoundTasks />}
          {failed ? (
            <TasksFailed />
          ) : all === null ? (
            <div className="tempty faint">Loading…</div>
          ) : (
            <>
              <div className="tfilters" role="group" aria-label="Filter">
                <SearchBox
                  value={q}
                  onChange={setQ}
                  onKeyDown={(e) => e.key === "Escape" && setQ("")}
                  placeholder="Search tasks"
                  className="tasksq"
                  inputRef={search}
                />
                {contexts.length > 0 && (
                  <button
                    type="button"
                    className={`chip f ${ctx ? "" : "on"}`}
                    aria-pressed={!ctx}
                    title="Show tasks from every context"
                    onClick={() => setCtx("")}
                  >
                    All
                  </button>
                )}
                {contexts.map((c) => (
                  <button
                    key={c}
                    type="button"
                    className={`chip f ${ctx === c ? "on" : ""}`}
                    aria-pressed={ctx === c}
                    title={ctx === c ? "Show tasks in every context" : `Show only tasks in @${c}`}
                    onClick={() => setCtx(ctx === c ? "" : c)}
                  >
                    @{c}
                  </button>
                ))}
                <select
                  title="Show only tasks that take this long or less"
                  className="sel sm"
                  aria-label="Effort"
                  value={effort}
                  onChange={(e) => setEffort(e.target.value)}
                >
                  {EFFORT_LIMITS.map(([v, l]) => (
                    <option key={v} value={v}>
                      {l}
                    </option>
                  ))}
                </select>
              </div>
              <div className="ttable" role="table" aria-label={view.label}>
                {groups.map((g) => (
                  <div key={g.key} className="tb-group" role="rowgroup">
                    {groupBy !== "none" && (
                      <TableBand
                        icon={GROUP_ICON[groupBy]}
                        label={g.label}
                        none={g.key === "" || g.key === "none"}
                        n={g.rows.length}
                        tip={`${fmtCount(g.rows.length)} ${g.rows.length === 1 ? "task" : "tasks"} ${groupTip(groupBy, g.label)}`}
                      />
                    )}
                    <TaskList
                      table
                      rows={g.rows}
                      manual={view.manual && groupBy === "none"}
                      selected={sel}
                      onSelect={(t) => setSel(taskKey(t))}
                      onOpen={() => document.querySelector<HTMLElement>(".tdetail .tdtitle")?.click()}
                      keys
                      empty={
                        dq
                          ? `No tasks in this list match “${dq}”.`
                          : ctx || effort
                            ? "No tasks here match the filter."
                            : "No tasks in this list."
                      }
                    />
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
        {naming && (
          <Popover anchor={naming.at} onClose={() => setNaming(null)} width={260} place="right">
            <div className="newtask row">
              <label className="inp grow">
                <input
                  autoFocus
                  aria-label="The list's name"
                  value={naming.name}
                  onChange={(e) => setNaming({ ...naming, name: e.target.value })}
                  onKeyDown={(e) => e.key === "Enter" && saveList(naming.name)}
                />
              </label>
              <button type="button" className="btn sm pri" title="Save the list under this name (↩)" onClick={() => saveList(naming.name)}>
                Save
              </button>
            </div>
          </Popover>
        )}
        {adding && (
          <Popover anchor={adding} onClose={() => setAdding(null)} width={460}>
            <div className="newtask">
              <CaptureBox autoFocus adds={adds} onDone={(saved) => saved && setAdding(null)} />
            </div>
          </Popover>
        )}
        <aside className="tpane">
          {selected ? <TaskDetail t={selected} /> : <p className="faint">Select a task to see where it lives.</p>}
        </aside>
      </div>
    </main>
  );
}

/** A task date's name in the detail pane, with its icon and what it means. */
function DateLabel({ id }: { id: TaskDateId }) {
  const d = taskDate(id);
  return (
    <dt className="datelabel">
      <Icon name={d.icon} size={13} />
      {d.label}
      <InfoTip name={d.label.toLowerCase()} text={d.explain} />
    </dt>
  );
}

function TaskDetail({ t }: { t: TaskRow }) {
  return (
    <div className="tdetail">
      <div className="row tdhead">
        <button
          type="button"
          className={`cb lg ${t.done ? "on" : ""} ${t.status === "/" && !t.done ? "inprog" : ""}`}
          role="checkbox"
          aria-checked={t.done}
          aria-label={t.done ? "Untick" : "Tick"}
          title={t.done ? "Mark the task not done" : "Mark the task done"}
          onClick={() => void toggleTask(t, !t.done)}
        >
          {t.status === "-" ? (
            <Icon name="minus" size={12} />
          ) : t.done ? (
            <Icon name="check" size={12} />
          ) : t.status === "/" ? (
            <span className="dot" aria-hidden="true" />
          ) : null}
        </button>
        <TaskTitle t={t} />
      </div>
      <dl className="kv">
        <DateLabel id="due" />
        <dd>
          <TaskDateField t={t} kind="due" value={t.due} />
        </dd>
        <DateLabel id="scheduled" />
        <dd>
          <TaskDateField t={t} kind="scheduled" value={t.scheduled} />
        </dd>
        <DateLabel id="start" />
        <dd>
          <TaskDateField t={t} kind="start" value={t.start ?? lineDate(t.lineText, "start")} />
        </dd>
        <DateLabel id="created" />
        <dd>
          <TaskDateField t={t} kind="created" value={t.created ?? lineDate(t.lineText, "created")} />
        </dd>
        {t.doneOn && (
          <>
            <DateLabel id="done" />
            <dd>{fmtShortDate(t.doneOn)}</dd>
          </>
        )}
        {taskFieldsOf(t.text)
          .filter((p) => !["due", "scheduled", "start", "created", "done", "priority"].includes(p.field.id))
          .map((p, i) => (
            <Fragment key={i}>
              <dt className="datelabel">
                <Icon name={fieldIcon(p)} size={13} />
                {p.field.label}
                <InfoTip name={p.field.label.toLowerCase()} text={p.field.explain} />
              </dt>
              <dd>{fieldValue(p, fmtShortDate)}</dd>
            </Fragment>
          ))}
        <TaskAttrs t={t} />
        {t.tags.some((g) => !g.startsWith("context/")) && (
          <>
            <dt className="datelabel">
              Tags
              <InfoTip name="tags" text="The task's other #tags, as written on its line" />
            </dt>
            <dd className="chips">
              {t.tags
                .filter((g) => !g.startsWith("context/"))
                .map((g) => (
                  <span key={g} className="chip">
                    #{g}
                  </span>
                ))}
            </dd>
          </>
        )}
      </dl>
      <div className="eyebrow">Lives in</div>
      <button
        type="button"
        className="card liveslink"
        title="Open the note the task lives in, at its heading"
        onClick={() => openDoc(t.path, t.heading ? { anchor: t.heading } : {})}
      >
        <Icon name="note" style={{ color: "var(--l-note)" }} />
        <span className="t">
          <b className="ell">{t.title}</b>
          <span className="faint">
            line {t.line + 1}
            {t.heading ? ` · under “${t.heading}”` : ""}
          </span>
        </span>
      </button>
      <p className="faint small">Saved in that note as a checklist line.</p>
      <div className="row tdacts">
        <button
          type="button"
          className="btn sm"
          title="Ask the assistant about this task's context and what would help get it done"
          onClick={() => {
            askWith(
              `About this task, from [[${t.title}]]: “${taskLabel(t.text)}”. What's the context, and what would help me get it done?`,
            );
            nav.go("ask");
          }}
        >
          <Icon name="ask" size={12} />
          Ask about this
        </button>
        {!t.done && (
          <button
            type="button"
            className="btn sm"
            title={t.tags.includes("waiting-for") ? "Take the task off the Waiting for list" : "Move the task to Waiting for"}
            onClick={() => void changeTask(t, "waiting")}
          >
            <Icon name="waiting" size={12} />
            {t.tags.includes("waiting-for") ? "No longer waiting" : "Waiting for someone"}
          </button>
        )}
        <TaskMore t={t} />
      </div>
    </div>
  );
}

/** The task's words, in the pane: click (or ↩ in the list) to change them, ↩ to keep, Esc to leave
 *  them. Only where the words can be found on the line once, so its dates and tags stay as they are. */
function TaskTitle({ t }: { t: TaskRow }) {
  const words = taskLabel(t.text);
  const [draft, setDraft] = useState<string | null>(null);
  const editable = !!retitled(t.lineText, words, words || "x");
  if (draft !== null)
    return (
      <input
        className="h2 tdtitle-in grow"
        autoFocus
        aria-label="The task's words"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onFocus={(e) => e.currentTarget.select()}
        onBlur={() => {
          void retitleTask(t, words, draft);
          setDraft(null);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") {
            e.stopPropagation();
            setDraft(null);
          }
        }}
      />
    );
  return editable ? (
    <button type="button" className="h2 tdtitle" title="Change the task's words (its dates and tags stay)" onClick={() => setDraft(words)}>
      <TaskText text={words} />
    </button>
  ) : (
    <h2 className="h2">
      <TaskText text={words} />
    </h2>
  );
}

/** The pane's ⋯ menu: cancel or delete the task, or open its note. */
function TaskMore({ t }: { t: TaskRow }) {
  const [at, setAt] = useState<DOMRect | null>(null);
  const run = (f: () => void) => () => {
    setAt(null);
    f();
  };
  return (
    <>
      <button
        type="button"
        className="btn sm"
        aria-label="More"
        title="Cancel or delete the task, or open its note"
        onClick={(e) => setAt(e.currentTarget.getBoundingClientRect())}
      >
        <Icon name="more" size={12} />
      </button>
      {at && (
        <Popover anchor={at} onClose={() => setAt(null)} width={220}>
          <div className="menu" role="menu">
            <button
              type="button"
              role="menuitem"
              title={`Open ${t.title}${t.heading ? `, at ${t.heading}` : ""}`}
              onClick={run(() => openDoc(t.path, t.heading ? { anchor: t.heading } : {}))}
            >
              <Icon name="note" size={14} />
              Open note
            </button>
            {t.status === "-" ? (
              <button
                type="button"
                role="menuitem"
                title="Make the cancelled task open again"
                onClick={run(() => void changeTask(t, "reopen"))}
              >
                <Icon name="refresh" size={14} />
                Reopen
              </button>
            ) : (
              !t.done && (
                <button
                  type="button"
                  role="menuitem"
                  title="Mark it cancelled; it stays in its note"
                  onClick={run(() => void changeTask(t, "cancel"))}
                >
                  <Icon name="cancelled" size={14} />
                  Cancel task
                </button>
              )
            )}
            <div className="sep" />
            <button
              type="button"
              role="menuitem"
              className="danger"
              title={keyLabel("Takes the line out of its note; ⌘Z puts it back")}
              onClick={run(() => void changeTask(t, "delete"))}
            >
              <Icon name="trash" size={14} />
              Delete task
            </button>
          </div>
        </Popover>
      )}
    </>
  );
}

function TaskDateField({ t, kind, value }: { t: TaskRow; kind: TaskDateKind; value: string | null }) {
  const label = { due: "Due date", scheduled: "Defer until", start: "Start date", created: "Created date" }[kind];
  return <DateField value={value} label={label} onChange={(d) => void setTaskDate(t, kind, d)} />;
}
