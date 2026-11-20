// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Taking an item out of the Inbox once it's clarified: the writes the Inbox screen makes, kept
// here so the MCP bridge (src/mcpActions.ts) makes the same ones.

import { ActionEdit, api, InboxItem } from "./api";
import { captureDone } from "./gtd";
import { isTranscriptPath, trashTranscript } from "./meetingFlow";
import { isThread } from "./skills/Reply";
import { sourceActions } from "./sourceActions";
import { toggleTaskEdit } from "./taskModel";

/** What an Inbox item can become, in the order its buttons show: the choices that make sense for
 *  its kind, the one it usually becomes first. A transcript is a meeting note (or kept, or
 *  trashed); a thread is usually answered; a capture isn't "done now" (Keep in Sources already
 *  takes it out), only a thread is waited on, and any capture can be asked about (Ask, which
 *  leaves it in the Inbox). The Inbox's buttons, keys and ticked-several choices, and
 *  clarify_inbox, all follow this. */
/** An item's id for clarify_inbox and list_inbox: an action by its id (lines repeat across
 *  initiatives), a capture by its path, the rest by kind and line. */
export const inboxKey = (i: InboxItem) =>
  i.kind === "action" && i.action?.id ? `action:${i.action.id}` : i.kind === "capture" ? `capture:${i.path}` : `${i.kind}:${i.line + 1}`;

export function inboxChoices(i: InboxItem): string[] {
  // An initiative's action: a task (a waiting-for when it's delegated), done or dropped.
  if (i.kind === "action")
    return i.action?.owner === "delegated"
      ? ["waiting", "next", "todo", "someday", "done", "move", "delete"]
      : ["next", "todo", "waiting", "someday", "done", "move", "delete"];
  if (i.kind !== "capture") return ["next", "todo", "project", "waiting", "done", "someday", "reference", "delete"];
  // A capture's source actions first, as Sources offers them (src/sourceActions.ts).
  const own: string[] = sourceActions(i.path);
  if (isTranscriptPath(i.path)) return [...own, "reference", "delete"];
  if (isThread(i.path)) return [...own, "next", "todo", "project", "waiting", "someday", "reference", "delete"];
  return [...own, "next", "todo", "project", "someday", "reference", "delete"];
}

/** The choices that make it something to do (GTD's "is it actionable?"); the rest file, park or
 *  drop it. */
const NEEDS_DOING = new Set(["reply", "next", "todo", "waiting", "project", "done"]);

/** An item's choices as the Inbox groups them: those that make it something to do, those that
 *  don't, in inboxChoices' order, and whether it can be asked about (Ask leaves it in the Inbox,
 *  so it's in neither group). */
export function choiceGroups(i: InboxItem): { doing: string[]; notDoing: string[]; ask: boolean } {
  const all = inboxChoices(i);
  return {
    doing: all.filter((c) => NEEDS_DOING.has(c)),
    notDoing: all.filter((c) => !NEEDS_DOING.has(c) && c !== "ask"),
    ask: all.includes("ask"),
  };
}

/** The choice an item's buttons lead with, highlighted: a capture's leading source action (a
 *  transcript's meeting note, a thread's reply, else Ingest). */
export const leadChoice = (i: InboxItem): string | null =>
  i.kind === "capture" ? (sourceActions(i.path)[0] ?? null) : i.kind === "action" ? inboxChoices(i)[0] : null;

/** What kind of thing an Inbox item is, for its icon and the name beside it: a thought, a task, or
 *  a capture by what its file is (a transcript, an email or chat thread, a Word file, a PDF…). */
