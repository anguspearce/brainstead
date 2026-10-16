// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A list of tasks you can work: tick, right-click for due and defer dates, drag to reorder (in
// manual lists). Used by Tasks, Today and the task blocks in documents.

import moment from "moment";
import { askWith } from "./askState";
import { useEffect, useRef, useState } from "react";
import type { TaskDateKind, TaskRow } from "./api";
import { DatePicker } from "./DatePicker";
import { Icon } from "./icons";
import { fmtShortDate, resolveDateWord } from "./md/dates";
import { taskLabel } from "./md/TaskBlock";
import { daysUntil, localToday } from "./md/taskQuery";
import { contextName, contextsInUse, effortMinutes, fmtEffort, projectName, useProjects } from "./gtd";
import { nav, openDoc } from "./nav";
import {
  changeTask,
  moveTask,
  retitled,
  retitleTask,
  retryTasks,
  setTaskDate,
  setTaskGtd,
  tasksFailed,
  toggleTask,
  useAllTasks,
} from "./taskModel";
import { fieldIcon, fieldTip, fieldValue, TASK_FIELDS, taskFieldsOf } from "./taskFields";
import { parseTask, PriorityName } from "./tasksq/fields";
import { urgency } from "./tasksq/task";
import { splitTaskDates, taskDate, TaskDateId } from "./taskDates";
import { useStore } from "./store";
import { useVaultOpening } from "./state";
import { Popover } from "./ui";
import { keyLabel } from "./platform";

export const taskKey = (t: TaskRow) => `${t.path}:${t.line}`;

/** A task's text with its inline markdown shown: `code`, **bold**, *italic*, and [[links]] as their label. */
export function TaskText({ text }: { text: string }) {
  const parts = text.split(/(`[^`]+`|\*\*[^*]+\*\*|\*[^*\s][^*]*\*|!?\[\[[^\]]+\]\])/g);
  return (
    <>
      {parts.map((p, i) => {
        if (i % 2 === 0) return p;
        if (p.startsWith("`")) return <code key={i}>{p.slice(1, -1)}</code>;
        if (p.startsWith("**")) return <b key={i}>{p.slice(2, -2)}</b>;
        if (p.startsWith("*")) return <i key={i}>{p.slice(1, -1)}</i>;
        const inner = p.replace(/^!?\[\[|\]\]$/g, "");
        const [target, alias] = inner.split("|");
        return (
          <span key={i} className="tlinkword">
            {alias ?? target.split("#")[0].split("/").pop()}
          </span>
        );
      })}
    </>
  );
}

function dueChip(due: string, done: boolean) {
  const n = daysUntil(due);
  const tip = n < 0 ? `Overdue by ${-n} day${n === -1 ? "" : "s"}` : n === 0 ? "Due today" : n === 1 ? "Due tomorrow" : `Due in ${n} days`;
  const tone = done ? "" : n < 0 ? "red" : n <= 3 ? "amber" : "";
  const text = n === 0 ? "Today" : n === 1 ? "Tomorrow" : n === -1 ? "Yesterday" : fmtShortDate(due);
  return (
    <span className={`chip ${tone}`} title={`${tip} · ${due}\n${taskDate("due").explain}`}>
      <Icon name={taskDate("due").icon} size={11} />
      {text}
    </span>
  );
}

/** An ⓘ beside a task attribute's name, whose tooltip says what the attribute means. */
export function InfoTip({ name, text }: { name: string; text: string }) {
  return (
    <span className="infotip" tabIndex={0} role="img" aria-label={`About ${name}: ${text}`} data-tip={text}>
      <Icon name="info" size={12} />
    </span>
  );
}

/** A date read from the task's line (for those the index doesn't send). */
export function lineDate(line: string, id: TaskDateId): string | null {
  const p = splitTaskDates(line ?? "").find((x) => typeof x !== "string" && x.date.id === id);
  return p && typeof p !== "string" ? p.day : null;
}

/** What a task query block shows of each task (its `hide`/`show` lines and short mode). */
export interface TaskDisplay {
  /** Fields and parts left out: "due date", "priority", "tags", "backlink"… */
  hidden: Set<string>;
  /** Icons only, with the value in the tooltip. */
  short: boolean;
  urgency: boolean;
}

const statusClass = (t: TaskRow) => (t.status === "-" ? "cancelled" : t.status === "/" ? "inprog" : "");
const urgencyOf = (t: TaskRow) =>
  urgency(parseTask(t.lineText) ?? (parseTask("- [ ] x") as NonNullable<ReturnType<typeof parseTask>>), moment(localToday(), "YYYY-MM-DD"));

/** Which `hide` name covers a field. */
const HIDE_NAME: Record<string, string> = {
  due: "due date",
  scheduled: "scheduled date",
  start: "start date",
  created: "created date",
  done: "done date",
  cancelled: "cancelled date",
  priority: "priority",
  recurrence: "recurrence rule",
  id: "id",
  dependsOn: "depends on",
  onCompletion: "on completion",
};

