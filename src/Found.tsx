// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Find tasks and projects (decided 2026-10-05): a run over your notes from the last 90 days
// (src-tauri/src/find.rs) suggests tasks, shown at the top of Tasks, and projects, at the top of
// Projects. Each rests on a quote from a note; Accept makes it (revertable in Changes), Edit
// changes it first, Skip drops it. Neither comes back in a later run.

import { useEffect, useState } from "react";
import { api, FindState, FindSuggestion } from "./api";
import { Icon } from "./icons";
import { nav, openDoc } from "./nav";
import { Store, useStore } from "./store";
import { reportEditError, undoLast } from "./taskModel";
import { toast } from "./Toast";
import { iconizeEmoji } from "./md/fieldEmoji";

export const found = new Store<FindState | null>(null);
let started = false;

export function startFind() {
  if (started) return;
  started = true;
  const load = () =>
    void api
      .findStatus()
      .then((s) => found.set(s))
      .catch(() => {});
  load();
  api.onFindChanged(load).catch(() => {});
}

/** Starts a scan, with a toast. */
export function findRun() {
  return api
    .findRun()
    .then(() => toast("Reading your notes from the last 90 days for tasks and projects…"))
    .catch((e) => toast(String((e as { message?: string }).message ?? e), undefined, "bad"));
}

const noteName = (path: string) => path.replace(/\.md$/, "").split("/").pop() ?? path;

/** The run's state in a line: running with its progress, or when it last ran. */
function RunState({ s }: { s: FindState | null }) {
  const r = s?.run;
  if (!r) return <span className="faint small">Reads your notes from the last 90 days.</span>;
  if (r.status === "running")
    return (
      <span className="faint small row">
        <span className="spin" />
        Reading {r.notes} notes{r.batches > 1 ? `, part ${Math.min(r.done + 1, r.batches)} of ${r.batches}` : ""}…
        <button type="button" className="btn sm ghost" title="Stop looking" onClick={() => void api.findStop()}>
          Stop
        </button>
      </span>
    );
  const when = new Date(r.finishedAt ?? r.startedAt).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
  return (
    <span className="faint small">
      {r.status === "error"
        ? `The last look failed: ${r.error ?? "it stopped"}`
        : r.status === "stopped"
          ? "Stopped"
          : `Last looked ${when}`}
    </span>
  );
}

function FindButton({ s, what }: { s: FindState | null; what: string }) {
  const running = s?.run?.status === "running";
  return (
    <button
      type="button"
      className="btn sm"
      disabled={running}
      title={`Have the AI read your notes from the last 90 days and suggest ${what}; nothing changes until you accept one`}
      onClick={() => void findRun()}
    >
      <Icon name="search" size={12} />
      {s?.run ? "Look again" : "Find tasks and projects"}
    </button>
  );
}

function Sources({ g }: { g: FindSuggestion }) {
  return (
    <>
      {g.sources.map((src) => (
        <button
          key={src.path + src.quote}
          type="button"
          className="tlink small"
          title={`“${src.quote}” (open ${noteName(src.path)})`}
          onClick={() => openDoc(src.path)}
        >
          {noteName(src.path)}
        </button>
      ))}
    </>
  );
}

/** Suggestions being accepted or skipped: a second click, or Enter and a click, does nothing. */
const deciding = new Set<string>();

