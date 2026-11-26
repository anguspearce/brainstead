// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A list of tasks you can work: tick, click a chip to change its field, right-click or ⋯ for its menu, drag to reorder (in
// manual lists). Used by Tasks, Today and the task blocks in documents.

import { UpdateForm } from "./UpdateBox";
import moment from "moment";
import { askWith } from "./askState";
import { Fragment, useEffect, useRef, useState } from "react";
import { iconizeEmoji } from "./md/fieldEmoji";
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
import { fieldIcon, fieldTip, fieldValue, priorityOf, TASK_FIELDS, taskFieldsOf } from "./taskFields";
import { parseTask, PriorityName } from "./tasksq/fields";
import { urgency } from "./tasksq/task";
import { splitTaskDates, taskDate, TaskDateId } from "./taskDates";
import { useStore } from "./store";
import { useVaultOpening } from "./state";
import { Popover } from "./ui";
import { keyLabel } from "./platform";

export const taskKey = (t: TaskRow) => `${t.path}:${t.line}`;

/** A task's text with its inline markdown shown: `code`, **bold**, *italic*, and [[links]] as their label
 *  (a project's link as the project's name, as its chip shows it). */
export function TaskText({ text }: { text: string }) {
  const parts = text.split(/(`[^`]+`|\*\*[^*]+\*\*|\*[^*\s][^*]*\*|!?\[\[[^\]]+\]\])/g);
  return (
    <>
      {parts.map((p, i) => {
        // Plain words, with any Tasks-format emoji drawn as their field's icon.
        if (i % 2 === 0) return <Fragment key={i}>{iconizeEmoji(p)}</Fragment>;
        if (p.startsWith("`")) return <code key={i}>{p.slice(1, -1)}</code>;
        if (p.startsWith("**")) return <b key={i}>{p.slice(2, -2)}</b>;
        if (p.startsWith("*")) return <i key={i}>{p.slice(1, -1)}</i>;
        const inner = p.replace(/^!?\[\[|\]\]$/g, "");
        const [target, alias] = inner.split("|");
        return (
          <span key={i} className="tlinkword">
            {alias ?? projectName(target.split("#")[0])}
          </span>
        );
      })}
    </>
  );
}