/** Every field on the task's line as a chip, none as its emoji. Created shows only in the
 *  Tasks screen's detail pane, unless a query block asks for it. */
export function FieldChips({ t, display }: { t: TaskRow; display?: TaskDisplay }) {
  const parts = taskFieldsOf(t.text);
  return (
    <>
      {parts.map((p, i) => {
        const id = p.field.id;
        if (display ? display.hidden.has(HIDE_NAME[id]) : id === "created") return null;
        if (id === "due" && !display?.short) return <span key={i}>{dueChip(p.value, t.done)}</span>;
        const shown = fieldValue(p, fmtShortDate);
        const faint = id === "done" || id === "created" || id === "cancelled" || id === "id" || id === "onCompletion";
        return (
          <span key={i} className={`chip ${faint ? "faint" : ""} f-${id}`} title={fieldTip(p, shown)}>
            <Icon name={fieldIcon(p)} size={11} />
            {!display?.short && shown}
          </span>
        );
      })}
    </>
  );
}

/** A task's contexts, effort and project as chips (stage 6). */
export function GtdChips({ t, project = true }: { t: TaskRow; project?: boolean }) {
  return (
    <>
      {(t.contexts ?? []).map((c) => (
        <span key={c} className="chip ctx" title={`Context: where or how it can be done (#context/${c})`}>
          @{c}
        </span>
      ))}
      {t.effortMin != null && (
        <span className="chip faint" title={`Effort: about ${fmtEffort(t.effortMin)} ([effort:: ${t.effort}])`}>
          <Icon name="stopwatch" size={11} />
          {fmtEffort(t.effortMin)}
        </span>
      )}
      {project && t.project && (
        <button
          type="button"
          className="chip proj"
          title="Its project: open it on the Projects screen"
          onClick={(e) => {
            e.stopPropagation();
            nav.go({ screen: "projects", path: t.project! });
          }}
        >
          <Icon name="project" size={11} />
          {projectName(t.project)}
        </button>
      )}
    </>
  );
}

const KIND_DATE: Record<TaskDateKind, TaskDateId> = { due: "due", scheduled: "scheduled", start: "start", created: "created" };
const valueOf = (t: TaskRow, k: TaskDateKind) =>
  k === "due" ? t.due : k === "scheduled" ? t.scheduled : k === "start" ? t.start : t.created;
/** The menu's heading for each date, set or not. */
const HEAD: Record<TaskDateKind, [set: string, unset: string]> = {
  due: ["Due", "Set due date"],
  scheduled: ["Deferred until", "Defer until"],
  start: ["Starts", "Set start date"],
  created: ["Created", "Set created date"],
};
/** The date's name in tooltips. */
const DATE_NAME: Record<TaskDateKind, string> = { due: "due date", scheduled: "defer date", start: "start date", created: "created date" };
const QUICK: Record<TaskDateKind, string[]> = {
  due: ["today", "tomorrow", "mon", "next week"],
  scheduled: ["tomorrow", "mon", "next week", "+2w"],
  start: ["today", "tomorrow", "mon", "next week"],
  created: ["today", "yesterday"],
};
const wordLabel = (w: string) => (w === "next week" ? "Next week" : w === "+2w" ? "2 weeks" : w[0].toUpperCase() + w.slice(1));

const PRIORITY_CHOICES: [PriorityName, string][] = [
  ["none", "None"],
  ["lowest", "Lowest"],
  ["low", "Low"],
  ["medium", "Medium"],
  ["high", "High"],
  ["highest", "Highest"],
];
const priorityOf = (t: TaskRow) => parseTask(t.lineText)?.priority ?? "none";

