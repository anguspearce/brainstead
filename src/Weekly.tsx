// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The guided weekly review (stage 6): GTD's Get clear, Get current and Get creative, step by step
// over the real lists, in the previous app's order. What you do on the way is logged; Finish writes
// it and your notes into the week's own note, Me. Weekly Review - YYYY-Www (src-tauri/src/weekly.rs),
// which the scheduled weekly summary reads. Progress is kept by Rust, so Pause and come back.

import { health, startKnowledge } from "./knowledge";
import { useEffect, useMemo, useState } from "react";
import { api, Weekday, WeeklyContext, WeeklyState, WeekPrepJob, WeekPrepStatus } from "./api";
import { askWith } from "./askState";
import { CaptureBox, prepareCapture } from "./Capture";
import { projectFlag, unclarified, useInbox, useProjects } from "./gtd";
import { Icon } from "./icons";
import { Markdown } from "./md/Markdown";
import { localToday } from "./md/taskQuery";
import { nav, openDoc, useViewState } from "./nav";
import { samplePrep, scenePrep } from "./scene";
import { settings, useStore } from "./store";
import { TaskList, TasksFailed } from "./TaskList";
import { reportEditError, tasksFailed, useAllTasks, viewRows, VIEWS } from "./taskModel";
import { toast } from "./Toast";
import { TopBar } from "./TopBar";
import { ago, Dialog } from "./ui";
import { PrepHeader, PrepRows, prepStep, WEEKLY_STATE_CHANGED, when } from "./weeklyPrep";
import { keyLabel } from "./platform";

/** The ISO week a local date is in: "2026-W40". */
export function isoWeek(day: string): string {
  const d = new Date(Date.UTC(+day.slice(0, 4), +day.slice(5, 7) - 1, +day.slice(8, 10)));
  const dow = d.getUTCDay() || 7;
  d.setUTCDate(d.getUTCDate() + 4 - dow);
  const y = d.getUTCFullYear();
  const w = Math.ceil(((d.getTime() - Date.UTC(y, 0, 1)) / 86400000 + 1) / 7);
  return `${y}-W${String(w).padStart(2, "0")}`;
}

/** The week a review done on `today` is for, by the weekly summary's rule
 *  (brainstead_core::reviews::schedule::weekly_target): on a Monday or Tuesday the week just
 *  ended, otherwise this one. */
export function reviewWeek(today: string): string {
  const d = new Date(`${today}T12:00:00`);
  if (d.getDay() === 1 || d.getDay() === 2) d.setDate(d.getDate() - 7);
  return isoWeek(localToday(d));
}

/** A paused review carries on with its own week until the next week's review is due: it's kept
 *  while its week is no older than the one a review a week ago would have been for. */
/** Whether a review has anything Start over would lose: a step on, a step done, a decision, notes or a suggestion handled. */
export function hasProgress(st: WeeklyState): boolean {
  return st.step > 0 || st.done.length > 0 || st.log.length > 0 || !!st.notes.trim() || Object.keys(st.handled ?? {}).length > 0;
}

export function keepsPaused(saved: WeeklyState | null, today: string): saved is WeeklyState {
  if (!saved) return false;
  const d = new Date(`${today}T12:00:00`);
  d.setDate(d.getDate() - 7);
  return saved.week >= reviewWeek(localToday(d));
}