/** A due date on a row: its words (Today, Tomorrow, a date), colour and tooltip. */
function dueLook(due: string, done: boolean) {
  const n = daysUntil(due);
  const tip = n < 0 ? `Overdue by ${-n} day${n === -1 ? "" : "s"}` : n === 0 ? "Due today" : n === 1 ? "Due tomorrow" : `Due in ${n} days`;
  const tone = done ? "" : n < 0 ? "red" : n <= 3 ? "amber" : "";
  const text = n === 0 ? "Today" : n === 1 ? "Tomorrow" : n === -1 ? "Yesterday" : fmtShortDate(due);
  return { tip: `${tip} · ${due}\n${taskDate("due").explain}`, tone, text };
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

/** Where a row's chip opens its field's picker: the field, and the chip to place the picker by. */
export type OnField = (f: TaskField, at: DOMRect) => void;

/** The line's fields a chip can change: its dates and priority. */
const EDITABLE: Partial<Record<string, TaskField>> = {
  due: "due",
  scheduled: "scheduled",
  start: "start",
  created: "created",
  priority: "priority",
};

/** A row's chip: a button opening its field's picker where the list can edit (onField), else plain. */
function Chip({
  field,
  onField,
  className,
  title,
  hint = "Click to change it",
  children,
}: {
  field?: TaskField;
  onField?: OnField;
  className: string;
  title: string;
  hint?: string;
  children: React.ReactNode;
}) {
  if (!field || !onField)
    return (
      <span className={className} title={title}>
        {children}
      </span>
    );
  return (
    <button
      type="button"
      className={`${className} ed`}
      title={`${title}\n${hint}`}
      onClick={(e) => {
        e.stopPropagation();
        onField(field, e.currentTarget.getBoundingClientRect());
      }}
    >
      {children}
    </button>
  );
}

/** Every field on the task's line as a chip, none as its emoji. Created shows only in the
 *  Tasks screen's detail pane, unless a query block asks for it. With onField, its dates and
 *  priority are buttons that open their picker. */
export function FieldChips({ t, display, onField }: { t: TaskRow; display?: TaskDisplay; onField?: OnField }) {
  const parts = taskFieldsOf(t.text);
  return (
    <>
      {parts.map((p, i) => {
        const id = p.field.id;
        if (display ? display.hidden.has(HIDE_NAME[id]) : id === "created") return null;
        if (id === "due" && !display?.short) {
          const d = dueLook(p.value, t.done);
          return (
            <Chip key={i} field="due" onField={onField} className={`chip f-due ${d.tone}`} title={d.tip}>
              <Icon name={taskDate("due").icon} size={11} />
              {d.text}
            </Chip>
          );
        }
        const shown = fieldValue(p, fmtShortDate);
        const faint = id === "done" || id === "created" || id === "cancelled" || id === "id" || id === "onCompletion";
        return (
          <Chip
            key={i}
            field={EDITABLE[id]}
            onField={onField}
            className={`chip ${faint ? "faint" : ""} f-${id}`}
            title={fieldTip(p, shown)}
          >
            <Icon name={fieldIcon(p)} size={11} />
            {!display?.short && shown}
          </Chip>
        );
      })}
    </>
  );
}

/** A task's contexts, effort and project as chips (stage 6). With onField each opens its picker;
 *  the project's offers to open the project too. */
export function GtdChips({ t, project = true, onField }: { t: TaskRow; project?: boolean; onField?: OnField }) {
  return (
    <>
      {(t.contexts ?? []).map((c) => (
        <Chip key={c} field="context" onField={onField} className="chip ctx" title={`Context: where or how it can be done (#context/${c})`}>
          @{c}
        </Chip>
      ))}
      {t.effortMin != null && (
        <Chip
          field="effort"
          onField={onField}
          className="chip faint f-effort"
          title={`Effort: about ${fmtEffort(t.effortMin)} ([effort:: ${t.effort}])`}
        >
          <Icon name="stopwatch" size={11} />
          {fmtEffort(t.effortMin)}
        </Chip>
      )}
      {project &&
        t.project &&
        (onField ? (
          <Chip
            field="project"
            onField={onField}
            className="chip proj"
            title={`Project: ${projectName(t.project)}`}
            hint="Click to open the project or change it"
          >
            <Icon name="project" size={11} />
            {projectName(t.project)}
          </Chip>
        ) : (
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
        ))}
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

/** A field the row's chips, the ⋯ menu's More… and the task pane each edit, with the same quick
 *  picks from the sections below, so the three can't drift. */
export type TaskField = TaskDateKind | "priority" | "context" | "effort" | "project";

/** What a row's + offers, when the task has none of it (created is on nearly every task, so it's in More…). */
const ADDABLE: { f: TaskField; label: string; icon: string }[] = [
  { f: "due", label: "Due date", icon: taskDate("due").icon },
  { f: "scheduled", label: "Defer until", icon: taskDate("scheduled").icon },
  { f: "start", label: "Start date", icon: taskDate("start").icon },
  { f: "priority", label: "Priority", icon: "priority" },
  { f: "context", label: "Context", icon: "tag" },
  { f: "effort", label: "Effort", icon: "stopwatch" },
  { f: "project", label: "Project", icon: "project" },
];
const hasField = (t: TaskRow, f: TaskField) =>
  f === "priority"
    ? priorityOf(t) !== "none"
    : f === "context"
      ? !!t.contexts?.length
      : f === "effort"
        ? t.effortMin != null
        : f === "project"
          ? !!t.project
          : !!valueOf(t, f);

/** One date with its quick picks, Pick… (a calendar, onPick) and Clear. */
function DateSection({ t, kind, done, onPick }: { t: TaskRow; kind: TaskDateKind; done: () => void; onPick: (k: TaskDateKind) => void }) {
  const d = taskDate(KIND_DATE[kind]);
  const v = valueOf(t, kind);
  const today = localToday();
  const set = (day: string | null) => {
    done();
    void setTaskDate(t, kind, day);
  };
  return (
    <div className="mdate">
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
            onClick={() => set(resolveDateWord(w, today))}
          >
            {wordLabel(w)}
          </button>
        ))}
        <button type="button" className="chip f" title={`Pick the ${DATE_NAME[kind]} from a calendar`} onClick={() => onPick(kind)}>
          Pick…
        </button>
        {v && (
          <button
            type="button"
            className="chip f"
            aria-label={`Clear ${HEAD[kind][0].toLowerCase()} date`}
            title={`Remove the ${DATE_NAME[kind]}`}
            onClick={() => set(null)}
          >
            <Icon name="x" size={11} />
            Clear
          </button>
        )}
      </div>
    </div>
  );
}