function TaskMenu({ t, at, onClose }: { t: TaskRow; at: DOMRect; onClose: () => void }) {
  const [pick, setPick] = useState<TaskDateKind | null>(null);
  // Renaming: its words in a box at the top of the menu, ↩ to keep them, Esc to go back. Only where
  // the words are on the line once, so its dates and tags stay as they are (as the task pane does).
  const [renaming, setRenaming] = useState(false);
  const words = taskLabel(t.text);
  const canRename = !!retitled(t.lineText, words, words || "x");
  // Start and created are folded away unless it has a start date (nearly every task has a created
  // date, stamped on capture, so that alone doesn't open them).
  const [more, setMore] = useState(!!t.start);
  const today = localToday();
  // Two columns when the window can take them beside the row; one on a narrow window.
  const wide = window.innerWidth >= 600;
  const set = (kind: TaskDateKind, d: string | null) => {
    onClose();
    void setTaskDate(t, kind, d);
  };
  const section = (kind: TaskDateKind) => {
    const d = taskDate(KIND_DATE[kind]);
    const v = valueOf(t, kind);
    return (
      <div key={kind} className="mdate">
        <div className="mlabel" title={d.explain}>
          <Icon name={d.icon} size={14} />
          {v ? `${HEAD[kind][0]} ${fmtShortDate(v)}` : HEAD[kind][1]}
          <span className="kbd">{d.shorthand}</span>
        </div>
        <div className="qchips">
          {QUICK[kind].map((w) => (
            <button
              key={w}
              type="button"
              className="chip f"
              title={`Set the ${DATE_NAME[kind]} to ${fmtShortDate(resolveDateWord(w, today) ?? today)}`}
              onClick={() => set(kind, resolveDateWord(w, today))}
            >
              {wordLabel(w)}
            </button>
          ))}
          <button type="button" className="chip f" title={`Pick the ${DATE_NAME[kind]} from a calendar`} onClick={() => setPick(kind)}>
            Pick…
          </button>
          {v && (
            <button
              type="button"
              className="chip f"
              aria-label={`Clear ${HEAD[kind][0].toLowerCase()} date`}
              title={`Remove the ${DATE_NAME[kind]}`}
              onClick={() => set(kind, null)}
            >
              <Icon name="x" size={11} />
              Clear
            </button>
          )}
        </div>
      </div>
    );
  };
  return (
    <Popover anchor={at} onClose={onClose} width={wide ? 540 : 276}>
      <div className={`menu tmenu${wide ? " wide" : ""}`} role="menu">
        {pick ? (
          <DatePicker value={valueOf(t, pick)} label={HEAD[pick][1]} onPick={(d) => set(pick, d)} onCancel={() => setPick(null)} />
        ) : renaming ? (
          <div className="mrename">
            <div className="mlabel">
              <Icon name="pencil" size={14} />
              Rename task
            </div>
            <input
              className="pin"
              autoFocus
              defaultValue={words}
              aria-label="The task's words"
              onFocus={(e) => e.currentTarget.select()}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  const to = e.currentTarget.value;
                  onClose();
                  void retitleTask(t, words, to);
                } else if (e.key === "Escape") {
                  e.stopPropagation();
                  setRenaming(false);
                }
              }}
            />
            <span className="faint small">{keyLabel("↩")} to rename; its dates and tags stay as they are.</span>
          </div>
        ) : (
          <>
            {/* Two columns where the window has room: dates, then the task's fields; its actions below. */}
            <div className="tmcols">
              <div className="tmcol">
                {section("due")}
                {section("scheduled")}
                {more ? (
                  <>
                    {section("start")}
                    {section("created")}
                  </>
                ) : (
                  <button
                    type="button"
                    role="menuitem"
                    className="faint"
                    title="Show the start and created dates too"
                    onClick={() => setMore(true)}
                  >
                    <Icon name="chevdown" size={14} />
                    More dates…
                  </button>
                )}
                <div className="sep" />
                <div className="mlabel" title={TASK_FIELDS.priority.explain}>
                  <Icon name="priority" size={14} />
                  Priority
                </div>
                <div className="qchips">
                  {PRIORITY_CHOICES.map(([p, label]) => (
                    <button
                      key={p}
                      type="button"
                      className={`chip f ${priorityOf(t) === p ? "on" : ""}`}
                      aria-pressed={priorityOf(t) === p}
                      title={p === "none" ? "Remove the priority" : `Set the priority to ${label.toLowerCase()}`}
                      onClick={() => {
                        onClose();
                        void changeTask(t, { priority: p });
                      }}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>
              <div className="tmcol">
                <GtdMenu t={t} onClose={onClose} />
              </div>
            </div>
            {/* Its words and state; a done task it can't rename has none of these, so no empty group. */}
            {(canRename || !t.done || t.status === "-") && <div className="sep" />}
            <div className="tmacts">
              {canRename && (
                <button
                  type="button"
                  role="menuitem"
                  title="Change the task's words (its dates and tags stay)"
                  onClick={() => setRenaming(true)}
                >
                  <Icon name="pencil" size={14} />
                  Rename…
                </button>
              )}
              {t.status !== "/" && !t.done && (
                <button
                  type="button"
                  role="menuitem"
                  title="Mark it as started but not finished"
                  onClick={() => (onClose(), void changeTask(t, "start"))}
                >
                  <Icon name="in-progress" size={14} />
                  Mark in progress
                </button>
              )}
              {!t.done && (
                <button
                  type="button"
                  role="menuitem"
                  title={
                    t.tags.includes("waiting-for")
                      ? "Remove the waiting-for tag"
                      : "Tag it waiting-for, so it shows on the Waiting for list"
                  }
                  onClick={() => (onClose(), void changeTask(t, "waiting"))}
                >
                  <Icon name="waiting" size={14} />
                  {t.tags.includes("waiting-for") ? "No longer waiting for it" : "Waiting for someone"}
                </button>
              )}
              {t.status === "-" || t.status === "/" ? (
                <button
                  type="button"
                  role="menuitem"
                  title={t.status === "-" ? "Make the cancelled task open again" : "Mark it as not started"}
                  onClick={() => (onClose(), void changeTask(t, "reopen"))}
                >
                  <Icon name="refresh" size={14} />
                  {t.status === "-" ? "Reopen" : "Back to to-do"}
                </button>
              ) : (
                !t.done && (
                  <button
                    type="button"
                    role="menuitem"
                    title="Mark it cancelled; it stays in its note"
                    onClick={() => (onClose(), void changeTask(t, "cancel"))}
                  >
                    <Icon name="cancelled" size={14} />
                    Cancel task
                  </button>
                )
              )}
            </div>
            <div className="sep" />
            <button
              type="button"
              role="menuitem"
              title={`Open the note this task is in${t.heading ? `, at ${t.heading}` : ""}`}
              onClick={() => {
                onClose();
                openDoc(t.path, t.heading ? { anchor: t.heading } : {});
              }}
            >
              <Icon name="note" size={14} />
              Open {t.title}
            </button>
            <div className="sep" />
            <button
              type="button"
              role="menuitem"
              className="danger"
              title={keyLabel("Takes the line out of its note; ⌘Z puts it back")}
              onClick={() => (onClose(), void changeTask(t, "delete"))}
            >
              <Icon name="trash" size={14} />
              Delete task
            </button>
          </>
        )}
      </div>
    </Popover>
  );
}