export const STEPS: { phase: string; id: string; title: string; ask: string }[] = [
  {
    phase: "Get clear",
    id: "loose",
    title: "Collect loose ends",
    ask: "Anything on your mind, in your bag, inbox or on paper that isn't captured yet?",
  },
  {
    phase: "Get clear",
    id: "inbox",
    title: "Empty the inbox",
    ask: "Clarify everything you've captured into tasks, projects and reference.",
  },
  {
    phase: "Get clear",
    id: "notes",
    title: "Process this week's notes",
    ask: "Anything in this week's notes that needs a task, a follow-up or a link?",
  },
  {
    phase: "Get current",
    id: "next",
    title: "Next actions",
    ask: "Is everything on the list still right? Tick what's done, drop (right-click) what's not yours any more.",
  },
  {
    phase: "Get current",
    id: "back",
    title: "Look back: this week's meetings",
    ask: "Any commitments made in these meetings that aren't tasks yet?",
  },
  {
    phase: "Get current",
    id: "ahead",
    title: "Look ahead: check your calendar",
    ask: "Open your calendar for the next two weeks: anything to prepare for, book or ask about?",
  },
  { phase: "Get current", id: "projects", title: "Projects", ask: "Does every project have a next action?" },
  { phase: "Get current", id: "waiting", title: "Waiting for", ask: "Who needs a nudge?" },
  { phase: "Get creative", id: "someday", title: "Someday / maybe", ask: "Anything here you want to start now, or let go?" },
  {
    phase: "Get creative",
    id: "ghosts",
    title: "Links to pages not written yet",
    ask: "Links to pages that don't exist yet: write any that matter, or fix the link.",
  },
  { phase: "Get creative", id: "ideas", title: "New ideas", ask: "Any new projects or ideas worth catching?" },
];

/** What Finish writes into the week's review note, under its title and dates. */
export function reviewBody(s: WeeklyState): string {
  const lines = [`${s.done.length} of ${STEPS.length} steps done.`];
  if (s.log.length) lines.push("", ...s.log.map((l) => `- ${l}`));
  if (s.notes.trim()) lines.push("", s.notes.trim());
  return lines.join("\n");
}

/** A note's name without its folder or `.md`. */
const noteTitle = (path: string) => path.replace(/\.md$/, "").split("/").pop() ?? path;

export const fresh = (week: string): WeeklyState => ({ week, step: 0, done: [], log: [], notes: "", startedAt: Date.now() });

/** Goes into the review, past its start page: Start or Carry on, from Today and ⌘K. */
export const startWeekly = () => nav.go({ screen: "weekly", view: { "weekly:begun": true } });

const DAY_NAME: Record<Weekday, string> = {
  mon: "Monday",
  tue: "Tuesday",
  wed: "Wednesday",
  thu: "Thursday",
  fri: "Friday",
  sat: "Saturday",
  sun: "Sunday",
};

/** "Fridays at 16:00": when the review is scheduled (Settings › Jobs & schedule). */
export const scheduleLabel = (day: Weekday | undefined, time: string | undefined) =>
  `${DAY_NAME[day ?? "fri"]}s at ${/^\d\d:\d\d$/.test(time ?? "") ? time : "16:00"}`;