function PrioritySection({ t, done }: { t: TaskRow; done: () => void }) {
  return (
    <>
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
              done();
              void changeTask(t, { priority: p });
            }}
          >
            {label}
          </button>
        ))}
      </div>
    </>
  );
}

const EFFORTS = ["5m", "15m", "30m", "1h", "2h", "4h"];
const COMMON_CONTEXTS = ["calls", "computer", "errands", "office", "personal"];

function ContextSection({ t, done }: { t: TaskRow; done: () => void }) {
  const all = useAllTasks();
  const [typing, setTyping] = useState(false);
  const [q, setQ] = useState("");
  const mine = t.contexts ?? [];
  const known = [...new Set([...mine, ...(all ? contextsInUse(all) : []), ...COMMON_CONTEXTS])].slice(0, 8);
  const toggle = (c: string) => {
    done();
    void setTaskGtd(t, { contexts: mine.includes(c) ? mine.filter((x) => x !== c) : [...mine, c] });
  };
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
        {typing ? (
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
          <button type="button" className="chip f" title="Type a new context to add" onClick={() => (setTyping(true), setQ(""))}>
            Other…
          </button>
        )}
      </div>
    </>
  );
}

function EffortSection({ t, done }: { t: TaskRow; done: () => void }) {
  const run = (effort: string | null) => {
    done();
    void setTaskGtd(t, { effort });
  };
  return (
    <>
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
            onClick={() => run(e)}
          >
            {fmtEffort(effortMinutes(e)!)}
          </button>
        ))}
        {t.effort && (
          <button type="button" className="chip f" aria-label="Clear effort" title="Remove the effort" onClick={() => run(null)}>
            <Icon name="x" size={11} />
            Clear
          </button>
        )}
      </div>
    </>
  );
}

function ProjectSection({ t, done }: { t: TaskRow; done: () => void }) {
  const all = useAllTasks();
  const projects = useProjects();
  const [typing, setTyping] = useState(false);
  const [q, setQ] = useState("");
  const run = (project: string | null) => {
    done();
    void setTaskGtd(t, { project });
  };
  const active = (projects ?? []).filter((p) => p.status === "active" || p.path === t.project);
  // The few shown: its own project first, then those with the newest tasks (by created date).
  const latest = new Map<string, string>();
  for (const x of all ?? []) if (x.project && (x.created ?? "") > (latest.get(x.project) ?? "")) latest.set(x.project, x.created ?? "");
  const recent = [...active].sort(
    (a, b) => +(b.path === t.project) - +(a.path === t.project) || (latest.get(b.path) ?? "").localeCompare(latest.get(a.path) ?? ""),
  );
  const matching = active.filter((p) => !q || p.name.toLowerCase().includes(q.toLowerCase())).slice(0, 6);
  const choice = (p: { path: string; name: string }) => (
    <button
      key={p.path}
      type="button"
      className={`chip f pname ${t.project === p.path ? "on" : ""}`}
      title={t.project === p.path ? `Already part of ${p.name}` : `Make it part of the project ${p.name}`}
      onClick={() => run(p.path)}
    >
      {p.name}
    </button>
  );
  return (
    <>
      <div className="mlabel" title="The project it moves forward: a link to its Project. note">
        <Icon name="project" size={14} />
        {t.project ? `Project ${projectName(t.project)}` : "Project"}
      </div>
      <div className="qchips">
        {typing ? (
          <>
            <input
              className="qin"
              autoFocus
              placeholder="Find a project"
              aria-label="Find a project"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
            {matching.map(choice)}
            {!matching.length && <span className="faint small">No active project matches.</span>}
          </>
        ) : (
          <>
            {recent.slice(0, 3).map(choice)}
            {active.length > 0 && (
              <button type="button" className="chip f" title="Search all active projects" onClick={() => (setTyping(true), setQ(""))}>
                Find…
              </button>
            )}
            {!active.length && <span className="faint small">No active projects yet.</span>}
          </>
        )}
        {t.project && (
          <button type="button" className="chip f" aria-label="Clear project" title="Take it out of its project" onClick={() => run(null)}>
            <Icon name="x" size={11} />
            Clear
          </button>
        )}
      </div>
    </>
  );
}