const EFFORTS = ["5m", "15m", "30m", "1h", "2h", "4h"];
const COMMON_CONTEXTS = ["calls", "computer", "errands", "office", "personal"];

/** The task pane's own fields, editable in place: priority, context, effort and project (the
 *  same changes as the task menu's). Rows of a `dl.kv`. */
export function TaskAttrs({ t }: { t: TaskRow }) {
  const all = useAllTasks();
  const projects = useProjects();
  const [typing, setTyping] = useState(false);
  const [q, setQ] = useState("");
  const mine = t.contexts ?? [];
  const known = [...new Set([...mine, ...(all ? contextsInUse(all) : []), ...COMMON_CONTEXTS])].slice(0, 8);
  const toggle = (c: string) => void setTaskGtd(t, { contexts: mine.includes(c) ? mine.filter((x) => x !== c) : [...mine, c] });
  const active = (projects ?? []).filter((p) => p.status === "active" || p.path === t.project);
  const efforts = t.effort && !EFFORTS.includes(t.effort) ? [t.effort, ...EFFORTS] : EFFORTS;
  return (
    <>
      <dt className="datelabel">
        <Icon name="priority" size={13} />
        Priority
        <InfoTip name="priority" text={TASK_FIELDS.priority.explain} />
      </dt>
      <dd>
        <select
          title="How important the task is"
          className="sel sm"
          aria-label="Priority"
          value={priorityOf(t)}
          onChange={(e) => void changeTask(t, { priority: e.target.value as PriorityName })}
        >
          {PRIORITY_CHOICES.map(([p, label]) => (
            <option key={p} value={p}>
              {label}
            </option>
          ))}
        </select>
      </dd>
      <dt className="datelabel">
        <Icon name="tag" size={13} />
        Context
        <InfoTip name="context" text="Where or how it can be done, written as #context/name" />
      </dt>
      <dd className="chips">
        {known.map((c) => (
          <button
            key={c}
            type="button"
            className={`chip f ${mine.includes(c) ? "on" : ""}`}
            aria-pressed={mine.includes(c)}
            title={mine.includes(c) ? `Remove the context @${c}` : `Add the context @${c} (#context/${c})`}
            onClick={() => toggle(c)}
          >
            @{c}
          </button>
        ))}
        {typing ? (
          <input
            className="qin"
            autoFocus
            placeholder="New context"
            aria-label="New context"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onBlur={() => setTyping(false)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setTyping(false);
              if (e.key === "Enter" && contextName(q)) {
                toggle(contextName(q));
                setTyping(false);
              }
            }}
          />
        ) : (
          <button type="button" className="chip f" title="Type a new context to add" onClick={() => (setTyping(true), setQ(""))}>
            Other…
          </button>
        )}
      </dd>
      <dt className="datelabel">
        <Icon name="stopwatch" size={13} />
        Effort
        <InfoTip name="effort" text="About how long it takes, written as [effort:: 15m]" />
      </dt>
      <dd>
        <select
          title="How long the task takes"
          className="sel sm"
          aria-label="Effort"
          value={t.effort ?? ""}
          onChange={(e) => void setTaskGtd(t, { effort: e.target.value || null })}
        >
          <option value="">None</option>
          {efforts.map((e) => (
            <option key={e} value={e}>
              {effortMinutes(e) != null ? fmtEffort(effortMinutes(e)!) : e}
            </option>
          ))}
        </select>
      </dd>
      <dt className="datelabel">
        <Icon name="project" size={13} />
        Project
        <InfoTip name="project" text="The project it moves forward: a link to its Project. note" />
      </dt>
      <dd>
        <select
          title="The project this task is part of"
          className="sel sm grow"
          aria-label="Project"
          value={t.project ?? ""}
          onChange={(e) => void setTaskGtd(t, { project: e.target.value || null })}
        >
          <option value="">None</option>
          {active.map((p) => (
            <option key={p.path} value={p.path}>
              {p.name}
            </option>
          ))}
        </select>
      </dd>
    </>
  );
}

