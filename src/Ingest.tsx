// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Ingest runs (stage 7b): started from the Sources screen or a note's menu, one source at a time
// in the background. Each run shows its steps as they go (read, correct names, find the pages,
// draft, check the quotes, make the changes), can be stopped, and ends with its changes made and
// listed in Changes (a run from the daily check holds the ones that fail a check).

import { useEffect, useMemo, useRef, useState } from "react";
import { api, ChangeRow, IngestRun } from "./api";
import { Icon } from "./icons";
import { changes, pageName, startKnowledge } from "./knowledge";
import { nav, useViewState } from "./nav";
import { Store, useStore } from "./store";
import { Dialog, SearchBox, useDebounced } from "./ui";
import { reportEditError } from "./taskModel";
import { toast } from "./Toast";
import { DocButton, useIsGone } from "./docButton";
import { ingestRunEnded, meetingRunEnded } from "./meetingFlow";

export const runs = new Store<IngestRun[] | null>(null);
let started = false;

/** Keeps the runs list current: loaded once, then updated from each run's events. */
export function startRuns() {
  if (started) return;
  started = true;
  void api
    .ingestRuns()
    .then((r) => runs.set(r))
    .catch(() => runs.set([]));
  api
    .onIngestChanged((r) => {
      const was = (runs.get() ?? []).find((x) => x.id === r.id);
      runs.set(merge(runs.get() ?? [], r));
      const ended = !isActive(r) && (!was || isActive(was));
      if (!ended) return;
      // A meeting note made: ingested and its transcript trashed (src/meetingFlow.ts), or with
      // both off, offered. A note held in Changes is offered when it's accepted there instead.
      if (r.kind === "meeting" && r.note) {
        const made = r.status === "done" && meetingNoteMade(r);
        if (!meetingRunEnded(r, made) && made)
          offerFollowUp({ note: `${r.note.type.trim()}. ${r.note.name.trim()} - ${r.note.date.trim()}.md`, transcript: r.source });
      } else ingestRunEnded(r);
    })
    .catch(() => {});
}

/** The list with `r` in place of its old copy, or at the top when it's new. */
export function merge(list: IngestRun[], r: IngestRun): IngestRun[] {
  const i = list.findIndex((x) => x.id === r.id);
  if (i < 0) return [r, ...list];
  const n = list.slice();
  n[i] = r;
  return n;
}

/** Whether a finished meeting run made its note, rather than holding it in Changes (its last step says which). */
export function meetingNoteMade(r: IngestRun): boolean {
  const last = r.steps[r.steps.length - 1];
  return !!last && last.status === "done" && !/held in changes/i.test(last.detail ?? "");
}

export const isActive = (r: IngestRun) => r.status === "running" || r.status === "queued";

/** Where a transcript's meeting note is, from the runs (the Ingest runs pane's): being written,
 *  its note being ingested, failed at either, or held in Changes; null when nothing is under way or
 *  it all went through (the transcript then leaves for the Trash). `busy` while it's under way. */
export function meetingProgress(all: IngestRun[], transcript: string): { label: string; busy: boolean; why?: string } | null {
  // The newest meeting run for it (the list is newest first).
  const m = all.find((r) => r.kind === "meeting" && r.source === transcript);
  if (!m) return null;
  if (isActive(m)) return { label: m.status === "queued" ? "Waiting to write its note…" : "Writing note…", busy: true };
  if (m.status !== "done")
    return { label: m.status === "stopped" ? "Note stopped" : "Note failed", busy: false, why: m.error ?? undefined };
  if (!meetingNoteMade(m)) return { label: "Note held in Changes", busy: false, why: "Accept it in Changes to go on" };
  // The note's own ingest, started after it's made, by the note's file name.
  const note = m.note ? `${m.note.type.trim()}. ${m.note.name.trim()} - ${m.note.date.trim()}.md` : null;
  const ing = note ? all.find((r) => r.kind !== "meeting" && r.source.split("/").pop() === note && r.started >= m.started) : undefined;
  if (!ing) return null;
  if (isActive(ing)) return { label: "Ingesting…", busy: true };
  if (ing.status !== "done")
    return { label: ing.status === "stopped" ? "Ingest stopped" : "Ingest failed", busy: false, why: ing.error ?? undefined };
  return null;
}

/** Starts ingesting these files; says so. */
export function ingest(paths: string[]) {
  if (!paths.length) return toast("There's nothing here to ingest", undefined, "bad");
  startRuns();
  api
    .ingestStart(paths)
    .then((ids) =>
      toast(ids.length === 1 ? `Ingesting ${pageName(paths[0])}` : `Ingesting ${ids.length} sources, one after another`, {
        label: "Sources",
        run: () => nav.go("sources"),
      }),
    )
    .catch(reportEditError);
}

