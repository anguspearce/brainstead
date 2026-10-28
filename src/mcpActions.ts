// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What an assistant does through Brainstead's MCP server (§10 stage 9). The server writes a request
// for the running app (src-tauri/src/bridge.rs), which hands it here; each action runs the same
// functions the screens call, so undo, log.md, the read-only switch, Templater and run history
// behave as they do on screen. Changes made straight away say so in a toast with Undo. Prose,
// renames, deletes, and task, Inbox and project edits (D-20261005-10) come as agent changes: worked
// out here or by the MCP server, then made or held by the rule in src-tauri/src/changes.rs, each
// recorded in Changes with Revert.

import { compareMarkdown, compareNote } from "./skills/Compare";
import { extensionOutdatedText, rejectedText } from "./syncStatus";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import moment from "moment";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  BookmarkRow,
  ChangeKind,
  ClarifySuggestion,
  ChangeOrigin,
  Count,
  ChangeOutcome,
  ChangeRow,
  ChangeSubmit,
  CurrentStateRun,
  Instruction,
  FindSuggestion,
  FixNameRequest,
  InboxItem,
  IngestRun,
  LintReport,
  LogEntry,
  NoteSpec,
  ProjectRow,
  ReviewRun,
  Settings,
  ChatSummary,
  TaskDateKind,
  TaskLineEdit,
  TaskRow,
  Transcript,
  WeeklyState,
  WeekPrepSuggestion,
} from "./api";
import { ACTION as FIX_ACTION, chosenRows, startUnticked } from "./FixName";
import {
  areaProjects,
  captureDone as markCaptureDone,
  contextName,
  moveInProjectOrder,
  orderedAreas,
  orderedProjects,
  projectFlag,
  projectName,
  ProjectStatus,
  projectTasks,
  renameInProjectOrder,
  STATUS_LABEL,
} from "./gtd";
import { askEdit } from "./notes/dialogs";
import { captureStamp, prepareCapture } from "./Capture";
import { bandOf, inboxRows, taskLine, unclarified } from "./Inbox";
import { appendAsReference, inboxChoices, newReferenceNote, settleInboxItem, suggestKind, trashCapture } from "./inboxActions";
import {
  ADVISORY,
  byRun,
  changeText,
  IGNORE_ALL,
  decisions,
  fixOf,
  GhostHelp,
  ghostHelps,
  ghostPaths,
  makePage,
  originLabel,
  OWN_ACTIONS,
  pageName,
  runChanges,
  safeFixes,
  trendLabels,
  trendShown,
  trashByName,
} from "./knowledge";
import { localToday } from "./md/taskQuery";
import { composeFilename } from "./notes/filename";
import { Ask, Env, freeName, runTemplate, StopRun } from "./notes/templater";
import { runHooks, vaultEnv } from "./notes/TemplateRun";
import { deferredPast, rankBetween, retitled, todayRows, toggleTaskEdit, undoAction, viewRows, VIEWS } from "./taskModel";
import { taskLabel } from "./md/TaskBlock";
import { cancelled, completion, inQuote, LineChange, reopened, started, tagToggled, waitingToggled, withPriority } from "./tasksq/edits";
import { parseTask, statusOf, type PriorityName } from "./tasksq/fields";
import { toast } from "./Toast";
import { fmtBytes, fmtCount } from "./ui";
import { fresh, keepsPaused, reviewBody, reviewWeek, scheduleLabel, STEPS } from "./Weekly";
import {
  ask,
  CLI_LABEL,
  clis,
  DEFAULT_MODEL,
  findClis,
  isSaved,
  keepChat,
  MODEL_JOBS,
  modelLabel,
  pinSaved,
  renameSaved,
  trashSaved,
} from "./askState";
import { DOC_ACCENTS, DOC_STYLES, docDefaults, lookOf, withOwnLook } from "./docLook";
import { loadVoices, speechSettingsChanged, voiceLabel, voices } from "./speech/player";
import { applyReadSize, applyTheme, READ_SIZE, settings } from "./store";
import { INTERFACE_SIZE, setInterfaceSize } from "./interfaceSize";
import { imageView } from "./ImageViewer";
import { TOPICS } from "./help";
import { HelpView, openHelp } from "./help/HelpDrawer";
import { IMAGE_EXT } from "./md/wikilinks";
import { nav, place, Screen, SettingsPane } from "./nav";
import { focusMode } from "./focus";
import { DEFAULT_SCHEDULE, DEFAULT_WEEKLY_REVIEW } from "./Jobs";
import { foundDetail, handedOver, INSTALL, languageName, SCREEN_NAME, scriptsFolder, terminalCommands, WEBSITE } from "./Settings";
import { choice, prepStep, WEEKLY_STATE_CHANGED } from "./weeklyPrep";
import { draftNotes, followThrough, isTranscriptPath, keepNote, noteFile, specOf, specOk, trashTranscript } from "./meetingFlow";
import { DONE as MEETING_DONE, needsDate, noteExists, TYPES as MEETING_TYPES } from "./Meeting";
import { order as contradictionsOrder, reportMarkdown } from "./Contradictions";
import { KINDS as GRAPH_KINDS, visible as graphVisible } from "./graphModel";
import { checklist, isDone, isRetired, STOP_FIRST } from "./Switchover";
import { canIngest, ingestable } from "./Lists";
import { LAYERS as SEARCH_LAYERS } from "./Search";
import { fmtDay } from "./FileList";
import { passageText, PASSAGES_SHOWN } from "./Provenance";
import { meetingProgress, offerFollowUp, runMatches } from "./Ingest";
import { findingsMarkdown, findingsNote, governingDocs, REGISTER, REGISTER_STUB, STATUS as REGISTER_STATUS } from "./skills/DocCheck";
import { LABEL as TRIAGE } from "./skills/Triage";
import { ANSWERED, replyTask } from "./skills/Reply";
import { actionLabel, actionsIn, filterLog, rankLog } from "./activityModel";
import { EFFORT_LIMITS, GroupBy, groupTasks, matchesSearch, withinEffort } from "./taskGroups";
import { captureDefault, isMac, keyLabel, terminalApp, vaultFile } from "./platform";

const TODO_LIST = "Me. To Do List.md";
const SCRATCHPAD = "Me. Scratchpad.md";
/** A project's heading a waiting task goes under, as the Inbox files it. */
const WAITING_FOR = "Waiting for";

/** A request as Rust hands it on (core/src/bridge.rs). */
export interface McpRequest {
  id: string;
  action: string;
  args: Record<string, unknown>;
  chat?: string | null;
  model?: string | null;
}

type Args = Record<string, unknown>;
const str = (a: Args, k: string): string | undefined =>
  typeof a[k] === "string" && (a[k] as string).trim() ? (a[k] as string).trim() : undefined;
const has = (a: Args, k: string) => k in a && a[k] !== undefined;

/** A long list a page at a time, as every listing tool gives one: the rows whose text has all of
 *  `query`'s words (or that `match` the query, where the screen searches its own way), then `limit`
 *  of them (`def` unless said, at most 500) from `offset`, and a head saying how many there are and
 *  how to see the rest. */
export function pagedRows<T>(
  a: Args,
  rows: T[],
  text: (r: T) => string,
  noun: [string, string],
  def = 50,
  match?: (r: T, query: string) => boolean,
): { shown: T[]; head: string; page: PageInfo } {
  const words = (str(a, "query") ?? "").toLowerCase().split(/\s+/).filter(Boolean);
  const hit = !words.length
    ? rows
    : match
      ? rows.filter((r) => match(r, str(a, "query")!))
      : rows.filter((r) => words.every((w) => text(r).toLowerCase().includes(w)));
  const limit = Math.min(Math.max(Math.floor(Number(a.limit)) || def, 1), 500);
  const offset = Math.max(Math.floor(Number(a.offset)) || 0, 0);
  const shown = hit.slice(offset, offset + limit);
  const n = (k: number) => `${k} ${k === 1 ? noun[0] : noun[1]}`;
  let head = words.length ? `${n(hit.length)} matching “${words.join(" ")}”, of ${rows.length}` : n(rows.length);
  if (offset >= hit.length && hit.length) head += `; none from offset ${offset}`;
  else if (shown.length < hit.length) {
    const end = offset + shown.length;
    head += `; ${offset + 1}–${end} shown${end < hit.length ? `, offset ${end} for the next` : ""}`;
  }
  const next = offset + shown.length < hit.length ? offset + shown.length : null;
  return { shown, head, page: { total: rows.length, matching: hit.length, offset, next_offset: next } };
}

/** Where a page of rows sits, as a listing tool's data gives it (the server's page_schema). */
export interface PageInfo {
  total: number;
  matching: number;
  offset: number;
  next_offset: number | null;
}

/** What a listing tool gives back: its text, and the same rows as data, which the server passes on
 *  as the tool's structuredContent (its outputSchema says the shape). */
export interface Listing {
  text: string;
  structured: Record<string, unknown>;
}

/** A page of rows as a Listing: the head, a line a row, and each row as an item. `empty` is the
 *  text when there are none; `before` lines go above the head. */
export function listed<T>(
  a: Args,
  rows: T[],
  o: {
    text: (r: T) => string;
    noun: [string, string];
    def?: number;
    line: (r: T) => string;
    item: (r: T) => Record<string, unknown>;
    empty: string;
    before?: string[];
    match?: (r: T, query: string) => boolean;
  },
): Listing {
  const { shown, head, page } = pagedRows(a, rows, o.text, o.noun, o.def, o.match);
  const lines = rows.length ? [`${head}:`, ...shown.map(o.line)] : [o.empty];
  return { text: [...(o.before ?? []), ...lines].join("\n"), structured: { ...page, items: shown.map(o.item) } };
}

/** pagedRows for a list that's one line a row: its head, then the lines shown. */
const paged = (a: Args, lines: string[], noun: [string, string], def = 50) => {
  const { shown, head } = pagedRows(a, lines, (l) => l, noun, def);
  return [`${head}:`, ...shown].join("\n");
};

/** Says in the app what an assistant changed, with Undo. */
function told(what: string) {
  toast(`An assistant: ${what}`, undoAction, "ok");
}

/** As told, for what ⌘Z doesn't undo (runs, decisions in Changes, settings, Start over, Retire). */
function noted(what: string) {
  toast(`An assistant: ${what}`, undefined, "ok");
}

// ---- tasks

/** A task's words, without its tags, dates, fields and ids, to compare what a model gives with the
 *  line (the MCP server has the same rule). */
export function taskWords(s: string): string {
  return s
    .replace(
      /#[^\s#]+|[📅⏳🛫➕✅❌]\uFE0F?\s*\d{4}-\d{2}-\d{2}|[🔺⏫🔼🔽⏬🔁🏁⛔🆔]\uFE0F?|[[(]\w+::[^\])]*[\])]|\^[\w-]+|\[\[([^\]|]*\|)?|\]\]|^\s*[-*+]\s+\[.\]\s*/gu,
      " ",
    )
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();
}

/** `path:line` (1-based, as the tasks are listed) to a task, checked by its words. */
export function findTask(all: TaskRow[], id: string, text?: string): TaskRow {
  const m = /^(.*):(\d+)$/.exec(id.trim());
  if (!m) throw new Error(`“${id}” isn't a task id: use the path:line that list_tasks gives, as in Me. To Do List.md:12.`);
  const [, path, n] = m;
  const want = text ? taskWords(text) : "";
  const fits = (t: TaskRow) => {
    const have = taskWords(t.text);
    return !want || have.includes(want) || (!!have && want.includes(have));
  };
  const at = all.find((t) => t.path === path && t.line === Number(n) - 1);
  if (at && fits(at)) return at;
  if (want) {
    const same = all.filter((t) => t.path === path && fits(t));
    if (same.length === 1) return same[0];
  }
  throw new Error(`There's no task at ${id}${text ? ` reading “${text}”` : ""} any more. List the tasks again for its current place.`);
}

const taskId = (t: TaskRow) => `${t.path}:${t.line + 1}`;