export function inboxItemKind(i: InboxItem): { icon: string; label: string } {
  if (i.kind === "action")
    return i.action?.owner === "delegated"
      ? { icon: "waiting", label: `Waiting for ${i.action.person ?? "someone"}${i.action.overdue ? ", overdue" : ""}` }
      : { icon: "tasks", label: "Your action" };
  if (i.kind === "thought") return { icon: "bulb", label: "Scratchpad note" };
  if (i.kind === "task") return { icon: "tasks", label: "Task" };
  if (isTranscriptPath(i.path)) return { icon: "mic", label: "Teams transcript" };
  const name = i.path.split("/").pop() ?? "";
  if (/^Email\. /.test(name)) return { icon: "mail", label: "Email" };
  if (/^Teams\. /.test(name)) return { icon: "chat", label: "Teams chat" };
  if (/^Slack\. /.test(name)) return { icon: "chat", label: "Slack conversation" };
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (ext === "docx" || ext === "doc") return { icon: "file-word", label: "Word document" };
  if (ext === "pdf") return { icon: "file-pdf", label: "PDF" };
  if (ext === "pptx" || ext === "ppt") return { icon: "slides", label: "PowerPoint" };
  if (["xlsx", "xls", "csv"].includes(ext)) return { icon: "sheet", label: "Spreadsheet" };
  if (["png", "jpg", "jpeg", "gif", "webp", "heic"].includes(ext)) return { icon: "image", label: "Image" };
  if (ext === "md" || ext === "txt") return { icon: "note", label: "Note" };
  return { icon: "source", label: "File" };
}

/** What Suggest tells the model an item is: a Teams transcript (made a meeting note without asking
 *  it), a captured email or chat thread (which can also be a reply), another capture (which can be
 *  ingested), a file ingest can't read, or a thought or task. The Inbox and list_inbox's suggest
 *  both ask this way, and what Suggest may propose for each kind is what inboxChoices offers it:
 *  brainstead_core::reviews::prep's inbox_kind and fits are the same rules, and both are checked
 *  against tests/fixtures/inbox-choices.json. */
export function suggestKind(i: InboxItem): string {
  if (i.kind !== "capture") return i.kind;
  if (isTranscriptPath(i.path)) return "transcript";
  if (isThread(i.path)) return "thread";
  return sourceActions(i.path).includes("ingest") ? "capture" : "file";
}

/** Done now: a task ticked as everywhere else (a recurring one gets its next occurrence:
 *  toggleTaskEdit), a capture marked done, a thought taken out. */
/** Where an action's task went, for its `task::` field: the project's note, or the To Do list. */
export const taskWent = (toProject?: string | null) => `[[${(toProject ?? "Me. To Do List.md").split("/").pop()!.replace(/\.md$/, "")}]]`;

/** What an action becomes as a task, by default: its words, whose it is when delegated, and its
 *  initiative linked (not Unfiled). */
export function actionTaskText(i: InboxItem): string {
  const a = i.action;
  const who = a?.owner === "delegated" && a.person ? `${a.person}: ` : "";
  // The action's block, shown as its initiative: ticking the task marks the action done (D-20261118-26).
  const link = a ? ` ${taskLink(a.initiative, a.id)}` : "";
  return `${who}${i.text.trim()}${link}`;
}

/** A task's link to the action it was made from (brainstead_core::initiatives::task_link). */
export const taskLink = (initiative: string, id: string) => `[[${initiative}#^${id.replace(/^\^/, "")}|${initiative}]]`;

/** An action's line changed in place (done, dropped, made a task). */
export const setAction = (i: InboxItem, change: ActionEdit) => api.initiativeActionSet(i.path, i.line, i.lineText, change);

export function doneNow(i: InboxItem): Promise<unknown> {
  if (i.kind === "action") return setAction(i, { status: "done" });
  if (i.kind === "task") return toggleTaskEdit({ path: i.path, line: i.line, lineText: i.lineText }, true);
  if (i.kind === "capture") return captureDone(i.path);
  return api.inboxRemoveThought(i.line, i.block ?? i.lineText);
}

/** Does `one` to each item in turn, carrying on past one that fails: those that went through, and
 *  those that didn't with why. The Inbox's choice for several ticked items. */
export async function eachItem<T>(items: T[], one: (i: T) => Promise<unknown>): Promise<{ ok: T[]; failed: [T, string][] }> {
  const ok: T[] = [];
  const failed: [T, string][] = [];
  for (const i of items) {
    try {
      await one(i);
      ok.push(i);
    } catch (e) {
      failed.push([i, typeof e === "object" && e && "message" in e ? String(e.message) : String(e)]);
    }
  }
  return { ok, failed };
}