/** The task menu's GTD part: context, effort and project. */
function GtdMenu({ t, onClose }: { t: TaskRow; onClose: () => void }) {
  const all = useAllTasks();
  const projects = useProjects();
  const [typing, setTyping] = useState<"context" | "project" | null>(null);
  const [q, setQ] = useState("");
  const mine = t.contexts ?? [];
  const known = [...new Set([...mine, ...(all ? contextsInUse(all) : []), ...COMMON_CONTEXTS])].slice(0, 8);
  const run = (c: Parameters<typeof setTaskGtd>[1]) => {
    onClose();
    void setTaskGtd(t, c);
  };
  const toggle = (c: string) => run({ contexts: mine.includes(c) ? mine.filter((x) => x !== c) : [...mine, c] });
  const active = (projects ?? []).filter((p) => p.status === "active" || p.path === t.project);
  // The menu's few: its own project first, then those with the newest tasks (by created date).
  const latest = new Map<string, string>();
  for (const x of all ?? []) if (x.project && (x.created ?? "") > (latest.get(x.project) ?? "")) latest.set(x.project, x.created ?? "");
  const recent = [...active].sort(
    (a, b) => +(b.path === t.project) - +(a.path === t.project) || (latest.get(b.path) ?? "").localeCompare(latest.get(a.path) ?? ""),
  );
  const matching = active.filter((p) => !q || p.name.toLowerCase().includes(q.toLowerCase())).slice(0, 6);
  return (
    <>
      <div className="mlabel" title="Where or how it can be done, written as #context/name">
        <Icon name="tag" size={14} />
        Context
        <span className="kbd">@</span>
      </div>
      <div className="qchips">
        {known.map((c) => (
          <button
            key={c}
            type="button"
            className={`chip f ${mine.includes(c) ? "on" : ""}`}
            aria-pressed={mine.includes(c)}
            title={mine.includes(c) ? `Remove the context @${c}` : `Add the context @${c} (#context/${c})`}
            onClick={() => toggle(c)}
          >
            @{c}
          </button>
        ))}
        {typing === "context" ? (
          <input
            className="qin"
            autoFocus
            placeholder="New context"
            aria-label="New context"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && contextName(q)) toggle(contextName(q));
            }}
          />
        ) : (
          <button type="button" className="chip f" title="Type a new context to add" onClick={() => (setTyping("context"), setQ(""))}>
            Other…
          </button>
        )}
      </div>
      <div className="mlabel" title="About how long it takes, written as [effort:: 15m]">
        <Icon name="stopwatch" size={14} />
        {t.effortMin != null ? `Effort ${fmtEffort(t.effortMin)}` : "Effort"}
        <span className="kbd">effort:</span>
      </div>
      <div className="qchips">
        {EFFORTS.map((e) => (
          <button
            key={e}
            type="button"
            className={`chip f ${t.effort === e ? "on" : ""}`}
            title={`Set the effort to about ${fmtEffort(effortMinutes(e)!)}`}
            onClick={() => run({ effort: e })}
          >
            {fmtEffort(effortMinutes(e)!)}
          </button>
        ))}
        {t.effort && (
          <button
            type="button"
            className="chip f"
            aria-label="Clear effort"
            title="Remove the effort"
            onClick={() => run({ effort: null })}
          >
            <Icon name="x" size={11} />
            Clear
          </button>
        )}
      </div>
      <div className="mlabel" title="The project it moves forward: a link to its Project. note">
        <Icon name="project" size={14} />
        {t.project ? `Project ${projectName(t.project)}` : "Project"}
      </div>
      <div className="qchips">
        {typing === "project" ? (
          <>
            <input
              className="qin"
              autoFocus
              placeholder="Find a project"
              aria-label="Find a project"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
            {matching.map((p) => (
              <button
                key={p.path}
                type="button"
                className={`chip f pname ${t.project === p.path ? "on" : ""}`}
                title={t.project === p.path ? `Already part of ${p.name}` : `Make it part of the project ${p.name}`}
                onClick={() => run({ project: p.path })}
              >
                {p.name}
              </button>
            ))}
            {!matching.length && <span className="faint small">No active project matches.</span>}
          </>
        ) : (
          <>
            {recent.slice(0, 3).map((p) => (
              <button
                key={p.path}
                type="button"
                className={`chip f pname ${t.project === p.path ? "on" : ""}`}
                title={t.project === p.path ? `Already part of ${p.name}` : `Make it part of the project ${p.name}`}
                onClick={() => run({ project: p.path })}
              >
                {p.name}
              </button>
            ))}
            {active.length > 0 && (
              <button type="button" className="chip f" title="Search all active projects" onClick={() => (setTyping("project"), setQ(""))}>
                Find…
              </button>
            )}
            {!active.length && <span className="faint small">No active projects yet.</span>}
          </>
        )}
        {t.project && (
          <button
            type="button"
            className="chip f"
            aria-label="Clear project"
            title="Take it out of its project"
            onClick={() => run({ project: null })}
          >
            <Icon name="x" size={11} />
            Clear
          </button>
        )}
      </div>
    </>
  );
}