/** A task's text as a list shows it: its tags, dates, priority and effort come after it as fields. */
const shownText = (text: string) =>
  text
    .replace(/#[^\s#]+|[📅⏳🛫➕✅❌]\uFE0F?\s*\d{4}-\d{2}-\d{2}|[🔺⏫🔼🔽⏬]\uFE0F?|[[(]effort::[^\])]*[\])]|\^[\w-]+/gu, "")
    .replace(/\s+/g, " ")
    .trim();

/** A task's #tags but its contexts (#context/…), without the #. */
const otherTags = (t: TaskRow) => t.tags.map((x) => x.replace(/^#/, "")).filter((g) => !/^context\//.test(g));
/** Its priority as the task menu names it (edit_task's), or null for none. */
const priorityOf = (t: TaskRow) => {
  const p = parseTask(t.lineText)?.priority;
  return p && p !== "none" ? p : null;
};
/** Its status in edit_task's words, done added: open, in_progress, done or cancelled. */
const statusName = (t: TaskRow) => {
  const type = statusOf(t.status ?? (t.done ? "x" : " ")).type;
  return type === "IN_PROGRESS" ? "in_progress" : type === "DONE" ? "done" : type === "CANCELLED" ? "cancelled" : "open";
};

/** One line per task: its words, then its dates and chips, and its id to act on it by. */
export function taskLineOut(t: TaskRow, detail = false): string {
  const bits = [`[${t.status ?? (t.done ? "x" : " ")}] ${shownText(t.text)}`];
  if (t.due) bits.push(`due ${t.due}`);
  if (t.scheduled) bits.push(`deferred until ${t.scheduled}`);
  if (t.start) bits.push(`starts ${t.start}`);
  for (const c of t.contexts ?? []) bits.push(`@${c}`);
  if (t.effort) bits.push(`effort ${t.effort}`);
  const p = priorityOf(t);
  if (p) bits.push(`priority ${p}`);
  // Its #tags, the lists' among them; contexts are the @ chips above.
  for (const g of otherTags(t)) bits.push(`#${g}`);
  if (t.project) bits.push(`project ${projectName(t.project)}`);
  if (detail) {
    if (t.heading) bits.push(`under “${t.heading}”`);
    if (t.created) bits.push(`created ${t.created}`);
    if (t.doneOn) bits.push(`done ${t.doneOn}`);
  }
  return `- ${bits.join(" · ")}  (${taskId(t)})`;
}

/** A view's id as the assistants know it: the Deferred list is `deferred`. */
const viewName = (id: string) => (id === "scheduled" ? "deferred" : id);

async function listTasks(a: Args): Promise<Listing> {
  const all = await api.tasksAll();
  const today = localToday();
  // The Deferred list's id is `scheduled` (saved lists keep it); `deferred` is its name.
  const saved = str(a, "list");
  const named = saved ? (settings.get().taskLists ?? []).find((l) => l.name.toLowerCase() === saved.toLowerCase()) : undefined;
  if (saved && !named)
    throw new Error(
      `No saved list “${saved}”. list_task_lists lists them: ${(settings.get().taskLists ?? []).map((l) => l.name).join(", ") || "none"}.`,
    );
  if (named) a = { ...a, view: named.view, context: named.context || undefined, effort: named.effort || undefined, group: named.group };
  const asked = str(a, "view") ?? "next";
  const view = asked === "deferred" ? "scheduled" : asked;
  let rows: TaskRow[];
  // Today's bands, as the Today screen draws them: each task under its band's label.
  const band = new Map<TaskRow, string>();
  if (view === "today") {
    const r = todayRows(all, today);
    // A waiting task due today shows in both bands there, so it's a row of its own here too.
    const waiting = viewRows(
      all,
      VIEWS.find((v) => v.id === "waiting")!,
      today,
    )
      .filter((t) => !deferredPast(t, today))
      .map((t) => ({ ...t }));
    const bands: [string, TaskRow[]][] = [
      ["Overdue", r.overdue],
      ["Due today", r.due],
      ["Deferred until now", r.scheduled],
      ["Coming up this week", r.soon],
      ["Waiting for", waiting],
    ];
    for (const [label, rs] of bands) for (const t of rs) band.set(t, label);
    rows = bands.flatMap(([, rs]) => rs);
  } else if (view === "all") {
    rows = all.filter((t) => !t.done);
  } else {
    const v = VIEWS.find((x) => x.id === view);
    if (!v) throw new Error(`No view “${view}”. Views: today, all, ${VIEWS.map((x) => viewName(x.id)).join(", ")}.`);
    rows = viewRows(all, v, today, view === "next" ? await api.inboxList() : null);
  }
  const project = str(a, "project")?.toLowerCase();
  if (project) rows = rows.filter((t) => t.project && projectName(t.project).toLowerCase() === project.replace(/^project\. /, ""));
  const context = str(a, "context");
  if (context) rows = rows.filter((t) => (t.contexts ?? []).includes(contextName(context)));
  // The effort filter's choices, in minutes, as the screen and its saved lists have them.
  const effort = a.effort == null || a.effort === "" ? "" : String(a.effort);
  if (effort && !EFFORT_LIMITS.some(([v]) => v === effort)) throw new Error("effort is 15, 30 or 60 (minutes), or left out.");
  if (effort) rows = rows.filter((t) => withinEffort(t, effort));
  const group = (str(a, "group") ?? "none") as GroupBy;
  if (!["none", "project", "context", "due"].includes(group)) throw new Error("group is none, project, context or due.");
  // Grouped as the screen groups them: each group's rows together, under its heading.
  const under = new Map<TaskRow, string>();
  if (group !== "none") {
    const groups = groupTasks(rows, group, today);
    rows = groups.flatMap((g) => g.rows);
    for (const g of groups) for (const t of g.rows) under.set(t, g.label);
  }
  // Today's bands head the rows unless Group by asks for other headings.
  const banded = group === "none" && band.size > 0;
  // A page at a time; query matches as the Tasks screen's search does (matchesSearch: the task's
  // words, note, heading, project, tags and contexts, accents ignored).
  const shown = viewName(view);
  const out = listed(a, rows, {
    text: (t) => t.text,
    match: matchesSearch,
    noun: [`task in ${shown}`, `tasks in ${shown}`],
    line: (t) => taskLineOut(t, a.detail === true),
    item: (t) =>
      banded
        ? { ...taskItem(t), band: band.get(t) ?? null }
        : group === "none"
          ? taskItem(t)
          : { ...taskItem(t), group: under.get(t) ?? null },
    empty: `No tasks in ${shown}${project || context || effort ? " with those filters" : ""}.`,
  });
  if (group === "none" && !banded) return out;
  // A heading line where each group or band starts on the page.
  const [head, ...lines] = out.text.split("\n");
  const items = out.structured.items as { group?: string | null; band?: string | null }[];
  const label = (i: number) => (banded ? items[i].band : items[i].group);
  const text = [head, ...lines.flatMap((l, i) => (i === 0 || label(i) !== label(i - 1) ? [`${label(i)}:`, l] : [l]))];
  return { ...out, text: text.join("\n") };
}

/** A task as list_tasks' data gives it. */
const taskItem = (t: TaskRow) => ({
  id: taskId(t),
  text: shownText(t.text),
  path: t.path,
  due: t.due,
  // Deferred until: named as edit_task's argument that sets it.
  defer: t.scheduled,
  start: t.start,
  done: t.done,
  doneOn: t.doneOn,
  project: t.project ? projectName(t.project) : null,
  contexts: t.contexts ?? [],
  tags: otherTags(t),
  priority: priorityOf(t),
  status: statusName(t),
  created: t.created,
  effort: t.effort ?? null,
  heading: t.heading,
});

const DATE_KEYS: [string, TaskDateKind][] = [
  ["due", "due"],
  ["defer", "scheduled"],
  ["start", "start"],
  ["created", "created"],
];

// ---- agent changes for tasks, the Inbox and projects (D-20261005-10)

/** Where an assistant's change comes from: the session the MCP server says (with unattended), else this chat. */
const originOf = (a: Args, r: McpRequest): ChangeOrigin =>
  (a.origin as ChangeOrigin | undefined) ?? { kind: "chat", chat: r.chat ?? undefined, run: r.chat ?? undefined };

/** A change to submit for this request. */
function sub(a: Args, r: McpRequest, page: string, kind: ChangeKind, title: string, instruction: Instruction): ChangeSubmit {
  return { page, kind, title, reason: str(a, "reason") ?? "", instruction, origin: originOf(a, r), model: r.model ?? null };
}

/** Submits changes that belong together; made, or held together when any is held. */
async function submitAll(list: ChangeSubmit[]): Promise<{ applied: boolean; message: string; outs: ChangeOutcome[] }> {
  const outs = await api.changesSubmitMany(list);
  const applied = outs.every((o) => o.applied);
  const message = applied
    ? outs.map((o) => o.message).join(" ")
    : `Held for the user in Changes: ${outs.map((o) => o.flags.join(" ")).find(Boolean) ?? "a check didn't pass."} Nothing is changed until they accept it.`;
  return { applied, message, outs };
}

async function editTask(a: Args, r: McpRequest): Promise<string> {
  const id = str(a, "task");
  if (!id) throw new Error("Which task? Give its id from list_tasks (path:line).");
  const t = findTask(await api.tasksAll(), id, str(a, "text"));
  const today = localToday();
  const day = moment(today, "YYYY-MM-DD");
  // The line worked out edit by edit (a recurring tick makes two; the rest act on the last), then
  // made as one change.
  let lines = [t.lineText];
  const done: string[] = [];
  const last = () => lines[lines.length - 1];
  const put = (next: string[]) => (lines = [...lines.slice(0, -1), ...next]);
  const rust = async (edit: TaskLineEdit) => put([await api.changeTaskLine(last(), edit)]);
  const ts = (f: (l: string) => LineChange) => put(inQuote(last(), f).lines);
  // The words first, as the Tasks screen's rename changes them: its dates, fields and tags kept.
  const words = str(a, "words");
  if (words !== undefined) {
    const label = taskLabel(t.text);
    const line = retitled(last(), label, words);
    if (!line)
      throw new Error(`The task's words can't be changed here: “${label}” isn't on its line once. Change the line with edit_page instead.`);
    if (words !== label) {
      put([line]);
      done.push(`reworded to “${words}”`);
    }
  }
  if (has(a, "done")) {
    const c = a.done ? inQuote(last(), (l) => completion(l, day)) : null;
    if (c) put(c.lines);
    else await rust({ op: "toggle", done: !!a.done, today });
    done.push(a.done ? "ticked" : "unticked");
  }
  for (const [k, kind] of DATE_KEYS) {
    if (!has(a, k)) continue;
    const v = a[k] === null || a[k] === "" ? null : String(a[k]);
    await rust({ op: "date", kind, date: v });
    done.push(v ? `${k} ${v}` : `cleared ${k}`);
  }
  if (has(a, "priority")) {
    ts((l) => withPriority(l, String(a.priority) as PriorityName));
    done.push(`priority ${a.priority}`);
  }
  if (has(a, "contexts")) {
    const cs = (Array.isArray(a.contexts) ? a.contexts : [a.contexts]).filter(Boolean).map((c) => contextName(String(c)));
    await rust({ op: "contexts", names: cs });
    done.push(cs.length ? `contexts ${cs.join(", ")}` : "cleared contexts");
  }
  if (has(a, "effort")) {
    await rust({ op: "effort", effort: a.effort ? String(a.effort) : null });
    done.push(a.effort ? `effort ${a.effort}` : "cleared effort");
  }
  if (has(a, "project")) {
    const p = a.project ? await projectPath(String(a.project)) : null;
    await rust({ op: "project", project: p });
    done.push(p ? `project ${projectName(p)}` : "no project");
  }
  if (has(a, "waiting")) {
    const now = t.tags.some((g) => g === "waiting-for" || g === "#waiting-for");
    if (!!a.waiting !== now) {
      ts(waitingToggled);
      done.push(a.waiting ? "waiting for" : "no longer waiting");
    }
  }
  if (has(a, "followup")) {
    const now = t.tags.some((g) => g === "followup" || g === "#followup");
    if (!!a.followup !== now) {
      ts((l) => tagToggled(l, "followup", "Follow-up", "No longer a follow-up"));
      done.push(a.followup ? "follow-up" : "no longer a follow-up");
    }
  }
  const status = str(a, "status");
  if (status) {
    const how = ({ cancelled: (l: string) => cancelled(l, day), open: reopened, in_progress: started } as const)[
      status as "cancelled" | "open" | "in_progress"
    ];
    if (!how) throw new Error("status is cancelled, open or in_progress.");
    ts(how);
    done.push(status.replace("_", " "));
  }
  if (!done.length)
    throw new Error(
      "Nothing to change: give words, done, due, defer, start, created, priority, contexts, effort, project, waiting, followup or status.",
    );
  const was = taskWords(t.text);
  const o = await submitAll([
    sub(a, r, t.path, "task", `Edit “${was}”: ${done.join(", ")}`, { op: "lines", at: t.line, old: [t.lineText], new: lines }),
  ]);
  told(o.applied ? `${done.join(", ")}: “${was}”` : `held a change to “${was}” for you in Changes`);
  if (!o.applied) return o.message;
  const now = (await api.tasksAll()).find((x) => x.path === t.path && x.lineText === last()) ?? t;
  return `Done (${done.join(", ")}). The task is now:\n${taskLineOut(now)}\n${o.message}`;
}

/** The spacing a renumbered list's ranks get (1024, 2048…), as Rust's RANK_SPACING. */
const RANK_SPACING = 1024;

/** move_task: the drag in a manual list, as the Tasks screen does it (taskModel's rankBetween): in
 *  the list both tasks are in (or, with project, the project page's Next actions), between its real neighbours there, or the whole list renumbered when
 *  some rows have no rank or there's no room. Every line it changes is one change, made together. */
async function moveTask(a: Args, r: McpRequest): Promise<string> {
  const all = await api.tasksAll();
  const t = findTask(all, str(a, "task") ?? "", str(a, "text"));
  const ref = (k: string) => (str(a, k) ? findTask(all, str(a, k)!) : null);
  const after = ref("after");
  const before = ref("before");
  if (!after && !before) throw new Error("Give after or before: the id of the task it should follow or come ahead of.");
  const other = (after ?? before)!;
  if (other === t) throw new Error("A task can't move next to itself.");
  const both = (rows: TaskRow[]) => rows.includes(t) && (!after || rows.includes(after)) && (!before || rows.includes(before));
  // With project, the project page's Next actions (gtd.ts projectTasks, in its dragged order);
  // else the Tasks screen's manual list they're both in, as the screen drags within one list.
  let list: { v: { label: string }; rows: TaskRow[] } | undefined;
  const project = str(a, "project");
  if (project) {
    const path = await projectPath(project);
    const rows = projectTasks(all, path).next;
    if (!both(rows))
      throw new Error(
        `Those tasks aren't both in ${projectName(path)}'s Next actions (open tasks not waiting for or someday / maybe): list_tasks with project shows them.`,
      );
    list = { v: { label: `${projectName(path)}'s Next actions` }, rows };
  } else {
    const today = localToday();
    const inbox = await api.inboxList();
    const lists = VIEWS.filter((v) => v.manual).map((v) => ({ v, rows: viewRows(all, v, today, v.id === "next" ? inbox : null) }));
    list = lists.find((l) => both(l.rows));
  }
  if (!list)
    throw new Error(
      "Those tasks aren't in one list with a manual order (Next actions, Follow-ups, Waiting for, Someday / maybe): move a task next to one in its own list, or give project to order a project's Next actions.",
    );
  const rest = list.rows.filter((x) => x !== t);
  const at = after ? rest.indexOf(after) + 1 : rest.indexOf(before!);
  const order = [...rest.slice(0, at), t, ...rest.slice(at)];
  const where = `${after ? ` after “${taskWords(after.text)}”` : ""}${before ? ` before “${taskWords(before.text)}”` : ""}`;
  const title = `Move “${taskWords(t.text)}”${where}`;
  const line = (x: TaskRow, ln: string) => ({ at: x.line, old: [x.lineText], new: [ln] });
  let edits: [TaskRow, { at: number; old: string[]; new: string[] }][] = [];
  const gap = rankBetween(order, t);
  if (gap) {
    const ln = await api.changeTaskLine(t.lineText, { op: "rank", prev: gap.prev, next: gap.next }).catch(() => null);
    if (ln) edits = [[t, line(t, ln)]];
  }
  // No room, or rows with no rank: the list renumbered in its new order, as the screen does.
  let renumbered = false;
  if (!edits.length) {
    renumbered = true;
    for (const [i, x] of order.entries()) {
      const ln = await api.changeTaskLine(x.lineText, { op: "rank", prev: i * RANK_SPACING, next: null });
      if (ln !== x.lineText) edits.push([x, line(x, ln)]);
    }
  }
  if (!edits.length) return `“${taskWords(t.text)}” is there already.`;
  const o = await submitAll(
    edits.map(([x, e]) =>
      sub(a, r, x.path, "task", x === t ? title : `Renumber “${taskWords(x.text)}” in ${list.v.label}`, { op: "lines", ...e }),
    ),
  );
  told(o.applied ? `moved “${t.text}”` : `held a move of “${t.text}” for you in Changes`);
  const how = renumbered ? `; the list is renumbered, ${edits.length} lines, as there was no room or some tasks had no rank yet` : "";
  return o.applied ? `Moved “${t.text}”${where} in ${list.v.label}${how}. ${o.message}` : o.message;
}

/** A project by name; `activeOnly`, as the Inbox's forms list only active projects. */
async function projectPath(name: string, activeOnly = false): Promise<string> {
  const want = name
    .replace(/^\[\[|\]\]$/g, "")
    .replace(/^Project\. /, "")
    .replace(/\.md$/, "")
    .toLowerCase();
  const p = (await api.projectsList()).find((x) => x.name.toLowerCase() === want);
  if (!p) throw new Error(`There's no project called ${name}. list_projects shows them.`);
  if (activeOnly && p.status !== "active")
    throw new Error(
      `${p.name} isn't an active project (it's ${p.status}), and the Inbox only offers active ones: update_project can make it active first.`,
    );
  return p.path;
}

// ---- inbox

const inboxKey = (i: InboxItem) => `${i.kind}:${i.line + 1}`;

/** How many items Suggest asks about at once, as the Inbox's Suggest for all (first 20) does. */
const SUGGEST_AT_MOST = 20;

async function listInbox(a: Args): Promise<Listing> {
  // In the screen's order, band by band, so Suggest asks about the same first 20.
  const items = inboxRows(await api.inboxList());
  // A transcript's meeting note under way or failed, as its Inbox row says (meetingProgress).
  const allRuns = await api.ingestRuns().catch(() => [] as IngestRun[]);
  const changeRows = await api.changesList().catch(() => null);
  const prog = (i: InboxItem) => (i.kind === "capture" && isTranscriptPath(i.path) ? meetingProgress(allRuns, i.path, changeRows) : null);
  const where = (i: InboxItem) =>
    i.kind === "capture" ? `capture ${i.path}` : i.kind === "thought" ? `Scratchpad ${i.stamp ?? ""}` : "To Do list › Other";
  const line = (i: InboxItem) => {
    const p = prog(i);
    return `- ${i.text.replace(/\n/g, " ").slice(0, 200)} — ${where(i)}${p ? ` · ${p.label}${p.why ? ` (${p.why})` : ""}` : ""}  (${inboxKey(i)})`;
  };
  // Suggest: the model's suggestion for the first rows shown, asked as the Inbox screen asks it
  // (each item's text, and the active projects' names).
  const sugg = new Map<string, ClarifySuggestion>();
  if (a.suggest === true) {
    const ask = pagedRows(a, items, line, ["", ""]).shown.slice(0, SUGGEST_AT_MOST);
    if (ask.length) {
      const active = (await api.projectsList()).filter((p) => p.status === "active").map((p) => p.name);
      const got = await api.clarifySuggest(
        ask.map((i) => ({ id: inboxKey(i), kind: suggestKind(i), text: i.text.trim() })),
        active,
      );
      for (const g of got) sugg.set(g.id, g);
    }
  }
  const suggested = (i: InboxItem) => {
    const g = sugg.get(inboxKey(i));
    if (!g) return "";
    const bits = [
      g.text && g.becomes !== "delete" && `“${g.text}”`,
      g.project && `project ${g.project}`,
      g.context && `@${g.context}`,
      g.effort && `effort ${g.effort}`,
      g.due && `due ${g.due}`,
    ].filter(Boolean);
    // meeting and reply have no clarify_inbox answer: their own tools do them.
    const how = g.becomes === "meeting" ? " (start_run meeting_note)" : g.becomes === "reply" ? " (draft_reply)" : "";
    return `\n  suggests ${g.becomes}${how}${bits.length ? `: ${bits.join(" · ")}` : ""}${g.why ? ` — ${g.why}` : ""}`;
  };
  const out = listed(a, items, {
    text: line,
    noun: ["item to clarify", "items to clarify"],
    line: (i) => line(i) + suggested(i),
    item: (i) => {
      const g = sugg.get(inboxKey(i));
      return {
        id: inboxKey(i),
        kind: i.kind,
        group: bandOf(i),
        text: i.text,
        path: i.path,
        stamp: i.stamp,
        ...(a.suggest === true
          ? {
              suggestion: g
                ? { becomes: g.becomes, text: g.text, project: g.project, context: g.context, effort: g.effort, due: g.due, why: g.why }
                : null,
            }
          : {}),
      };
    },
    empty: "The Inbox is empty.",
  });
  if (!items.length) return out;
  // Each band's heading where it starts on the page, as the screen's bands (Captures, Scratchpad…).
  const [head, ...rows] = out.text.split("\n");
  const bands = (out.structured.items as { group: string }[]).map((x) => x.group);
  const lines: string[] = [];
  let k = -1;
  for (const l of rows) {
    if (l.startsWith("- ")) {
      k++;
      if (k === 0 || bands[k] !== bands[k - 1]) lines.push(`${bands[k]}:`);
    }
    lines.push(l);
  }
  const note =
    a.suggest === true
      ? [
          "Suggestions only, nothing changed: accept one with clarify_inbox, becomes as suggested and its values (its text as text, or as project_name for a project and note for reference).",
        ]
      : [];
  return { ...out, text: [...note, head, ...lines].join("\n") };
}

async function clarify(a: Args, r: McpRequest): Promise<string> {
  const key = str(a, "item") ?? "";
  const items = (await api.inboxList()).filter(unclarified);
  const i = items.find((x) => inboxKey(x) === key);
  if (!i) throw new Error(`No Inbox item ${key}. list_inbox shows them, each with its id.`);
  // The choices the Inbox offers this kind of item (inboxChoices); meeting and reply are other tools'.
  const may = inboxChoices(i);
  const becomes = str(a, "becomes") ?? "";
  if (!may.includes(becomes) || ["meeting", "reply", "ask"].includes(becomes)) {
    const own = may
      .filter((b) => b === "meeting" || b === "reply")
      .map((b) => (b === "meeting" ? "start_run meeting_note" : "draft_reply"));
    // Ask about this source is the assistant reading it (read_section), not a clarify.
    const here = may.filter((b) => b !== "meeting" && b !== "reply" && b !== "ask");
    throw new Error(
      `The Inbox doesn't offer ${becomes || "that"} for this item. It can be: ${here.join(", ")}${own.length ? `; or use ${own.join(" or ")}` : ""}.`,
    );
  }
  const { said, changes, after } = await clarifyChanges(i, a, r);
  if (!changes.length) {
    // What it did, when it says (a transcript's move to the Trash, made or held).
    const out = await after?.();
    told(`clarified “${i.text.slice(0, 60)}” as ${said}`);
    return `Clarified as ${said}.${typeof out === "string" ? ` ${out}` : ""}`;
  }
  const o = await submitAll(changes);
  if (o.applied) await after?.();
  told(o.applied ? `clarified “${i.text.slice(0, 60)}” as ${said}` : `held clarifying “${i.text.slice(0, 60)}” for you in Changes`);
  return o.applied
    ? `Clarified as ${said}. ${o.message}`
    : `${o.message}${i.kind === "capture" ? " The capture stays in the Inbox until then." : ""}`;
}

/** An Inbox item's block or line, to find it by. */
const itemLines = (i: InboxItem) =>
  (i.kind === "thought" ? (i.block ?? i.lineText) : i.lineText).replace(/\r\n/g, "\n").replace(/\n+$/, "").split("\n");

/** What clarifying an item does to the vault, as agent changes (the Inbox screen's writes, from
 *  src/inboxActions.ts), and what to do once they're made (a capture marked done, an ingest). */
export async function clarifyChanges(
  i: InboxItem,
  a: Args,
  r: McpRequest,
): Promise<{ said: string; changes: ChangeSubmit[]; after?: () => Promise<unknown> }> {
  const becomes = str(a, "becomes");
  const text = str(a, "text") ?? (i.kind === "capture" ? `Follow up on [[${i.text}]]` : i.text.trim());
  const proj = str(a, "project") ? await projectPath(str(a, "project")!, true) : null;
  const fields = { text, project: proj, context: str(a, "context") ?? null, effort: str(a, "effort") ?? null, due: str(a, "due") ?? null };
  const short = i.text.replace(/\s+/g, " ").trim().slice(0, 60);
  const captureDone = i.kind === "capture" ? () => markCaptureDone(i.path) : undefined;
  // The item out of the Inbox: a task's line, a thought's block; a capture is marked done after.
  const takeOut = (): ChangeSubmit[] =>
    i.kind === "capture"
      ? []
      : [
          sub(a, r, i.path, i.kind === "task" ? "task" : "edit", `Take “${short}” out of the Inbox`, {
            op: "lines",
            at: i.line,
            old: itemLines(i),
            new: [],
          }),
        ];
  // Under a project's Next actions, or the heading given (Waiting for, as the Inbox files a waiting task).
  const addTask = (page: string, line: string, heading?: string) =>
    sub(a, r, page, "task", `Add “${taskWords(line)}”`, { op: "add_task", line, ...(heading ? { heading } : {}) });
  // The item settled as a task line (src/inboxActions.ts's settleInboxItem).
  const settle = (line: string, toProject: string | null, heading?: string): ChangeSubmit[] =>
    i.kind === "task"
      ? toProject
        ? [...takeOut(), addTask(toProject, line, heading)]
        : [sub(a, r, i.path, "task", `Clarify “${short}”`, { op: "lines", at: i.line, old: [i.lineText], new: [line] })]
      : [addTask(toProject ?? TODO_LIST, line, toProject ? heading : undefined), ...takeOut()];
  switch (becomes) {
    case "next":
      // As the Inbox's Next action form saves it: with no project or context it would land back in the Inbox.
      if (!proj && !fields.context?.trim())
        throw new Error(
          "A next action needs a project or a context (calls, office…), as the Inbox's form does; without either it would land back in the Inbox.",
        );
      return {
        said: `a next action${proj ? ` in ${projectName(proj)}` : ""}`,
        changes: settle(taskLine(fields), proj),
        after: captureDone,
      };
    case "waiting":
      return {
        said: `waiting for${proj ? ` in ${projectName(proj)}` : ""}`,
        changes: settle(taskLine({ ...fields, tag: "waiting-for" }), proj, WAITING_FOR),
        after: captureDone,
      };
    case "someday":
      return { said: "someday / maybe", changes: settle(taskLine({ ...fields, tag: "someday-maybe" }), proj), after: captureDone };
    case "project": {
      const name = str(a, "project_name") ?? text;
      const note = await api.changeProjectNote(name, "active", str(a, "area") ?? null, str(a, "done_looks_like") ?? null);
      const made = sub(a, r, note.path, "new", `New project ${projectName(note.path)}`, { op: "page", content: note.content });
      const rest = i.kind === "task" ? [...takeOut(), addTask(note.path, taskLine({ text: i.text.trim() }))] : takeOut();
      return { said: `the project ${projectName(note.path)}`, changes: [made, ...rest], after: captureDone };
    }
    case "done": {
      if (i.kind === "capture") return { said: "done", changes: [], after: captureDone };
      if (i.kind === "thought") return { said: "done", changes: takeOut() };
      const c = inQuote(i.lineText, (l) => completion(l, moment(localToday(), "YYYY-MM-DD")));
      const lines = c ? c.lines : [await api.changeTaskLine(i.lineText, { op: "toggle", done: true, today: localToday() })];
      return {
        said: "done",
        changes: [sub(a, r, i.path, "task", `Tick “${short}”`, { op: "lines", at: i.line, old: [i.lineText], new: lines })],
      };
    }
    case "reference": {
      if (i.kind === "capture") return { said: "kept in Sources", changes: [], after: captureDone };
      const note = str(a, "note");
      const title = str(a, "new_note");
      const body = i.text.trim();
      let filed: ChangeSubmit;
      if (note) {
        const path = note.endsWith(".md") ? note : `${note}.md`;
        filed = sub(a, r, path, "edit", `File “${short}” as reference`, { op: "append", text: `- ${body.replace(/\n/g, " ")}` });
      } else if (title) {
        const path = `${title.replace(/[/\\:*?"<>|]/g, "")}.md`;
        filed = sub(a, r, path, "new", `New note ${path.replace(/\.md$/, "")}`, { op: "page", content: `${body}\n` });
      } else throw new Error("To file it as reference, give note (a note's path to add it to) or new_note (a title).");
      return { said: `filed in ${note ?? title}`, changes: [filed, ...takeOut()] };
    }
    case "ingest":
      if (i.kind !== "capture") throw new Error("Only a capture (an email or Teams chat in Sources) can be ingested.");
      return {
        said: "ingesting; its changes are listed in Changes",
        changes: [],
        after: async () => {
          await api.ingestStart([i.path], originOf(a, r).trigger === "scheduled");
          await markCaptureDone(i.path);
        },
      };
    case "delete":
      // A transcript as the Inbox's Delete trashes it (trashCapture): its links pointed at its
      // meeting note first, each write recorded as the assistant's.
      if (i.kind === "capture" && isTranscriptPath(i.path))
        return {
          said: "deleted",
          changes: [],
          after: async () =>
            (await trashTranscript(i.path, undefined, { origin: originOf(a, r), reason: str(a, "reason"), model: r.model })).outcome
              ?.message,
        };
      if (i.kind === "capture")
        return {
          said: "deleted",
          // Not marked clarified, as the Inbox's Move to the Trash: restored, it's back in the Inbox.
          changes: [sub(a, r, i.path, "trash", `Move ${i.path.replace(/\.md$/, "").split("/").pop()} to the Trash`, { op: "trash" })],
        };
      return { said: "deleted", changes: takeOut() };
    default:
      throw new Error("becomes is next, waiting, someday, project, done, reference, ingest or delete.");
  }
}

/** Clarifies one Inbox item as `a` says (becomes, text, project…) and returns what it became, for
 *  the caller to say: the MCP server's clarify, and the weekly review's suggestions. */
export async function clarifyItem(i: InboxItem, a: Args): Promise<string> {
  const becomes = str(a, "becomes");
  const text = str(a, "text") ?? (i.kind === "capture" ? `Follow up on [[${i.text}]]` : i.text.trim());
  const proj = str(a, "project") ? await projectPath(str(a, "project")!) : null;
  const fields = { text, project: proj, context: str(a, "context") ?? null, effort: str(a, "effort") ?? null, due: str(a, "due") ?? null };
  let said: string;
  switch (becomes) {
    case "next":
      await settleInboxItem(i, taskLine(fields), proj);
      said = `a next action${proj ? ` in ${projectName(proj)}` : ""}`;
      break;
    case "waiting":
      await settleInboxItem(i, taskLine({ ...fields, tag: "waiting-for" }), proj, WAITING_FOR);
      said = "waiting for";
      break;
    case "someday":
      await settleInboxItem(i, taskLine({ ...fields, tag: "someday-maybe" }), proj);
      said = "someday / maybe";
      break;
    case "project": {
      const name = str(a, "project_name") ?? text;
      const made = await api.projectCreate(name, "active", str(a, "area") ?? null, str(a, "done_looks_like") ?? null);
      await settleInboxItem(i, i.kind === "task" ? taskLine({ text: i.text.trim() }) : null, made.path);
      said = `the project ${projectName(made.path)}`;
      break;
    }
    case "done":
      if (i.kind === "task") await toggleTaskEdit({ path: i.path, line: i.line, lineText: i.lineText }, true);
      else if (i.kind === "capture") await markCaptureDone(i.path);
      else await api.inboxRemoveThought(i.line, i.block ?? i.lineText);
      said = "done";
      break;
    case "reference": {
      if (i.kind === "capture") {
        await markCaptureDone(i.path);
        said = "kept in Sources";
        break;
      }
      const note = str(a, "note");
      const title = str(a, "new_note");
      if (note) await appendAsReference(note.endsWith(".md") ? note : `${note}.md`, i.text.trim());
      else if (title) await newReferenceNote(title, i.text.trim());
      else throw new Error("To file it as reference, give note (a note's path to add it to) or new_note (a title).");
      await settleInboxItem(i, null);
      said = `filed in ${note ?? title}`;
      break;
    }
    case "ingest":
      if (i.kind !== "capture") throw new Error("Only a capture (an email or Teams chat in Sources) can be ingested.");
      await api.ingestStart([i.path]);
      await markCaptureDone(i.path);
      said = "ingesting; its changes are listed in Changes";
      break;
    case "delete":
      if (i.kind === "capture") {
        await trashCapture(i.path);
      } else await settleInboxItem(i, null);
      said = "deleted";
      break;
    default:
      throw new Error("becomes is next, waiting, someday, project, done, reference, ingest or delete.");
  }
  return said;
}

/** Quick capture: a task to the To Do list's Inbox (the top of its Other section) or a thought to
 *  the Scratchpad, written as the capture box writes them (its shorthand for dates, contexts and
 *  effort turned into their formats), as a change in Changes. */
async function captureTool(a: Args, r: McpRequest): Promise<string> {
  const kind = str(a, "kind");
  if (kind !== "task" && kind !== "thought") throw new Error("kind is task (to the Inbox) or thought (to the Scratchpad).");
  const text = prepareCapture(kind, typeof a.text === "string" ? a.text : "");
  if (!text) throw new Error("Give text: what to capture.");
  const c =
    kind === "task"
      ? sub(a, r, TODO_LIST, "task", `Capture “${taskWords(text)}”`, { op: "add_task", line: `- [ ] ${text}` })
      : sub(a, r, SCRATCHPAD, "edit", `Capture a thought: “${text.replace(/\s+/g, " ").slice(0, 60)}”`, {
          op: "add_thought",
          text,
          stamp: captureStamp(),
        });
  const o = await submitAll([c]);
  told(
    o.applied ? `captured ${kind === "task" ? "a task to the Inbox" : "a thought to the Scratchpad"}` : "held a capture for you in Changes",
  );
  if (!o.applied) return o.message;
  return `${kind === "task" ? `Added to the Inbox (the To Do list's Other): ${text}` : "Added to the Scratchpad"}; it waits in the Inbox to clarify (list_inbox). ${o.message}`;
}

// ---- projects

/** A project's status as the tools name it, the Projects screen's tabs: the vault's `done` is Completed. */
const statusOut = (s: string) => (s === "done" ? "completed" : s);
/** A project status the tools take, as the vault writes it (`completed` is `done` there). */
function statusIn(a: Args): string | undefined {
  if (!has(a, "status")) return undefined;
  const v = String(a.status ?? "").trim();
  if (!["active", "on-hold", "someday", "completed"].includes(v)) throw new Error("status is active, on-hold, someday or completed.");
  return v === "completed" ? "done" : v;
}

/** The Projects screen's flag on an active project: Stuck (no next action) or Quiet (nothing for two weeks). */
const flagOut = (p: ProjectRow): "stuck" | "quiet" | null => {
  const f = projectFlag(p);
  return f === "none" ? "stuck" : f;
};
const flagLine = (p: ProjectRow) => {
  const f = flagOut(p);
  return f === "stuck" ? " · Stuck: it has no next action" : f === "quiet" ? " · Quiet: nothing has happened for two weeks" : "";
};

/** A project's line: its name, status, next actions and flag; with detail its area, what done looks like and every count. */
const projectOut = (p: ProjectRow, detail = true) =>
  detail
    ? `- ${p.name} · ${statusOut(p.status)}${p.area ? ` · ${p.area}` : ""} · ${p.next} next, ${p.waiting} waiting, ${p.someday} someday, ${p.done} done${flagLine(p)}${p.outcome ? ` · done looks like: ${p.outcome}` : ""}  (${p.path})`
    : `- ${p.name} · ${statusOut(p.status)} · ${p.next} next${flagLine(p)}  (${p.path})`;

async function updateProject(a: Args, r: McpRequest): Promise<string> {
  const path = await projectPath(str(a, "project") ?? "");
  const set: [string, string | null][] = [];
  const said: string[] = [];
  // The tool's names, the screen's, and the property each writes in the note.
  const fields = [
    ["status", "status", "status"],
    ["area", "area", "area"],
    ["done_looks_like", "done looks like", "outcome"],
  ] as const;
  for (const [k, words, prop] of fields) {
    if (!has(a, k)) continue;
    const v = a[k] ? String(a[k]).trim() : null;
    if (k === "status") {
      const st = statusIn({ status: v })!;
      set.push([prop, st]);
      // In the screen's words: Mark complete, or the tab it moves to.
      said.push(`${words} ${STATUS_LABEL[st as ProjectStatus]}${st === "done" ? " (Mark complete)" : ""}`);
      continue;
    }
    if (v?.includes("\n")) throw new Error(`${words[0].toUpperCase()}${words.slice(1)} is one line.`);
    set.push([prop, v || null]);
    said.push(v ? `${words} ${v}` : `cleared ${words}`);
  }
  if (!said.length) throw new Error("Give status (active, on-hold, someday, completed), area or done_looks_like.");
  const name = projectName(path);
  const o = await submitAll([sub(a, r, path, "edit", `Set ${said.join(", ")} for ${name}`, { op: "properties", set })]);
  told(o.applied ? `${said.join(", ")} for ${name}` : `held a change to ${name} for you in Changes`);
  return o.applied ? `Set ${said.join(", ")} for ${name}. ${o.message}` : o.message;
}

/** rename_area: the area's pencil on the Projects screen, over the same projects (`areaProjects`). */
async function renameArea(a: Args, r: McpRequest): Promise<string> {
  const area = str(a, "area")?.trim() ?? "";
  const to = str(a, "to")?.trim() ?? "";
  if (!area || !to) throw new Error("Give the area as it is now and its new name.");
  if (to.includes("\n")) throw new Error("An area's name is one line.");
  const all = await api.projectsList();
  const ps = areaProjects(all, area);
  if (!ps.length) {
    const areas = [...new Set(all.map((p) => p.area).filter(Boolean))].sort();
    throw new Error(`No project is under the area ${area}. The areas are: ${areas.join(", ") || "none"}.`);
  }
  if (to === area) return `${area} is already called that.`;
  const title = `Renamed the area ${area} to ${to}`;
  const o = await submitAll(ps.map((p) => sub(a, r, p.path, "edit", title, { op: "properties", set: [["area", to]] })));
  const n = `${ps.length} ${ps.length === 1 ? "project" : "projects"}`;
  if (o.applied)
    renameInProjectOrder(
      "areas",
      area,
      to,
      all.map((p) => p.area ?? ""),
    );
  told(o.applied ? `renamed the area ${area} to ${to}` : `held renaming the area ${area} for you in Changes`);
  return o.applied ? `Renamed the area ${area} to ${to} on ${n}. ${o.message}` : o.message;
}

/** move_project: dragging a project among its area's, or an area among the areas, on the Projects
 *  screen; the same setting (`moveInProjectOrder`, D-20261012-01). */
async function moveProject(a: Args): Promise<string> {
  const after = str(a, "after") ?? null;
  const before = str(a, "before") ?? null;
  if (!after === !before) throw new Error("Give after or before (one of them).");
  const all = await api.projectsList();
  const where = after ? `after ${after}` : `before ${before}`;
  const area = str(a, "area");
  if (area) {
    const areas = orderedAreas(all.map((p) => p.area ?? "")).filter(Boolean);
    const find = (x: string) => {
      const hit = areas.find((y) => y.toLowerCase() === x.toLowerCase());
      if (!hit) throw new Error(`There's no area called ${x}. The areas are: ${areas.join(", ") || "none"}.`);
      return hit;
    };
    const it = find(area);
    const other = find((after ?? before)!);
    if (other === it) throw new Error("Give another area to put it next to.");
    moveInProjectOrder(all, "areas", it, after && other, before && other);
    told(`moved the area ${it} ${where}`);
    return `Moved the area ${it} ${where}. The areas are now: ${orderedAreas(all.map((p) => p.area ?? ""))
      .map((x) => x || "No area")
      .join(", ")}.`;
  }
  const project = str(a, "project");
  if (!project) throw new Error("Give project or area.");
  const path = await projectPath(project);
  const otherPath = await projectPath((after ?? before)!);
  const p = all.find((x) => x.path === path)!;
  const o = all.find((x) => x.path === otherPath)!;
  if (path === otherPath) throw new Error("Give another project to put it next to.");
  if ((p.area ?? "") !== (o.area ?? ""))
    throw new Error(
      `${o.name} is in ${o.area || "No area"}, not ${p.area || "No area"}: a project is ordered among its own area's. update_project changes its area.`,
    );
  moveInProjectOrder(all, "projects", path, after && otherPath, before && otherPath);
  const now = orderedProjects(all).filter((x) => (x.area ?? "") === (p.area ?? ""));
  told(`moved ${p.name} ${where}`);
  return `Moved ${p.name} ${where}. ${p.area || "No area"} is now: ${now.map((x) => x.name).join(", ")}.`;
}

async function createProject(a: Args, r: McpRequest): Promise<string> {
  const name = str(a, "name");
  if (!name) throw new Error("Give the project's name.");
  const note = await api.changeProjectNote(name, statusIn(a) ?? "active", str(a, "area") ?? null, str(a, "done_looks_like") ?? null);
  const o = await submitAll([sub(a, r, note.path, "new", `New project ${projectName(note.path)}`, { op: "page", content: note.content })]);
  told(o.applied ? `made the project ${name}` : `held the project ${name} for you in Changes`);
  return o.message;
}

// ---- notes

/** Runs a template as New note does, answering its questions from `answers` in order, and puts
 *  the note (an agent change, in Changes). */
async function noteFromTemplate(a: Args, chat: string | null): Promise<string> {
  const origin: ChangeOrigin = (a.origin as ChangeOrigin | undefined) ?? { kind: "chat", chat: chat ?? undefined, run: chat ?? undefined };
  const template = str(a, "template") ?? "";
  const files = (await api.filesList(null)).map((f) => f.path);
  const tpath = files.find(
    (p) =>
      p.startsWith("Templates/") &&
      (p === template || p.replace(/^Templates\//, "").replace(/\.md$/, "") === template.replace(/^Templates\//, "").replace(/\.md$/, "")),
  );
  if (!tpath)
    throw new Error(
      `No template “${template}”. The templates: ${
        files
          .filter((p) => p.startsWith("Templates/") && p.endsWith(".md"))
          .map((p) => p.slice(10, -3))
          .join(", ") || "none"
      }.`,
    );
  // An answer is text, or for a question with several choices (tp.system.multi_suggester) a list
  // of them, as ticking several on the screen does; null is the screen's Cancel (no answer, and the
  // template carries on), as "" is for a question with choices.
  const answers = (Array.isArray(a.answers) ? a.answers : []).map((x) =>
    x === null ? null : Array.isArray(x) ? x.map(String) : String(x),
  );
  let n = 0;
  const asked: string[] = [];
  // Past the answers given, a question about the type, name or date takes the note's own.
  const guess = (text: string) =>
    /prefix|type|kind/i.test(text)
      ? str(a, "type")
      : /date|day|when/i.test(text)
        ? str(a, "date")
        : /name|title|topic|subject|who|with/i.test(text)
          ? str(a, "title")
          : undefined;
  const ask = async (q: Ask): Promise<string | number[] | null> => {
    const given = n < answers.length ? answers[n] : undefined;
    n++;
    if (given === null || (given === "" && q.kind !== "prompt")) {
      asked.push(`“${q.text}” → (cancelled)`);
      return null;
    }
    const ans = given ?? guess(q.text);
    if (q.kind === "prompt") {
      if (Array.isArray(ans)) throw new Error(`“${q.text}” asks for text, not a list of choices.`);
      const v = ans ?? q.value;
      asked.push(`“${q.text}” → ${v || "(nothing)"}`);
      return v;
    }
    const picks = ans === undefined ? [] : Array.isArray(ans) ? ans : [ans];
    if (q.kind === "suggest" && picks.length > 1) throw new Error(`“${q.text}” takes one choice, not ${picks.length}.`);
    asked.push(`“${q.text}” → ${picks.length ? picks.join(", ") : "(no choice)"}`);
    if (ans === undefined) return null;
    return picks.map((p) => {
      const at = q.labels.findIndex((l) => l.toLowerCase() === p.toLowerCase());
      if (at >= 0) return at;
      if (/^\d+$/.test(p) && Number(p) < q.labels.length) return Number(p);
      throw new Error(`“${p}” isn't one of the choices for “${q.text}”: ${q.labels.join(", ")}.`);
    });
  };
  // The other notes it makes (tp.file.create_new), as New note makes them, but as changes beside
  // the note's own; a test run, as the template editor's, only lists them.
  const testRun = a.test_run === true;
  const others: [path: string, content: string][] = [];
  // Once the note is made, a note its finishing steps (tp.hooks) make is a change of its own.
  let made = false;
  const late: string[] = [];
  const create = async (p: string, c: string) => {
    if (!made) return void others.push([p, c]);
    const o = await makePage(p, c, `New note ${p.replace(/\.md$/, "")}`, origin);
    late.push(o.message);
  };
  const env = await vaultEnv(tpath, ask as Env["ask"], create);
  let r;
  try {
    r = await runTemplate((await api.docRead(tpath)).content, env);
  } catch (e) {
    if (e instanceof StopRun) throw new Error("The template stopped.", { cause: e });
    throw e;
  }
  const folder = (str(a, "folder") ?? "").replace(/^\/+|\/+$/g, "");
  const named = composeFilename({ type: str(a, "type") ?? null, title: str(a, "title") ?? "", date: str(a, "date") ?? null });
  const taken = new Set(files.map((p) => p.toLowerCase()));
  const path = r.named ? r.path : named ? (folder ? `${folder}/${named}` : named) : freeName(r.path, (p) => taken.has(p.toLowerCase()));
  if (testRun) {
    const also = others.length ? `\nIt would also make: ${others.map(([p]) => p).join(", ")}.` : "";
    const qa = asked.length ? `\nThe template asked: ${asked.join("; ")}.` : "";
    return `Test run of ${tpath}, nothing written: it would make ${path}${taken.has(path.toLowerCase()) ? " (which exists already)" : ""}.${qa}${also}\n\n${r.content}`;
  }
  if (taken.has(path.toLowerCase())) throw new Error(`${path} exists already. To change it, use edit_page.`);
  const qa = asked.length ? ` The template asked: ${asked.join("; ")}.` : "";
  // The note and the others it made, made or held together.
  const all: [string, string][] = [[path, r.content], ...others];
  const outs = await api.changesSubmitMany(
    all.map(([p, content]) => ({
      page: p,
      kind: "new",
      title: `New note ${p.replace(/\.md$/, "")}`,
      instruction: { op: "page", content },
      origin,
    })),
  );
  made = outs.every((o) => o.applied);
  const name = path.replace(/\.md$/, "");
  const more = others.length ? ` and ${others.length} more` : "";
  told(made ? `made the note ${name}${more}` : `held the note ${name}${more} for you in Changes`);
  // Its finishing steps (tp.hooks.on_all_templates_executed), once the note is made, as New note runs them.
  const hooks = !r.hooks.length
    ? ""
    : made
      ? ` Its finishing steps ran${await runHooks(r.hooks).then((f) => (f.length ? `, but ${f.join(" ")}` : ""))}.${late.length ? ` ${late.join(" ")}` : ""}`
      : " Its finishing steps (tp.hooks) didn't run, as the note isn't made yet.";
  const also = others.length ? `, with ${others.map(([p]) => p).join(", ")}, which the template makes too` : "";
  return `${outs.map((o) => o.message).join(" ")} It was made from ${tpath}${also}.${qa}${hooks}`;
}

// ---- agent changes

/** A change the MCP server checked: made or held by the rule, told in a toast. */
async function submitChange(a: Args): Promise<string> {
  const c = a.change as ChangeSubmit | undefined;
  if (!c) throw new Error("No change given.");
  const o = await api.changeSubmit(c);
  const name = c.page.replace(/\.md$/, "").split("/").pop();
  told(o.applied ? `changed ${name}: ${c.title}` : `held a change to ${name} for you in Changes`);
  return o.message;
}

/** A meeting note accepted from Changes: the screen's follow-up offered in the app (offerFollowUp,
 *  ingest it and move its transcript to the Trash), and what's next said for the assistant. */
function followUpOf(note: string, transcript: string): string {
  offerFollowUp({ note, transcript });
  return `${pageName(note)} is a meeting note: Brainstead now asks the user whether to ingest it and move its transcript (${transcript}) to the Trash, as accepting it on the Changes screen does. Leave that to them, or, if they ask you, start_run ingest with ${note} and trash_note the transcript.`;
}

/** The changes tool: the feed and the held changes, one change, accept, reject, revert. */
async function changesTool(a: Args): Promise<string | Listing> {
  const action = str(a, "action") ?? "list";
  const id = str(a, "id");
  if (action === "list") {
    const rows = await api.changesList(str(a, "page"));
    const empty = { total: 0, matching: 0, offset: 0, next_offset: null, items: [] };
    if (!rows.length) return { text: "No changes yet.", structured: { held: empty, made: empty } };
    const text = changeText;
    // Short: what, where, its state and flags; detail adds its kind, who made it and why.
    const line = (c: (typeof rows)[number]) =>
      a.detail === true
        ? `- ${c.title} · ${c.page} · ${c.kind} · ${c.status}${c.flags.length ? ` · ${c.flags.join(" ")}` : ""} · ${originLabel(c)}${c.reason ? ` · why: ${c.reason}` : ""}  (id ${c.id})`
        : `- ${c.title} · ${c.page} · ${c.status}${c.flags.length ? ` · ${c.flags.join(" ")}` : ""}  (id ${c.id})`;
    const groupOf = new Map<string, string>();
    for (const g of byRun(rows)) for (const c of g.rows) groupOf.set(c.id, g.group);
    const item = (c: (typeof rows)[number]) => ({
      id: c.id,
      title: c.title,
      page: c.page,
      kind: c.kind,
      status: c.status,
      flags: c.flags,
      origin: originLabel(c),
      group: groupOf.get(c.id) ?? "",
      reason: c.reason,
      created: c.created,
    });
    // One list, as the screen shows it: the held ones first, then those made, newest first; limit,
    // offset and query page through it as a whole, so the next page carries on where this one ended.
    const isHeld = (c: (typeof rows)[number]) => c.status === "held";
    const heldRows = rows.filter(isHeld);
    const madeRows = rows.filter((c) => !isHeld(c));
    const all = pagedRows(a, [...heldRows, ...madeRows], text, ["change", "changes"]);
    const matching = (rs: typeof rows) => pagedRows({ query: a.query, limit: 1 }, rs, text, ["", ""]).page.matching;
    const part = (rs: typeof rows) => {
      const shown = all.shown.filter((c) => rs.includes(c));
      return { shown, page: { ...all.page, total: rs.length, matching: matching(rs), items: shown.map(item) } };
    };
    const held = part(heldRows);
    const made = part(madeRows);
    const runs = (cs: typeof rows) => byRun(cs).flatMap((g) => [`Run “${g.label}” (group ${g.group}):`, ...g.rows.map(line)]);
    const text_ = [
      `${all.head}: ${heldRows.length ? `${held.page.matching} held for the user` : "nothing held"}, then ${made.page.matching} made, newest first.`,
      ...(held.shown.length ? ["Held for the user:", ...runs(held.shown)] : []),
      ...(made.shown.length ? ["Made (changes revert undoes one, revert_run a whole run by its group):", ...runs(made.shown)] : []),
    ].join("\n");
    return { text: text_, structured: { held: held.page, made: made.page } };
  }
  if (action === "history") {
    const cur = settings.get();
    const days = has(a, "days") ? Number(a.days) : undefined;
    const mb = has(a, "mb") ? Number(a.mb) : undefined;
    if (days !== undefined && ![30, 90, 180, 365].includes(days)) throw new Error("days is 30, 90, 180 or 365.");
    if (mb !== undefined && ![100, 250, 500, 1000, 2000].includes(mb)) throw new Error("mb is 100, 250, 500, 1000 or 2000.");
    const size = (m: number) => (m >= 1000 ? `${m / 1000} GB` : `${m} MB`);
    if (days === undefined && mb === undefined)
      return {
        text: `Changes keeps its history for ${cur.changesKeepDays ?? 90} days or up to ${size(cur.changesKeepMb ?? 500)}, whichever comes first (Settings › AI assistants › Keep the history of agent changes). Held changes are kept however old.`,
        structured: { history: { days: cur.changesKeepDays ?? 90, mb: cur.changesKeepMb ?? 500 } },
      };
    settings.update({ ...(days !== undefined ? { changesKeepDays: days } : {}), ...(mb !== undefined ? { changesKeepMb: mb } : {}) });
    await settings.flush();
    const now = settings.get();
    noted(`set Changes to keep ${now.changesKeepDays ?? 90} days or ${size(now.changesKeepMb ?? 500)} of history`);
    return `Changes now keeps its history for ${now.changesKeepDays ?? 90} days or up to ${size(now.changesKeepMb ?? 500)}, whichever comes first. Older history goes at the next daily tidy.`;
  }
  // Assistants accept only while the user is there, and only what was held for a failed check (D-20261005-09).
  if ((action === "accept" || action === "accept_run") && a.unattended === true)
    throw new Error(
      "Nothing held can be accepted in a session nobody is watching: the user accepts held changes on Brainstead's Changes screen.",
    );
  // A run's changes, only those query finds when given, as Accept all, Reject all and Revert all do
  // with Changes' search on.
  const inRun = async (group: string, status: ChangeRow["status"]) => {
    const q = (str(a, "query") ?? "").trim();
    const todo = runChanges(await api.changesList(), group, status, q);
    if (q && !todo.length) throw new Error(`No ${status} change in that run matches “${q}”.`);
    return { todo, ids: q ? todo.map((c) => c.id) : undefined };
  };
  if (action === "accept_run" || action === "reject_run") {
    const group = str(a, "group");
    if (!group) throw new Error("Which run? Give its group from list_changes.");
    const { todo, ids } = await inRun(group, "held");
    const m = action === "accept_run" ? await api.changesAcceptAll(group, true, ids) : await api.changesRejectAll(group, ids);
    noted(`${action === "accept_run" ? "accepted" : "rejected"} ${m.done} held change${m.done === 1 ? "" : "s"}`);
    // The run's meeting notes that were made, offered their follow-up, as the screen's Accept all does.
    const made = action === "accept_run" ? new Set((await api.changesList()).filter((c) => c.status === "applied").map((c) => c.id)) : null;
    const offered = made
      ? todo
          .filter((c) => c.origin.kind === "meeting" && c.origin.chat && made.has(c.id))
          .map((c) => followUpOf(c.to ?? c.page, c.origin.chat!))
      : [];
    return [
      `${action === "accept_run" ? "Accepted" : "Rejected"} ${m.done}.${m.failed.length ? ` Couldn't: ${m.failed.join("; ")}` : ""}`,
      ...offered,
    ].join(" ");
  }
  if (action === "revert_run") {
    const group = str(a, "group");
    if (!group) throw new Error("Which run? Give its group from list_changes.");
    const m = await api.changesRevertAll(group, (await inRun(group, "applied")).ids);
    noted(`reverted ${m.done} change${m.done === 1 ? "" : "s"} from a run`);
    return `Reverted ${m.done}.${m.failed.length ? ` Left as they are (edited since): ${m.failed.join("; ")}` : ""}`;
  }
  if (!id) throw new Error("Which change? Give its id from list_changes.");
  if (action === "show") {
    const v = await api.changeGet(id);
    const changes = v.changes.map(
      // One that can't be made: what it asked for, which has no place on the page (as the screen labels it).
      (h) =>
        `${v.problem ? "What it asked for" : h.section || "Start of the page"}:\n${h.old.map((l) => `- ${l}`).join("\n")}\n${h.new.map((l) => `+ ${l}`).join("\n")}`,
    );
    const quotes = v.quotes.map(
      (q) =>
        `- “${q.text}” from ${q.source}${q.checked === false ? " (not found in the source)" : q.checked === null ? " (not checked)" : ""}`,
    );
    const shown = [
      `${v.title} · ${v.to ?? v.page} · ${v.kind} · ${v.status}`,
      v.reason && `Why: ${v.reason}`,
      v.flags.length ? `${v.status === "held" ? "Held because" : "Flagged"}: ${v.flags.join(" ")}` : "",
      v.problem ? `Can't be made: ${v.problem}` : "",
      ...changes,
      quotes.length ? `Quotes:\n${quotes.join("\n")}` : "",
    ]
      .filter(Boolean)
      .join("\n\n");
    return {
      text: shown,
      structured: {
        change: {
          id,
          title: v.title,
          page: v.to ?? v.page,
          kind: v.kind,
          status: v.status,
          reason: v.reason,
          flags: v.flags,
          problem: v.problem ?? null,
          hunks: v.changes.map((h) => ({ section: h.section, old: h.old, new: h.new })),
          quotes: v.quotes.map((q) => ({ text: q.text, source: q.source, checked: q.checked })),
        },
      },
    };
  }
  if (action === "accept") {
    const row = (await api.changesList()).find((c) => c.id === id);
    const o = await api.changeAccept(id, true);
    noted("accepted a held change");
    // A meeting note held in Changes: its follow-up offered once it's made, as the screen's Accept does.
    if (row?.origin.kind === "meeting" && row.origin.chat) return `${o.message} ${followUpOf(o.page, row.origin.chat)}`;
    return o.message;
  }
  if (action === "reject") {
    await api.changeReject(id);
    noted("turned down a held change");
    return "Rejected: the page is left as it is.";
  }
  if (action === "revert") {
    const r = await api.changeRevert(id);
    if (r.ok) noted("reverted a change");
    return r.ok ? r.message : `${r.message} (The page as it was before is in Changes, for the user to copy from.)`;
  }
  throw new Error("action is list, show, accept, reject, accept_run, reject_run, revert, revert_run or history.");
}

// ---- runs

/** The summaries' runs, by name. */
const SUMMARY_RUNS: Record<string, "daily" | "weekly"> = {
  daily_summary: "daily",
  weekly_summary: "weekly",
};

async function startRun(a: Args, r: McpRequest): Promise<string> {
  const run = str(a, "run");
  const summary = run ? SUMMARY_RUNS[run] : undefined;
  // Nobody watching: every run's changes that fail a check are held, as a scheduled run's are.
  const unattended = a.unattended === true;
  if (summary) {
    const day = str(a, "date") || undefined;
    const r = await api.reviewRunNow(summary, day, unattended);
    noted(`started the ${summary} summary${day ? ` for ${day}` : ""}`);
    return `The ${summary} summary has started (${r}). It's written when it finishes; run_status shows it, and summary reads it.`;
  }
  switch (run) {
    case "ingest": {
      const sources = (Array.isArray(a.sources) ? a.sources : []).map(String);
      if (!sources.length) throw new Error("Give sources: the paths in sources/ to ingest.");
      // Only what the file menu offers Ingest on: what an ingest can read, and not a wiki page or a template.
      const cant = sources.filter((p) => !ingestable(p));
      if (cant.length)
        throw new Error(
          `An ingest can't read ${cant.join(", ")}: it takes notes and text, PDFs, Office files (Word, PowerPoint, Excel) and images.`,
        );
      const own = sources.filter((p) => !canIngest(p));
      if (own.length)
        throw new Error(
          `${own.join(", ")} can't be ingested: wiki pages are what an ingest writes, and Templates/ holds the app's templates, so the file menu doesn't offer Ingest on them.`,
        );
      const ids = await api.ingestStart(sources, unattended);
      noted(`started ingesting ${sources.length} source${sources.length === 1 ? "" : "s"}`);
      return `Ingesting ${sources.join(", ")} (run ${ids.join(", ")}). Its page changes are made and listed in Changes${unattended ? "; any that fail a check are held for the user" : ""}; run_status shows how it's going.`;
    }
    case "daily_check":
      await api.dailyCheckRunNow(unattended);
      noted("started the daily check");
      return "The daily check has started; run_status shows how it's going.";
    case "find_tasks":
      await api.findRun();
      noted("started looking for tasks and projects in the notes");
      return "Reading the user's notes from the last 90 days for tasks and projects; the suggestions show at the top of Tasks and Projects, and nothing changes until the user accepts one. list_suggestions lists them; run_status shows how it's going.";
    case "weekly_prep": {
      // The review's week, as the screen's Prepare is: a paused review keeps its own.
      const { week, st } = await weeklyNow();
      await api.weekprepRun(week);
      noted(`started preparing the weekly review of ${week}`);
      return `Preparing the review of ${week}${st ? " (the paused review's week)" : ""} has started; its suggestions show in the Weekly review when it finishes, and nothing changes until the user accepts one. run_status shows how it's going.`;
    }
    case "contradictions":
      if (!(await api.contradictionsRun(unattended))) return "A contradictions check is running already.";
      noted("started a contradictions check");
      return "The contradictions check has started; run_status shows how it's going, and its report comes to the Contradictions screen.";
    case "meeting_note": {
      const transcript = str(a, "transcript");
      const all = await api.meetingTranscripts();
      const t = all.find((x) => x.path === transcript);
      if (!t)
        throw new Error(
          `No transcript to do at ${transcript ?? "(none given)"}. The transcripts still to do: ${
            all
              .filter((x) => !x.done)
              .map((x) => x.path)
              .join(", ") || "none"
          }.`,
        );
      const type = str(a, "type");
      if (type && !MEETING_TYPES.includes(type)) throw new Error(`type is ${MEETING_TYPES.join(", ")}, as the screen offers them.`);
      // The note's name and date as the screen builds them, from what's given over what was read.
      const edits: Partial<NoteSpec> = {};
      if (type) edits.type = type;
      if (str(a, "name") !== undefined) edits.name = str(a, "name");
      if (str(a, "date") !== undefined) edits.date = str(a, "date");
      const spec = specOf(t, edits);
      // As the screen's card offers nothing while a note is being drafted.
      if (
        (await api.ingestRuns()).some(
          (r) => r.kind === "meeting" && r.source === t.path && (r.status === "running" || r.status === "queued"),
        )
      )
        throw new Error(`A note is being drafted from ${t.path} already; run_status shows how it's going.`);
      // The card's Ingest the existing note: no new note, the one there kept, then ingested and the
      // transcript moved to the Trash, as Settings › AI assistants' Meeting notes switches say (keepNote).
      if (a.ingest_existing === true) {
        if (!noteExists(t, edits))
          throw new Error(
            `There's no ${noteFile(spec)} to keep: give the name, type and date of the note that's there, or leave ingest_existing out to draft a new one.`,
          );
        // Recorded as the assistant's: the link rewrites and the move to the Trash, held when nobody's watching.
        const origin = { ...originOf(a, r), ...(unattended ? { trigger: "scheduled" } : {}) };
        const did = await keepNote(t.path, noteFile(spec), { origin, reason: str(a, "reason"), model: r.model });
        noted(`kept ${pageName(noteFile(spec))} as the note from ${t.path}`);
        const then = [did.ingested && "ingesting it (run_status shows how it's going)", did.trashed && "moved the transcript to the Trash"]
          .filter(Boolean)
          .join(" and ");
        return `Kept ${noteFile(spec)} as it is${then ? `, ${then}` : "; ingesting and trashing are off in Settings › AI assistants, so nothing else was done"}.`;
      }
      if (needsDate(t, edits))
        throw new Error(
          `The date in ${t.path}'s name (${t.inferred.date}) is the day it was captured, which may not be the meeting's: ask the user for the meeting date and give it as date.`,
        );
      // As Draft the note refuses: a name, and a date as YYYY-MM-DD.
      if (!specOk(spec))
        throw new Error(
          `${t.path} needs a name and a date: give name (who the 1-1 was with, or the meeting's name) and date (YYYY-MM-DD) as the user confirms them.`,
        );
      // As the screen's Draft the note waits: not over a note there already.
      if (noteExists(t, edits))
        throw new Error(
          `${noteFile(spec)} exists already: the user can open it to fill it in, ingest it as it is (ingest_existing), or give another name or date for a new note.`,
        );
      await draftNotes([[t.path, spec]], unattended);
      noted(`started a meeting note from ${t.path}`);
      const f = followThrough();
      const then = [f.ingest && "ingested", f.trash && "the transcript moved to the Trash"].filter(Boolean).join(" and ");
      return `Drafting the ${spec.type} note “${spec.name}” for ${spec.date}; it's made in the vault when it's ready (listed in Changes)${then ? `, then ${then}` : ""}.`;
    }
    default:
      throw new Error(
        "run is ingest, daily_check, daily_summary, weekly_summary, weekly_prep, find_tasks, contradictions or meeting_note.",
      );
  }
}

/** How many summary runs Settings › Jobs & schedule's Recent runs lists. */
const RECENT_SUMMARIES = 8;

/** A summary run as Recent runs shows it, with its change in Changes: a summary's block is written
 *  (or held) as an agent change in the run's group (src-tauri/src/reviews.rs), so changes revert
 *  with its id undoes it, as Undo does. */
export function summaryRunLine(r: ReviewRun, changes: ChangeRow[]): string {
  const name = `${r.kind === "daily" ? "Daily summary" : "Weekly summary"} ${r.target}`;
  const how = r.trigger === "schedule" ? "scheduled" : r.trigger === "manual" ? "run now" : r.trigger;
  const when = r.startedAt.slice(0, 16).replace("T", " ");
  const c = changes.find((x) => x.group === r.id);
  const state =
    r.status === "error"
      ? `failed: ${r.error ?? "no reason given"}`
      : r.status === "running"
        ? "running"
        : r.status === "stopped"
          ? "stopped"
          : r.undone || c?.status === "reverted"
            ? `undone${r.file ? `, in ${r.file}` : ""}`
            : c?.status === "held"
              ? `held in Changes for ${r.file ?? "its note"}`
              : r.file
                ? `${r.replaced ? "replaced its block in" : "written to"} ${r.file}`
                : r.status;
  const change = !c
    ? ""
    : c.status === "applied"
      ? ` · change ${c.id} (changes revert with it undoes the summary)`
      : c.status === "held"
        ? ` · change ${c.id} (held for the user)`
        : ` · change ${c.id} (${c.status})`;
  // Its chat with the model, as Recent runs' Open opens it.
  return `- ${name} · ${how} · ${when} · ${state}${change}${r.chat ? ` · chat ${r.chat}` : ""}`;
}

async function runStatus(a: Args = {}): Promise<string> {
  const [runs, check, reviews, prep, find, contra, cs, changes] = await Promise.all([
    api.ingestRuns(),
    api.dailyCheckStatus(),
    api.reviewsStatus(),
    api.weekprepJob(),
    api.findStatus().catch(() => null),
    api.contradictionsReport().catch(() => null),
    api.currentStateStatus().catch(() => null),
    api.changesList().catch(() => [] as ChangeRow[]),
  ]);
  const f = find?.run;
  const summaries = (reviews.runs ?? []).slice(0, RECENT_SUMMARIES);
  const c = contra?.last;
  // Meeting notes are runs in the ingest queue of their own kind: each listed apart, with its id
  // for stop_run.
  const going = (r: (typeof runs)[number]) => r.status === "running" || r.status === "queued";
  const ingests = runs.filter((r) => r.kind !== "meeting");
  const meetings = runs.filter((r) => r.kind === "meeting");
  const active = ingests.filter(going);
  const writing = meetings.filter(going);
  // With query, the ingests the runs pane's search finds (runMatches), up to 20; else the last five.
  const query = str(a, "query");
  const recent = query ? ingests.filter((r) => runMatches(r, query)).slice(0, 20) : ingests.slice(0, 5);
  // Finished and failed meeting notes, as the runs pane lists them.
  const written = meetings.filter((r) => !going(r)).slice(0, 5);
  // What the checks left out of a run, as the runs pane's "left out by the checks" lists it.
  const left = (r: (typeof runs)[number]) =>
    r.dropped.length ? `; left out by the checks: ${r.dropped.map((d) => `${d.page} (${d.reason})`).join(", ")}` : "";
  return [
    active.length ? `Ingest running: ${active.map((r) => `${r.source} (${r.status}, id ${r.id})`).join("; ")}` : "No ingest running.",
    `${query ? `Ingests matching “${query}”` : "Recent ingests"}: ${recent.map((r) => `${r.source} — ${r.status}${r.error ? ` (${r.error})` : ""}, ${r.proposals.length} change${r.proposals.length === 1 ? "" : "s"}${left(r)} (id ${r.id})`).join("; ") || "none"}`,
    writing.length
      ? `Meeting notes being written: ${writing.map((r) => `${r.source} (${r.status}, id ${r.id})`).join("; ")}`
      : "No meeting note being written.",
    `Recent meeting notes: ${
      written
        .map(
          (r) =>
            `${r.source} — ${r.status}${r.error ? ` (${r.error})` : ""}${r.note && r.status === "done" ? `, ${noteFile(r.note)}` : ""}${left(r)} (id ${r.id})`,
        )
        .join("; ") || "none"
    }`,
    `Daily check: ${check.running ? `running (${check.doing}, ${Math.round(check.progress * 100)}%)` : `last ${check.lastRun ?? "never"}, next ${check.next ?? "not scheduled"}`}${check.summary ? ` — ${check.summary}` : ""}`,
    `Daily and weekly summaries: ${reviews.running.length ? `running the ${reviews.running.join(" and ")} summary; ` : ""}next daily ${reviews.next.daily ?? "not scheduled"}, next weekly ${reviews.next.weekly ?? "not scheduled"}`,
    `Weekly review preparation: ${
      prep.running
        ? `preparing ${prep.running}`
        : `last ${prep.last ? `${prep.last.startedAt} for ${prep.last.week} (${prep.last.status === "done" ? `${prep.last.count} suggestions` : `${prep.last.status}${prep.last.error ? `: ${prep.last.error}` : ""}`})${prep.last.chat ? `, chat ${prep.last.chat}` : ""}` : "never"}, next ${prep.next ?? "not scheduled"}`
    }`,
    `Find tasks and projects: ${
      !f
        ? "never run"
        : f.status === "running"
          ? `running (${f.done} of ${f.batches} batches, ${f.found} found)`
          : `last ${f.startedAt}, ${f.status}${f.error ? ` (${f.error})` : ""}, ${find?.suggestions.length ?? 0} suggestions waiting`
    }`,
    `Contradictions check: ${
      !c?.started
        ? "never run"
        : c.running
          ? `running (${c.doing || "checking"})`
          : `last ${c.finished ?? c.started}, ${c.contradictions} contradictions${c.error ? ` (${c.error})` : ""}`
    }`,
    `Write Current state: ${
      !cs?.total ? "not run" : `${cs.running ? "running" : "last run"}: ${cs.done} of ${cs.total} pages done, ${cs.written} written`
    }`,
    summaries.length
      ? `Recent summary runs (Settings › Jobs & schedule), newest first:\n${summaries.map((r) => summaryRunLine(r, changes ?? [])).join("\n")}`
      : "Recent summary runs: none yet.",
  ].join("\n");
}

async function stopRun(a: Args): Promise<string> {
  const run = str(a, "run");
  const summary = run ? SUMMARY_RUNS[run] : undefined;
  if (summary) {
    await api.reviewsStop(summary);
    noted(`stopped the ${summary} summary`);
    return `Stopping the ${summary} summary.`;
  }
  if (run === "ingest" || run === "meeting_note") {
    // A meeting note is a run in the ingest queue, of its own kind.
    // Without an id, the one running, else the next waiting, as the screen's Stop on that row does.
    const meeting = run === "meeting_note";
    const given = str(a, "id");
    const mine = given ? [] : (await api.ingestRuns()).filter((r) => (r.kind === "meeting") === meeting);
    const r = mine.find((x) => x.status === "running") ?? mine.find((x) => x.status === "queued");
    const id = given ?? r?.id;
    if (!id) return meeting ? "No meeting note is being written or waiting." : "No ingest is running or waiting.";
    await api.ingestStop(id);
    if (r?.status === "queued") {
      noted(`stopped a waiting ${meeting ? "meeting note" : "ingest"} of ${r.source}`);
      return `Stopping the waiting ${meeting ? "meeting note" : "ingest"} of ${r.source} (run ${r.id}); it won't start.`;
    }
  } else if (run === "daily_check") await api.dailyCheckStop();
  else if (run === "weekly_prep") await api.weekprepStop();
  else if (run === "contradictions") await api.contradictionsStop();
  else if (run === "find_tasks") await api.findStop();
  else if (run === "write_current_state") await api.currentStateStop();
  else
    throw new Error(
      "run is ingest, daily_check, daily_summary, weekly_summary, weekly_prep, find_tasks, contradictions, meeting_note or write_current_state.",
    );
  noted(`stopped the ${run.replace("_", " ")}`);
  return `Stopping the ${run.replace("_", " ")}.`;
}

// ---- the weekly review

/** The review in progress (a paused one keeps its week), or the week a new one would be for. */
async function weeklyNow(): Promise<{ week: string; st: WeeklyState | null }> {
  const today = localToday();
  const saved = await api.weeklyStateRead().catch(() => null);
  return keepsPaused(saved, today) ? { week: saved.week, st: saved } : { week: reviewWeek(today), st: null };
}

/** An Inbox item to file as reference without a note named: the screen sends it to the Inbox. */
const toFile = (s: WeekPrepSuggestion) =>
  s.action?.do === "clarify" && s.action.becomes === "reference" && !s.action.text && s.action.item.kind !== "capture";

/** What accepting a suggestion does, in words; a prompt has nothing to accept. */
function offers(s: WeekPrepSuggestion, week: string, projects: ProjectRow[]): string {
  if (!s.action) return "nothing, it's a prompt to think about";
  if (toFile(s)) return "nothing, file it as reference with clarify_inbox and a note";
  return choice(s, week, projects)?.title ?? s.action.do;
}

async function weeklyStatus(): Promise<string> {
  const { week, st } = await weeklyNow();
  const [prep, projects] = await Promise.all([api.weekprepStatus(week), api.projectsList()]);
  const active = projects.filter((p) => p.status === "active");
  const r = settings.get().weeklyReview;
  const job = await api.weekprepJob().catch(() => null);
  const out = [
    `The weekly review for ${week}. It's scheduled for ${scheduleLabel(r?.day, r?.time)} (Settings › Jobs & schedule); the user can start it any time from its start page.`,
  ];
  if (job?.next) out.push(`Its suggestions are next prepared on schedule at ${job.next}.`);
  const note = await api.weeklyReviewNote(week).catch(() => null);
  if (note) out.push(`Finished: the user's review of the week is in ${note} (read it with read_section; finishing again rewrites it).`);
  if (st) {
    const done = st.done.map((i) => STEPS[i]?.title).filter(Boolean);
    out.push(
      `In progress: on step ${st.step + 1} of ${STEPS.length}, ${STEPS[st.step]?.title}; ${done.length} done${done.length ? ` (${done.join(", ")})` : ""}.`,
    );
    if (st.log.length) out.push("So far:", ...st.log.map((l) => `- ${l}`));
    if (st.notes.trim()) out.push(`Notes: ${st.notes.trim()}`);
  } else if (!note) out.push(`Not started: ${STEPS.length} steps, from ${STEPS[0].title} to ${STEPS[STEPS.length - 1].title}.`);
  if (prep.running) out.push("Its suggestions are being prepared now.");
  if (prep.error) out.push(`The suggestions couldn't be prepared: ${prep.error}`);
  if (!prep.prep) {
    if (!prep.running && !prep.error) out.push("No suggestions are prepared for this week; start_run weekly_prep prepares them.");
    return out.join("\n");
  }
  const handled = st?.handled ?? {};
  const open = prep.prep.suggestions.filter((x) => !handled[x.id]);
  out.push(
    `Suggestions prepared ${prep.prep.preparedAt}: ${open.length} still to decide, ${prep.prep.suggestions.length - open.length} accepted or skipped.`,
  );
  for (const step of STEPS) {
    const rows = open.filter((x) => x.step === prepStep(step.id));
    if (!rows.length) continue;
    out.push("", `${step.title}:`);
    for (const x of rows) {
      const src = x.source ? ` · from ${x.source.path}${x.source.quote ? ` (“${x.source.quote}”)` : ""}` : "";
      out.push(`- ${x.text}${src} · accepting: ${offers(x, week, active)}  (id ${x.id})`);
    }
  }
  return out.join("\n");
}

async function weeklySuggestion(a: Args): Promise<string> {
  const id = str(a, "id");
  const how = str(a, "action");
  if (!id) throw new Error("Which suggestion? Give its id from weekly_review.");
  if (how !== "accept" && how !== "skip") throw new Error("action is accept or skip.");
  const { week, st } = await weeklyNow();
  const prep = (await api.weekprepStatus(week)).prep;
  const s = prep?.suggestions.find((x) => x.id === id);
  if (!s) throw new Error(`No suggestion ${id} for ${week}. weekly_review lists them.`);
  // As on the screen, where the suggestions are steps of a review: this doesn't start one.
  if (!st)
    throw new Error(
      "The weekly review isn't started: its suggestions are accepted or skipped in it. weekly_step start starts it, as the user asks.",
    );
  if (st.handled?.[id]) throw new Error(`That suggestion was ${st.handled[id]} already.`);
  let line = "";
  if (how === "accept") {
    const projects = (await api.projectsList()).filter((p) => p.status === "active");
    const c = toFile(s) ? null : choice(s, week, projects, a.unattended === true);
    if (!c) throw new Error(`Accepting it does ${offers(s, week, projects)}. Skip it once it's dealt with.`);
    line = await c.run();
  }
  // Kept with the review's progress, as the screen keeps it, so it stays hidden there.
  await api.weeklyStateWrite({
    ...st,
    handled: { ...st.handled, [id]: how === "accept" ? "accepted" : "skipped" },
    log: line ? [...st.log, line] : st.log,
  });
  window.dispatchEvent(new Event(WEEKLY_STATE_CHANGED));
  if (how === "skip") {
    noted(`skipped a weekly review suggestion: “${s.text.slice(0, 60)}”`);
    return `Skipped “${s.text}”.`;
  }
  return s.action?.do === "link"
    ? line.startsWith("Held ")
      ? `Accepted: ${line.replace(/\.$/, "")}. Nothing is changed until they accept it.`
      : `Accepted: ${line}; the link is made (revertable in Changes).`
    : `Accepted: ${line}. ${keyLabel("⌘Z")} in the app undoes it.`;
}

/** The weekly review's Start over: its progress, notes, decisions and handled suggestions dropped,
 *  from step 1 for this week, and the week's suggestions prepared if it has none, as starting the
 *  review on the screen prepares them (weekprepEnsure). */
async function weeklyStartOver(): Promise<string> {
  const week = reviewWeek(localToday());
  const saved = await api.weeklyStateRead().catch(() => null);
  await api.weeklyStateWrite(fresh(week));
  window.dispatchEvent(new Event(WEEKLY_STATE_CHANGED));
  const preparing = await api.weekprepEnsure(week).catch(() => false);
  noted(`started the weekly review over for ${week}`);
  return `Started the weekly review over for ${week}: it's at step 1 of ${STEPS.length}${saved ? `; the review of ${saved.week} that was paused on step ${saved.step + 1} is dropped` : ""}. What it changed in the vault stays.${preparing ? " Its suggestions are being prepared (run_status shows how it's going)." : ""}`;
}

/** Settings › General › Moving over: its checklist as the screen shows it, a tick on the lines the
 *  user ticks, and its Retire them, which waits for "The other app is stopped" as the screen's does
 *  (D-20261005-12). */
async function movingOver(a: Args): Promise<string> {
  const action = str(a, "action") ?? "list";
  const [st, cap] = await Promise.all([api.switchoverStatus(), api.captureStatus().catch(() => null)]);
  const s = settings.get();
  const ticked = s.movingOver ?? {};
  const items = checklist(s, st, cap?.recent ?? []);
  if (action === "list") {
    if (!st.previous) return "Moving over isn't shown: the previous app's own files aren't in the vault.";
    const needed = items.filter((i) => !i.optional);
    return [
      `Moving over from another notes app (Settings › General): ${needed.filter((i) => isDone(i, ticked)).length} of ${needed.length} done.`,
      ...items.map(
        (i) =>
          `- [${isDone(i, ticked) ? "x" : " "}] ${i.label}${i.optional ? " · optional" : ""} · ${i.done === undefined ? "ticked by the user" : "Brainstead checks this"}: ${i.detail}  (${i.id})`,
      ),
    ].join("\n");
  }
  if (action === "tick" || action === "untick") {
    const id = str(a, "item");
    const i = items.find((x) => x.id === id);
    const manual = items.filter((x) => x.done === undefined);
    if (!i || i.done !== undefined)
      throw new Error(
        `${i ? `${i.label}: Brainstead checks this itself.` : `No item ${id ?? "(none given)"}.`} The items ticked by hand: ${manual.map((x) => x.id).join(", ")}.`,
      );
    const on = action === "tick";
    if (!!ticked[i.id] === on) return `“${i.label}” is ${on ? "ticked" : "not ticked"} already.`;
    settings.update({ movingOver: { ...ticked, [i.id]: on } });
    await settings.flush();
    noted(`${on ? "ticked" : "unticked"} “${i.label}” in Moving over`);
    return `${on ? "Ticked" : "Unticked"} “${i.label}”.`;
  }
  if (action !== "retire") throw new Error("action is list, tick, untick or retire.");
  if (isRetired(st)) return "Nothing to retire: the previous app's skills and scripts are gone already.";
  if (!ticked.otherStopped) throw new Error(`${STOP_FIRST} (moving_over tick otherStopped, once the user has stopped it.)`);
  const said = await api.switchoverRetire();
  noted(`retired the previous app's skills and scripts`);
  return `${said} They can be restored from there.`;
}

// ---- knowledge

async function healthFix(a: Args): Promise<string> {
  const v = await api.healthReport(true);
  const r = v.report;
  if (!r) throw new Error("Knowledge health has no report yet; try again in a moment.");
  const all = safeFixes(r);
  const wanted = Array.isArray(a.items) ? (a.items as unknown[]).map(String) : null;
  const fixes = wanted
    ? r.checks
        .filter((c) => c.id !== "page-shape")
        .flatMap((c) => c.items.filter((i) => i.safe && wanted.includes(i.text)).map((i) => fixOf(c.id, i)))
    : all;
  if (!fixes.length)
    return wanted
      ? "None of those items has a safe fix. lint lists the issues; the ones marked safe can be fixed here."
      : "Nothing to fix safely.";
  const said = await api.healthFix(fixes);
  told(`fixed ${fixes.length} Knowledge health issue${fixes.length === 1 ? "" : "s"}`);
  return `${said} ${keyLabel("⌘Z")} in the app undoes it.`;
}

/** ignore_issue: Knowledge health's Ignore, or Show again with show_again. */
async function healthIgnore(a: Args): Promise<string> {
  const v = await api.healthReport(true);
  const r = v.report;
  if (!r) throw new Error("Knowledge health has no report yet; try again in a moment.");
  if (a.show_again) {
    const check = typeof a.check === "string" && a.check ? a.check : null;
    const n = await api.healthUnignore(check);
    told(`showed ${n} ignored Knowledge health issue${n === 1 ? "" : "s"} again`);
    return n ? `${n} ignored issue${n === 1 ? " is" : "s are"} listed again.` : "Nothing was ignored.";
  }
  if (a.all === true) {
    const check = typeof a.check === "string" ? a.check : "";
    if (!IGNORE_ALL.has(check)) throw new Error(`Ignore all is for ${[...IGNORE_ALL].join(" and ")}: give one as check.`);
    const n = await api.healthIgnoreAll(check);
    told(`ignored ${n} Knowledge health issue${n === 1 ? "" : "s"}`);
    return n
      ? `Ignored ${n} issue${n === 1 ? "" : "s"}, each until its ${check === "uncited-claims" ? "line" : "page"} changes. show_again lists them again.`
      : "Nothing to ignore.";
  }
  const text = String(a.item ?? "");
  const c = r.checks.find((c) => c.items.some((i) => i.text === text));
  const i = c?.items.find((i) => i.text === text);
  if (!c || !i) return "That issue isn't in Knowledge health: give its text as lint gives it.";
  if (i.safe) return "That issue has a safe fix: use fix_health instead.";
  if (OWN_ACTIONS.has(c.id))
    return `${c.title} issues have buttons of their own instead of Ignore: health_issue, start_run ingest or reshape_pages.`;
  await api.healthIgnore(c.id, text);
  told("ignored a Knowledge health issue");
  return c.id === "uncited-claims"
    ? "Ignored: it's left out of Knowledge health until its line changes. show_again lists it again."
    : "Ignored: it's left out of Knowledge health until its page changes. show_again lists it again.";
}

/** reshape_pages: Reshape pages, as Knowledge health's Page shape check does it. */
async function reshapePages(a: Args): Promise<string> {
  const pages = Array.isArray(a.pages) ? (a.pages as unknown[]).map(String).filter((p) => p.trim()) : null;
  const r = await api.healthReshape(pages?.length ? pages : null, a.unattended === true);
  told(`reshaped ${r.applied} wiki page${r.applied === 1 ? "" : "s"}`);
  return [
    `Reshaped ${r.applied} page${r.applied === 1 ? "" : "s"}, each a change in Changes (run group ${r.run}; changes revert_run undoes them all).`,
    r.failed.length ? `Couldn't: ${r.failed.join("; ")}` : "",
    r.left ? `${r.left} page${r.left === 1 ? "" : "s"} need the user first; page_shape says why.` : "",
  ]
    .filter(Boolean)
    .join(" ");
}

/** write_current_state: Knowledge health's Write Current state run: start it, or its status, or stop it. */
async function writeCurrentState(a: Args): Promise<string> {
  const said = (r: CurrentStateRun) =>
    `${r.running ? "Running" : "Last run"}: ${r.done} of ${r.total} pages done, ${r.written} written, ${r.nothing} with nothing current to say${r.failed.length ? `; couldn't: ${r.failed.join("; ")}` : ""}${r.run ? ` (run group ${r.run}; changes revert_run undoes it)` : ""}.`;
  if (a.stop === true) {
    await api.currentStateStop();
    return "Stopping after the page it's on.";
  }
  if (a.status === true) return said(await api.currentStateStatus());
  const pages = Array.isArray(a.pages) ? (a.pages as unknown[]).map(String).filter((p) => p.trim()) : null;
  const limit = typeof a.limit === "number" && a.limit > 0 ? Math.floor(a.limit) : null;
  const ok = await api.currentStateStart(pages?.length ? pages : null, limit, a.unattended === true);
  if (!ok) return `Write Current state is running already. ${said(await api.currentStateStatus())}`;
  told("started Write Current state");
  const r = await api.currentStateStatus();
  return `Started on ${r.total} page${r.total === 1 ? "" : "s"} with ${r.model}; each is a change in Changes. Ask with status for progress.`;
}

async function fixName(a: Args): Promise<string> {
  // Fix name's fields: Written as and Correct spelling.
  const wrong = str(a, "written_as");
  const right = str(a, "correct_spelling");
  if (!wrong || !right) throw new Error("Give written_as (the name as it's misspelt) and correct_spelling.");
  // As the screen, which plans nothing while the two are spelt the same.
  if (wrong.trim() === right.trim()) throw new Error("correct_spelling is spelt the same as written_as: there's nothing to fix.");
  // Files to leave alone (part of the name each), Where it's from, and Remember this correction, on
  // unless false, as on the screen.
  const guards = (Array.isArray(a.files_to_leave_alone) ? a.files_to_leave_alone : []).map((g) => String(g).trim()).filter(Boolean);
  const remember = a.remember !== false;
  // Ask about each file: every note to rewrite starts unticked, to be given in files once read, and
  // the remembered correction is marked ambiguous, as on the screen.
  const askEach = a.ask_about_each_file === true;
  const req: FixNameRequest = {
    wrong,
    right,
    rightPage: null,
    guards,
    ambiguous: askEach,
    note: remember ? (str(a, "where_its_from") ?? "") : "",
    skipSubstitution: !remember,
  };
  const plan = await api.fixnamePlan(req);
  if (!a.apply) {
    // As the screen lists it: every file the name is in, with what happens to it; only the
    // rewrites and the alias change.
    if (!plan.rows.length) return `“${wrong}” isn't in any note.`;
    const unticked = startUnticked(plan.rows, askEach);
    const change = chosenRows(plan.rows, unticked).length;
    const row = (r: (typeof plan.rows)[number]) =>
      `- ${r.file}: ${r.count} line${r.count === 1 ? "" : "s"} · ${FIX_ACTION[r.action] ?? r.action}${unticked.has(r.file) ? " · unticked: read it, then give it in files" : ""}${r.inFilename ? " · In the file name: rename it separately" : ""}`;
    return [
      `Fixing “${wrong}” → “${right}” would change ${change} of the ${plan.rows.length} file${plan.rows.length === 1 ? "" : "s"} it's in${unticked.size ? ` (${unticked.size} more to tick one by one: Ask about each file)` : ""}${plan.rightPage ? ` (${right} has a wiki page, which gets “${wrong}” as an alias)` : ""}:`,
      ...plan.rows.slice(0, 40).map(row),
      ...(plan.rows.length > 40 ? [`…and ${plan.rows.length - 40} more.`] : []),
      change || unticked.size
        ? "Call again with apply true to make the change, and files to make it only in those."
        : "None of them would change.",
    ].join("\n");
  }
  // files: the ticked files, as Fix name's ticks are; the rest stay as they are.
  const files = Array.isArray(a.files) ? a.files.map(String).filter((f) => f.trim()) : null;
  const can = chosenRows(plan.rows, new Set());
  if (files) {
    const unknown = files.filter((f) => !can.some((r) => r.file === f));
    if (unknown.length)
      throw new Error(
        `Not among the files the fix would change: ${unknown.join(", ")}. They are: ${can.map((r) => r.file).join(", ") || "none"}.`,
      );
  }
  // Without files, the ones ticked to start with: with Ask about each file, no note to rewrite.
  const take = files
    ? chosenRows(plan.rows, new Set(can.map((r) => r.file).filter((f) => !files.includes(f))))
    : chosenRows(plan.rows, startUnticked(plan.rows, askEach));
  if (!take.length) return "Nothing to change: no file given.";
  const said = await api.fixnameApply({ ...req, rightPage: plan.rightPage }, take);
  told(`fixed the name “${wrong}” → “${right}”`);
  return said;
}

// ---- the rest

async function trash(a: Args, r: McpRequest): Promise<string | Listing> {
  const action = str(a, "action") ?? "list";
  if (action === "list") {
    const all = (await api.trashList()).sort((x, y) => y.deletedAt.localeCompare(x.deletedAt));
    const bytes = all.reduce((n, t) => n + t.sizeBytes, 0);
    // As the Trash screen says it (fmtBytes counts in 1024s).
    const size = fmtBytes(bytes);
    const { shown, head, page } = pagedRows(a, all, (t) => t.originalRel, ["item", "items"]);
    const text = all.length
      ? [
          `The Trash (${size}), newest first: ${head}:`,
          ...shown.map((t) => `- ${t.originalRel} · deleted ${t.deletedAt.slice(0, 10)}  (id ${t.id})`),
        ].join("\n")
      : "The Trash is empty.";
    const items = shown.map((t) => ({ id: t.id, path: t.originalRel, layer: t.layer, deleted: t.deletedAt, bytes: t.sizeBytes }));
    return { text, structured: { ...page, bytes, size, items } };
  }
  if (action === "restore") {
    const id = str(a, "id");
    if (!id) throw new Error("Give id: the Trash entry to restore, from list_trash.");
    const e = (await api.trashList()).find((t) => t.id === id);
    if (!e) throw new Error(`Nothing in the Trash has the id ${id}. list_trash lists what's there.`);
    // Through Changes, so the hold rules apply: a restore into Templates/, or of code that runs, waits for the user.
    const to = str(a, "to")?.trim() || e.originalRel;
    const o = await submitAll([sub(a, r, to, "restore", `Restore ${pageName(to)} from the Trash`, { op: "restore", id })]);
    told(o.applied ? `restored ${to} from the Trash` : `held restoring ${to} from the Trash for you in Changes`);
    return o.applied ? `Restored to ${to}. Revert it in Changes.` : o.message;
  }
  throw new Error("action is list or restore. Moving a note to the Trash is trash_note.");
}

async function bookmarks(a: Args): Promise<string | Listing> {
  const page = str(a, "page");
  const keep = str(a, "keep");
  const remove = str(a, "remove");
  if (remove) {
    // As Triage's Archive and Remove: by the bookmark's target, so one whose note is gone comes off too.
    await api.bookmarkRemove(remove);
    told(`took the bookmark ${remove} off`);
    return `Took the bookmark ${remove} off. Undoable with ${keyLabel("⌘Z")}.`;
  }
  if (keep) {
    await api.bookmarkKeep(keep);
    told(`kept the bookmark ${keep}`);
    return `Kept the bookmark ${keep}: it won't need triage for two weeks.`;
  }
  if (!page) {
    // As the sidebar lists them, with Triage's untouched days.
    const line = (b: BookmarkRow) =>
      `- ${b.title}${b.path ? ` (${b.path})` : " (missing)"}${!b.missing && b.stale && b.days !== null ? ` · untouched ${b.days} days` : ""}`;
    return listed(a, await api.bookmarksStatus(), {
      text: line,
      noun: ["bookmark", "bookmarks"],
      line,
      item: (b) => ({ title: b.title, target: b.target, path: b.path, days: b.days, stale: b.stale }),
      empty: "No bookmarks.",
    });
  }
  // As a note's Bookmark: only a text note (the file menu offers it on .md and .txt) that's there.
  if (!/\.(md|txt)$/i.test(page))
    throw new Error(`${page} isn't a note: Bookmark is on notes (.md or .txt) only, as the file menu offers it.`);
  await api.docRead(page).catch(() => {
    throw new Error(`There's no note at ${page}. A bookmark whose note is gone comes off with remove (its target, from list_bookmarks).`);
  });
  const on = await api.bookmarkToggle(page);
  told(on ? `bookmarked ${page}` : `took the bookmark off ${page}`);
  return on ? `Bookmarked ${page}.` : `Took the bookmark off ${page}.`;
}

async function savedSearches(a: Args): Promise<string> {
  const name = str(a, "name");
  const query = str(a, "query");
  if (a.delete === true) {
    if (!name) throw new Error("Give name: the saved search to delete.");
    await api.smartListDelete(name);
    told(`deleted the saved search ${name}`);
    return `Deleted the saved search “${name}”.`;
  }
  if (!name || !query) {
    const rows = await api.smartLists();
    // Its layers in search's names (notes, wiki, sources, templates), so running it matches the screen.
    const named = (l: string) => (l === "wiki" ? l : `${l}s`);
    return rows.length
      ? paged(
          a,
          rows.map((s) => `- ${s.name}: ${s.query}${s.layers.length ? ` · layers ${s.layers.map(named).join(", ")}` : ""}`),
          ["saved search", "saved searches"],
        )
      : "No saved searches.";
  }
  // As Save search allows it: a query of two letters or more, not one saved already, and only the
  // screen's layers (notes, wiki, sources), all of them kept as none.
  if (query.length < 2) throw new Error("A saved search needs a query of two letters or more, as Save search does.");
  const lists = await api.smartLists();
  const same = lists.find((l) => l.query === query);
  if (same) throw new Error(`That search is saved already, as “${same.name}”.`);
  // search's layer names (notes, sources…) or the app's (note, source…).
  const want = [...new Set((Array.isArray(a.layers) ? a.layers : []).map((l) => String(l).replace(/s$/, "")))];
  const bad = want.filter((l) => !(SEARCH_LAYERS as string[]).includes(l));
  if (bad.length) throw new Error(`A saved search's layers are notes, wiki and sources, as Search's filters are (not ${bad.join(", ")}).`);
  const layers = want.length === SEARCH_LAYERS.length ? [] : want;
  await api.smartListSave(name, query, layers);
  told(`saved the search ${name}`);
  return `Saved the search “${name}”.`;
}

async function activity(a: Args): Promise<Listing> {
  const date = str(a, "date");
  const { log: all } = await api.activity();
  const full = (e: LogEntry) =>
    `- ${e.date}${e.time ? ` ${e.time}` : ""} ${e.action} ${e.title}${e.description ? ` — ${e.description}` : ""}`;
  const short = (e: LogEntry) => (a.detail === true ? full(e) : `- ${e.date} ${e.action} ${e.title}`);
  const entryItem = (e: LogEntry) => ({ date: e.date, time: e.time, action: e.action, title: e.title, description: e.description });
  // The screen's action chips: an action as the log names it or as its chip says it.
  const actions = actionsIn(all);
  const want = str(a, "action")?.toLowerCase();
  const action = want ? actions.find((x) => x.toLowerCase() === want || actionLabel(x).toLowerCase() === want) : undefined;
  if (want && !action) throw new Error(`No “${want}” entries in log.md. Its actions: ${actions.join(", ") || "none"}.`);
  // The screen's order while searching: Best match (its default) or Latest; newest first otherwise.
  const order = str(a, "order") ?? "best_match";
  if (order !== "best_match" && order !== "latest") throw new Error("order is best_match or latest, as the Activity screen's are.");
  const query = str(a, "query");
  // The entries the screen shows: its action chip, the day picked, and its search (filterLog).
  const filtered = filterLog(all, { action: action ?? null, day: date ?? null, q: "" });
  const log = query && order === "best_match" ? rankLog(filtered, query) : filtered;
  const matches = (e: LogEntry, q: string) => filterLog([e], { action: null, day: null, q }).length > 0;
  if (date) {
    // A day picked, as on the screen: the files changed that day, then that day's entries in log.md
    // (which the action and query narrow, as they narrow the screen's list; the files stay).
    type Row = { file: string } | { entry: LogEntry };
    const files = (await api.activityDay(date)).map((f): Row => ({ file: f.path }));
    const rows: Row[] = [...files, ...log.map((e): Row => ({ entry: e }))];
    return listed(a, rows, {
      text: (r) => ("file" in r ? r.file : r.entry.title),
      match: (r, q) => "file" in r || matches(r.entry, q),
      noun: [`file changed or entry in log.md on ${date}`, `files changed and entries in log.md on ${date}`],
      def: 80,
      line: (r) => ("file" in r ? `- ${r.file}` : short(r.entry)),
      item: (r) => ("file" in r ? { date, path: r.file } : entryItem(r.entry)),
      empty: `Nothing changed on ${date}.`,
    });
  }
  return listed(a, log, {
    text: (e) => e.title,
    // query's words in the entry's title, description or action's label, as the screen's search looks.
    match: matches,
    noun:
      query && order === "best_match"
        ? ["entry in log.md, best match first", "entries in log.md, best match first"]
        : ["entry in log.md, newest first", "entries in log.md, newest first"],
    def: 30,
    // Short: the day, what and which; detail adds the time and the description.
    line: short,
    item: entryItem,
    empty: "log.md is empty.",
  });
}

async function graph(a: Args): Promise<string> {
  // The page by its name or [[link]], as open and note_look take it; the whole wiki without one.
  const page = str(a, "page");
  let center: string | null = null;
  if (page) {
    [center] = await api.linksResolve([page.replace(/^\[\[|\]\]$/g, "")]);
    if (!center) throw new Error(`There's no page called ${page}.`);
  }
  // The kind chips, by their labels: the kinds turned off, the centre kept, as the screen's visible().
  const want = (Array.isArray(a.hide) ? a.hide : []).map(String);
  const bad = want.filter((l) => !GRAPH_KINDS.some(([, label]) => label === l));
  if (bad.length)
    throw new Error(`hide takes the Graph screen's chips: ${GRAPH_KINDS.map(([, l]) => l).join(", ")} (not ${bad.join(", ")}).`);
  const hidden = GRAPH_KINDS.filter(([, label]) => want.includes(label)).map(([k]) => k);
  // Two links away unless said, as the Graph screen starts.
  const g = graphVisible(await api.graph(center, Math.min(Math.max(Number(a.depth) || 2, 1), 3)), hidden, center);
  const name = new Map(g.nodes.map((n) => [n.id, n.title ?? n.id]));
  const lines = g.edges.map(([s, t]) => `- ${name.get(s)} → ${name.get(t)}`);
  // Cut to the nearest 400 pages, as the screen says under it.
  const links = `${g.nodes.length} pages${g.truncated ? " (the nearest 400: there are more)" : ""}; ${paged(a, lines, ["link", "links"], 200)}`;
  if (a.detail !== true) return links;
  // detail: the pages too, nearest first, each with its type, steps from the page and links.
  const pages = [...g.nodes]
    .sort((x, y) => x.depth - y.depth || y.degree - x.degree)
    .map((n) => `- ${n.title ?? n.id}${n.type ? ` · ${n.type}` : ""} · ${n.layer} · ${n.depth} away · ${n.degree} links`);
  return [links, "", "Pages:", ...pages.slice(0, 200), ...(pages.length > 200 ? [`…and ${pages.length - 200} more`] : [])].join("\n");
}

async function status(): Promise<string> {
  const [s, v, u, sw, perm, info] = await Promise.all([
    api.settingsRead(),
    api.vaultStatus(),
    api.undoPeek(),
    api.switchoverStatus().catch(() => null),
    api.permissions().catch(() => null),
    api.appInfo().catch(() => null),
  ]);
  const old = sw && (sw.skills || sw.scripts);
  const n = v.stats;
  // As Settings › Vault shows the index, its counts or why it failed.
  const index =
    v.state === "ready"
      ? `${fmtCount(n.files)} files indexed: ${fmtCount(n.notes)} notes, ${fmtCount(n.wiki)} wiki, ${fmtCount(n.sources)} sources, ${fmtCount(n.templates)} templates, ${fmtCount(n.openTasks)} open tasks, ${fmtCount(n.unresolvedLinks)} unresolved links; updated ${new Date(n.updatedAt).toLocaleString("en-GB")}`
      : v.state === "indexing"
        ? "Indexing…"
        : v.state === "error"
          ? `Index failed: ${v.error ?? "no reason given"} (rebuild_index tries again)`
          : "No vault";
  return [
    `Vault: ${s.vaultPath ?? "none chosen"}${s.readOnly ? " (read-only: Brainstead won't change it until Settings › Vault › Read-only is off)" : ""}`,
    `Index: ${index}`,
    // Settings › Permissions' Full Disk Access, where macOS asks for it.
    perm?.applies
      ? `Full Disk Access: ${perm.fullDiskAccess === true ? "granted" : perm.fullDiskAccess === false ? "not granted (Settings › Permissions says how to give it)" : `${isMac ? "macOS" : "the system"} can't say`}`
      : "",
    info ? `Brainstead ${info.version} (${info.build}); its app data folder: ${info.dataDir}; its website: ${WEBSITE}` : "",
    `Brainstead runs the daily and weekly summaries (Settings › Jobs & schedule): ${s.summariesHere ? "on" : "off"}`,
    u ? `${keyLabel("⌘Z")} would undo: ${u.label}` : "Nothing to undo.",
    old
      ? "The previous app's skills or scripts are still in the vault (.claude/skills, scripts/): don't use them; use these tools. The user can retire them in Settings › General › Moving over."
      : "",
  ]
    .filter(Boolean)
    .join("\n");
}

/** Settings › Capture extensions, as the pane shows it: the folder the extensions load from, the
 *  browsers that know Brainstead's capture host, and the last captures. */
/** Teams and Outlook sync (Settings › Teams and Outlook sync), doing what the pane does: read its
 *  state, run a pass, turn it on or off, choose a channel, or skip a sender. macOS and Windows. */
async function syncTool(a: Args): Promise<string> {
  const action = str(a, "action") ?? "get";
  const s = settings.get();
  if (action === "get") {
    const [state, meetings] = await Promise.all([
      api.syncSignInState(),
      s.syncEnabled ? api.syncTodayMeetings().catch(() => api.syncCachedMeetings()) : Promise.resolve([]),
    ]);
    const ignore = s.syncIgnoreSenders ?? [];
    return [
      `Teams and Outlook sync is ${s.syncEnabled ? "on" : "off"}.`,
      state.signedIn
        ? `Signed in (handed over ${state.handedOver ? new Date(state.handedOver).toISOString().slice(0, 16).replace("T", " ") : "recently"}).${state.stale.length ? ` Gone stale, waiting for Teams to renew: ${state.stale.join(", ")}.` : ""}${state.rejected.length ? ` ${rejectedText(state.rejected)}` : ""}${state.handedOver && state.extensionOutdated !== null ? ` ${extensionOutdatedText(state)}` : ""}`
        : "Not signed in — open Teams in the browser so the extension hands the sign-in over.",
      `1:1 and group chats sync automatically; channels chosen: ${(s.syncChannels ?? []).length}.`,
      ignore.length ? `Mail skipped from: ${ignore.join(", ")}.` : "No senders skipped.",
      meetings.length
        ? `Upcoming meetings today:\n${meetings.map((m) => `- ${new Date(m.start).toISOString().slice(11, 16)} ${m.subject}`).join("\n")}`
        : "No more meetings today.",
    ].join("\n");
  }
  if (action === "on" || action === "off") {
    settings.update({ syncEnabled: action === "on" });
    return `Teams and Outlook sync turned ${action}.`;
  }
  if (action === "now") {
    const r = await api.syncNow();
    if (r.blocked) return `Didn't run: ${r.blocked}`;
    return `Synced: ${r.chats} chat day(s), ${r.mail} email(s), ${r.transcripts} transcript(s).${r.problems.length ? ` Problems: ${r.problems.join("; ")}.` : ""}`;
  }
  if (action === "ignore" || action === "unignore") {
    const sender = str(a, "sender")?.trim().toLowerCase();
    if (!sender) throw new Error("sender is the address or domain to skip (or stop skipping).");
    const have = s.syncIgnoreSenders ?? [];
    settings.update({ syncIgnoreSenders: action === "ignore" ? [...new Set([...have, sender])] : have.filter((x) => x !== sender) });
    return `${action === "ignore" ? "Skipping" : "No longer skipping"} ${sender}.`;
  }
  if (action === "channel" || action === "unchannel") {
    const id = str(a, "id")?.trim();
    if (!id) throw new Error("id is the channel's conversation id (teams_sync get, or list the channels, to find it).");
    const have = s.syncChannels ?? [];
    settings.update({ syncChannels: action === "channel" ? [...new Set([...have, id])] : have.filter((x) => x !== id) });
    return `${action === "channel" ? "Syncing" : "No longer syncing"} channel ${id}.`;
  }
  if (action === "channels") {
    const chans = await api.syncListChannels();
    const chosen = new Set(s.syncChannels ?? []);
    return chans.length ? chans.map((c) => `- ${c.topic} (${chosen.has(c.id) ? "on" : "off"}) · ${c.id}`).join("\n") : "No channels found.";
  }
  throw new Error("action is get, on, off, now, channels, channel, unchannel, ignore or unignore.");
}

/** On-demand pull from Teams and Outlook: gather the chats, mail and transcripts about a topic over
 *  the last N days into the vault (reusing what's already there) and list the source paths to read. */
async function syncPull(a: Args): Promise<string> {
  const query = str(a, "query") ?? "";
  const days = typeof a.days === "number" ? Math.max(1, Math.min(90, Math.floor(a.days))) : 7;
  const kindsArg = Array.isArray(a.kinds) ? (a.kinds as string[]).filter((k) => ["chats", "mail", "transcripts"].includes(k)) : undefined;
  const r = await api.syncPull(query, days, kindsArg);
  if (r.blocked) throw new Error(r.blocked);
  if (!r.items.length)
    return `Nothing found for ${query ? `"${query}"` : "that window"} in the last ${days} days.${r.problems.length ? ` Problems: ${r.problems.join("; ")}.` : ""}`;
  const pulled = r.items.filter((i) => !i.alreadyHad).length;
  const byKind = (k: string) => r.items.filter((i) => i.kind === k);
  const section = (k: string, label: string) => {
    const items = byKind(k);
    if (!items.length) return [];
    return [
      `${label} (${items.length}):`,
      ...items.map(
        (i) =>
          `- ${i.date} · ${i.title}${i.path ? ` → ${i.path}` : " (already in the vault; find it with search)"}${i.alreadyHad ? "" : " [new]"}`,
      ),
    ];
  };
  return [
    `${r.items.length} source(s) for ${query ? `"${query}"` : "the window"} over the last ${days} days — ${pulled} newly pulled, ${r.items.length - pulled} already in the vault. Read them with read_section or search, then write your summary.`,
    ...section("email", "Emails"),
    ...section("chat", "Teams chats"),
    ...section("transcript", "Meeting transcripts"),
    ...(r.problems.length ? ["", `Problems: ${r.problems.join("; ")}`] : []),
  ].join("\n");
}

async function captureExtensions(): Promise<string> {
  const c = await api.captureStatus();
  const name = { outlook: "Outlook", teams: "Teams", other: "Other" } as const;
  return [
    `The extensions' folder: ${c.folder ?? "not set up yet"}.`,
    // As the pane shows each one's: the version Load unpacked loads from that folder.
    ...(Object.keys(c.versions ?? {}).length
      ? [
          `Versions in the folder: ${Object.entries(c.versions)
            .map(([k, v]) => `${k} ${v}`)
            .join(", ")} (the browser's extensions page shows the version it has loaded).`,
        ]
      : []),
    c.browsers.length
      ? `Browsers: ${c.browsers.map((b) => `${b.name} (${b.registered ? "knows the capture host" : "doesn't know the capture host yet"}${b.current ? ", the one in use" : ""})`).join("; ")}.`
      : "No browser found that takes the extensions.",
    c.recent.length
      ? `Last captures, newest first:\n${c.recent.map((x) => `- ${x.at.slice(0, 16).replace("T", " ")} · ${name[x.extension] ?? x.extension} · ${x.what}${x.path ? ` → ${x.path}` : ""}${x.error ? ` · refused: ${x.error}` : ""}`).join("\n")}`
      : "No captures yet.",
  ].join("\n");
}

/** Glance's counts, as Activity and Knowledge health show them: notes and wiki pages by type and
 *  by tag, most first, and the files the most others link to. */
async function glanceTool(): Promise<string> {
  const g = await api.glance();
  const counts = (cs: Count[]) => cs.map((c) => `${c.name || "(none)"} ${fmtCount(c.n)}`).join(", ") || "none";
  return [
    `Notes by type: ${counts(g.noteTypes)}`,
    `Notes by tag: ${counts(g.noteTags)}`,
    `Wiki pages by type: ${counts(g.wikiTypes)}`,
    `Wiki pages by tag: ${counts(g.wikiTags)}`,
    `Most linked: ${g.mostLinked.map((m) => `${m.title} (${m.path}, ${fmtCount(m.links)} links)`).join("; ") || "none"}`,
  ].join("\n");
}

/** Settings › AI assistants' Found on this computer, as the pane lists it; look_again is its Look
 *  again (findClis(true)), for one installed or signed in since. */
async function assistantsFound(a: Args): Promise<string> {
  await findClis(a.look_again === true);
  const found = clis.get() ?? [];
  const n = found.filter((c) => c.path).length;
  // The pane's other groups: Brainstead's tools' Terminal commands, and Skills Brainstead does now.
  const [info, skills] = await Promise.all([api.appInfo().catch(() => null), handedOver().catch(() => [])]);
  const cmds = info ? terminalCommands(found, info.exe) : [];
  return [
    `Found on this computer (Settings › AI assistants)${a.look_again === true ? ", looked for again just now" : ""}: ${n ? `${n} assistant${n === 1 ? "" : "s"}` : "no assistants"}.`,
    ...found.map(
      (c) =>
        `- ${CLI_LABEL[c.cli]}${c.version ? ` · ${c.version}` : ""}: ${foundDetail(c)}${c.path ? "" : ` How to install: ${INSTALL[c.cli]}`}`,
    ),
    "",
    cmds.length
      ? `Brainstead's tools from an assistant in ${terminalApp}: run its command once, and it registers Brainstead for all its sessions:`
      : `Brainstead's tools from ${terminalApp}: no assistant is installed yet, so there's no command to show.`,
    ...cmds.map(([name, cmd]) => `- ${name}: ${cmd}`),
    ...(skills.length
      ? [
          "",
          "Skills Brainstead does now (picking one in Ask opens its screen; the vault's own copies keep working):",
          ...skills.map((s) => `- /${s.name}: ${SCREEN_NAME[s.opens!] ?? s.opens}`),
        ]
      : []),
  ].join("\n");
}

/** A source's Provenance card, as Sources shows it: ingested or not and by which run, its size and
 *  date, what the index holds, and the pages citing it with the passages they quote. */
async function provenance(a: Args): Promise<string> {
  const source = (str(a, "source") ?? "").replace(/^\[\[|\]\]$/g, "");
  if (!source) throw new Error("Give source: the source's path in the vault, as pending_sources gives it.");
  const p = await api.sourceProvenance(source);
  if (!p) throw new Error(`There's no source at ${source} in the index: pending_sources lists the sources.`);
  const run = p.lastRun;
  const passages = p.citers.reduce((n, c) => n + c.passages.length, 0);
  const state = p.citers.length
    ? run?.finished
      ? `Ingested ${fmtDay(Date.parse(run.finished))}`
      : "Cited in the wiki"
    : "Not ingested yet";
  const ran = run ? ` (run ${run.id}${run.status === "done" ? "" : `, ${run.status}`}, ${run.model.replace(/^[a-z]+:/, "")})` : "";
  return [
    `${source}: ${state}${ran}.`,
    `Size ${fmtBytes(p.size)} · changed ${fmtDay(p.mtime)}.`,
    `Text: ${p.chunks ? `${p.chunks} passage${p.chunks === 1 ? "" : "s"} indexed, searchable` : "none read yet"} · SHA-256 ${p.sha256}.`,
    `Cited by ${p.citers.length} page${p.citers.length === 1 ? "" : "s"}${passages ? `, ${passages} passage${passages === 1 ? "" : "s"}` : ""}${p.citers.length ? ":" : ". No wiki page cites it yet: start_run ingest has the wiki take it in."}`,
    ...p.citers.flatMap((c) => [
      `- ${c.title} (${c.path}): ${c.passages.length ? `${c.passages.length} passage${c.passages.length === 1 ? "" : "s"}` : "in its sources"}`,
      ...c.passages.slice(0, PASSAGES_SHOWN).map((x) => `  - “${passageText(x.context)}”${x.anchor ? ` at “${x.anchor}”` : ""}`),
    ]),
  ].join("\n");
}

/** Settings › Vault's Rebuild index: every file read again; the files aren't changed. */
/** Check for Updates…, as Settings › About (Windows) and the Brainstead menu (macOS) do. */
async function checkUpdates(): Promise<string> {
  await api.updateCheck();
  noted("checked for updates");
  return "Checking for a new version: Brainstead says in its window what it found, and offers any update there.";
}

async function rebuildIndex(): Promise<string> {
  const v = await api.vaultStatus();
  if (v.state === "indexing") return "The index is being built already; app_status says how it stands.";
  await api.rebuildIndex();
  noted("started rebuilding the index");
  return "Rebuilding the index: every file in the vault is read again, and the files aren't changed. app_status says how it stands.";
}

/** The user's tools that start Claude Code sessions: list them (and suggestions), add or remove one. */
async function automated(a: Args): Promise<string> {
  const action = str(a, "action") ?? "list";
  const list = await api.automatedList();
  if (action === "list") {
    const sugg = await api.automatedSuggest();
    const line = (t: (typeof list)[number]) =>
      `- ${t.label}${t.cwd_contains ? ` · folder contains ${t.cwd_contains}` : ""}${t.opening ? ` · first message starts with “${t.opening}”` : ""}`;
    return [
      list.length ? paged(a, list.map(line), ["tool listed", "tools listed"]) : "No tools listed.",
      ...(sugg.length
        ? [
            "Folders whose sessions in the last two weeks were nearly all one prompt and done, as a tool's (not listed):",
            ...sugg.map((g) => `- ${g.cwd} · ${g.count} single-prompt sessions of ${g.total}`),
          ]
        : []),
    ].join("\n");
  }
  const name = str(a, "name");
  if (!name) throw new Error("Give the tool's name.");
  if (action === "add") {
    const t = { label: name, cwd_contains: str(a, "folder_contains") ?? "", opening: str(a, "first_message_starts_with") ?? "" };
    if (!t.cwd_contains && !t.opening)
      throw new Error("Give folder_contains or first_message_starts_with: what its sessions have in common.");
    await api.automatedSave([...list.filter((x) => x.label !== name), t]);
    told(`listed ${name} as a tool that starts sessions`);
    return `Listed ${name}: its sessions count as automated in the daily summary.`;
  }
  if (action === "remove") {
    if (!list.some((x) => x.label === name))
      throw new Error(`No tool called ${name}. The tools: ${list.map((x) => x.label).join(", ") || "none"}.`);
    await api.automatedSave(list.filter((x) => x.label !== name));
    told(`took ${name} off the tools that start sessions`);
    return `Took ${name} off: its sessions count as the user's work again.`;
  }
  throw new Error("action is list, add or remove.");
}

/** Find tasks and projects' suggestions: list them, accept one (optionally reworded) or skip one. */
async function suggestions(a: Args): Promise<string> {
  const action = str(a, "action") ?? "list";
  const s = await api.findStatus();
  if (action === "list") {
    const r = s.run;
    const line = (g: FindSuggestion) =>
      g.kind === "task"
        ? `- task: ${g.text}${g.project ? ` · project ${g.project}` : ""}${g.waiting ? " · waiting for" : ""}${g.due ? ` · due ${g.due}` : ""} · from ${g.sources.map((x) => `${x.path} (“${x.quote}”)`).join(", ")}  (id ${g.id})`
        : `- project: ${g.text}${g.outcome ? ` · done looks like: ${g.outcome}` : ""}${g.tasks.length ? ` · first: ${g.tasks.join("; ")}` : ""} · from ${g.sources.map((x) => x.path).join(", ")}  (id ${g.id})`;
    return [
      r
        ? `Last look: ${r.status}${r.status === "running" ? ` (${r.done} of ${r.batches} parts)` : ""}, ${r.notes} notes read.`
        : "No look yet: start_run find_tasks starts one.",
      s.suggestions.length ? paged(a, s.suggestions.map(line), ["suggestion", "suggestions"]) : "No suggestions waiting.",
    ].join("\n");
  }
  const id = str(a, "id");
  if (!id) throw new Error("Which suggestion? Give its id from list_suggestions.");
  if (action === "accept") {
    const edit: Record<string, unknown> = {};
    if (str(a, "text")) edit.text = str(a, "text");
    if (str(a, "done_looks_like")) edit.outcome = str(a, "done_looks_like");
    const m = await api.findDecide(id, "accept", Object.keys(edit).length ? edit : null, a.unattended === true);
    // Unattended, one that fails a check is held for the user.
    if (m?.startsWith("Held ")) {
      told("held a suggested task or project for you in Changes");
      return `${m} Nothing is changed until they accept it.`;
    }
    // With Undo ⌘Z, as Found's toast offers it.
    told("accepted a suggested task or project");
    return `${(m ?? "Made it.").replace(/ Revert it in Changes\.$/, "")} ${keyLabel("⌘Z")} in the app undoes it (Undo, as Found's toast offers), and it's in Changes too.`;
  }
  if (action === "skip") {
    await api.findDecide(id, "skip", null);
    noted("skipped a suggestion");
    return "Skipped: it won't be suggested again.";
  }
  throw new Error("action is list, accept or skip.");
}

/** Doc check's register; with start_register, its Start the register, a new note through Changes. */
async function docCheckRegister(a: Args, r: McpRequest): Promise<unknown> {
  if (a.start_register !== true) {
    // The register as the screen lists it, each version's status in its words (In force, Draft, Superseded).
    const reg = await api.canonicalRegister();
    if (!reg.exists) return `There's no register yet (${REGISTER}): start_register starts it, as Start the register does.`;
    return [
      reg.entries.length ? `The canonical docs register (${REGISTER}); governing_document takes a key:` : `${REGISTER} has no rows yet.`,
      ...reg.entries.map(
        (e) =>
          `- ${e.key} · ${e.title} ${e.version} · ${REGISTER_STATUS[e.status]?.[0] ?? e.status} · ${e.path}${e.exists ? "" : " (not found)"}${e.aliases.length ? ` · also ${e.aliases.join(", ")}` : ""}`,
      ),
      ...(reg.problems.length ? ["Problems in the register:", ...reg.problems.map((p) => `- ${p}`)] : []),
    ].join("\n");
  }
  const there = (await api.filesList(null)).some((f) => f.path === REGISTER);
  if (there) throw new Error(`${REGISTER} is there already: doc_check without document reads it, and edit_page adds its rows.`);
  const o = await submitAll([sub(a, r, REGISTER, "new", "Start the canonical docs register", { op: "page", content: REGISTER_STUB })]);
  told(o.applied ? "started the canonical docs register" : "held the canonical docs register for you in Changes");
  return `${o.message} Add a row for each version of a governing document (edit_page, on ${REGISTER}).`;
}

/** Doc check's Check; with save, its Save as note: the findings as a new note, through Changes. */
async function docCheck(a: Args, r: McpRequest): Promise<unknown> {
  const document = str(a, "document") ?? "";
  const mode = a.mode === "earlier_feedback" ? "callouts" : "standard";
  // Against the version in force with no governing document named: the picker's first, as the screen starts with.
  let governing = str(a, "governing_document") ?? "";
  if (!governing && mode === "standard") {
    governing = governingDocs(await api.canonicalRegister())[0]?.key ?? "";
    if (!governing) throw new Error(`Add a governing document to the register (${REGISTER}) first, as the screen asks.`);
  }
  // As the screen keys it: the governing document only counts against the version in force.
  const key = mode === "callouts" ? "" : governing;
  // The kept result while neither document has changed, as the screen shows it, unless fresh asks again.
  const saved = a.fresh === true ? null : await api.docCheckSaved(document, key, mode).catch(() => null);
  const result = saved && !saved.stale ? saved.result : await api.docCheck(document, key, mode);
  // In the screen's words (Conflict, Not covered, In force…), as Copy findings gives them.
  const name = document.split("/").pop() ?? document;
  const kept =
    saved && !saved.stale
      ? `(The check from ${saved.at.slice(0, 16).replace("T", " ")}: neither document has changed since. fresh checks again.)\n\n`
      : "";
  // As the screen, which offers no Save as note for documents that aren't about the same thing.
  if (result.unrelated)
    return `${kept}${result.verdict}: ${result.summary} Nothing was checked${a.save === true ? ", so nothing was saved as a note" : ""}.`;
  const found = kept + findingsMarkdown(name, result);
  if (a.save !== true) return found;
  const path = findingsNote(name);
  const o = await submitAll([sub(a, r, path, "new", `Doc check of ${name}`, { op: "page", content: found })]);
  told(o.applied ? `saved the doc check of ${name} as a note` : `held the doc check of ${name} for you in Changes`);
  return `${found}\nSaved as ${path}: ${o.message}`;
}

/** compare_docs: Compare's Any two documents, A and B, with its kept result while neither changed. */
async function compareDocs(a: Args, r: McpRequest): Promise<unknown> {
  const da = str(a, "document_a") ?? "";
  const db = str(a, "document_b") ?? "";
  if (!da || !db) throw new Error("Give document_a and document_b, by path.");
  if (da === db) throw new Error("Give two different documents.");
  const mode = a.look_for === "what_changed" ? "changes" : "differences";
  const saved = a.fresh === true ? null : await api.docCheckSaved(da, db, mode).catch(() => null);
  const result = saved && !saved.stale ? saved.result : await api.docCheck(da, db, mode);
  const an = da.split("/").pop() ?? da;
  const bn = db.split("/").pop() ?? db;
  const kept =
    saved && !saved.stale
      ? `(The comparison from ${saved.at.slice(0, 16).replace("T", " ")}: neither has changed since. fresh compares again.)\n\n`
      : "";
  // As the screen, which offers no Save as note for documents that aren't about the same thing.
  if (result.unrelated)
    return `${kept}${result.verdict}: ${result.summary} Nothing was compared${a.save === true ? ", so nothing was saved as a note" : ""}.`;
  const found = kept + compareMarkdown(an, bn, mode, result);
  if (a.save !== true) return found;
  const path = compareNote(an, bn);
  const o = await submitAll([
    sub(a, r, path, "new", `Compare ${an} and ${bn}`, { op: "page", content: compareMarkdown(an, bn, mode, result) }),
  ]);
  told(o.applied ? `saved the comparison of ${an} and ${bn} as a note` : `held the comparison of ${an} and ${bn} for you in Changes`);
  return `${found}\nSaved as ${path}: ${o.message}`;
}

/** save_chat: Ask's Save for an open chat, or History's Save for a closed one; the open one in Ask
 *  when no chat is named. */
async function saveChat(a: Args): Promise<string> {
  const want = str(a, "chat");
  const s = ask.get();
  const named = (c: { filename: string; title: string }) => c.filename === want || c.title.toLowerCase() === want!.toLowerCase();
  const open = want ? s.chats.find(named) : s.chats.find((x) => x.id === s.active);
  if (open) {
    if (isSaved(open)) return `“${open.title}” is saved already, as ${open.filename}; it keeps up to date there.`;
    const f = await keepChat(open.id);
    told(`saved the chat “${open.title}”`);
    return `Saved “${open.title}” to the vault as ${f}; it keeps up to date there as the chat goes on.`;
  }
  if (!want) throw new Error("No chat is open in Ask: give chat, its file or title from list_chats.");
  const all = await api.chatsList();
  const c = all.find((x) => x.filename === want) ?? all.find(named);
  if (!c) throw new Error(`No chat “${want}”: list_chats gives them.`);
  if (isSaved(c)) return `“${c.title}” is saved already, as ${c.filename}.`;
  const saved = await api.chatSave(await api.chatRead(c.filename), true);
  told(`saved the chat “${c.title}”`);
  return `Saved “${c.title}” to the vault as ${saved.filename}.`;
}

/** Triage's suggestions, in its choices' names: Keep, Ingest into the wiki, Make a task or Archive
 *  from the model, and Remove for a bookmark whose note is gone, as the screen offers only that. */
async function triageSuggest(a: Args): Promise<string> {
  const items = (Array.isArray(a.items) ? a.items : []) as { target: string; path: string }[];
  const missing = new Set((await api.bookmarksStatus()).filter((b) => b.missing).map((b) => b.target));
  const asked = items.filter((i) => !missing.has(i.target));
  const got = asked.length ? await api.bookmarksSuggest(asked) : [];
  const lines = items.map((i) => {
    if (missing.has(i.target)) return `- ${i.target}: ${TRIAGE.remove[0]} — its note is gone.`;
    const g = got.find((x) => x.target === i.target);
    if (!g) return `- ${i.target}: no suggestion.`;
    const what =
      g.decision === "promote" && g.page
        ? ` (the ${g.kind ?? "concept"} page ${g.page})`
        : g.decision === "task" && g.task
          ? ` (“${g.task}”)`
          : "";
    return `- ${i.target}: ${TRIAGE[g.decision][0]}${what} — ${g.why}${g.summary ? ` ${g.summary}` : ""}`;
  });
  return [
    "Suggestions only, nothing changed. Keep is bookmarks keep; Archive and Remove take the bookmark off (bookmarks remove, by its target); Ingest into the wiki is edit_page making the page, and Make a task is capture with kind task (to the Inbox), each then taking the bookmark off with bookmarks remove, as the screen's buttons do.",
    ...lines,
  ].join("\n");
}

/** Draft a reply, as its screen shows the result: whether it answered the thread and each point,
 *  the draft, what's left to fill in, what it drew on, and its Add task. */
async function draftReply(a: Args): Promise<string> {
  // The screen's Tone: Brief (the default), Warm or Formal.
  const tone = str(a, "tone") ?? "brief";
  if (!["brief", "warm", "formal"].includes(tone)) throw new Error("tone is brief, warm or formal (brief when left out).");
  const thread = str(a, "thread") ?? null;
  // The screen's Ask for a change: the reply as it stands and what to change, revised as asked.
  const current = str(a, "current") ?? null;
  const change = str(a, "change") ?? null;
  if (!current !== !change) throw new Error("To revise a reply, give both current (the reply as it stands) and change (what to change).");
  const r = await api.draftReply(thread, str(a, "text") ?? null, tone, current, change);
  const said = ANSWERED[r.answered];
  const head = [said ? `${said[0]}${r.verdict ? `: ${r.verdict}` : ""}` : "", ...r.callouts.map((c) => `- ${c.status}: ${c.point}`)];
  const tail = [
    r.gaps.length ? `To fill in:\n${r.gaps.map((g) => `- ${g}`).join("\n")}` : "",
    r.grounded.length ? `Drew on ${r.grounded.join(", ")}.` : "",
    r.task ? `Its Add task, if the user wants it: capture with kind task and text “${replyTask(r.task, thread)}”.` : "",
  ].filter(Boolean);
  return [
    ...head.filter(Boolean),
    `Drafted reply (${tone[0].toUpperCase()}${tone.slice(1)}), not sent: give it to the user to paste into Outlook or Teams.`,
    "",
    r.draft,
    ...(tail.length ? ["", ...tail] : []),
  ].join("\n");
}

/** trash_note on a Teams transcript (the MCP server sends those here): its links pointed at its
 *  meeting note first, as every way of trashing one does (trashTranscript), each write recorded in
 *  Changes as the assistant's. */
async function trashTranscriptTool(a: Args, r: McpRequest): Promise<string> {
  const page = str(a, "page");
  if (!page || !isTranscriptPath(page)) throw new Error("Which transcript? Give its path in sources/.");
  const { outcome } = await trashTranscript(page, undefined, { origin: originOf(a, r), reason: str(a, "reason"), model: r.model });
  if (outcome?.applied) told(`moved ${pageName(page)} to the Trash`);
  else noted(`held moving ${pageName(page)} to the Trash for you in Changes`);
  return outcome?.message ?? `Moved ${page} to the Trash.`;
}

/** add_image: the image saved to images/ as a paste is, and recorded in Changes. */
async function addImageTool(a: Args, r: McpRequest): Promise<string> {
  const name = str(a, "name");
  const data = str(a, "base64");
  if (!name || !data) throw new Error("Give the image's name and bytes.");
  const o = await api.imageAdd(name, data, str(a, "reason") ?? null, originOf(a, r), r.model ?? null);
  told(`added ${pageName(o.page)}`);
  return o.message;
}

/** Each action by name. */
const ACTIONS: Record<string, (a: Args, r: McpRequest) => Promise<unknown>> = {
  "tasks.list": listTasks,
  "task.edit": editTask,
  "task.move": moveTask,
  "inbox.list": (a) => listInbox(a),
  "inbox.clarify": clarify,
  "inbox.capture": captureTool,
  "projects.list": async (a) =>
    listed(a, orderedProjects(await api.projectsList()), {
      text: (p) => projectOut(p),
      noun: ["project", "projects"],
      line: (p) => projectOut(p, a.detail === true),
      item: (p) => ({
        name: p.name,
        path: p.path,
        status: statusOut(p.status),
        area: p.area,
        done_looks_like: p.outcome,
        flag: flagOut(p),
        next: p.next,
        waiting: p.waiting,
        someday: p.someday,
        done: p.done,
      }),
      empty: "No projects.",
    }),
  "project.create": createProject,
  "project.update": updateProject,
  "project.rename_area": renameArea,
  "project.move": moveProject,
  "note.from_template": (a, r) => noteFromTemplate(a, r.chat ?? null),
  "change.submit": submitChange,
  changes: changesTool,
  "run.start": startRun,
  "note.trash_transcript": trashTranscriptTool,
  "image.add": addImageTool,
  "run.status": runStatus,
  "run.stop": stopRun,
  "health.fix": healthFix,
  "health.ignore": healthIgnore,
  "health.reshape": reshapePages,
  "health.current_state": writeCurrentState,
  "health.lint": healthLint,
  "health.issue": healthIssue,
  contradictions: contradictionsTool,
  "weekly.step": weeklyStep,
  chats: chatsTool,
  task_lists: taskLists,
  settings: settingsTool,
  "sources.import": importSources,
  open: openApp,
  fix_name: fixName,
  trash: trash,
  bookmarks: bookmarks,
  saved_searches: savedSearches,
  activity: activity,
  graph: graph,
  status: status,
  "index.rebuild": rebuildIndex,
  "updates.check": checkUpdates,
  "assistants.found": assistantsFound,
  "source.provenance": provenance,
  "capture.status": captureExtensions,
  sync: syncTool,
  "sync.pull": syncPull,
  glance: glanceTool,
  automated: automated,
  suggestions: suggestions,
  "triage.suggest": triageSuggest,
  "reply.draft": draftReply,
  "doccheck.register": docCheckRegister,
  "doccheck.run": docCheck,
  "compare.run": compareDocs,
  "weekly.status": weeklyStatus,
  "weekly.suggestion": weeklySuggestion,
  "weekly.start_over": weeklyStartOver,
  moving_over: movingOver,
  note_look: noteLook,
  "chat.save": saveChat,
  "meeting.transcripts": async (a) => {
    // The screen's Show: To do (not yet ingested or linked from a note), or All.
    const show = str(a, "show") ?? "to_do";
    if (show !== "to_do" && show !== "all") throw new Error("show is to_do or all, as the screen's Show is (to_do when left out).");
    const [all, ran] = await Promise.all([api.meetingTranscripts(), api.ingestRuns()]);
    const rows = show === "all" ? all : all.filter((t) => !t.done);
    // As the screen marks them: a note there already, one being drafted or drafted, and its questions.
    const meetings = ran.filter((r) => r.kind === "meeting");
    const drafting = (t: Transcript) => meetings.some((r) => r.source === t.path && (r.status === "running" || r.status === "queued"));
    const drafted = (t: Transcript) => meetings.some((r) => r.source === t.path && r.status === "done");
    const exists = (t: Transcript) => (t.inferred.exists ? noteFile(specOf(t)) : null);
    const line = (t: Transcript) =>
      `- ${t.path} · ${t.done ? `done: ${MEETING_DONE[t.done]}` : `looks like ${t.inferred.type ?? "a meeting"} ${t.inferred.name ?? t.inferred.topic ?? ""} ${t.inferred.date ?? ""}${t.inferred.dateCheck ? " (the capture day: check the meeting date)" : ""}`}${
        exists(t)
          ? ` · note exists: ${exists(t)}${t.noteIngested ? " (ingested already: only the transcript is left, trash_note it if the user agrees)" : " (start_run meeting_note with ingest_existing ingests it as it is)"}`
          : ""
      }${drafting(t) ? " · being drafted" : drafted(t) ? " · a note was drafted (see Changes)" : ""}${(t.inferred.ask ?? []).map((q) => `\n  - ${q}`).join("")}`;
    return listed(a, rows, {
      text: line,
      noun: show === "all" ? ["transcript", "transcripts"] : ["transcript to write up", "transcripts to write up"],
      line,
      item: (t) => ({
        path: t.path,
        type: t.inferred.type ?? null,
        name: t.inferred.name ?? t.inferred.topic ?? null,
        date: t.inferred.date ?? null,
        dateCheck: !!t.inferred.dateCheck,
        done: t.done ?? null,
        exists: exists(t),
        ask: t.inferred.ask ?? [],
        drafted: drafting(t) ? "drafting" : drafted(t) ? "drafted" : null,
      }),
      empty: show === "all" ? "No transcripts in Sources." : "No transcripts still to write up.",
    });
  },
};

/** Runs one request; what it gives back goes to the server as is (a string is shown as text). */
export async function runAction(r: McpRequest): Promise<unknown> {
  const f = ACTIONS[r.action];
  if (!f) throw new Error(`Brainstead doesn't know the action ${r.action}.`);
  return f(r.args ?? {}, r);
}

/** Why it failed, as one line for the assistant. */
function why(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

/** Listens for requests from the bridge once, at startup, and says it's ready. */
export async function startMcpBridge() {
  const off = await listen<McpRequest>("mcp-request", (ev) => {
    const r = ev.payload;
    runAction(r)
      .then((result) => invoke("mcp_reply", { id: r.id, ok: true, result: result ?? "Done." }))
      .catch((e) => invoke("mcp_reply", { id: r.id, ok: false, result: why(e) }));
  });
  await invoke("mcp_ready").catch(() => {});
  return off;
}

// ---- Knowledge health, as its screen shows it

/** Each missing link's In the Trash and Did you mean, as Knowledge health's rows show them (ghostHelps). */
async function ghostsOf(r: LintReport): Promise<Map<string, GhostHelp>> {
  if (!r.checks.some((c) => (c.id === "missing-pages" || c.id === "broken-sources") && c.items.length)) return new Map();
  const [files, trash] = await Promise.all([api.filesList(null), api.trashList()]);
  return ghostHelps(r, ghostPaths(files), trashByName(trash));
}

/** A missing link's help in a line, as its row shows it: In the Trash since…, Did you mean…? */
function ghostNote(g: GhostHelp | undefined): string {
  if (!g) return "";
  const trashed = g.inTrash
    ? ` (in the Trash since ${g.inTrash.deletedAt.slice(0, 10)}: restore_from_trash with id ${g.inTrash.id} puts ${g.inTrash.originalRel} back)`
    : "";
  const near = g.near ? ` Did you mean ${pageName(g.near)}? (health_issue link_to with to: ${g.near})` : "";
  return `${trashed}${near}`;
}

/** lint, while the app is open: the screen's report, ignored issues and dismissed pairs left out. */
async function healthLint(a: Args): Promise<string> {
  const view = await api.healthReport(true);
  const r = view.report;
  if (!r) throw new Error("Knowledge health has no report yet; try again in a moment.");
  const page = str(a, "page")?.trim();
  // The screen's summary card: Need a decision, what's safe to fix, and the trend's ends.
  const need = decisions(r);
  const safe = safeFixes(r).length;
  const trend = trendShown(view.history)
    ? (({ from, to, tip }) => ` Trend: ${from} → ${to} (${tip.replace(/^Issues needing a decision each day, /, "")})`)(
        trendLabels(view.history),
      )
    : view.history.length < 2
      ? " The trend shows after a second day."
      : "";
  const summary = page ? "" : `Need a decision: ${need}; ${safe ? `${safe} more are safe to fix` : "nothing safe to fix"}.${trend}`;
  const out: string[] = [];
  const quiet: string[] = [];
  const ghosts = await ghostsOf(r);
  for (const c of r.checks) {
    const items = page
      ? c.items.filter(
          (i) => i.page === page || i.pages?.includes(page) || (i.page && pageName(i.page).toLowerCase() === page.toLowerCase()),
        )
      : c.items;
    if (c.ignored && !page) quiet.push(`${c.title} ${c.ignored}`);
    if (!items.length) continue;
    const counted = ADVISORY.has(c.id) ? ", not counted: only worth a look" : "";
    out.push("", `${c.title} (${items.length}${counted}; check ${c.id}):`);
    for (const i of items.slice(0, 40))
      out.push(`- ${i.text}${i.safe && c.id !== "page-shape" ? " [safe fix]" : ""}${ghostNote(ghosts.get(`${c.id}:${i.text}`))}`);
    if (items.length > 40) out.push(`- …and ${items.length - 40} more`);
  }
  const ignored = quiet.length ? `Ignored, so not listed: ${quiet.join(", ")}. ignore_issue with show_again lists a check's again.` : "";
  if (!out.length) return ["No issues.", summary, ignored].filter(Boolean).join(" ");
  return [`Knowledge health${page ? ` for ${page}` : ""}:`, ...(summary ? [summary] : []), ...out, ...(ignored ? ["", ignored] : [])].join(
    "\n",
  );
}

/** health_issue: the buttons an issue has of its own on Knowledge health. */
async function healthIssue(a: Args): Promise<string> {
  const r = (await api.healthReport(true)).report;
  if (!r) throw new Error("Knowledge health has no report yet; try again in a moment.");
  const action = str(a, "action");
  const text = str(a, "item") ?? "";
  // Orphan pages' Add them to index.md: the whole check at once, no item.
  if (action === "add_to_index") {
    const said = await api.catalogueUpdate();
    told("brought index.md's catalogue up to date");
    return `${said.charAt(0).toUpperCase()}${said.slice(1)}.`;
  }
  const want = {
    create: "missing-pages",
    link_to: str(a, "item")?.includes(" not found") ? "broken-sources" : "missing-pages",
    not_duplicates: "duplicates",
    move_to_trash: "unreferenced-images",
  }[action ?? ""];
  if (!want) throw new Error("action is create, link_to, not_duplicates, move_to_trash or add_to_index.");
  const c = r.checks.find((x) => x.id === want);
  const i = c?.items.find((x) => x.text === text);
  if (!c || !i) return `That isn't an issue under ${c?.title ?? want}: give its text as lint gives it.`;
  if (action === "create") {
    // A file in the Trash isn't a page to write: the screen offers Restore or Link to it instead of Create.
    const g = (await ghostsOf(r)).get(`${c.id}:${i.text}`);
    if (g?.inTrash)
      return `${g.inTrash.originalRel} has been in the Trash since ${g.inTrash.deletedAt.slice(0, 10)}, so there's no Create: restore_from_trash with id ${g.inTrash.id} puts it back${g.near ? `, or health_issue link_to with to: ${g.near} points the links at the note it most likely became` : ", or health_issue link_to points the links at another page"}.`;
    // As the screen's Create: an entity page (a concept page is edit_page's, in wiki/concepts/).
    const p = await api.healthCreatePage(i.name!, "entities");
    told(`made the page ${pageName(p)}`);
    return `Made ${p}. ${keyLabel("⌘Z")} in the app undoes it; edit_page fills it in.`;
  }
  if (action === "link_to") {
    const to = str(a, "to")?.trim();
    if (!to) throw new Error("Give to: the page the links should point at.");
    const [target] = await api.linksResolve([to]);
    if (!target) return `There's no page called ${to}.`;
    // A broken source is on its one page; a missing page's links are on every page using it.
    const pages = c.id === "broken-sources" && i.page ? [i.page] : (i.pages ?? []);
    const n = await api.healthLinkGhost(i.name!, target, pages, a.unattended === true);
    noted(`pointed the links to ${i.name} at ${pageName(target)}`);
    return `Pointed the links to ${i.name} at ${pageName(target)} in ${n} page${n === 1 ? "" : "s"}, each a change in Changes, where the user can revert it.`;
  }
  if (action === "not_duplicates") {
    if (!i.page || !i.pages?.[0]) return "That pair can't be marked.";
    await api.healthDismiss(i.page, i.pages[0]);
    noted("marked two pages as not duplicates");
    return `${pageName(i.page)} and ${pageName(i.pages[0])} won't be listed as possible duplicates again.`;
  }
  if (!i.page) return "That image can't be found.";
  await api.healthTrashImage(i.page);
  told(`moved ${i.page} to the Trash`);
  return `Moved ${i.page} to the Trash; restore_from_trash restores it.`;
}

/** contradictions: the Contradictions screen's findings, its Mark resolved and Ignore, and Save report as note. */
async function contradictionsTool(a: Args, r: McpRequest): Promise<string> {
  const action = str(a, "action") ?? "list";
  const rep = await api.contradictionsReport();
  if (action === "save") {
    // As the screen offers Save report as note: a finished check, and none running.
    if (rep.last.running)
      throw new Error("A contradictions check is running: save its report once it's finished (run_status shows how it's going).");
    if (!rep.last.finished) return "There's no finished check to save; start_run contradictions runs one.";
    const today = localToday();
    const base = `Contradictions - ${today}`;
    for (let n = 1; n < 20; n++) {
      const path = `${n === 1 ? base : `${base}-${n}`}.md`;
      const c = sub(a, r, path, "new", `Contradictions report ${today}`, { op: "page", content: reportMarkdown(rep, today) });
      try {
        return (await submitAll([c])).message;
      } catch (e) {
        if (!/already/i.test(String((e as { message?: string })?.message ?? e))) throw e;
      }
    }
    throw new Error("Couldn't find a free name for the report.");
  }
  if (action === "mark") {
    const id = str(a, "id");
    const as = str(a, "as");
    if (!id || !rep.items.some((i) => i.id === id)) throw new Error("Give id: a finding's id from list_contradictions.");
    if (as !== "resolved" && as !== "ignored") throw new Error("as is resolved (Mark resolved) or ignored (Ignore).");
    await api.contradictionsMark(id, as);
    noted(`marked a contradiction ${as}`);
    return as === "resolved" ? "Marked resolved." : "Ignored: it won't be flagged again.";
  }
  if (action !== "list") throw new Error("action is list, mark or save.");
  const l = rep.last;
  const head = [
    l.finished
      ? `Checked ${l.finished.slice(0, 16).replace("T", " ")}: ${l.pages} pages, ${l.claims} claims, ${l.clashes} clashes, ${l.contradictions} contradictions.`
      : l.running
        ? `A check is running: ${l.doing || "checking"}.`
        : "Not checked yet: start_run contradictions runs the check.",
  ];
  // The screen's banners: why the last check failed, and the fixes it couldn't make.
  if (l.error) head.push(` It failed: ${l.error}`);
  if (l.failed) head.push(` ${l.failed === 1 ? "1 fix" : `${l.failed} fixes`} not made: ${l.failures.join("; ")}.`);
  // Every finding, in the screen's order (real ones by severity, then unclear, then the settled);
  // the ones to decide are a real contradiction or an unclear one, not yet resolved or ignored.
  const settled = new Set(["compatible", "evolution", "resolved", "ignored"]);
  const shown = contradictionsOrder(rep.items);
  const toDecide = shown.filter((i) => !i.verdict || !settled.has(i.verdict.verdict)).length;
  const left = rep.items.length - toDecide;
  const page = pagedRows(
    a,
    shown,
    (i) => [i.subject, i.attribute, ...i.claims.map((c) => `${c.page} ${c.value}`)].join(" "),
    ["finding", "findings"],
    20,
  );
  if (rep.items.length) {
    head.push(` ${toDecide} to decide${left ? `; ${left} not a conflict, newer superseding older, resolved or ignored` : ""}.`);
    if (shown.length) head.push(` ${page.head}.`);
  }
  // The judge's fix, and its change in Changes (the check makes or holds it, as the screen's The fix
  // is in Changes says): found by the finding's id, which the change's origin carries.
  const changes = page.shown.some((i) => i.verdict?.patch) ? await api.changesList().catch(() => [] as ChangeRow[]) : [];
  const fixOut = (i: (typeof rep.items)[number]) => {
    if (!i.verdict?.fix) return [];
    const c = changes.find((x) => x.origin.kind === "contradiction" && x.origin.chat === i.id);
    const where = !c
      ? ""
      : c.status === "held"
        ? ` · waiting in Changes for the user (change ${c.id})`
        : c.status === "applied"
          ? ` · made, in Changes (change ${c.id}; changes revert undoes it)`
          : ` · its change ${c.id} was ${c.status}`;
    return [`  - The judge's fix: ${i.verdict.fix}${i.verdict.correct ? ` (right: ${pageName(i.verdict.correct)})` : ""}${where}`];
  };
  // Each finding's first ten claims; its page in Brainstead has them all.
  const CLAIMS = 10;
  const rows = page.shown.map((i) =>
    [
      `- ${i.subject} · ${i.attribute} (id ${i.id}): ${i.verdict ? `${i.verdict.verdict}${i.verdict.severity ? `, ${i.verdict.severity}` : ""}. ${i.verdict.summary}` : "not judged yet"}`,
      ...fixOut(i),
      ...i.claims
        .slice(0, CLAIMS)
        .map((c) => `  - [[${pageName(c.page)}]]${c.asOf ? ` (as of ${c.asOf})` : ""}: ${c.value}${c.quote ? ` · “${c.quote}”` : ""}`),
      ...(i.claims.length > CLAIMS ? [`  - …and ${i.claims.length - CLAIMS} more claims`] : []),
    ].join("\n"),
  );
  return [head.join(""), ...rows].join("\n");
}

// ---- the weekly review, done

/** weekly_step: Start or Carry on, Next, the notes, and Finish and save. */
async function weeklyStep(a: Args): Promise<string> {
  const action = str(a, "action");
  const { week, st } = await weeklyNow();
  const save = async (s: WeeklyState | null) => {
    await api.weeklyStateWrite(s);
    window.dispatchEvent(new Event(WEEKLY_STATE_CHANGED));
  };
  if (action === "start") {
    if (st) return `The review of ${week} carries on at step ${st.step + 1} of ${STEPS.length}, ${STEPS[st.step].title}.`;
    await save(fresh(week));
    void api.weekprepEnsure(week).catch(() => false);
    noted(`started the weekly review for ${week}`);
    return `Started the weekly review for ${week}: step 1 of ${STEPS.length}, ${STEPS[0].title}. ${STEPS[0].ask}`;
  }
  if (!st) throw new Error("The weekly review isn't started: action start starts it.");
  if (action === "done") {
    const to = typeof a.step === "number" ? Math.floor(a.step) - 1 : st.step + 1;
    const i = Math.max(0, Math.min(STEPS.length - 1, to));
    await save({ ...st, step: i, done: [...new Set([...st.done, st.step])] });
    noted(`went on to ${STEPS[i].title} in the weekly review`);
    return `${STEPS[st.step].title} is done. Now step ${i + 1} of ${STEPS.length}, ${STEPS[i].title}: ${STEPS[i].ask}`;
  }
  if (action === "notes") {
    const line = str(a, "log")?.trim();
    const text = str(a, "text");
    if (!line && text === undefined) throw new Error("Give text (the notes in full) or log (a line to add).");
    await save(line ? { ...st, log: [...st.log, line] } : { ...st, notes: text ?? "" });
    noted("wrote in the weekly review");
    return line ? "Added to the review's decisions." : "The review's notes are set.";
  }
  if (action === "finish") {
    const file = await api.weeklyFinish(week, reviewBody({ ...st, done: [...new Set([...st.done, st.step])] }));
    await save(null);
    told(`saved the weekly review in ${file}`);
    return `Saved the review of ${week} in ${file}. ${keyLabel("⌘Z")} in the app undoes it.`;
  }
  throw new Error("action is start, done, notes or finish.");
}

// ---- Ask's chats

async function chatsTool(a: Args): Promise<string | Listing> {
  const action = str(a, "action") ?? "list";
  const all = await api.chatsList();
  const saved = (c: ChatSummary) => !c.filename.startsWith("local:");
  if (action === "list") {
    const line = (c: ChatSummary) =>
      `- ${c.title} · ${c.updatedAt.slice(0, 16).replace("T", " ")} · ${saved(c) ? `saved as ${c.filename}` : "not saved"}${c.state === "pinned" ? " · pinned" : ""}  (${c.filename})`;
    return listed(
      a,
      [...all].sort((x, y) => y.updatedAt.localeCompare(x.updatedAt)),
      {
        text: line,
        noun: ["chat, newest first", "chats, newest first"],
        line,
        item: (c) => ({ file: c.filename, title: c.title, updated: c.updatedAt, saved: saved(c), pinned: c.state === "pinned" }),
        empty: "No chats.",
      },
    );
  }
  const want = str(a, "chat")?.trim();
  if (!want) throw new Error("Give chat: its file or title, from list_chats.");
  const c = all.find((x) => x.filename === want) ?? all.find((x) => x.title.toLowerCase() === want.toLowerCase());
  if (!c) throw new Error(`No chat “${want}”: list_chats gives them.`);
  if (action === "read") {
    const full = await api.chatRead(c.filename);
    const turns = full.transcript.filter((t) => t.role === "user" || t.role === "assistant");
    return {
      text: [
        `${c.title} (${turns.length} messages):`,
        ...turns.map((t) => `\n${t.role === "user" ? "User" : "Assistant"}: ${t.text}`),
      ].join("\n"),
      structured: { chat: { file: c.filename, title: c.title, messages: turns.map((t) => ({ role: t.role, text: t.text })) } },
    };
  }
  if (action === "rename") {
    const title = str(a, "title")?.trim();
    if (!title) throw new Error("Give title: the chat's new title.");
    await renameSaved(c, title);
    noted(`renamed the chat “${c.title}” to “${title}”`);
    return `Renamed “${c.title}” to “${title}”.`;
  }
  if (action === "pin" || action === "unpin") {
    const pin = action === "pin";
    if ((c.state === "pinned") === pin) return `“${c.title}” is ${pin ? "pinned" : "not pinned"} already.`;
    await pinSaved(c, pin);
    noted(`${pin ? "pinned" : "unpinned"} the chat “${c.title}”`);
    return pin
      ? `Pinned “${c.title}”: it's kept whatever happens.`
      : `Unpinned “${c.title}”${saved(c) ? "" : "; as it isn't saved, it's deleted once it's closed and 20 newer chats are in History"}.`;
  }
  if (action === "trash") {
    await trashSaved(c);
    noted(`moved the chat “${c.title}” to the Trash`);
    return `Moved “${c.title}” to the Trash.`;
  }
  throw new Error("action is list, read, rename, pin, unpin or trash.");
}

// ---- Tasks' saved lists

async function taskLists(a: Args): Promise<string> {
  const action = str(a, "action") ?? "list";
  const saved = settings.get().taskLists ?? [];
  const effortName = (e: string) => EFFORT_LIMITS.find(([v]) => v === e)?.[1].toLowerCase() ?? "";
  if (action === "list")
    return saved.length
      ? paged(
          a,
          saved.map(
            (l) =>
              `- ${l.name}: ${viewName(l.view)}${l.context ? `, @${l.context}` : ""}${l.effort ? `, ${effortName(l.effort)}` : ""}${l.group !== "none" ? `, by ${l.group}` : ""}`,
          ),
          ["saved list", "saved lists"],
        )
      : "No saved lists.";
  const name = str(a, "name")?.trim();
  if (!name) throw new Error("Give name: the list's name.");
  if (action === "remove") {
    if (!saved.some((l) => l.name === name)) return `There's no saved list “${name}”.`;
    settings.update({ taskLists: saved.filter((l) => l.name !== name) });
    await settings.flush();
    noted(`removed the saved list ${name}`);
    return `Removed the saved list “${name}”.`;
  }
  if (action !== "save") throw new Error("action is list, save or remove.");
  const asked = str(a, "view") ?? "next";
  const view = asked === "deferred" ? "scheduled" : asked;
  // Only the Tasks screen's own lists can be saved, as Save this list… does there (today and all aren't among them).
  if (!VIEWS.some((v) => v.id === view))
    throw new Error(`No view “${asked}” to save. Views: ${VIEWS.map((v) => viewName(v.id)).join(", ")}.`);
  const effort = a.effort == null ? "" : String(a.effort);
  if (effort && !EFFORT_LIMITS.some(([v]) => v === effort)) throw new Error("effort is 15, 30 or 60 (minutes), or left out.");
  const group = str(a, "group") ?? "none";
  if (!["none", "project", "context", "due"].includes(group)) throw new Error("group is none, project, context or due.");
  // none is no context: any context, as the list with none chosen.
  const ctx = str(a, "context");
  const l = { name, view, context: ctx && ctx.toLowerCase() !== "none" ? contextName(ctx) : "", effort, group };
  settings.update({ taskLists: [...saved.filter((x) => x.name !== name), l] });
  await settings.flush();
  noted(`saved the task list ${name}`);
  return `Saved the list “${name}”; it shows under Tasks' saved lists.`;
}

// ---- Settings

type SettingKind = "bool" | "time" | "text" | "number" | "day" | "theme" | "choice" | "model" | "speed" | "size" | "percent" | "shortcut";
/** A choice setting's choices, as its menu offers them: each value and how the screen names it. */
type Choices = () => Promise<[string, string][]> | [string, string][];
/** What the settings tool can change, by key: its pane, label, kind and default, a choice's
 *  choices, and the name of its default choice when it has one (set by null: As macOS…, As the system off a Mac). The vault,
 *  Read-only and the excluded folders stay the user's (D-20261006-20); a note's own look is
 *  note_look's (settings.docLooks, by path). */
type SettingRow = [key: string, pane: string, label: string, kind: SettingKind, def: unknown, choices?: Choices, none?: string];

/** The models the assistants found offer, as Ask's model menu lists them. */
async function modelChoices(): Promise<[string, string][]> {
  // Looked up again when none were found, as Ask does when it opens: one may be installed since.
  await findClis(!clis.get()?.length);
  const found = clis.get() ?? [];
  return found.filter((c) => c.path).flatMap((c) => c.models.map((m) => [m.id, modelLabel(m.id, found)] as [string, string]));
}

/** Settings as list_settings shows them; a function, so the platform's defaults are the ones in force. */
const settingRows = (): SettingRow[] => [
  ["ownerName", "General", "Your name", "text", ""],
  ["theme", "General", "Appearance (system, light or dark)", "theme", "system"],
  ["interfaceSize", "General", "Appearance › Interface size (80 to 150, %)", "percent", INTERFACE_SIZE.default],
  ["openAtLogin", "General", "Open at login", "bool", false],
  ["menuBar", "General", "Show in the menu bar", "bool", true],
  ["menuBarOnly", "General", `Only in the menu bar when the window is closed${isMac ? "" : " (Mac only; no effect here)"}`, "bool", false],
  ["offerGuide", "General", "Offer Getting started when Brainstead opens", "bool", true],
  ["captureShortcut", "General", `Quick capture shortcut (as ${captureDefault()})`, "shortcut", captureDefault()],
  ["spellCheck", "Notes", "Check spelling", "bool", true],
  ["grammarCheck", "Notes", "Check grammar", "bool", true],
  [
    "spellLanguage",
    "Notes",
    "Spelling language",
    "choice",
    null,
    // By name, as its menu lists them (English (South Africa)), the code (en_ZA) taken too.
    async () => (await api.spellLanguages())[0].map((l) => [l, languageName(l)]),
    isMac ? "As macOS" : "As the system",
  ],
  [
    "speechVoice",
    "Notes",
    "Read aloud › Voice",
    "choice",
    null,
    async () => {
      if (!voices.get().length) await loadVoices();
      return voices.get().map((v) => [v.id, voiceLabel(v)]);
    },
    "System voice",
  ],
  ["speechRate", "Notes", "Read aloud › Speed (0.5 to 2)", "speed", 1],
  // Not in Settings: a note's top bar has it, for every note.
  ["readSize", "", "A note's Text size (12 to 24, px)", "size", READ_SIZE.default],
  ["speechHighlight", "Notes", "Read aloud › Highlight each word", "bool", true],
  ["docStyle", "Notes", "Document look › Theme", "choice", "brainstead", () => DOC_STYLES.map(([id, name]) => [id, name])],
  ["docAccent", "Notes", "Document look › Colour", "choice", "brainstead", () => DOC_ACCENTS.map(([id, name]) => [id, name])],
  [
    "docTheme",
    "Notes",
    "Light or dark › Documents",
    "choice",
    null,
    () => [
      ["light", "Light"],
      ["dark", "Dark"],
    ],
    "As the app",
  ],
  ["templateScripts", "Notes", "User scripts folder", "text", "Templates/scripts"],
  ["askModel", "AI assistants", "New chats use", "model", DEFAULT_MODEL, modelChoices],
  ...MODEL_JOBS.map(([job, label]): SettingRow => [
    `jobModels.${job}`,
    "AI assistants",
    `Models by job › ${label}`,
    "model",
    null,
    modelChoices,
    job === "weekprep" ? "As the summaries, else as new chats" : "As new chats",
  ]),
  ["askSuggest", "AI assistants", "Suggest a next message", "bool", true],
  ["meetingIngest", "AI assistants", "Ingest a meeting note once it's made", "bool", true],
  ["meetingTrash", "AI assistants", "Then move the transcript to the Trash", "bool", true],
  ["ingestOnArrival", "AI assistants", "Ingest new sources as they arrive", "bool", false],
  ["syncEnabled", "Teams and Outlook sync", "Sync Teams and Outlook", "bool", false],
  ["syncInitialDays", "Teams and Outlook sync", "Days of history to load on first sync (0 = no limit)", "number", 7],
  ["syncMailAttachments", "Teams and Outlook sync", "Bring in email attachments", "bool", true],
  ["refreshStale", "AI assistants", "Also refresh pages whose sources changed", "bool", false],
  ["summariesHere", "Jobs & schedule", "Brainstead runs the daily and weekly summaries", "bool", false],
  ["holdSummaries", "Jobs & schedule", "Hold the summaries for me", "bool", false],
  ["summaries.dailyEnabled", "Jobs & schedule", "Daily summary", "bool", false],
  ["summaries.dailyTime", "Jobs & schedule", "Daily summary time", "time", null],
  ["summaries.weeklyEnabled", "Jobs & schedule", "Weekly summary", "bool", false],
  ["summaries.weeklyDay", "Jobs & schedule", "Weekly summary day", "day", null],
  ["summaries.weeklyTime", "Jobs & schedule", "Weekly summary time", "time", null],
  ["weeklyReview.day", "Jobs & schedule", "Weekly review day", "day", null],
  ["weeklyReview.time", "Jobs & schedule", "Weekly review time", "time", null],
  ["weekprepEnabled", "Jobs & schedule", "Prepare the weekly review", "bool", true],
  ["dailyCheckEnabled", "Jobs & schedule", "Daily check", "bool", false],
  ["dailyCheckTime", "Jobs & schedule", "Daily check time", "time", "09:00"],
  ["logDays", "About", "The app's logs › Keep (days: 7, 14, 30 or 90)", "number", 14],
];
const DAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/** The keys a shortcut can end with, as a key press gives them (Settings' shortcutFromEvent). */
const SHORTCUT_KEY =
  /^([A-Z0-9]|F([1-9]|1\d|2[0-4])|Space|Enter|Tab|Backspace|Delete|Escape|Home|End|PageUp|PageDown|Insert|Arrow(Up|Down|Left|Right)|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Backquote|Comma|Period|Slash)$/;
const SHORTCUT_MODS = ["Control", "Alt", "Shift", "Super"];

/** A shortcut as the screen records one: one or more of Control, Alt, Shift and Super, in that
 *  order, then a key ("Control+Alt+Space"). */
function shortcutSpec(spec: string): string {
  const parts = spec
    .split("+")
    .map((p) => p.trim())
    .filter(Boolean);
  const key = parts.pop() ?? "";
  const mods = parts.map((p) => SHORTCUT_MODS.find((m) => m.toLowerCase() === p.toLowerCase()) ?? p);
  const k = key.length === 1 ? key.toUpperCase() : key;
  if (!mods.length || mods.some((m) => !SHORTCUT_MODS.includes(m)) || new Set(mods).size < mods.length || !SHORTCUT_KEY.test(k))
    throw new Error(
      "captureShortcut is one or more of Control, Alt, Shift and Super, then a key, joined by +: Control+Alt+Space, Super+Shift+K, Control+F5.",
    );
  return [...SHORTCUT_MODS.filter((m) => mods.includes(m)), k].join("+");
}

async function settingsTool(a: Args): Promise<string> {
  const s = settings.get();
  /** The settings kept as objects, with their defaults filled in. */
  const nested: Record<string, Record<string, unknown>> = {
    summaries: { ...DEFAULT_SCHEDULE, ...s.summaries },
    weeklyReview: { ...DEFAULT_WEEKLY_REVIEW, ...s.weeklyReview },
    jobModels: { ...s.jobModels },
  };
  // Open at login is macOS's login item, not a setting in the file; null where macOS can't say
  // (a copy run from a folder), and then it isn't offered, as the screen hides it.
  const login = (await api.loginItem().catch(() => null)) ?? null;
  const offered = settingRows().filter(([k]) => k !== "openAtLogin" || login !== null);
  const valueOf = (key: string, def: unknown) => {
    if (key === "openAtLogin") return login;
    const [head, sub] = key.split(".");
    const v = sub ? nested[head][sub] : (s as unknown as Record<string, unknown>)[key];
    return v ?? def;
  };
  const shown = (v: unknown, none?: string) => (v == null && none ? `null (${none})` : JSON.stringify(v));
  const where = (pane: string, label: string) => (pane ? `Settings › ${pane} › ${label}` : label);
  const action = str(a, "action") ?? "get";
  if (action === "get") {
    // A choice's choices once: the models only under New chats use, which every job's model takes.
    const lines = await Promise.all(
      offered.map(async ([k, pane, label, kind, def, choices, none]) => {
        let extra = "";
        if (kind === "model" && k !== "askModel") extra = ` (a model, as askModel's; null for ${none})`;
        else if (choices) {
          const cs = await Promise.resolve(choices()).catch(() => [] as [string, string][]);
          const list = cs.map(([v, l]) => (v === l ? v : `${v} (${l})`)).join(", ");
          extra = ` (one of: ${list || "none here"}${none ? `; null for ${none}` : ""})`;
        }
        return `- ${k} · ${where(pane, label)}: ${shown(valueOf(k, def), none)}${extra}`;
      }),
    );
    return paged(a, lines, ["setting", "settings"], 100);
  }
  if (action !== "set") throw new Error("action is get or set.");
  const key = str(a, "key");
  const row = offered.find(([k]) => k === key);
  if (!row)
    throw new Error(
      key === "openAtLogin"
        ? `Open at login can't be changed for this copy of Brainstead (${isMac ? "macOS" : "the system"} manages it only for the installed app).`
        : `Not a setting this tool changes: ${key}. list_settings lists them; the vault, Read-only and the folders left out are the user's, in the app.`,
    );
  const [, pane, label, kind, , choices, none] = row;
  // The screen shows Only in the menu bar… only while Show in the menu bar is on.
  if (key === "menuBarOnly" && !isMac)
    throw new Error(
      "Only in the menu bar when the window is closed is a Mac-only setting: on Windows, closing the window quits when Show in the menu bar is off and hides to the tray when it is on.",
    );
  if (key === "menuBarOnly" && s.menuBar === false)
    throw new Error(
      "Only in the menu bar when the window is closed is there only while Show in the menu bar (menuBar) is on: turn that on first.",
    );
  let v: unknown = a.value;
  if (v === null) {
    if (!none) throw new Error(`${key} has no default choice to go back to: give a value.`);
    v = undefined;
  } else if (kind === "bool") {
    if (typeof v === "string") v = v === "true" ? true : v === "false" ? false : v;
    if (typeof v !== "boolean") throw new Error(`${key} is true or false.`);
  } else if (kind === "time") {
    if (typeof v !== "string" || !/^([01]\d|2[0-3]):[0-5]\d$/.test(v)) throw new Error(`${key} is a time, HH:MM.`);
  } else if (kind === "day") {
    v = String(v ?? "")
      .slice(0, 3)
      .toLowerCase();
    if (!DAYS.includes(v as string)) throw new Error(`${key} is a day: mon, tue… sun.`);
  } else if (kind === "theme") {
    if (!["system", "light", "dark"].includes(String(v))) throw new Error(`${key} is system, light or dark.`);
  } else if (kind === "number") {
    v = Number(v);
    if (![7, 14, 30, 90].includes(v as number)) throw new Error(`${key} is 7, 14, 30 or 90.`);
  } else if (kind === "speed") {
    v = Number(v);
    if (!Number.isFinite(v) || (v as number) < 0.5 || (v as number) > 2) throw new Error(`${key} is a number from 0.5 to 2.`);
    // The slider's steps.
    v = Math.round((v as number) * 20) / 20;
  } else if (kind === "size") {
    v = Number(v);
    if (!Number.isInteger(v) || (v as number) < READ_SIZE.min || (v as number) > READ_SIZE.max)
      throw new Error(`${key} is a whole number from ${READ_SIZE.min} to ${READ_SIZE.max} (px).`);
  } else if (kind === "percent") {
    v = Number(v);
    if (!Number.isInteger(v) || (v as number) < INTERFACE_SIZE.min || (v as number) > INTERFACE_SIZE.max)
      throw new Error(
        `${key} is a whole number from ${INTERFACE_SIZE.min} to ${INTERFACE_SIZE.max} (%), in steps of ${INTERFACE_SIZE.step}.`,
      );
    // The slider's steps, as the screen snaps them.
    v = Math.round((v as number) / INTERFACE_SIZE.step) * INTERFACE_SIZE.step;
  } else if (kind === "shortcut") {
    v = shortcutSpec(String(v ?? ""));
  } else if (kind === "choice" || kind === "model") {
    const cs = await Promise.resolve(choices!());
    const want = String(v).trim().toLowerCase();
    const hit = cs.find(([id, name]) => id.toLowerCase() === want || name.toLowerCase() === want);
    if (!hit)
      throw new Error(
        `${key} is one of: ${cs.map(([id]) => id).join(", ") || "none here"}${none ? `, or null for ${none}` : ""}. list_settings lists them.`,
      );
    v = hit[0];
  } else if (key === "templateScripts") {
    // As the screen keeps it: no slashes at its ends, and the default unset.
    v = scriptsFolder(String(v ?? ""));
  } else v = String(v ?? "").trim() || undefined;
  const [head, sub] = key!.split(".");
  if (key === "openAtLogin") {
    await api.loginItemSet(v as boolean);
  } else if (sub) {
    const next = { ...nested[head], [sub]: v };
    if (v === undefined) delete next[sub];
    settings.update({ [head]: next } as Partial<Settings>);
  } else if (kind === "theme") {
    settings.update({ theme: v as Settings["theme"], docTheme: undefined });
    applyTheme(v as Settings["theme"]);
  } else if (kind === "size") {
    settings.update({ readSize: applyReadSize(v as number) });
  } else if (kind === "percent") {
    await setInterfaceSize(v as number);
  } else settings.update({ [key!]: v } as Partial<Settings>);
  await settings.flush();
  if (key === "speechVoice" || key === "speechRate") speechSettingsChanged();
  const said = v === undefined ? (none ?? "its default") : JSON.stringify(v);
  // The shortcut is registered as the settings are saved: say if the system wouldn't give it, as
  // the screen does under it.
  if (kind === "shortcut") {
    const [, error] = await api.captureShortcutStatus();
    if (error)
      return `${where(pane, label)} is set to ${said}, but it isn't working: ${error} The shortcut that was registered before still works.`;
    noted(`set ${label} to ${said}`);
    return `${where(pane, label)} is now ${said}, registered with the system.`;
  }
  noted(`set ${label} to ${said}`);
  return `${where(pane, label)} is now ${said}.`;
}

// ---- a note's own look

/** A theme or colour by its id or the name the Look menu gives it. */
function lookChoice(list: [string, string, ...unknown[]][], v: unknown, what: string): string {
  const want = String(v).trim().toLowerCase();
  const hit = list.find(([id, name]) => id === want || name.toLowerCase() === want);
  if (!hit) throw new Error(`${what} is one of: ${list.map(([id, name]) => `${id} (${name})`).join(", ")}.`);
  return hit[0];
}

/** A note's own look, as the note's Look button sets it: kept in settings by path (docLooks),
 *  never in the note (src/docLook.ts). Read one note's or all those with their own, set its theme
 *  or colour, Use the defaults on one, or Settings › Notes' Use the defaults for all. */
async function noteLook(a: Args): Promise<string | Listing> {
  const s = settings.get();
  const action = str(a, "action") ?? "list";
  const styleName = (id: string) => DOC_STYLES.find(([x]) => x === id)?.[1] ?? id;
  const accentName = (id: string) => DOC_ACCENTS.find(([x]) => x === id)?.[1] ?? id;
  const page = str(a, "page");
  let path: string | null = null;
  if (page) {
    [path] = await api.linksResolve([page.replace(/^\[\[|\]\]$/g, "")]);
    if (!path) throw new Error(`There's no page called ${page}.`);
  }
  if (action === "list") {
    const d = docDefaults(s);
    const defaults = `The defaults (Settings › Notes › Document look): ${styleName(d.style)}, ${accentName(d.accent)}.`;
    if (path) {
      const l = lookOf(s, path);
      return `${pageName(path)} looks ${styleName(l.style)}, ${accentName(l.accent)}: ${l.own ? "its own look" : "the defaults"}. ${defaults}`;
    }
    const rows = Object.keys(s.docLooks ?? {}).sort();
    const line = (p: string) => {
      const l = lookOf(s, p);
      return `- ${p} · ${styleName(l.style)}, ${accentName(l.accent)}`;
    };
    return listed(a, rows, {
      text: line,
      noun: ["note with its own look", "notes with their own look"],
      line,
      item: (p) => ({ path: p, theme: lookOf(s, p).style, colour: lookOf(s, p).accent }),
      empty: `No note has its own look. ${defaults}`,
      before: rows.length ? [defaults] : [],
    });
  }
  if (action === "use_defaults_for_all") {
    const n = Object.keys(s.docLooks ?? {}).length;
    if (!n) return "No note has its own look: they all use the defaults already.";
    settings.update({ docLooks: {} });
    await settings.flush();
    noted("set every note to the default look");
    return `${fmtCount(n)} ${n === 1 ? "note uses" : "notes use"} the defaults again, as Use the defaults for all does.`;
  }
  if (action !== "set" && action !== "use_defaults") throw new Error("action is list, set, use_defaults or use_defaults_for_all.");
  if (!path) throw new Error("Give page: the note whose look to change.");
  const d = docDefaults(s);
  const patch =
    action === "use_defaults"
      ? { style: d.style, accent: d.accent }
      : {
          ...(has(a, "theme") ? { style: lookChoice(DOC_STYLES, a.theme, "theme") } : {}),
          ...(has(a, "colour") ? { accent: lookChoice(DOC_ACCENTS, a.colour, "colour") } : {}),
        };
  if (!Object.keys(patch).length) throw new Error("Give theme or colour, or both.");
  settings.update({ docLooks: withOwnLook(s.docLooks ?? {}, path, patch, d) });
  await settings.flush();
  const l = lookOf(settings.get(), path);
  noted(`set the look of ${pageName(path)}`);
  return `${pageName(path)} now looks ${styleName(l.style)}, ${accentName(l.accent)}${l.own ? "" : " (the defaults)"}. ${keyLabel("⌘Z")} doesn't undo it; use_defaults puts it back.`;
}

// ---- Sources' Import

async function importSources(a: Args): Promise<string> {
  const paths = (Array.isArray(a.paths) ? a.paths : []).map(String).filter((p) => p.trim());
  if (!paths.length) throw new Error("Give paths: the files to add, by their full path.");
  const r = await api.sourcesImport(paths);
  const ok = r.filter((x) => x.path).map((x) => x.path!);
  const bad = r.filter((x) => !x.path);
  if (ok.length) told(`added ${ok.length} file${ok.length === 1 ? "" : "s"} to Sources`);
  const toIngest = ok.filter(ingestable).filter((p) => !isTranscriptPath(p));
  const ingesting = settings.get().ingestOnArrival && toIngest.length ? await api.ingestStart(toIngest, a.unattended === true) : [];
  return [
    ok.length ? `Added to Sources: ${ok.join(", ")}.` : "Nothing was added.",
    ingesting.length ? `Ingesting ${toIngest.length} of them, as new sources are (run ${ingesting.join(", ")}).` : "",
    bad.length ? `Not added: ${bad.map((x) => `${x.name} (${x.error ?? "it didn't work"})`).join("; ")}.` : "",
  ]
    .filter(Boolean)
    .join(" ");
}

// ---- the window

/** The sidebar's names for the screens open takes, where they differ from the app's. */
const SCREEN_IDS: Record<string, Screen> = {
  weekly_review: "weekly",
  changes: "review",
  knowledge_health: "health",
  doc_check: "doccheck",
  compare: "compare",
};

/** The screens that open on a file of their own, and the argument that names it (as their tools name it). */
const SCREEN_FILE: Partial<Record<Screen, string>> = { meeting: "transcript", reply: "thread", doccheck: "document" };

/** open's help: getting-started (the list of guides), a guide (at its first step) or a topic, by id or title. */
function helpView(want: string): { view: HelpView; title: string } {
  const w = want.toLowerCase();
  if (w === "getting-started" || w === "getting started") return { view: { start: true }, title: "Getting started" };
  const t = TOPICS.find((x) => x.id === w || x.title.toLowerCase() === w);
  if (!t) throw new Error(`There's no help topic ${want}. The help tool lists them; getting-started opens the list of guides.`);
  return { view: t.kind === "guide" ? { guide: t.id, step: 0 } : { topic: t.id }, title: t.title };
}

/** open: the window to the front, on a screen, a Settings pane, a note or a search; or an image
 *  full size over it, as a click on one in a note shows it. */
async function openApp(a: Args): Promise<string> {
  const image = str(a, "image")?.trim();
  if (image) {
    const name = image.replace(/^!?\[\[|\]\]$/g, "").split("|")[0];
    const p = IMAGE_EXT.test(name) ? await api.assetFind(name).catch(() => null) : null;
    if (!p) throw new Error(`There's no image called ${image} in the vault.`);
    await invoke("main_show", { screen: null });
    imageView.set({ url: convertFileSrc(vaultFile(settings.get().vaultPath ?? "", p)), alt: pageName(p) });
    return `Brainstead shows ${p} full size.`;
  }
  const page = str(a, "page")?.trim();
  const name = str(a, "screen");
  const screen = name ? (SCREEN_IDS[name] ?? (name as Screen)) : null;
  let path: string | null = null;
  // meeting, reply and doc_check open on their transcript, thread or document, as the Inbox and a
  // note's buttons open them; the file must be in the vault.
  const fileArg = screen ? SCREEN_FILE[screen] : undefined;
  const asked = fileArg ? str(a, fileArg) : undefined;
  let file: string | null = null;
  if (asked && screen === "meeting") {
    const all = await api.meetingTranscripts();
    if (!all.some((t) => t.path === asked))
      throw new Error(`No transcript at ${asked}. list_transcripts lists them: ${all.map((t) => t.path).join(", ") || "none"}.`);
    file = asked;
  } else if (asked) {
    [file] = await api.linksResolve([asked.replace(/^\[\[|\]\]$/g, "")]);
    if (!file) throw new Error(`There's no ${asked} in the vault.`);
  }
  if (page) {
    [path] = await api.linksResolve([page.replace(/^\[\[|\]\]$/g, "")]);
    if (!path) throw new Error(`There's no page called ${page}.`);
  }
  // Focus mode is a document's, as ⌘. is.
  const focus = typeof a.focus === "boolean" ? a.focus : undefined;
  if (focus && (path ? screen === "graph" : screen || place.get().place.screen !== "doc"))
    throw new Error("Focus mode shows a note, wiki page or source: give page, or open one first.");
  // The help drawer beside the screen, as ? and its Getting started button and topic picker open it.
  const helpArg = str(a, "help")?.trim();
  const help = helpArg ? helpView(helpArg) : null;
  await invoke("main_show", { screen: null });
  const lines = (Array.isArray(a.lines) ? a.lines : []).map((l) => String(l).trim()).filter(Boolean);
  // Graph with a page opens around it, as a note's Graph button (openGraph) does.
  if (path && screen === "graph") nav.go({ screen, path });
  else if (path) {
    // edit opens it in Edit, as a project's Edit button (editDoc) does.
    if (a.edit === true) askEdit(path);
    nav.go({ screen: "doc", path, ...(lines.length ? { lines } : {}) });
  } else if (screen === "settings") nav.go({ screen, pane: (str(a, "pane") as SettingsPane | undefined) ?? "general" });
  else if (screen === "search") nav.go({ screen, q: str(a, "query") ?? "" });
  else if (screen && file) nav.go({ screen, path: file });
  else if (screen) nav.go(screen);
  if (focus !== undefined) focusMode.set(focus);
  if (help) openHelp(help.view);
  const how = focus ? " in focus mode" : "";
  if (help && !path && !screen) return `Brainstead shows the help: ${help.title}.`;
  if (help)
    return `Brainstead is open${path ? ` on ${pageName(path)}` : name ? ` on ${name.replace(/_/g, " ")}` : ""}, with the help: ${help.title}.`;
  if (path && screen === "graph") return `Brainstead is open on Graph, around ${pageName(path)}.`;
  if (path) return `Brainstead is open on ${pageName(path)}${a.edit === true ? " in Edit" : ""}${how}.`;
  if (focus !== undefined)
    return focus ? `Brainstead shows ${pageName(place.get().place.path ?? "")} in focus mode.` : "Focus mode is off.";
  return `Brainstead is open${name ? ` on ${name.replace(/_/g, " ")}` : ""}${file ? ` with ${file}` : ""}.`;
}