const STEP_ICON: Record<string, string> = { done: "check", failed: "x", skipped: "minus", running: "refresh", waiting: "" };

/** A finished run's state as its card shows it: Done, Failed, Stopped. */
const runLabel = (r: IngestRun) =>
  r.status === "done"
    ? "Done"
    : r.status === "failed"
      ? "Failed"
      : r.status === "stopped"
        ? "Stopped"
        : r.status === "queued"
          ? "Waiting"
          : "Running";

/** Has every word of `q` in what its card shows: the source (or meeting note), state, model and
 *  day. The pane's search box and run_status' query both match on this. */
export function runMatches(r: IngestRun, q: string): boolean {
  const text = [pageName(r.source), r.source, r.note?.name, runLabel(r), r.model, r.started.slice(0, 10)]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => text.includes(w));
}

export function RunCard({ r, on }: { r: IngestRun; on?: boolean }) {
  const gone = useIsGone(r.source);
  useEffect(startKnowledge, []);
  // The run's changes: made, or held for you.
  const rows = useStore(changes);
  const ids = new Set(r.proposals);
  const mine = rows?.filter((c) => ids.has(c.id)) ?? [];
  const queued = mine.filter((c) => c.status === "held").length;
  return (
    <div className={`card run ${r.status} ${on ? "on" : ""}`} data-run={r.id}>
      <div className="row">
        <span className="lb source" />
        <DocButton path={r.source} gone={gone} className="blink grow ell">
          {pageName(r.source)}
        </DocButton>
        {isActive(r) ? (
          <button
            type="button"
            className="btn sm"
            title="Stop this ingest; nothing more is changed from it"
            onClick={() => void api.ingestStop(r.id)}
          >
            <Icon name="stop" size={12} />
            Stop
          </button>
        ) : (
          <span className={`chip sm ${r.status === "done" ? "green" : r.status === "failed" ? "amber" : ""}`}>{runLabel(r)}</span>
        )}
      </div>
      <div className="runsteps">
        {r.steps.map((s) => (
          <div key={s.name} className={`runstep ${s.status}`}>
            <span className="sd">
              {s.status === "running" ? (
                <span className="spin" />
              ) : STEP_ICON[s.status] ? (
                <Icon name={STEP_ICON[s.status]} size={11} />
              ) : null}
            </span>
            <span className="grow">
              <span className="nm">{s.name}</span>
              {s.detail && <span className="faint small">{s.detail}</span>}
            </span>
            {s.ms > 0 && <span className="faint mono small">{s.ms < 1000 ? `${s.ms} ms` : `${(s.ms / 1000).toFixed(1)} s`}</span>}
          </div>
        ))}
      </div>
      {r.dropped.length > 0 && (
        <details className="faint small">
          <summary>{r.dropped.length} left out by the checks</summary>
          {r.dropped.map((d) => (
            <div key={d.page}>
              {pageName(d.page)}: {d.reason}
            </div>
          ))}
        </details>
      )}
      {r.status === "done" && mine.length > 0 && (
        <button
          type="button"
          className="btn sm"
          title={
            queued
              ? "Open Changes to accept or reject the changes held from this run"
              : "Open Changes to see what this run changed, with Revert"
          }
          onClick={() => showRunInChanges(mine)}
        >
          <Icon name="review" size={12} />
          {queued ? `Review ${queued} held change${queued === 1 ? "" : "s"}` : `See ${mine.length} change${mine.length === 1 ? "" : "s"}`}
        </button>
      )}
      <span className="faint small">
        {r.model.replace(/^[a-z]+:/, "")} · {r.started.slice(0, 16).replace("T", " ")}
      </span>
    </div>
  );
}

/** Changes, with the run's first change chosen and scrolled to: its first held one (held changes
 *  are listed first), else its newest, as Changes lists them. The search is cleared so it shows. */
function showRunInChanges(mine: ChangeRow[]) {
  const first = mine.find((c) => c.status === "held") ?? mine[0];
  nav.go({ screen: "review", view: { "review:sel": first.id, "review:q": "" } });
}

/** Runs drawn at a time in the pane; more as its end comes into view. */
const RUNS_PAGE = 12;