/** One field's section, as a chip's picker, More… and the pane show it. */
function FieldSection({ t, f, done, onPick }: { t: TaskRow; f: TaskField; done: () => void; onPick: (k: TaskDateKind) => void }) {
  if (f === "priority") return <PrioritySection t={t} done={done} />;
  if (f === "context") return <ContextSection t={t} done={done} />;
  if (f === "effort") return <EffortSection t={t} done={done} />;
  if (f === "project") return <ProjectSection t={t} done={done} />;
  return <DateSection t={t} kind={f} done={done} onPick={onPick} />;
}

/** Pick… on a date: the calendar shown in place of the fields until a day is picked or it's cancelled. */
function useDatePick(t: TaskRow, done: () => void) {
  const [pick, setPick] = useState<TaskDateKind | null>(null);
  const picker = pick ? (
    <DatePicker
      value={valueOf(t, pick)}
      label={HEAD[pick][1]}
      onPick={(d) => {
        setPick(null);
        done();
        void setTaskDate(t, pick, d);
      }}
      onCancel={() => setPick(null)}
    />
  ) : null;
  return [picker, setPick] as const;
}

/** The task's words in a box: ↩ to keep them, Esc to go back. Only where the words are on the
 *  line once (canRename), so its dates and tags stay as they are. */
function RenameBox({ t, done, onBack }: { t: TaskRow; done: () => void; onBack: () => void }) {
  const words = taskLabel(t.text);
  return (
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
            done();
            void retitleTask(t, words, to);
          } else if (e.key === "Escape") {
            e.stopPropagation();
            onBack();
          }
        }}
      />
      <span className="faint small">{keyLabel("↩")} to rename; its dates and tags stay as they are.</span>
    </div>
  );
}

const canRename = (t: TaskRow) => {
  const words = taskLabel(t.text);
  return !!retitled(t.lineText, words, words || "x");
};

/** The task's verbs: update, rename, in progress, waiting for, cancel; More… (the menu's fields); its note; delete.
 *  `lead`: fields above them (the pane), so a line before them. */
function TaskActions({
  t,
  done,
  item,
  lead,
  onRename,
  onMore,
  onUpdate,
}: {
  t: TaskRow;
  done: () => void;
  item?: string;
  lead?: boolean;
  onRename: () => void;
  onMore?: () => void;
  /** Update…: say what's happened, for an assistant to apply (src/updates.ts). */
  onUpdate?: () => void;
}) {
  const rename = canRename(t);
  // A done task it can't rename has none of its state verbs, so no empty group.
  const any = rename || !t.done || t.status === "-";
  return (
    <>
      {lead && any && <div className="sep" />}
      {onUpdate && !t.done && (
        <button
          type="button"
          role={item}
          title="Say what's happened (“Maya has replied”) and an assistant applies it in the background: done, a new date, a follow-up"
          onClick={onUpdate}
        >
          <Icon name="zap" size={14} />
          Update…
        </button>
      )}
      {rename && (
        <button type="button" role={item} title="Change the task's words (its dates and tags stay)" onClick={onRename}>
          <Icon name="pencil" size={14} />
          Rename…
        </button>
      )}
      {t.status !== "/" && !t.done && (
        <button type="button" role={item} title="Mark it as started but not finished" onClick={() => (done(), void changeTask(t, "start"))}>
          <Icon name="in-progress" size={14} />
          Mark in progress
        </button>
      )}
      {!t.done && (
        <button
          type="button"
          role={item}
          title={t.tags.includes("waiting-for") ? "Remove the waiting-for tag" : "Tag it waiting-for, so it shows on the Waiting for list"}
          onClick={() => (done(), void changeTask(t, "waiting"))}
        >
          <Icon name="waiting" size={14} />
          {t.tags.includes("waiting-for") ? "No longer waiting for it" : "Waiting for someone"}
        </button>
      )}
      {t.status === "-" || t.status === "/" ? (
        <button
          type="button"
          role={item}
          title={t.status === "-" ? "Make the cancelled task open again" : "Mark it as not started"}
          onClick={() => (done(), void changeTask(t, "reopen"))}
        >
          <Icon name="refresh" size={14} />
          {t.status === "-" ? "Reopen" : "Back to to-do"}
        </button>
      ) : (
        !t.done && (
          <button
            type="button"
            role={item}
            title="Mark it cancelled; it stays in its note"
            onClick={() => (done(), void changeTask(t, "cancel"))}
          >
            <Icon name="cancelled" size={14} />
            Cancel task
          </button>
        )
      )}
      {onMore && (
        <>
          {any && <div className="sep" />}
          <button type="button" role={item} title="Its dates, priority, context, effort and project" onClick={onMore}>
            <Icon name="list" size={14} />
            More…
            <Icon name="chevright" size={14} className="mend" />
          </button>
        </>
      )}
      {(lead || any || onMore) && <div className="sep" />}
      <button
        type="button"
        role={item}
        title={`Open the note this task is in${t.heading ? `, at ${t.heading}` : ""}`}
        onClick={() => {
          done();
          openDoc(t.path, t.heading ? { anchor: t.heading } : {});
        }}
      >
        <Icon name="note" size={14} />
        Open {t.title}
      </button>
      <div className="sep" />
      <button
        type="button"
        role={item}
        className="danger"
        title={keyLabel("Takes the line out of its note; ⌘Z puts it back")}
        onClick={() => (done(), void changeTask(t, "delete"))}
      >
        <Icon name="trash" size={14} />
        Delete task
      </button>
    </>
  );
}