export function TaskList({
  rows,
  manual,
  selected,
  onSelect,
  showNote = true,
  empty = "Nothing here.",
  display,
  rowAction,
  table,
  hideTags,
  keys,
  onOpen,
  hideProject,
}: {
  rows: TaskRow[];
  manual: boolean;
  selected?: string | null;
  onSelect?: (t: TaskRow) => void;
  showNote?: boolean;
  empty?: string;
  /** A task query's layout options (`hide`, `show`, short mode). */
  display?: TaskDisplay;
  /** A button at the end of each row (Today's Draft nudge). */
  rowAction?: (t: TaskRow) => React.ReactNode;
  /** Flat rows with dividers, as a table's (the Tasks screen), rather than in a card. */
  table?: boolean;
  /** Tags not worth repeating on every row (#waiting-for in Today's Waiting for band). */
  hideTags?: string[];
  /** Keyboard: ↑ ↓ choose a row, ↩ opens it, Space ticks it, ⌫ deletes it (the app's screens;
   *  not the task blocks in a note, where the keys are the editor's). */
  keys?: boolean;
  /** ↩ on a row; without it, ↩ opens the note the task is in. */
  onOpen?: (t: TaskRow) => void;
  /** Inside a project: its chip on every row says nothing. */
  hideProject?: boolean;
}) {
  const [menu, setMenu] = useState<{ t: TaskRow; at: DOMRect } | null>(null);
  // The keyboard's row where nothing else keeps a selection (Today); the Tasks screen's is `selected`.
  const [cursor, setCursor] = useState<string | null>(null);
  // While dragging: the list in its new order, shown until the vault reloads it.
  const [order, setOrder] = useState<TaskRow[] | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const list = useRef<HTMLUListElement>(null);
  useEffect(() => setOrder(null), [rows]);
  // Screenshot mode: a task's menu open (scene.ts `taskMenu`), once its row is there.
  useEffect(() => {
    const want = document.documentElement.dataset.taskMenu;
    const t = want ? rows.find((r) => taskKey(r) === want) : null;
    const btn = t && list.current?.querySelector<HTMLElement>(`li[data-key="${CSS.escape(want!)}"] .tmore`);
    if (!t || !btn) return;
    delete document.documentElement.dataset.taskMenu;
    setMenu({ t, at: btn.getBoundingClientRect() });
  }, [rows]);
  const shown = order ?? rows;

  const startDrag = (e: React.PointerEvent, t: TaskRow) => {
    if (!manual || e.button !== 0) return;
    e.preventDefault();
    const startY = e.clientY;
    let moved = false;
    let cur = [...rows];
    const move = (m: PointerEvent) => {
      if (!moved && Math.abs(m.clientY - startY) < 4) return;
      moved = true;
      setDragging(taskKey(t));
      const items = [...(list.current?.querySelectorAll<HTMLElement>("li[data-key]") ?? [])];
      const others = cur.filter((x) => x !== t);
      let at = others.length;
      for (let k = 0, j = 0; k < items.length; k++) {
        if (items[k].dataset.key === taskKey(t)) continue;
        const r = items[k].getBoundingClientRect();
        if (m.clientY < r.top + r.height / 2) {
          at = j;
          break;
        }
        j++;
      }
      cur = [...others.slice(0, at), t, ...others.slice(at)];
      setOrder(cur);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setDragging(null);
      // A move that couldn't be written goes back to the order on disk.
      if (moved && cur.indexOf(t) !== rows.indexOf(t)) void moveTask(cur, t).then((ok) => ok || setOrder(null));
      else setOrder(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const cur = onSelect ? selected : cursor;
  const pick = (t: TaskRow | undefined) => {
    if (!t) return;
    if (onSelect) onSelect(t);
    else setCursor(taskKey(t));
    // Keep the row in sight once it's drawn as chosen.
    requestAnimationFrame(() =>
      list.current?.querySelector<HTMLElement>(`li[data-key="${CSS.escape(taskKey(t))}"]`)?.scrollIntoView({ block: "nearest" }),
    );
  };
  const onKey = (e: React.KeyboardEvent<HTMLUListElement>) => {
    if (e.metaKey || e.ctrlKey || e.altKey || typingIn(e.target)) return;
    const i = shown.findIndex((t) => taskKey(t) === cur);
    const t = i >= 0 ? shown[i] : undefined;
    // A row's own buttons keep ↩ and Space.
    const onButton = (e.target as HTMLElement).closest?.("button") && e.target !== e.currentTarget;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const d = e.key === "ArrowDown" ? 1 : -1;
      e.preventDefault();
      if (i < 0) pick(d > 0 ? shown[0] : shown[shown.length - 1]);
      else if (shown[i + d]) pick(shown[i + d]);
      else crossList(list.current, d);
    } else if (!t || onButton) return;
    else if (e.key === "Enter") {
      e.preventDefault();
      if (onOpen) onOpen(t);
      else openDoc(t.path, t.heading ? { anchor: t.heading } : {});
    } else if (e.key === " ") {
      e.preventDefault();
      void toggleTask(t, !t.done);
    } else if (e.key === "Backspace" || e.key === "Delete") {
      e.preventDefault();
      // The row below takes its place; one further down the same note moves up a line.
      const next = shown[i + 1] ?? shown[i - 1];
      void changeTask(t, "delete");
      if (next) {
        const moved = { ...next, line: next.path === t.path && next.line > t.line ? next.line - 1 : next.line };
        if (onSelect) onSelect(moved);
        else setCursor(taskKey(moved));
      }
    }
  };

  if (!rows.length) return <div className="tempty faint">{empty}</div>;
  return (
    <>
      <ul
        className={`tlist ${table ? "table" : ""} ${keys ? "keys" : ""}`}
        ref={list}
        tabIndex={keys ? 0 : undefined}
        aria-label={keys ? "Tasks: ↑ ↓ to choose, ↩ to open, Space to tick, ⌫ to delete" : undefined}
        onKeyDown={keys ? onKey : undefined}
        onFocus={(e) => {
          // Arrived from the list above or below by ↑ ↓: start at its near end.
          const from = e.currentTarget.dataset.enter;
          if (e.target !== e.currentTarget || !from) return;
          delete e.currentTarget.dataset.enter;
          pick(from === "first" ? shown[0] : shown[shown.length - 1]);
        }}
      >
        {shown.map((t) => {
          const k = taskKey(t);
          return (
            <li
              key={k}
              data-key={k}
              className={`trow ${table ? "tb-row" : ""} ${t.done ? "done" : ""} ${statusClass(t)} ${selected === k ? "sel" : ""} ${!onSelect && cursor === k ? "kcur" : ""} ${dragging === k ? "dragging" : ""}`}
              onClick={() => (onSelect ? onSelect(t) : keys && setCursor(k))}
              onContextMenu={(e) => {
                // The task's menu only, not also the note's around it (a task query or Dataview in a note).
                e.preventDefault();
                e.stopPropagation();
                setMenu({ t, at: new DOMRect(e.clientX, e.clientY, 0, 0) });
              }}
            >
              {manual && (
                <span className="grip" onPointerDown={(e) => startDrag(e, t)} aria-hidden="true">
                  <Icon name="grip" size={12} />
                </span>
              )}
              <button
                type="button"
                className={`cb ${t.done ? "on" : ""}`}
                role="checkbox"
                aria-checked={t.done}
                aria-label={t.done ? `Untick ${taskLabel(t.text)}` : `Tick ${taskLabel(t.text)}`}
                title={t.done ? "Mark it not done" : "Mark it done"}
                onClick={(e) => {
                  e.stopPropagation();
                  void toggleTask(t, !t.done);
                }}
              >
                {t.status === "-" ? (
                  <Icon name="minus" size={11} />
                ) : t.done ? (
                  <Icon name="check" size={11} />
                ) : t.status === "/" ? (
                  // Started: a dot in the box, not a fill that would read as how far along it is.
                  <span className="dot" aria-hidden="true" />
                ) : null}
              </button>
              <div className="t">
                <div className="tt">
                  <TaskText text={taskLabel(t.text)} />
                </div>
                <div className="ts">
                  <FieldChips t={t} display={display} />
                  {display?.urgency && (
                    <span
                      className="chip faint"
                      title={`Urgency: ${urgencyOf(t).toFixed(2)}\nHow pressing it is, from its due, deferred and start dates and its priority.`}
                    >
                      {urgencyOf(t).toFixed(2)}
                    </span>
                  )}
                  <GtdChips t={t} project={!hideProject} />
                  {!display?.hidden.has("tags") &&
                    t.tags
                      .filter((g) => !g.startsWith("context/") && !hideTags?.includes(g))
                      .map((g) => (
                        <span key={g} className="tag">
                          #{g}
                        </span>
                      ))}
                  {/* A task in its project's own note: the project chip already says where it is. */}
                  {showNote && !display?.hidden.has("backlink") && !(t.project === t.path && !hideProject) && (
                    <button
                      type="button"
                      className="tlink"
                      title={`Open ${t.title}${t.heading ? `, at ${t.heading}` : ""}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        openDoc(t.path, t.heading ? { anchor: t.heading } : {});
                      }}
                    >
                      {t.title}
                    </button>
                  )}
                </div>
              </div>
              {rowAction?.(t)}
              <button
                type="button"
                className="ibtn tmore"
                aria-label="Task actions"
                title="Dates, priority, context and more for this task"
                onClick={(e) => {
                  e.stopPropagation();
                  setMenu({ t, at: e.currentTarget.getBoundingClientRect() });
                }}
              >
                <Icon name="more" size={14} />
              </button>
            </li>
          );
        })}
      </ul>
      {menu && <TaskMenu t={menu.t} at={menu.at} onClose={() => setMenu(null)} />}
    </>
  );
}

/** In a text box or the editor, where the list's keys are for typing. */
export const typingIn = (el: EventTarget | null) =>
  !!(el as HTMLElement | null)?.closest?.("input, textarea, select, [contenteditable]:not([contenteditable=false])");

/** Moves the keyboard to the task list above or below this one on the screen (Today's bands, a
 *  grouped Tasks list). */
function crossList(from: HTMLElement | null, d: number) {
  const lists = [...document.querySelectorAll<HTMLElement>("ul.tlist.keys")];
  const to = lists[lists.indexOf(from!) + d];
  if (!from || !to) return;
  to.dataset.enter = d > 0 ? "first" : "last";
  to.focus();
}

/** A screen's keys around its task lists: `/` to the search box, if there is one, and ↑ ↓ into
 *  the first list when nothing has the keyboard yet. */
export function useTaskListKeys(search?: React.RefObject<HTMLInputElement | null>) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey || e.defaultPrevented || typingIn(e.target)) return;
      if (document.querySelector(".dialog, .popover")) return;
      if (e.key === "/" && search?.current) {
        search.current.focus();
        search.current.select();
        e.preventDefault();
      } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && (e.target === document.body || e.target === document.documentElement)) {
        const first = document.querySelector<HTMLElement>("ul.tlist.keys");
        if (!first) return;
        first.dataset.enter = e.key === "ArrowDown" ? "first" : "last";
        first.focus();
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [search]);
}

/** Said in place of a task list that couldn't be read, with Retry. Nothing while the tasks load fine. */
export function TasksFailed() {
  const err = useStore(tasksFailed);
  return err ? <LoadFailed what="the tasks" error={err} onRetry={retryTasks} /> : null;
}

/** "Couldn't load …" with a Retry button, where an empty list would say something untrue. While the
 *  vault is still opening a read can fail only because it isn't ready yet: it says it's loading,
 *  and the view reloads once the vault is ready. */
export function LoadFailed({ what, error, onRetry, compact }: { what: string; error?: string; onRetry: () => void; compact?: boolean }) {
  const opening = useVaultOpening();
  if (opening)
    return (
      <div className={`loadfail loading ${compact ? "compact" : ""}`} role="status">
        <span className="spin" />
        <span className="grow">Loading {what}…</span>
      </div>
    );
  return (
    <div className={`loadfail ${compact ? "compact" : ""}`} role="alert" title={error}>
      <Icon name="alert" size={compact ? 13 : 15} />
      <span className="grow">Couldn’t load {what}.</span>
      <button type="button" className="btn sm" title={`Try loading ${what} again`} onClick={onRetry}>
        <Icon name="refresh" size={12} />
        Retry
      </button>
    </div>
  );
}

/** A Waiting for row's button: Ask drafts a short nudge to copy into Outlook or Teams. Compact (Today)
 *  is an icon that shows on the row's hover, focus or selection. */
export function DraftNudge({ t, compact }: { t: TaskRow; compact?: boolean }) {
  return (
    <button
      type="button"
      className={compact ? "ibtn nudge" : "btn sm"}
      aria-label={compact ? "Draft nudge" : undefined}
      title="Draft nudge: Ask drafts a short nudge to copy into Outlook or Teams"
      onClick={(e) => {
        e.stopPropagation();
        askWith(
          `Draft a short, friendly nudge for this, to copy into Outlook or Teams (don't send anything):\n- ${t.text} (in [[${t.title}]])`,
        );
        nav.go("ask");
      }}
    >
      <Icon name="send" size={compact ? 14 : 12} />
      {!compact && "Draft nudge"}
    </button>
  );
}
