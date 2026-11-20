// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Update…: a line saying what has happened ("Lena has already replied to Maya"), on an Inbox
// item, a task or a project, which an assistant applies in the background with Brainstead's own
// tools (D-20261120-04). It runs as an Ask chat in a tab of its own, so its reasoning can be read;
// every change it makes goes through Changes, made at once as anything you start is, with Revert.
// While it works it's in the Running line (src/runs.ts); when it ends a toast says what changed.

import type { InboxItem, ProjectRow, TaskRow } from "./api";
import { ask, backgroundChat, DEFAULT_MODEL, selectChat, stop } from "./askState";
import { inboxKey } from "./inboxActions";
import { pageName } from "./knowledge";
import { nav } from "./nav";
import { Running, windowRuns } from "./runs";
import { settings } from "./store";

export type UpdateTarget = { kind: "inbox"; item: InboxItem } | { kind: "task"; task: TaskRow } | { kind: "project"; project: ProjectRow };

/** The thing updated, in a few words, for the tab, the Running line and the toast. */
export function targetName(t: UpdateTarget): string {
  const words = t.kind === "inbox" ? t.item.text : t.kind === "task" ? t.task.text : t.project.name;
  const one = words.split("\n")[0].trim();
  return one.length > 60 ? `${one.slice(0, 57)}…` : one;
}

/** What the assistant is told: the update, the thing it's about as Brainstead knows it, and how to
 *  apply it. The tools it names are the MCP server's (src-tauri/mcp/src/lib.rs). */
export function updatePrompt(t: UpdateTarget, update: string): string {
  const about: string[] = [];
  if (t.kind === "inbox") {
    const i = t.item;
    if (i.kind === "action" && i.action) {
      const a = i.action;
      about.push(
        `An action on the initiative "${a.initiative}" (its page ${i.path}), waiting in the Inbox: "${i.text.trim()}".`,
        a.owner === "delegated"
          ? `It's delegated to ${a.person ?? "someone"}${a.since ? ` since ${a.since}` : ""}.`
          : "It's the user's own.",
        a.due ? `Due ${a.due}.` : "",
        a.quotes.length ? `Where it came from: ${a.quotes.slice(0, 3).join("; ")}` : "",
        `Its Inbox id is ${inboxKey(i)}.`,
      );
    } else if (i.kind === "capture")
      about.push(`A capture waiting in the Inbox: ${i.path} (${i.lineText}). Its Inbox id is ${inboxKey(i)}.`);
    else
      about.push(
        `${i.kind === "thought" ? "A Scratchpad note" : "A task"} waiting in the Inbox, in ${i.path}: "${i.text.trim()}".`,
        `Its Inbox id is ${inboxKey(i)}.`,
      );
  } else if (t.kind === "task") {
    const k = t.task;
    about.push(
      `A task in ${k.path}, line ${k.line + 1}: ${k.lineText.trim()}`,
      k.project ? `Its project: ${pageName(k.project)} (${k.project}).` : "",
      k.tags.includes("waiting-for") ? "It's a waiting-for." : "",
    );
  } else {
    const p = t.project;
    about.push(
      `The project "${p.name}" (${p.path}), ${p.status}${p.area ? `, in ${p.area}` : ""}.`,
      p.outcome ? `Its outcome: ${p.outcome}` : "",
      `It has ${p.next} next actions and ${p.waiting} waiting-fors.`,
    );
  }
  return [
    `The user has an update: "${update.trim()}"`,
    "",
    "It's about this:",
    ...about.filter(Boolean).map((l) => `- ${l}`),
    "",
    "Apply the update to the vault with Brainstead's tools, as the user would by hand, then stop. Read what you need first (list_inbox, list_initiatives, list_tasks, list_projects, read_section).",
    "- An action or task the update settles: mark it done or dropped (clarify_inbox for an Inbox item, set_action for an initiative's action, edit_task for a task).",
    "- One it changes: its wording, date, owner or who it waits for (set_action, edit_task, update_project).",
    "- A new step it implies, such as something to chase: add it (create_task, a #waiting-for task for someone else's), in the same project.",
    "- Record what happened, dated today, on the initiative's or project's note where such notes are kept, in a line or two.",
    "Change only what the update implies, and nothing in the wiki unless the update is a fact the wiki records. If it's unclear what it means, change nothing and say what you'd need to know.",
    "End with one line starting `Done:` saying what you changed, in under fifteen words, or `Not changed:` and why.",
  ].join("\n");
}

/** The last line of the answer that starts Done: or Not changed:, for the toast. */
export function outcomeLine(answer: string): { text: string; changed: boolean } | null {
  const lines = answer
    .split("\n")
    .map((l) => l.replace(/\*\*|__|`/g, "").replace(/^[*_\s-]+|[*_\s]+$/g, ""))
    .filter(Boolean);
  for (let i = lines.length - 1; i >= 0; i--) {
    const m = /^(Done|Not changed):\s*(.+)$/i.exec(lines[i]);
    if (m) return { text: m[2].trim(), changed: m[1].toLowerCase() === "done" };
  }
  return null;
}

/** Starts an update in the background; returns its chat's id. */
export function startUpdate(t: UpdateTarget, update: string): string {
  const name = targetName(t);
  const model = settings.get().askModel ?? DEFAULT_MODEL;
  const id = backgroundChat(`Update: ${name}`, updatePrompt(t, update), model, "update");
  const open = () => {
    selectChat(id);
    nav.go("ask");
  };
  const run: Running = {
    key: `update:${id}`,
    label: `Updating ${name}`,
    short: "Update",
    doing: `“${update.trim()}”`,
    progress: null,
    stop: async () => stop(id),
    place: { screen: "ask" },
    end: () => {
      const c = ask.get().chats.find((x) => x.id === id);
      const last = [...(c?.transcript ?? [])].reverse().find((x) => x.role === "assistant" || x.role === "error");
      if (!last || last.role === "error") return { text: `Couldn't update ${name}`, failed: true, place: { screen: "ask" }, open };
      const o = outcomeLine(last.text);
      return {
        text: o ? `${o.changed ? "Updated" : "Not updated"}: ${name}: ${o.text}` : `Updated ${name}`,
        failed: false,
        place: { screen: "ask" },
        open,
      };
    },
  };
  windowRuns.set([...windowRuns.get(), run]);
  // It's done when its chat stops answering.
  const off = ask.subscribe(() => {
    const c = ask.get().chats.find((x) => x.id === id);
    if (c && c.busy) return;
    off();
    windowRuns.set(windowRuns.get().filter((r) => r.key !== run.key));
  });
  return id;
}
