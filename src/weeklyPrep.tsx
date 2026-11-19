// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The weekly review's prepared suggestions (src-tauri/src/weekprep.rs): a row each above a step's
// own content, as the one suggestion component's (src/Suggestions.tsx), with what accepting does,
// Accept, Edit and Skip. Accepting runs one of the app's own undoable
// actions (the task edits the Tasks screen uses, the Inbox's clarify, Quick capture's add) and says
// so with Undo; a link is made too, revertable in Changes. Nothing is done without a click.

import { useState } from "react";
import { api, InboxItem, ProjectRow, TaskRow, WeekPrepAction, WeekPrepStatus, WeekPrepStep, WeekPrepSuggestion, WeekPrepTask } from "./api";
import { taskLine, unclarified } from "./Inbox";
import { fmtShortDate } from "./md/dates";
import { taskLabel } from "./md/TaskBlock";
import { clarifyItem, findTask } from "./mcpActions";
import { nav, openDoc } from "./nav";
import { inQuote, tagToggled } from "./tasksq/edits";
import { changeTaskEdit, reportEditError, retitled, toggleTaskEdit, undoAction } from "./taskModel";
import { toast } from "./Toast";
import { iconizeEmoji } from "./md/fieldEmoji";
import { SuggestButton, SuggestionRow } from "./Suggestions";

export type Handled = Record<string, "accepted" | "skipped">;

/** Fired in the window when an assistant changes the review's saved progress (src/mcpActions.ts),
 *  so an open Weekly review reads it again. */
export const WEEKLY_STATE_CHANGED = "brainstead-weekly-state";

/** The preparation's step for a review step (src/Weekly.tsx's STEPS); the creative prompts go
 *  with New ideas. */
export function prepStep(id: string): WeekPrepStep | null {
  if (id === "ideas") return "creative";
  if (id === "ghosts") return null;
  return id as WeekPrepStep;
}

/** Steps whose suggestions are prompts to think about, with nothing to click. */
const PROMPTS: WeekPrepStep[] = ["loose", "creative"];

/** What accepting does: its button's words and tooltip, and the action (resolves to the line for
 *  the review's log). */
interface Choice {
  verb: string;
  title: string;
  run: () => Promise<string>;
}

const BECOMES: Record<string, string> = {
  next: "a next action",
  waiting: "waiting for",
  someday: "someday / maybe",
  done: "done",
  delete: "deleted",
  project: "a project",
  reference: "reference",
  ingest: "ingested",
};

/** The task an action is on, as it is now. */
async function current(t: WeekPrepTask): Promise<TaskRow> {
  return findTask(await api.tasksAll(), `${t.path}:${t.line + 1}`, t.text);
}