export function WeeklyScreen() {
  const today = localToday();
  const [st, setSt] = useState<WeeklyState | null>(null);
  // The review is shown once it's started or carried on; till then, its start page. Each visit
  // from the sidebar starts on the start page.
  const [begun, setBegun] = useViewState("weekly:begun", false, false);
  const [paused, setPaused] = useState(false);
  const [ctx, setCtx] = useState<WeeklyContext | null>(null);
  const [prep, setPrep] = useState<WeekPrepStatus | null>(null);
  const projects = useProjects();
  const root = useStore(settings).vaultPath ?? "";
  // A paused review carries on with its own week; a new one is for the week the summary's rule picks.
  useEffect(() => {
    void api
      .weeklyStateRead()
      .then((s) => {
        const keep = keepsPaused(s, today);
        setPaused(keep);
        setSt(keep ? s : fresh(reviewWeek(today)));
      })
      .catch(() => setSt(fresh(reviewWeek(today))));
  }, [today]);
  // An assistant accepted or skipped a suggestion (weekly_suggestion): read the progress again.
  useEffect(() => {
    const reload = () =>
      void api
        .weeklyStateRead()
        .then((s) => s && setSt(s))
        .catch(() => {});
    window.addEventListener(WEEKLY_STATE_CHANGED, reload);
    return () => window.removeEventListener(WEEKLY_STATE_CHANGED, reload);
  }, []);
  const week = st?.week ?? "";
  useEffect(() => {
    if (!week) return;
    void api
      .weeklyContext(week)
      .then(setCtx)
      .catch(() => {});
  }, [week]);
  // The week's prepared suggestions, reloaded as a run goes. Starting the review prepares them
  // when the week has none (no assistant: none, and the steps work as before); the start page
  // only shows how they stand.
  useEffect(() => {
    if (!week) return;
    // Screenshot mode: the scene's sample suggestions, nothing prepared.
    if (scenePrep.get()) {
      setPrep(samplePrep(week));
      return;
    }
    const load = () =>
      void api
        .weekprepStatus(week)
        .then(setPrep)
        .catch(() => {});
    const off = api.onWeekprepChanged(load).catch(() => () => {});
    if (begun)
      void api
        .weekprepEnsure(week)
        .catch(() => false)
        .then(load);
    else load();
    return () => void off.then((f) => f());
  }, [week, begun]);
  const save = (s: WeeklyState) => {
    setSt(s);
    void api.weeklyStateWrite(s).catch(() => {});
  };
  /** A suggestion accepted or skipped, kept with the progress; one accepted is logged. */
  const handle = (id: string, how: "accepted" | "skipped", line: string) =>
    setSt((cur) => {
      if (!cur) return cur;
      const next = { ...cur, handled: { ...cur.handled, [id]: how }, log: how === "accepted" && line ? [...cur.log, line] : cur.log };
      void api.weeklyStateWrite(next).catch(() => {});
      return next;
    });
  if (!st) return <main className="main" />;
  if (!begun)
    return (
      <StartPage
        st={st}
        paused={paused}
        ctx={ctx}
        prep={prep}
        onStart={(over) => {
          const s = over ? fresh(reviewWeek(today)) : paused ? st : fresh(st.week);
          if (over || !paused) save(s);
          setPaused(true);
          setBegun(true);
        }}
      />
    );
  const step = STEPS[st.step];
  const log = (line: string) => save({ ...st, log: [...st.log, line] });
  const go = (i: number) => save({ ...st, step: Math.max(0, Math.min(STEPS.length - 1, i)), done: [...new Set([...st.done, st.step])] });
  const finish = () =>
    api
      .weeklyFinish(week, reviewBody({ ...st, done: [...new Set([...st.done, st.step])] }))
      .then((file) => {
        void api.weeklyStateWrite(null);
        toast(`Saved ${noteTitle(file)} (${keyLabel("⌘Z")} undoes it)`, { label: "Open", run: () => openDoc(file) }, "ok");
        nav.go("today");
      })
      .catch(reportEditError);
  const left = STEPS.length - st.done.length;

  return (
    <main className="main">
      <TopBar
        title="Weekly review"
        sub={ctx ? `week ${week.slice(-2)} · ${ctx.range}` : week}
        ask={{ about: `this week's review: ${step.title.toLowerCase()}`, prompt: `For my weekly review, ${step.title.toLowerCase()}: ` }}
      >
        <span className="faint small">
          Step {st.step + 1} of {STEPS.length}
          {left > 0 ? ` · about ${left * 3} min left` : ""}
        </span>
        <span className="pbar">
          <i style={{ width: `${(st.done.length / STEPS.length) * 100}%` }} />
        </span>
        <button type="button" className="btn" onClick={() => nav.go("today")} title="Your progress is kept; come back to carry on">
          Pause
        </button>
      </TopBar>
      <div className="body weekly">
        <nav className="wsteps" aria-label="Steps">
          {STEPS.map((s, i) => (
            <div key={s.id}>
              {(i === 0 || STEPS[i - 1].phase !== s.phase) && <div className="wph">{s.phase}</div>}
              <button
                type="button"
                className={`wst ${i === st.step ? "now" : st.done.includes(i) ? "done" : ""}`}
                aria-current={i === st.step ? "step" : undefined}
                title={
                  i === st.step
                    ? `Step ${i + 1}: ${s.title}, where you are`
                    : `Go to step ${i + 1}: ${s.title}. Step ${st.step + 1}, where you are, is marked done`
                }
                onClick={() => go(i)}
              >
                <span className="sd">{st.done.includes(i) && i !== st.step ? <Icon name="check" size={11} /> : i + 1}</span>
                {s.title}
              </button>
            </div>
          ))}
        </nav>
        <div className="wmain">
          <div>
            <h1 className="h2">{step.title}</h1>
            <p className="muted">{step.ask}</p>
          </div>
          <PrepHeader status={prep} week={week} />
          {prep?.prep && prepStep(step.id) && (
            <PrepRows
              step={prepStep(step.id)!}
              rows={prep.prep.suggestions.filter((x) => x.step === prepStep(step.id))}
              handled={st.handled ?? {}}
              onHandle={handle}
              week={week}
              projects={(projects ?? []).filter((p) => p.status === "active")}
            />
          )}
          <Step id={step.id} ctx={ctx} log={log} />
          <label className="nnf">
            <span className="faint">Notes for this week's review (added when you finish)</span>
            <textarea className="wnotes" value={st.notes} onChange={(e) => save({ ...st, notes: e.target.value })} rows={3} />
          </label>
          <div className="row">
            {st.step > 0 && (
              <button
                type="button"
                className="btn"
                title={`Back to step ${st.step}: ${STEPS[st.step - 1].title}`}
                onClick={() => go(st.step - 1)}
              >
                Back
              </button>
            )}
            <span className="grow" />
            {st.step < STEPS.length - 1 ? (
              <button
                type="button"
                className="btn pri lg"
                title="Mark this step done and go on to the next"
                onClick={() => go(st.step + 1)}
              >
                Next: {STEPS[st.step + 1].title}
                <Icon name="forward" size={14} />
              </button>
            ) : (
              <button
                type="button"
                className="btn pri lg"
                title="Save this review, with your notes, in this week's own review note and go back to Today"
                onClick={() => void finish()}
              >
                <Icon name="check" size={14} />
                Finish and save
              </button>
            )}
          </div>
        </div>
        <aside className="wside">
          <span className="eyebrow">Your week, from the vault</span>
          {ctx?.block ? (
            <div className="wblock">
              <Markdown content={ctx.block.replace(/^## .*\n/, "")} path={ctx.file} root={root} />
            </div>
          ) : (
            <p className="faint small">This week's summary isn't written yet.</p>
          )}
          {ctx && (
            <dl className="kv small">
              <dt>Other notes</dt>
              <dd>{ctx.notes.length}</dd>
              <dt>Meetings</dt>
              <dd>{ctx.meetings.length}</dd>
            </dl>
          )}
          {ctx?.review ? (
            <p className="small">
              Your review:{" "}
              <button type="button" className="tlink" title={`Open ${noteTitle(ctx.review)}`} onClick={() => openDoc(ctx.review!)}>
                open
              </button>
              <span className="faint"> · finishing again rewrites it, keeping what you added under Added later</span>
            </p>
          ) : (
            <p className="faint small">Saved to Me. Weekly Review - {week} when you finish.</p>
          )}
        </aside>
      </div>
    </main>
  );
}

/** Before the review: the week it's for, when it's scheduled and its suggestions are prepared, and
 *  Start, or Carry on and Start over for a paused one. */
function StartPage({
  st,
  paused,
  ctx,
  prep,
  onStart,
}: {
  st: WeeklyState;
  paused: boolean;
  ctx: WeeklyContext | null;
  prep: WeekPrepStatus | null;
  onStart: (over: boolean) => void;
}) {
  const r = useStore(settings).weeklyReview;
  const [job, setJob] = useState<WeekPrepJob | null>(null);
  const [confirmOver, setConfirmOver] = useState(false);
  useEffect(() => {
    const load = () =>
      void api
        .weekprepJob()
        .then(setJob)
        .catch(() => {});
    load();
    const off = api.onWeekprepChanged(load).catch(() => () => {});
    return () => void off.then((f) => f());
  }, []);
  const week = st.week;
  const n = prep?.prep?.suggestions.filter((x) => !st.handled?.[x.id]).length ?? 0;
  const suggestions = prep?.running
    ? "Suggestions from your week are being prepared now."
    : prep?.prep
      ? `${n} suggestion${n === 1 ? "" : "s"} from your week ready, prepared ${when(prep.prep.preparedAt)}.`
      : prep?.error
        ? `The suggestions couldn't be prepared: ${prep.error} Starting tries again.`
        : job?.next
          ? `Suggestions from your week are prepared ${when(job.next)}, or when you start.`
          : "Suggestions from your week are prepared when you start, if an AI assistant is set up.";
  return (
    <main className="main">
      <TopBar title="Weekly review" sub={ctx ? `week ${week.slice(-2)} · ${ctx.range}` : week} />
      {confirmOver && (
        <Dialog onClose={() => setConfirmOver(false)} width={440} label="Start the review over">
          <div className="confirm">
            <h2 className="h2">Start the review over?</h2>
            <p className="muted">
              You&apos;re on step {st.step + 1} with {st.done.length} of {STEPS.length} done. Its progress, notes and decisions are dropped,
              and they can&apos;t be brought back. What you changed in your lists stays changed.
            </p>
            <div className="row">
              <span className="grow" />
              <button type="button" className="btn lg" title="Keep the paused review" onClick={() => setConfirmOver(false)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn lg danger-pri"
                title="Drop this review's progress and start again from step 1"
                onClick={() => {
                  setConfirmOver(false);
                  onStart(true);
                }}
              >
                Start over
              </button>
            </div>
          </div>
        </Dialog>
      )}
      <div className="body">
        <div className="wstart">
          <h1 className="h2">{paused ? "Your weekly review is paused" : `The review of week ${week.slice(-2)}`}</h1>
          <p className="muted">
            {STEPS.length} steps in three phases, Get clear, Get current and Get creative, over your real lists: about {STEPS.length * 3}{" "}
            minutes.
            {paused && ` You're on step ${st.step + 1}, ${STEPS[st.step].title}, with ${st.done.length} of ${STEPS.length} done.`}
          </p>
          <dl className="kv">
            <dt>Week</dt>
            <dd>{ctx ? `${week.slice(-2)} · ${ctx.range}` : week}</dd>
            <dt>Scheduled</dt>
            <dd>
              {scheduleLabel(r?.day, r?.time)}: Today shows it then, and you can start any time.{" "}
              <button
                type="button"
                className="tlink"
                title="Change the review's day and time in Settings › Jobs & schedule"
                onClick={() => nav.go({ screen: "settings", pane: "jobs" })}
              >
                Change
              </button>
            </dd>
            <dt>Suggestions</dt>
            <dd>{suggestions}</dd>
            {ctx?.review && (
              <>
                <dt>Done</dt>
                <dd>
                  You've finished this week's review:{" "}
                  <button type="button" className="tlink" title={`Open ${noteTitle(ctx.review)}`} onClick={() => openDoc(ctx.review!)}>
                    open it
                  </button>
                  . Finishing again rewrites it, keeping what you added under Added later.
                </dd>
              </>
            )}
          </dl>
          <div className="row">
            {paused ? (
              <>
                <button type="button" className="btn pri lg" title="Pick up the review where you left it" onClick={() => onStart(false)}>
                  Carry on
                  <Icon name="forward" size={14} />
                </button>
                <button
                  type="button"
                  className="btn lg"
                  title="Drop this review's progress, notes and decisions, and start again from step 1"
                  onClick={() => (hasProgress(st) ? setConfirmOver(true) : onStart(true))}
                >
                  Start over
                </button>
              </>
            ) : (
              <button
                type="button"
                className="btn pri lg"
                title="Start the review at step 1, preparing its suggestions if they aren't ready"
                onClick={() => onStart(false)}
              >
                Start the review
                <Icon name="forward" size={14} />
              </button>
            )}
          </div>
        </div>
      </div>
    </main>
  );
}

function Step({ id, ctx, log }: { id: string; ctx: WeeklyContext | null; log: (l: string) => void }) {
  const all = useAllTasks();
  const projects = useProjects();
  const inbox = useInbox();
  const today = localToday();
  const view = (v: string) =>
    all
      ? viewRows(
          all,
          VIEWS.find((x) => x.id === v)!,
          today,
          inbox,
        )
      : [];
  const flagged = useMemo(() => (projects ?? []).filter((p) => projectFlag(p)), [projects]);
  const failed = useStore(tasksFailed);
  if (failed && (id === "next" || id === "waiting" || id === "someday"))
    return (
      <div className="card tcard">
        <TasksFailed />
      </div>
    );
  switch (id) {
    case "loose":
    case "ideas":
      return (
        <div className="card cardcap">
          <CaptureBox compact />
        </div>
      );
    case "inbox": {
      const n = (inbox ?? []).filter(unclarified).length;
      return (
        <div className="card wkcard">
          <b>{n ? `${n} to clarify` : "The inbox is empty."}</b>
          {n > 0 && (
            <button type="button" className="btn" title="Go to the Inbox to clarify what's in it" onClick={() => nav.go("inbox")}>
              Open the Inbox
            </button>
          )}
        </div>
      );
    }
    case "notes":
    case "back": {
      const list = id === "notes" ? (ctx?.notes ?? []) : (ctx?.meetings ?? []);
      return (
        <div className="card side-card">
          {!ctx && <p className="faint">Loading…</p>}
          {ctx && !list.length && (
            <p className="faint">{id === "notes" ? "No other notes dated this week." : "No meeting notes this week."}</p>
          )}
          {list.map((n) => (
            <button key={n.path} type="button" className="blink" title={`Open ${n.title}`} onClick={() => openDoc(n.path)}>
              <span className="lb note">{n.title}</span>
              {n.mtime > 0 && <span className="faint">{ago(n.mtime)}</span>}
            </button>
          ))}
        </div>
      );
    }
    case "ahead":
      return (
        <div className="card wkcard">
          <p className="muted">Brainstead can't read your calendar, so open it yourself and capture what you find here.</p>
          <CaptureBox compact />
        </div>
      );
    case "next":
      return (
        <div className="card tcard">
          <TaskList rows={view("next")} manual={false} keys empty="No next actions." />
        </div>
      );
    case "projects":
      return (
        <div className="card">
          {!projects && <p className="faint pad">Loading…</p>}
          {projects && !flagged.length && (
            <p className="pad">Every active project has a next action and has moved in the last two weeks.</p>
          )}
          {flagged.map((p) => (
            <div key={p.path} className="wproj">
              <span className="dot" style={{ background: projectFlag(p) === "none" ? "var(--red)" : "var(--amber)" }} />
              <div className="t">
                <b>{p.name}</b>
                <div className="faint small">{projectFlag(p) === "none" ? "No next action" : `Nothing for ${ago(p.lastTouched)}`}</div>
                <AddNext path={p.path} onAdded={(t) => log(`Next action for ${p.name}: ${t}`)} />
              </div>
              <button
                type="button"
                className="btn sm"
                title={`Move ${p.name} to Someday/Maybe`}
                onClick={() =>
                  void api
                    .projectSet(p.path, "status", "someday")
                    .then(() => log(`${p.name} moved to someday`))
                    .catch(reportEditError)
                }
              >
                Someday
              </button>
              <button
                type="button"
                className="btn sm"
                title={`Mark ${p.name} as completed`}
                onClick={() =>
                  void api
                    .projectSet(p.path, "status", "done")
                    .then(() => log(`${p.name} completed`))
                    .catch(reportEditError)
                }
              >
                Done
              </button>
            </div>
          ))}
          {projects && (
            <p className="faint small pad">
              {(projects ?? []).filter((p) => p.status === "active").length} active projects ·{" "}
              <button type="button" className="tlink" title="Go to the Projects screen" onClick={() => nav.go("projects")}>
                open Projects
              </button>
            </p>
          )}
        </div>
      );
    case "waiting": {
      const rows = view("waiting");
      return (
        <div className="card tcard">
          <TaskList rows={rows} manual={false} keys empty="Not waiting on anyone." />
          {!!rows.length && (
            <div className="pad">
              <button
                type="button"
                className="btn sm"
                title="Ask the AI to draft a follow-up for each of these, to copy into Outlook or Teams; nothing is sent"
                onClick={() => {
                  askWith(
                    `Draft a short, friendly nudge for each of these, to copy into Outlook or Teams (don't send anything):\n${rows.map((t) => `- ${t.text} (in [[${t.title}]])`).join("\n")}`,
                  );
                  nav.go("ask");
                }}
              >
                <Icon name="ask" size={12} />
                Draft nudges in Ask
              </button>
            </div>
          )}
        </div>
      );
    }
    case "someday":
      return (
        <>
          <div className="card tcard">
            <TaskList rows={view("someday")} manual={false} keys empty="Nothing on someday / maybe." />
          </div>
          {(projects ?? []).some((p) => p.status === "someday") && (
            <div className="card side-card">
              <div className="eyebrow">Someday projects</div>
              {(projects ?? [])
                .filter((p) => p.status === "someday")
                .map((p) => (
                  <div key={p.path} className="row">
                    <span className="grow">{p.name}</span>
                    <button
                      type="button"
                      className="btn sm"
                      title={`Make ${p.name} an active project`}
                      onClick={() =>
                        void api
                          .projectSet(p.path, "status", "active")
                          .then(() => log(`${p.name} started`))
                          .catch(reportEditError)
                      }
                    >
                      Start now
                    </button>
                  </div>
                ))}
            </div>
          )}
        </>
      );
    case "ghosts":
      return <Ghosts fallback={ctx?.ghostLinks ?? []} />;
  }
  return null;
}

/** Missing pages: Knowledge health's list (the same one it shows), else the review's inputs. */
function Ghosts({ fallback }: { fallback: { target: string; refs: number }[] }) {
  useEffect(startKnowledge, []);
  const h = useStore(health);
  const items = h?.report?.checks.find((c) => c.id === "missing-pages")?.items;
  const list = items ? items.map((i) => ({ target: i.name ?? i.text, refs: i.count ?? 0 })) : fallback;
  return (
    <div className="card side-card">
      {!list.length && <p className="faint">No missing pages.</p>}
      {list.slice(0, 12).map((g) => (
        <div key={g.target} className="row">
          <span className="grow">[[{g.target}]]</span>
          <span className="faint small">
            linked from {g.refs} page{g.refs === 1 ? "" : "s"}
          </span>
        </div>
      ))}
      {list.length > 0 && (
        <button
          type="button"
          className="btn sm"
          title="Go to Knowledge health to create these pages or link them"
          onClick={() => nav.go("health")}
        >
          Create or link them in Knowledge health
        </button>
      )}
    </div>
  );
}

function AddNext({ path, onAdded }: { path: string; onAdded: (text: string) => void }) {
  const [v, setV] = useState("");
  const add = () => {
    const t = v.trim();
    if (!t) return;
    setV("");
    api
      .projectAddTask(path, prepareCapture("task", t))
      .then(() => onAdded(t))
      .catch(reportEditError);
  };
  return (
    <label className="inp wadd">
      <Icon name="plus" size={14} />
      <input
        value={v}
        placeholder="Add its next action"
        onChange={(e) => setV(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && add()}
      />
    </label>
  );
}