/** What a choice for several items says when it's done: "Kept in Sources (4)", or, when some
 *  failed, "4 done, 1 failed: why" (the first failure's reason). */
export function eachSaid(done: string, ok: number, failed: string[]): string {
  if (!failed.length) return `${done} (${ok})`;
  return `${ok} done, ${failed.length} failed: ${failed[0]}`;
}

/** Takes the item out of the Inbox: a thought's block goes; a task's line becomes `line` (or goes);
 *  a capture is marked done. With `toProject`, the new line goes under the project's `heading`. */
export async function settleInboxItem(
  i: InboxItem,
  line: string | null,
  toProject?: string | null,
  heading = "Next actions",
): Promise<unknown> {
  if (i.kind === "action") {
    // The task made, then the action marked with where it went; deleted, it's dropped.
    if (!line) return setAction(i, { status: "dropped" });
    if (toProject) await api.projectAddTask(toProject, line.replace(/^- \[ \] /, ""), heading);
    else await api.capture("task", line.replace(/^- \[ \] /, ""), "");
    return setAction(i, { task: taskWent(toProject) });
  }
  if (i.kind === "task") {
    if (toProject && line) return api.inboxMoveTask(i.line, i.lineText, toProject, heading, line);
    return api.taskReplace({ path: i.path, line: i.line, lineText: i.lineText }, line ? [line] : [], line ? "Clarified" : "Deleted");
  }
  if (line) {
    if (toProject) await api.projectAddTask(toProject, line.replace(/^- \[ \] /, ""), heading);
    else await api.capture("task", line.replace(/^- \[ \] /, ""), "");
  }
  if (i.kind === "capture") return captureDone(i.path);
  return api.inboxRemoveThought(i.line, i.block ?? i.lineText);
}

/** To Do: the item as a task at the end of the To Do list's To Do, as it is (an action's due date
 *  and its link come with it), then out of the Inbox: an action marked with where its task went, a
 *  task's line or a thought taken out, a capture marked done. */
export async function makeTask(i: InboxItem, text: string): Promise<unknown> {
  const words = text.replace(/^- \[.\] /, "").trim();
  const line = i.kind === "action" && i.action?.due && !/📅/.test(words) ? `${words} 📅 ${i.action.due}` : words;
  // A task on the To Do list's Other leaves it first, so adding above it doesn't move its line.
  if (i.kind === "task") {
    await api.taskReplace({ path: i.path, line: i.line, lineText: i.lineText }, [], "Made a task of");
    return api.todoAddTask(line);
  }
  await api.todoAddTask(line);
  if (i.kind === "action") return setAction(i, { task: taskWent(null) });
  if (i.kind === "capture") return captureDone(i.path);
  return api.inboxRemoveThought(i.line, i.block ?? i.lineText);
}

/** Moves a capture's file to the Trash (Delete), a transcript's links first pointed at its meeting
 *  note, as every transcript trashed is (trashTranscript). It isn't marked clarified: restored
 *  from the Trash, it's back in the Inbox. */
export const trashCapture = (path: string) => (isTranscriptPath(path) ? trashTranscript(path) : api.trashMove(path));

/** A note's text with the item added as a bullet at its end, a blank line before it. The note's own
 *  ending (its trailing blank lines) is left as it was: only what's missing is added. */
export function withReference(content: string, text: string): string {
  const eol = content.includes("\r\n") ? "\r\n" : "\n";
  const ended = content === "" || content.endsWith("\n");
  const blank = content.trim() === "" || /\n[ \t]*\r?\n$/.test(content);
  return `${content}${ended ? "" : eol}${blank ? "" : eol}- ${text.replace(/\n/g, " ")}${eol}`;
}

/** Adds the item's text as a bullet at the end of a note (File as reference). */
export async function appendAsReference(path: string, text: string): Promise<void> {
  const d = await api.docRead(path);
  await api.docSave(path, withReference(d.content, text), d.version);
}

/** A new note holding the item (File as reference, to a note that isn't there yet). */
export async function newReferenceNote(title: string, text: string): Promise<string> {
  const path = `${title.replace(/[/\\:*?"<>|]/g, "")}.md`;
  await api.docCreate(path, `${text}\n`);
  return path;
}