/** The side pane: the runs going now, then the last ones, RUNS_PAGE at a time. */
export function RunsPane() {
  useEffect(startRuns, []);
  const stored = useStore(runs);
  const all = useMemo(() => stored ?? [], [stored]);
  // The search narrows the runs; ↑↓ in the box step through them, Esc or ✕ clears it.
  const [q, setQ] = useViewState("runs:q", "");
  const dq = useDebounced(q).trim();
  const list = useMemo(() => (dq ? all.filter((r) => runMatches(r, dq)) : all), [all, dq]);
  const [active, setActive] = useState(0);
  useEffect(() => setActive(0), [dq]);
  const idx = Math.min(active, Math.max(0, list.length - 1));
  const step = (to: number) => {
    setActive(to);
    setShown((n) => Math.max(n, to + 1));
    requestAnimationFrame(() =>
      document.querySelector<HTMLElement>(`.runs [data-run="${CSS.escape(list[to].id)}"]`)?.scrollIntoView({ block: "nearest" }),
    );
  };
  const key = (e: React.KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "Escape") {
      if (q) setQ("");
      else return;
    } else if (!dq || !list.length) return;
    else if (e.key === "ArrowDown") step((idx + 1) % list.length);
    else if (e.key === "ArrowUp") step((idx - 1 + list.length) % list.length);
    else return;
    e.preventDefault();
  };
  const [shown, setShown] = useState(RUNS_PAGE);
  const end = useRef<HTMLDivElement>(null);
  const more = shown < list.length;
  useEffect(() => {
    const el = end.current;
    if (!el || !more) return;
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && setShown((n) => n + RUNS_PAGE), {
      rootMargin: "300px",
    });
    io.observe(el);
    return () => io.disconnect();
  }, [more, shown]);
  return (
    <aside className="runs">
      <div className="runshead">
        <div className="eyebrow">Ingest runs</div>
        {all.length > 0 && (
          <SearchBox value={q} onChange={setQ} onKeyDown={key} placeholder="Search runs" className="runsq">
            {dq && (
              <span className="faint nav">
                {list.length ? `${idx + 1}/${list.length}` : "0"}
                <span className="kbd">↑↓</span>
              </span>
            )}
          </SearchBox>
        )}
      </div>
      {!all.length && <p className="faint small">Ingest a source to see its run here: each step, then its changes in Changes.</p>}
      {all.length > 0 && !list.length && <p className="faint small">No runs match “{dq}”.</p>}
      {list.slice(0, shown).map((r, i) => (
        <RunCard key={r.id} r={r} on={!!dq && i === idx} />
      ))}
      {more && (
        <div ref={end} className="faint small">
          Loading older runs…
        </div>
      )}
    </aside>
  );
}

export interface FollowUpOffer {
  note: string;
  transcript: string;
}

/** After a meeting note is made: what's offered next, one note at a time, oldest first. */
export const followUp = new Store<FollowUpOffer[]>([]);

/** Queues the offer for a note just made; a note already waiting isn't offered twice. */
export function offerFollowUp(f: FollowUpOffer) {
  followUp.set(queueOffer(followUp.get(), f));
}

/** The queue with `f` at the end, unless its note is already in it. */
export const queueOffer = (q: FollowUpOffer[], f: FollowUpOffer): FollowUpOffer[] => (q.some((x) => x.note === f.note) ? q : [...q, f]);

export function FollowUp() {
  useEffect(startRuns, []);
  const q = useStore(followUp);
  const f = q[0];
  if (!f) return null;
  // Keyed by the note, so each note's dialog starts with both boxes ticked.
  return <FollowUpDialog key={f.note} f={f} />;
}

function FollowUpDialog({ f }: { f: FollowUpOffer }) {
  const [ing, setIng] = useState(true);
  const [trash, setTrash] = useState(true);
  const close = () => followUp.set(followUp.get().filter((x) => x.note !== f.note));
  const go = async () => {
    close();
    if (ing) ingest([f.note]);
    if (trash)
      await api
        .trashMove(f.transcript)
        .then(() => toast("Transcript moved to the Trash", { label: "Open Trash", run: () => nav.go("trash") }))
        .catch(reportEditError);
  };
  return (
    <Dialog onClose={close} width={460} label="After the note">
      <div className="confirm">
        <h2 className="h2">{pageName(f.note)} is in the vault</h2>
        <label className="row">
          <input type="checkbox" checked={ing} onChange={() => setIng(!ing)} />
          <span>Ingest it into the wiki (its changes show in Changes)</span>
        </label>
        <label className="row">
          <input type="checkbox" checked={trash} onChange={() => setTrash(!trash)} />
          <span>Move the transcript to the Trash ({pageName(f.transcript)})</span>
        </label>
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" title="Close without ingesting the note or trashing the transcript" onClick={close}>
            Neither
          </button>
          <span title={!ing && !trash ? "Tick at least one of the two above" : "Do what's ticked above"}>
            <button type="button" className="btn lg pri" disabled={!ing && !trash} onClick={() => void go()}>
              Go
            </button>
          </span>
        </div>
      </div>
    </Dialog>
  );
}