/** What a task's popover shows: the ⋯ menu's verbs, its More… (every field), a row's + (fields
 *  it hasn't got), renaming, or one field's picker (a chip). */
export type TaskPopView = "menu" | "more" | "add" | "rename" | "update" | TaskField;

/** A task's popover by its row: the ⋯ menu, a chip's picker or the row's +. Closes after each change. */
function TaskPop({ t, at, view: first, onClose }: { t: TaskRow; at: DOMRect; view: TaskPopView; onClose: () => void }) {
  const [view, setView] = useState<TaskPopView>(first);
  const [picker, setPick] = useDatePick(t, onClose);
  // More… in two columns when the window can take them beside the row; one on a narrow window.
  const wide = window.innerWidth >= 600 && view === "more" && !picker;
  const width = wide ? 540 : view === "menu" || view === "add" ? 250 : view === "update" ? 360 : 290;
  let body: React.ReactNode;
  if (picker) body = picker;
  else if (view === "rename") body = <RenameBox t={t} done={onClose} onBack={() => setView("menu")} />;
  else if (view === "update") body = <UpdateForm target={{ kind: "task", task: t }} onClose={onClose} />;
  else if (view === "menu")
    body = (
      <TaskActions
        t={t}
        done={onClose}
        item="menuitem"
        onRename={() => setView("rename")}
        onMore={() => setView("more")}
        onUpdate={() => setView("update")}
      />
    );
  else if (view === "more")
    body = (
      <div className="tmcols">
        <div className="tmcol">
          {(["due", "scheduled", "start", "created"] as const).map((k) => (
            <DateSection key={k} t={t} kind={k} done={onClose} onPick={setPick} />
          ))}
        </div>
        <div className="tmcol">
          <PrioritySection t={t} done={onClose} />
          <ContextSection t={t} done={onClose} />
          <EffortSection t={t} done={onClose} />
          <ProjectSection t={t} done={onClose} />
        </div>
      </div>
    );
  else if (view === "add") {
    const left = ADDABLE.filter((a) => !hasField(t, a.f));
    body = (
      <>
        <div className="mhead faint small">Add to this task</div>
        {left.map((a) => (
          <button key={a.f} type="button" role="menuitem" title={`Set its ${a.label.toLowerCase()}`} onClick={() => setView(a.f)}>
            <Icon name={a.icon} size={14} />
            {a.label}
          </button>
        ))}
        {!left.length && <div className="mhead faint small">It has every field: click a chip to change one.</div>}
      </>
    );
  } else
    body = (
      <>
        {view === "project" && t.project && (
          <>
            <button
              type="button"
              role="menuitem"
              title="Open the project on the Projects screen"
              onClick={() => {
                onClose();
                nav.go({ screen: "projects", path: t.project! });
              }}
            >
              <Icon name="project" size={14} />
              Open {projectName(t.project)}
            </button>
            <div className="sep" />
          </>
        )}
        <FieldSection t={t} f={view} done={onClose} onPick={setPick} />
      </>
    );
  return (
    <Popover anchor={at} onClose={onClose} width={width}>
      <div className={`menu tmenu${wide ? " wide" : ""}`} role="menu">
        {body}
      </div>
    </Popover>
  );
}

