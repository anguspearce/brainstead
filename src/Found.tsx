// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Find tasks and projects (decided 2026-10-05): a run over your notes from the last 90 days
// (src-tauri/src/find.rs) suggests tasks, shown at the top of Tasks, and projects, at the top of
// Projects, as the one suggestion component (src/Suggestions.tsx): a chip that opens to the list.
// Each rests on a quote from a note; Accept makes it (revertable in Changes), Edit changes it
// first, Skip drops it. Neither comes back in a later run.

import { useEffect, useState } from "react";
import { api, FindState, FindSuggestion } from "./api";
import { nav, openDoc } from "./nav";
import { Store, useStore } from "./store";
import { reportEditError, undoLast } from "./taskModel";
import { toast } from "./Toast";
import { iconizeEmoji } from "./md/fieldEmoji";
import { SuggestButton, SuggestChip, SuggestionRow, useSuggestFold } from "./Suggestions";

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

/** The run's state in a line, at the top of the open list: when it last looked, or why it failed. */
function RunState({ s }: { s: FindState | null }) {
  const r = s?.run;
  if (!r) return <span className="faint small">Suggest reads your notes from the last 90 days.</span>;
  if (r.status === "running")
    return (
      <span className="faint small">
        Reading {r.notes} notes{r.batches > 1 ? `, part ${Math.min(r.done + 1, r.batches)} of ${r.batches}` : ""}…
      </span>
    );
  const when = new Date(r.finishedAt ?? r.startedAt).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
  return (
    <span className={`small ${r.status === "error" ? "err" : "faint"}`}>
      {r.status === "error"
        ? `The last look failed: ${r.error ?? "it stopped"}`
        : r.status === "stopped"
          ? "Stopped"
          : `Last looked ${when}`}
    </span>
  );
}

/** The chip and Suggest (again), with Stop while it runs: in Tasks' filter bar, and Projects' line. */
export function FoundBar({ kind }: { kind: "task" | "project" }) {
  useEffect(startFind, []);
  const s = useStore(found);
  const [open, setOpen] = useSuggestFold(kind === "task" ? "tasks" : "projects");
  const n = (s?.suggestions ?? []).filter((g) => g.kind === kind).length;
  const running = s?.run?.status === "running";
  const what = kind === "task" ? "tasks you haven't written down" : "projects your notes keep coming back to";
  return (
    <span className="suggbar">
      <SuggestChip n={n} busy={running} open={open} onToggle={() => setOpen(!open)} />
      {running ? (
        <button type="button" className="btn sm ghost" title="Stop reading your notes" onClick={() => void api.findStop()}>
          Stop
        </button>
      ) : (
        <SuggestButton
          again={!!s?.run}
          title={`Have the AI read your notes from the last 90 days and suggest ${what}; nothing changes until you accept one`}
          onClick={() => void findRun()}
        />
      )}
    </span>
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

/** The open list of suggested tasks (under Tasks' filters) or projects (under Projects' line):
 *  nothing while it's folded. */
export function FoundList({ kind }: { kind: "task" | "project" }) {
  useEffect(startFind, []);
  const s = useStore(found);
  const [open] = useSuggestFold(kind === "task" ? "tasks" : "projects");
  const rows = (s?.suggestions ?? []).filter((g) => g.kind === kind);
  if (!open || !rows.length) return null;
  return (
    <section className="sugglist" aria-label="Suggestions">
      <div className="sugghead">
        <RunState s={s} />
      </div>
      {rows.map((g) => (kind === "task" ? <FoundTask key={g.id} g={g} /> : <FoundProject key={g.id} g={g} />))}
    </section>
  );
}

const SKIP = "Leave it out; it won't be suggested again";

function FoundTask({ g }: { g: FindSuggestion }) {
  const [busy, decide] = useDecide(g);
  return (
    <SuggestionRow
      id={g.id}
      busy={busy}
      text={iconizeEmoji(g.text)}
      detail={`${g.project ? `${g.project} · ` : "To Do list · "}${g.waiting ? "waiting for · " : ""}${g.due ? `due ${g.due} · ` : ""}`}
      source={<Sources g={g} />}
      accept={{
        title: `Add it${g.project ? ` to ${g.project}'s Next actions` : " to the To Do list"}; revertable in Changes`,
        run: (e) => void decide("accept", e && { text: e.text }),
      }}
      edit={{ title: "Change the wording before adding it", fields: [{ key: "text", label: "The task", value: g.text }] }}
      skip={{ title: SKIP, run: () => void decide("skip") }}
    />
  );
}

function FoundProject({ g }: { g: FindSuggestion }) {
  const [busy, decide] = useDecide(g);
  return (
    <SuggestionRow
      id={g.id}
      busy={busy}
      text={
        <>
          {iconizeEmoji(g.text)}
          {g.outcome && <div className="small">{g.outcome}</div>}
        </>
      }
      detail={g.tasks.length > 0 ? `First: ${g.tasks.join(" · ")} · ` : ""}
      source={<Sources g={g} />}
      accept={{
        title: "Make the project note with what done looks like and its first next actions; revertable in Changes",
        run: (e) => void decide("accept", e && { text: e.text, outcome: e.outcome }),
      }}
      edit={{
        title: "Change the name or what done looks like before making it",
        fields: [
          { key: "text", label: "The project's name", value: g.text },
          { key: "outcome", label: "What done looks like", value: g.outcome ?? "" },
        ],
      }}
      skip={{ title: SKIP, run: () => void decide("skip") }}
    />
  );
}