/** Accept (or skip) with a toast offering Undo and the way to Changes; `busy` is true meanwhile. */
function useDecide(g: FindSuggestion) {
  const [busy, setBusy] = useState(false);
  const decide = (action: "accept" | "skip", edit?: Record<string, unknown>) => {
    if (deciding.has(g.id)) return;
    deciding.add(g.id);
    setBusy(true);
    return api
      .findDecide(g.id, action, edit ?? null)
      .then((m) =>
        action === "accept"
          ? toast((m ?? "Made it").replace(/ Revert it in Changes\.$/, ""), { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok")
          : toast("Skipped: it won't be suggested again", { label: "Changes", run: () => nav.go("review") }),
      )
      .catch(reportEditError)
      .finally(() => {
        deciding.delete(g.id);
        setBusy(false);
      });
  };
  return [busy, decide] as const;
}

/** Suggested tasks, at the top of Tasks. */
export function FoundTasks() {
  useEffect(startFind, []);
  const s = useStore(found);
  const rows = (s?.suggestions ?? []).filter((g) => g.kind === "task");
  return (
    <section className="found">
      <div className="foundhead">
        <Icon name="zap" size={14} />
        <span className="pt grow">{rows.length ? `${rows.length} suggested from your notes` : "Tasks in your notes"}</span>
        <RunState s={s} />
        <FindButton s={s} what="tasks you haven't written down" />
      </div>
      {rows.map((g) => (
        <FoundRow key={g.id} g={g} />
      ))}
    </section>
  );
}

function FoundRow({ g }: { g: FindSuggestion }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(g.text);
  const [busy, decide] = useDecide(g);
  return (
    <div className="foundrow">
      <div className="t">
        {editing ? (
          <label className="inp">
            <input
              aria-label="The task"
              value={text}
              autoFocus
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void decide("accept", { text });
                if (e.key === "Escape") setEditing(false);
              }}
            />
          </label>
        ) : (
          <div className="pt">{iconizeEmoji(g.text)}</div>
        )}
        <div className="faint small">
          {g.project ? `${g.project} · ` : "To Do list · "}
          {g.waiting ? "waiting for · " : ""}
          {g.due ? `due ${g.due} · ` : ""}
          <Sources g={g} />
        </div>
      </div>
      <button
        type="button"
        className="btn sm pri"
        disabled={busy}
        title={`Add it${g.project ? ` to ${g.project}'s Next actions` : " to the To Do list"}; revertable in Changes`}
        onClick={() => void decide("accept", editing ? { text } : undefined)}
      >
        Accept
      </button>
      {!editing && (
        <button type="button" className="btn sm" title="Change the wording before adding it" onClick={() => setEditing(true)}>
          Edit
        </button>
      )}
      <button
        type="button"
        className="btn sm ghost"
        disabled={busy}
        title="Leave it out; it won't be suggested again"
        onClick={() => void decide("skip")}
      >
        Skip
      </button>
    </div>
  );
}

/** Suggested projects, at the top of Projects. */
export function FoundProjects() {
  useEffect(startFind, []);
  const s = useStore(found);
  const rows = (s?.suggestions ?? []).filter((g) => g.kind === "project");
  return (
    <section className="found">
      <div className="foundhead">
        <Icon name="zap" size={14} />
        <span className="pt grow">{rows.length ? `${rows.length} suggested from your notes` : "Projects in your notes"}</span>
        <FindButton s={s} what="projects your notes keep coming back to" />
      </div>
      {rows.length === 0 && (
        <div className="foundrow">
          <RunState s={s} />
        </div>
      )}
      {rows.map((g) => (
        <FoundProject key={g.id} g={g} />
      ))}
    </section>
  );
}

function FoundProject({ g }: { g: FindSuggestion }) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(g.text);
  const [outcome, setOutcome] = useState(g.outcome ?? "");
  const [busy, decide] = useDecide(g);
  return (
    <div className="foundrow">
      <div className="t">
        {editing ? (
          <>
            <label className="inp">
              <input aria-label="The project's name" value={name} autoFocus onChange={(e) => setName(e.target.value)} />
            </label>
            <label className="inp">
              <input
                aria-label="What done looks like"
                placeholder="What done looks like"
                value={outcome}
                onChange={(e) => setOutcome(e.target.value)}
              />
            </label>
          </>
        ) : (
          <>
            <div className="pt">{iconizeEmoji(g.text)}</div>
            {g.outcome && <div className="small">{g.outcome}</div>}
          </>
        )}
        {g.tasks.length > 0 && <div className="faint small">First: {g.tasks.join(" · ")}</div>}
        <div className="faint small">
          <Sources g={g} />
        </div>
      </div>
      <button
        type="button"
        className="btn sm pri"
        disabled={busy}
        title="Make the project note with what done looks like and its first next actions; revertable in Changes"
        onClick={() => void decide("accept", editing ? { text: name, outcome } : undefined)}
      >
        Accept
      </button>
      {!editing && (
        <button
          type="button"
          className="btn sm"
          title="Change the name or what done looks like before making it"
          onClick={() => setEditing(true)}
        >
          Edit
        </button>
      )}
      <button
        type="button"
        className="btn sm ghost"
        disabled={busy}
        title="Leave it out; it won't be suggested again"
        onClick={() => void decide("skip")}
      >
        Skip
      </button>
    </div>
  );
}