/** Everything that can be changed about a task, in one column: the Tasks screen's pane. The same
 *  sections and verbs as a row's chips and its ⋯ menu, so they can't drift. */
export function TaskEditor({ t }: { t: TaskRow }) {
  const done = () => {};
  const [picker, setPick] = useDatePick(t, done);
  const [renaming, setRenaming] = useState(false);
  // Start and created are folded away unless it has a start date (nearly every task has a created
  // date, stamped on capture, so that alone doesn't open them).
  const [more, setMore] = useState(!!t.start);
  return (
    <div className="menu tmenu inpane">
      {picker ??
        (renaming ? (
          <RenameBox t={t} done={() => setRenaming(false)} onBack={() => setRenaming(false)} />
        ) : (
          <>
            <DateSection t={t} kind="due" done={done} onPick={setPick} />
            <DateSection t={t} kind="scheduled" done={done} onPick={setPick} />
            {more ? (
              <>
                <DateSection t={t} kind="start" done={done} onPick={setPick} />
                <DateSection t={t} kind="created" done={done} onPick={setPick} />
              </>
            ) : (
              <button type="button" className="faint" title="Show the start and created dates too" onClick={() => setMore(true)}>
                <Icon name="chevdown" size={14} />
                More dates…
              </button>
            )}
            <div className="sep" />
            <PrioritySection t={t} done={done} />
            <ContextSection t={t} done={done} />
            <EffortSection t={t} done={done} />
            <ProjectSection t={t} done={done} />
            <TaskActions t={t} done={done} lead onRename={() => setRenaming(true)} />
          </>
        ))}
    </div>
  );
}