const hasTag = (t: TaskRow, tag: string) => t.tags.some((x) => x.replace(/^#/, "") === tag);

/** Says what an edit did, with Undo. */
function said(r: { undo: string | null } | string, fallback: string): string {
  const text = (typeof r === "string" ? r : r.undo) || fallback;
  toast(text, undoAction, "ok");
  return text;
}

/** The Inbox item a clarify is on, as the Inbox lists it now. */
async function inboxItem(want: { kind: string; path: string; line: number; lineText: string }): Promise<InboxItem> {
  const items = (await api.inboxList()).filter(unclarified);
  const same = (i: InboxItem) => i.kind === want.kind && i.path === want.path && i.lineText === want.lineText;
  const i = items.find((x) => same(x) && x.line === want.line) ?? items.find(same);
  if (!i) throw new Error("That Inbox item has changed or gone since the suggestions were prepared.");
  return i;
}

/** The task line an add writes, without its checkbox: as the Inbox writes one, with its tags and
 *  start date. */
export function addLine(a: Extract<WeekPrepAction, { do: "add" }>, project: ProjectRow | undefined): string {
  const line = taskLine({ text: a.text, project: project?.path ?? null, context: a.context, due: a.due }).replace(/^- \[ \] /, "");
  const extra = [...a.tags.map((t) => `#${t}`), a.scheduled ? `⏳ ${a.scheduled}` : ""].filter(Boolean).join(" ");
  if (!extra) return line;
  // Before the due date, which taskLine puts last.
  const due = / 📅 \d{4}-\d{2}-\d{2}$/.exec(line);
  return due ? `${line.slice(0, due.index)} ${extra}${due[0]}` : `${line} ${extra}`;
}

/** What a suggestion offers; none for a prompt. `unattended`: an assistant nobody is watching
 *  accepts it, so a link (a change in Changes) is held when a check fails, and an ingest a clarify
 *  starts runs as a scheduled one (never on an Ollama model, brainstead_core::ask::job_run_model). */
export function choice(s: WeekPrepSuggestion, week: string, projects: ProjectRow[], unattended = false): Choice | null {
  const a = s.action;
  if (!a) return null;
  switch (a.do) {
    case "add": {
      const p = a.project ? projects.find((x) => x.name.toLowerCase() === a.project!.toLowerCase()) : undefined;
      const text = addLine(a, p);
      const waiting = a.tags.includes("waiting-for");
      return {
        verb: "Add task",
        title: p ? `Add “${a.text}” to the project ${p.name}` : `Add “${a.text}” to the To Do list, under Other`,
        run: async () =>
          said(
            await (p ? api.projectAddTask(p.path, text, waiting ? "Waiting for" : "Next actions") : api.taskAdd(text)),
            `Added “${a.text}”`,
          ),
      };
    }
    case "tick":
      return {
        verb: "Tick",
        title: `Tick “${taskLabel(a.task.text)}” off as done`,
        run: async () => said(await toggleTaskEdit(await current(a.task), true), "Ticked"),
      };
    case "defer":
      return {
        verb: `Defer to ${fmtShortDate(a.date)}`,
        title: `Defer “${taskLabel(a.task.text)}” until ${a.date}`,
        run: async () => said(await api.taskSetDate(await current(a.task), "scheduled", a.date), "Deferred"),
      };
    case "waiting":
      return {
        verb: "Waiting for",
        title: `Mark “${taskLabel(a.task.text)}” as waiting for someone`,
        run: async () => {
          const t = await current(a.task);
          if (hasTag(t, "waiting-for")) throw new Error("That task is already waiting for someone.");
          return said(await changeTaskEdit(t, "waiting"), "Waiting for");
        },
      };
    case "someday":
      return {
        verb: "Someday",
        title: `Move “${taskLabel(a.task.text)}” to Someday / maybe`,
        run: async () => {
          const t = await current(a.task);
          if (hasTag(t, "someday-maybe")) throw new Error("That task is on Someday / maybe already.");
          const c = inQuote(t.lineText, (l) => tagToggled(l, "someday-maybe", "Moved to someday", "Made a next action"));
          return said(await api.taskReplace(t, c.lines, c.label), "Moved to someday");
        },
      };
    case "edit":
      return {
        verb: "Change wording",
        title: `Change the task's words to “${a.text}” (its dates and tags stay)`,
        run: async () => {
          const t = await current(a.task);
          const line = retitled(t.lineText, taskLabel(t.text), a.text);
          if (!line) throw new Error("The task's words couldn't be changed simply: change them in Tasks.");
          return said(await api.taskReplace(t, [line], "Renamed"), "Renamed");
        },
      };
    case "clarify": {
      const b = a.becomes;
      // A note to file it in, when it's reference; a capture is simply kept in Sources.
      if (b === "reference" && !a.text && a.item.kind !== "capture")
        return { verb: "Open the Inbox", title: "File it as reference from the Inbox", run: async () => (nav.go("inbox"), "") };
      return {
        verb: `Clarify as ${BECOMES[b] ?? b}`,
        title: `Clarify this Inbox item as ${BECOMES[b] ?? b}${a.text && b !== "reference" ? `: “${a.text}”` : ""}`,
        run: async () => {
          const i = await inboxItem(a.item);
          const args =
            b === "project"
              ? { becomes: b, project_name: a.text ?? undefined }
              : b === "reference"
                ? { becomes: b, note: a.text ?? undefined }
                : {
                    becomes: b,
                    text: a.text ?? undefined,
                    project: a.project ?? undefined,
                    context: a.context ?? undefined,
                    due: a.due ?? undefined,
                  };
          const as = await clarifyItem(i, args, unattended);
          toast(`Clarified as ${as}`, undoAction, "ok");
          return `Clarified “${i.text.slice(0, 60)}” as ${as}`;
        },
      };
    }
    case "link":
      return {
        verb: "Link",
        title: `Link “${a.phrase}” to [[${a.target}]] in ${a.path.replace(/\.md$/, "")}; revertable in Changes`,
        run: async () => {
          const o = await api.weekprepLink(week, a.path, a.phrase, a.target, unattended);
          const where = a.path.replace(/\.md$/, "");
          if (!o.applied) {
            toast("Held for you in Changes", { label: "Open", run: () => nav.go("review") }, "ok");
            return `Held linking ${a.target} in ${where} for the user in Changes: ${o.flags.join(" ")}`;
          }
          toast("Linked; it's in Changes, with Revert", { label: "Open", run: () => nav.go("review") }, "ok");
          return `Linked ${a.target} in ${where}`;
        },
      };
  }
  return null;
}

/** What the row adds under its line: the task to add, or the new wording. */
function detail(s: WeekPrepSuggestion): string | null {
  const a = s.action;
  if (a?.do === "edit") return `→ ${a.text}`;
  if (a?.do === "add") return `→ ${a.text}${a.project ? ` · ${a.project}` : ""}${a.due ? ` · due ${a.due}` : ""}`;
  if (a?.do === "clarify" && a.text) return `→ ${a.text}`;
  return null;
}

/** The words Edit can change before accepting: the task to add, the new wording, or what an Inbox
 *  item becomes (not a note to file it in); null when there are none. */
export function editableText(s: WeekPrepSuggestion): string | null {
  const a = s.action;
  if (a?.do === "add" || a?.do === "edit") return a.text;
  if (a?.do === "clarify" && a.text && a.becomes !== "reference") return a.text;
  return null;
}

/** The suggestion with its words changed, as Edit then Accept (and weekly_suggestion's text) do. */
export function withText(s: WeekPrepSuggestion, text: string | undefined): WeekPrepSuggestion {
  const t = text?.trim();
  if (!t || editableText(s) === null || !s.action) return s;
  return { ...s, action: { ...s.action, text: t } as WeekPrepAction };
}

const noteName = (path: string) => path.replace(/\.md$/, "").split("/").pop() ?? path;

function Source({ s }: { s: WeekPrepSuggestion }) {
  if (!s.source) return null;
  const src = s.source;
  return (
    <button
      type="button"
      className="tlink small wsrc"
      title={src.quote ? `“${src.quote}” (open ${noteName(src.path)})` : `Open ${noteName(src.path)}`}
      onClick={() => openDoc(src.path)}
    >
      {noteName(src.path)}
    </button>
  );
}

/** A step's suggestions, always open (D-20261119-06): rows with Accept, Edit where there are words
 *  to change, and Skip (src/Suggestions.tsx), or a short list of prompts. The ones accepted or
 *  skipped are hidden. */
export function PrepRows({
  step,
  rows,
  handled,
  onHandle,
  week,
  projects,
}: {
  step: WeekPrepStep;
  rows: WeekPrepSuggestion[];
  handled: Handled;
  onHandle: (id: string, how: "accepted" | "skipped", log: string) => void;
  week: string;
  projects: ProjectRow[];
}) {
  const [busy, setBusy] = useState<string | null>(null);
  const open = rows.filter((r) => !handled[r.id]);
  if (!open.length) return null;
  if (PROMPTS.includes(step))
    return (
      <div className="wprep" aria-label="Suggestions">
        <div className="eyebrow">To think about</div>
        <ul className="wprompts">
          {open.map((s) => (
            <li key={s.id}>
              {iconizeEmoji(s.text)} <Source s={s} />
            </li>
          ))}
        </ul>
      </div>
    );
  return (
    <div className="wprep" aria-label="Suggestions">
      <div className="eyebrow">
        {open.length === 1 ? "1 suggestion" : `${open.length} suggestions`}
        {open.length < rows.length ? ` · ${rows.length - open.length} dealt with` : ""}
      </div>
      {open.map((s) => {
        const c = choice(s, week, projects);
        const more = detail(s);
        const words = editableText(s);
        const act = (e?: Record<string, string>) => {
          const cc = e ? choice(withText(s, e.text), week, projects) : c;
          if (!cc || busy) return;
          setBusy(s.id);
          cc.run()
            .then((line) => line && onHandle(s.id, "accepted", line))
            .catch((err) => (err instanceof Error ? toast(err.message, undefined, "bad") : reportEditError(err)))
            .finally(() => setBusy(null));
        };
        return (
          <SuggestionRow
            key={s.id}
            id={s.id}
            busy={busy === s.id}
            text={iconizeEmoji(s.text)}
            detail={c ? `${c.verb}${more ? ` ${more}` : ""}` : more}
            source={<Source s={s} />}
            accept={c ? { title: c.title, run: act } : undefined}
            edit={
              words !== null && c
                ? { title: "Change the words before accepting it", fields: [{ key: "text", label: "The words", value: words }] }
                : undefined
            }
            skip={{ title: "Leave this suggestion and hide it", run: () => onHandle(s.id, "skipped", "") }}
          />
        );
      })}
    </div>
  );
}

export const when = (local: string) => {
  const d = new Date(local);
  return Number.isNaN(d.getTime())
    ? local
    : d.toLocaleString("en-GB", { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
};

/** When the suggestions were prepared, with Suggest again; a bar while they're being prepared; or
 *  why the last try failed, with Suggest again too. Nothing when there's no preparation at all (no assistant):
 *  the steps work as before. */
export function PrepHeader({ status, week }: { status: WeekPrepStatus | null; week: string }) {
  if (!status || (!status.prep && !status.running && !status.error)) return null;
  const run = () => void api.weekprepRun(week).catch((e) => toast(String(e), undefined, "bad"));
  if (status.running)
    return (
      <div className="wprephead jprog">
        <span className="faint small">Preparing suggestions from your week…</span>
        <div className="bar busy" role="progressbar" aria-label="Preparing suggestions">
          <i />
        </div>
      </div>
    );
  if (status.error)
    return (
      <div className="wprephead">
        <span className="err small grow">The suggestions couldn't be prepared: {status.error}</span>
        <SuggestButton again title="Try making this week's suggestions again" onClick={run} />
      </div>
    );
  return (
    <div className="wprephead">
      <span className="faint small grow">Suggestions prepared {when(status.prep!.preparedAt)}</span>
      <SuggestButton again title="Read the week again and make fresh suggestions for every step" onClick={run} />
    </div>
  );
}