export function TaskList({
  rows,
  manual,
  selected,
  onSelect,
  onDeselect,
  showNote = true,
  empty = "Nothing here.",
  display,
  rowAction,
  table,
  hideTags,
  keys,
  onOpen,
  hideProject,
  every,
}: {
  rows: TaskRow[];
  manual: boolean;
  selected?: string | null;
  onSelect?: (t: TaskRow) => void;
  /** Clicking the selected row again: nothing selected. */
  onDeselect?: () => void;
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
  /** With manual: the whole list, rows the screen hides included, so a renumber keeps their places
   *  (taskModel's withHiddenRows). */
  every?: TaskRow[];
}) {
  // The task's popover: its ⋯ menu, a chip's picker or the row's +.
  const [menu, setMenu] = useState<{ t: TaskRow; at: DOMRect; view: TaskPopView } | null>(null);
  // The keyboard's row where nothing else keeps a selection (Today); the Tasks screen's is `selected`.
  const [cursor, setCursor] = useState<string | null>(null);
  // While dragging: the list in its new order, shown until the vault reloads it.
  const [order, setOrder] = useState<TaskRow[] | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const list = useRef<HTMLUListElement>(null);
  useEffect(() => {
    setOrder(null);
  }, [rows]);
  // Screenshot mode: a task's menu open (scene.ts `taskMenu`, `path:line`, or `path:line|view` for
  // More…, the row's + or a chip's picker), once its row is there.
  useEffect(() => {
    const [want, view = "menu"] = (document.documentElement.dataset.taskMenu ?? "").split("|");
    const t = want ? rows.find((r) => taskKey(r) === want) : null;
    const row = t && list.current?.querySelector<HTMLElement>(`li[data-key="${CSS.escape(want)}"]`);
    const btn =
      row &&
      (row.querySelector<HTMLElement>(
        view === "add"
          ? ".tadd"
          : view === "menu" || view === "more"
            ? ".tmore"
            : `.ed.${{ context: "ctx", project: "proj" }[view] ?? `f-${view}`}`,
      ) ??
        row.querySelector<HTMLElement>(".tmore"));
    if (!t || !btn) return;
    delete document.documentElement.dataset.taskMenu;
    setMenu({ t, at: btn.getBoundingClientRect(), view: view as TaskPopView });
  }, [rows]);
  const shown = order ?? rows;
  const onField =
    (t: TaskRow): OnField =>
    (view, at) =>
      setMenu({ t, at, view });

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
      if (moved && cur.indexOf(t) !== rows.indexOf(t)) void moveTask(cur, t, every).then((ok) => ok || setOrder(null));
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
              onClick={() =>
                // A second click on the row it's on lets it go.
                onSelect ? (selected === k && onDeselect ? onDeselect() : onSelect(t)) : keys && setCursor(cursor === k ? null : k)
              }
              onContextMenu={(e) => {
                // Text selected in the row keeps the system menu, for Copy.
                const sel = window.getSelection();
                if (sel?.toString() && sel.anchorNode && e.currentTarget.contains(sel.anchorNode)) {
                  e.stopPropagation();
                  return;
                }
                // The task's menu only, not also the note's around it (a task query or Dataview in a note).
                e.preventDefault();
                e.stopPropagation();
                setMenu({ t, at: new DOMRect(e.clientX, e.clientY, 0, 0), view: "menu" });
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
                  <RowTitle t={t} />
                </div>
                <div className="ts">
                  <FieldChips t={t} display={display} onField={onField(t)} />
                  {display?.urgency && (
                    <span
                      className="chip faint"
                      title={`Urgency: ${urgencyOf(t).toFixed(2)}\nHow pressing it is, from its due, deferred and start dates and its priority.`}
                    >
                      {urgencyOf(t).toFixed(2)}
                    </span>
                  )}
                  <GtdChips t={t} project={!hideProject} onField={onField(t)} />
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
              <button
                type="button"
                className="ibtn tadd"
                aria-label="Add a field"
                title="Add a due or defer date, priority, context, effort or project"
                onClick={(e) => {
                  e.stopPropagation();
                  setMenu({ t, at: e.currentTarget.getBoundingClientRect(), view: "add" });
                }}
              >
                <Icon name="plus" size={14} />
              </button>
              {/* Beside ⋯, not after the + that shows only on hover. */}
              {rowAction?.(t)}
              <button
                type="button"
                className="ibtn tmore"
                aria-label="Task actions"
                title="Update, rename, waiting for, in progress, cancel, open its note or delete; More… for its fields"
                onClick={(e) => {
                  e.stopPropagation();
                  setMenu({ t, at: e.currentTarget.getBoundingClientRect(), view: "menu" });
                }}
              >
                <Icon name="more" size={14} />
              </button>
            </li>
          );
        })}
      </ul>
      {menu && <TaskPop key={`${taskKey(menu.t)}|${menu.view}`} t={menu.t} at={menu.at} view={menu.view} onClose={() => setMenu(null)} />}
    </>
  );
}

/** A row's words, with a pencil (on hover, or the selected row) that turns them into a box to
 *  rename the task: ↩ keeps, Esc leaves them. Only where its words are on the line once, so its
 *  dates and tags stay as they are. */
function RowTitle({ t }: { t: TaskRow }) {
  const words = taskLabel(t.text);
  const [draft, setDraft] = useState<string | null>(null);
  const editable = !!retitled(t.lineText, words, words || "x");
  if (draft !== null)
    return (
      <input
        className="trename-in"
        autoFocus
        aria-label="The task's words"
        value={draft}
        onClick={(e) => e.stopPropagation()}
        onChange={(e) => setDraft(e.target.value)}
        onFocus={(e) => e.currentTarget.select()}
        onBlur={() => {
          if (draft !== words) void retitleTask(t, words, draft);
          setDraft(null);
        }}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") setDraft(null);
        }}
      />
    );
  return (
    <>
      <TaskText text={words} />
      {editable && (
        <button
          type="button"
          className="ibtn trename"
          aria-label="Rename the task"
          title="Rename the task (its dates and tags stay)"
          onClick={(e) => {
            e.stopPropagation();
            setDraft(words);
          }}
        >
          <Icon name="pencil" size={12} />
        </button>
      )}
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
