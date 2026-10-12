// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Settings › Jobs & schedule: the scheduled daily and weekly summaries (src-tauri/src/reviews.rs).
// Only one app runs them at a time, so Brainstead runs them only when you say it should; then
// The previous app's schedule should be switched off. Each run opens as a chat tab, and its block
// can be undone while the file is as the run left it.

import { useEffect, useState } from "react";
import { api, DailyCheckStatus, ReviewRun, ReviewsStatus, SummarySchedule, Weekday, WeeklyReviewTime, WeekPrepJob } from "./api";
import { Icon } from "./icons";
import { openReviewRun, selectReviewRun } from "./askState";
import { nav } from "./nav";
import { settings, useStore } from "./store";
import { toast } from "./Toast";
import { Popover, Switch } from "./ui";
import { DatePicker } from "./DatePicker";
import { fmtShortDate } from "./md/dates";
import { DocButton, useIsGone } from "./docButton";
import { AutomatedTools } from "./AutomatedTools";

export const DEFAULT_SCHEDULE: SummarySchedule = {
  dailyEnabled: false,
  dailyTime: "06:30",
  weeklyEnabled: false,
  weeklyDay: "fri",
  weeklyTime: "16:00",
};

export const DEFAULT_WEEKLY_REVIEW: WeeklyReviewTime = { day: "fri", time: "16:00" };

const DAYS: [Weekday, string][] = [
  ["mon", "Monday"],
  ["tue", "Tuesday"],
  ["wed", "Wednesday"],
  ["thu", "Thursday"],
  ["fri", "Friday"],
  ["sat", "Saturday"],
  ["sun", "Sunday"],
];

const when = (iso: string | null) =>
  iso
    ? new Date(iso).toLocaleString("en-GB", { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })
    : "—";

export function useReviews(): ReviewsStatus | null {
  const [st, setSt] = useState<ReviewsStatus | null>(null);
  useEffect(() => {
    let live = true;
    const load = () =>
      api
        .reviewsStatus()
        .then((s) => live && setSt(s))
        .catch(() => {});
    void load();
    const off = api.onReviewsChanged(() => void load());
    return () => {
      live = false;
      void off.then((f) => f());
    };
  }, []);
  return st;
}

function yesterday(): string {
  const d = new Date();
  d.setDate(d.getDate() - 1);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export function Jobs() {
  const s = useStore(settings);
  const st = useReviews();
  const sch = { ...DEFAULT_SCHEDULE, ...s.summaries };
  const wr = { ...DEFAULT_WEEKLY_REVIEW, ...s.weeklyReview };
  const here = !!s.summariesHere;
  const set = (patch: Partial<SummarySchedule>) => settings.update({ summaries: { ...sch, ...patch } });
  const setReview = (patch: Partial<WeeklyReviewTime>) => settings.update({ weeklyReview: { ...wr, ...patch } });
  const [dailyFor, setDailyFor] = useState("");
  const runNow = (kind: "daily" | "weekly", day?: string) =>
    api
      .reviewRunNow(kind, day)
      .then((id) => {
        selectReviewRun(id);
        nav.go("ask");
      })
      .catch((e) => toast(String(e), undefined, "bad"));
  const running = (k: "daily" | "weekly") => !!st?.running.includes(k);
  const last = (k: "daily" | "weekly") => st?.runs.find((r) => r.kind === k) ?? null;

  return (
    <>
      <h2 className="h3">Your weekly review</h2>
      <div className="card">
        <div className="job">
          <div className="t">
            <div className="pt">Weekly review</div>
            <div className="faint">
              You go through your week, step by step. Today reminds you from this time until the end of the next day.
            </div>
          </div>
          <span className="jtime">
            <select
              title="The day of your weekly review (its preparation runs four hours before it)"
              className="sel"
              value={wr.day}
              aria-label="Weekly review day"
              onChange={(e) => setReview({ day: e.target.value as Weekday })}
            >
              {DAYS.map(([d, l]) => (
                <option key={d} value={d}>
                  {l}
                </option>
              ))}
            </select>
            <TimeField value={wr.time} onChange={(t) => setReview({ time: t })} label="Weekly review time" />
          </span>
        </div>
        <WeekPrep here={here} />
      </div>
      <div className="card jobnote">
        <Icon name="info" size={16} />
        <span>
          Jobs run while Brainstead is running, even with its window closed; a run missed while it was closed or asleep catches up when it
          next runs. Times use this computer's clock.
        </span>
      </div>
      <h2 className="h3">Summaries Brainstead writes</h2>
      <div className="card">
        <div className="srow">
          <Icon name="calendar" style={{ color: here ? "var(--green)" : "var(--ink3)" }} />
          <div className="t">
            <div className="pt">Brainstead runs the daily and weekly summaries</div>
            <div className="faint">
              {here
                ? "Make sure no other app also runs them, so only one app writes them."
                : "Off: Brainstead doesn't run them. Switch this on if no other app does."}
            </div>
          </div>
          <Switch
            label="Brainstead runs the daily and weekly summaries"
            on={here}
            onChange={(v) => {
              settings.update({ summariesHere: v });
              if (v)
                toast("If another app also writes the daily and weekly summaries, switch them off there, so only one app writes them.");
            }}
          />
        </div>
      </div>
      <div className="card">
        <div className="srow">
          <Icon name="review" style={{ color: s.holdSummaries ? "var(--accent)" : "var(--ink3)" }} />
          <div className="t">
            <div className="pt">Hold the summaries for me</div>
            <div className="faint">
              {s.holdSummaries
                ? "Each summary is held in Changes until you accept it."
                : "Off: summaries are written straight away, with Undo in Recent runs below and Revert in Changes."}
            </div>
          </div>
          <Switch label="Hold the summaries for me" on={!!s.holdSummaries} onChange={(v) => settings.update({ holdSummaries: v })} />
        </div>
      </div>
      {here && s.readOnly && !s.holdSummaries && (
        <p className="faint">
          Brainstead is read-only: a scheduled summary is held in Changes for you, and Run now needs read-only turned off in Settings ›
          Vault.
        </p>
      )}
      <div className="card">
        {!here && (
          <p className="faint small pad joboff">
            Greyed out because “Brainstead runs the daily and weekly summaries” is off: they won't run on schedule until it's on. Run now
            still works.
          </p>
        )}
        <div className={`job ${here ? "" : "off"}`}>
          <div className="t">
            <div className="pt">Daily summary</div>
            <div className="faint">What you did yesterday, written for you.</div>
            <RunLine run={last("daily")} next={here && sch.dailyEnabled ? (st?.next.daily ?? null) : null} running={running("daily")} />
          </div>
          <label className="jtime">
            <span className="muted">Every day at</span>
            <TimeField value={sch.dailyTime} onChange={(t) => set({ dailyTime: t })} label="Daily summary time" />
          </label>
          <label className="jtime" title="Run now writes the summary for this day instead of yesterday (leave empty for yesterday)">
            <span className="muted">for</span>
            <DayButton value={dailyFor} max={yesterday()} onChange={setDailyFor} />
          </label>
          <button
            type="button"
            className="btn sm"
            title={
              running("daily")
                ? "Stop this summary"
                : dailyFor
                  ? `Write the daily summary for ${dailyFor} now`
                  : "Run the daily summary for yesterday now, whether or not it's scheduled"
            }
            onClick={() => void (running("daily") ? api.reviewsStop("daily") : runNow("daily", dailyFor || undefined))}
          >
            {running("daily") ? "Stop" : "Run now"}
          </button>
          <Switch label="Daily summary" on={sch.dailyEnabled} onChange={(v) => set({ dailyEnabled: v })} />
        </div>
        <div className={`job ${here ? "" : "off"}`}>
          <div className="t">
            <div className="pt">Weekly summary</div>
            <div className="faint">What you did last week, written for you. Your weekly review shows it beside its steps.</div>
            <RunLine run={last("weekly")} next={here && sch.weeklyEnabled ? (st?.next.weekly ?? null) : null} running={running("weekly")} />
          </div>
          <span className="jtime">
            <select
              title="The day the weekly summary is written"
              className="sel"
              value={sch.weeklyDay}
              aria-label="Weekly summary day"
              onChange={(e) => set({ weeklyDay: e.target.value as Weekday })}
            >
              {DAYS.map(([d, l]) => (
                <option key={d} value={d}>
                  {l}
                </option>
              ))}
            </select>
            <TimeField value={sch.weeklyTime} onChange={(t) => set({ weeklyTime: t })} label="Weekly summary time" />
          </span>
          <button
            type="button"
            className="btn sm"
            title={running("weekly") ? "Stop this summary" : "Run the weekly summary now, whether or not it's scheduled"}
            onClick={() => void (running("weekly") ? api.reviewsStop("weekly") : runNow("weekly"))}
          >
            {running("weekly") ? "Stop" : "Run now"}
          </button>
          <Switch label="Weekly summary" on={sch.weeklyEnabled} onChange={(v) => set({ weeklyEnabled: v })} />
        </div>
      </div>
      <p className="faint">
        Brainstead gathers each summary's notes, tasks and Claude Code sessions itself, the model (Settings › AI assistants) writes it from
        them with read-only tools, and Brainstead puts it at the top of the month's summaries note. Run now works whether or not the
        schedule is on.
      </p>
      <AutomatedTools />
      <DailyCheck />
      {!!st?.runs.length && (
        <section className="sgroup">
          <h2 className="h3">Recent runs</h2>
          <div className="card">
            {st.runs.slice(0, 8).map((r) => (
              <RunRow key={r.id} r={r} />
            ))}
          </div>
        </section>
      )}
    </>
  );
}

function RunLine({ run, next, running }: { run: ReviewRun | null; next: string | null; running: boolean }) {
  const bits: string[] = [];
  if (running) bits.push("Running now");
  else if (run)
    bits.push(
      `Last run ${when(run.startedAt)}${run.status === "error" ? " · failed" : run.file ? ` · ${run.file.replace(/\.md$/, "")}` : ""}`,
    );
  if (next) bits.push(`next ${when(next)}`);
  return <div className="faint">{bits.join(" · ") || "Not run yet"}</div>;
}

function RunRow({ r }: { r: ReviewRun }) {
  const gone = useIsGone(r.file);
  return (
    <div className="srow">
      <Icon
        name={r.status === "error" ? "x" : r.status === "running" ? "refresh" : r.undone ? "back" : "check"}
        style={{ color: r.status === "error" ? "var(--red)" : r.status === "running" ? "var(--amber)" : "var(--green)" }}
      />
      <div className="t">
        <div className="pt">
          {r.kind === "daily" ? "Daily summary" : "Weekly summary"} {r.target}
          <span className="faint"> · {r.trigger === "schedule" ? "scheduled" : "run now"}</span>
        </div>
        <div className="faint">
          {when(r.startedAt)}
          {r.status === "error"
            ? ` · ${r.error}`
            : r.undone
              ? " · undone"
              : r.file
                ? ` · ${r.replaced ? "replaced its block in" : "written to"} ${r.file}`
                : ""}
        </div>
      </div>
      {r.chat && (
        <button
          type="button"
          className="btn sm"
          title="Open the chat this run had with the model, in Ask"
          onClick={() => void openReviewRun(r.chat!)}
        >
          Open
        </button>
      )}
      {r.file && r.status === "done" && !r.undone && (
        <>
          <DocButton path={r.file} gone={gone} className="btn sm ghost" title={`Open ${r.file}, the note this run wrote its review into`}>
            Show
          </DocButton>
          <button
            type="button"
            className="btn sm"
            title="Put the note and log.md back as they were before this run (only if neither has been edited since)"
            onClick={() =>
              void api
                .reviewUndo(r.id)
                .then(() => toast("Review undone"))
                .catch((e) => toast(String(e), undefined, "bad"))
            }
          >
            Undo
          </button>
        </>
      )}
    </div>
  );
}

/** The weekly review's preparation (src-tauri/src/weekprep.rs): on the review day, 4 hours before
 *  the review, while Brainstead runs the jobs. Its runs aren't in Recent runs, which lists the
 *  summaries' (with Undo); its last run is on its own line, with Open for the run's chat. */
function WeekPrep({ here }: { here: boolean }) {
  const s = useStore(settings);
  const [st, setSt] = useState<WeekPrepJob | null>(null);
  const load = () =>
    void api
      .weekprepJob()
      .then(setSt)
      .catch(() => {});
  useEffect(() => {
    load();
    const off = api.onWeekprepChanged(() => load()).catch(() => () => {});
    return () => void off.then((f) => f());
  }, [s.weekprepEnabled, s.summariesHere, s.weeklyReview?.day, s.weeklyReview?.time]);
  const on = s.weekprepEnabled !== false;
  const running = !!st?.running;
  const last = st?.last ?? null;
  const line = running
    ? `Preparing ${st?.running} now`
    : [
        last
          ? `Last run ${when(last.startedAt)} · ${last.week} · ${
              last.status === "error"
                ? "failed"
                : last.status === "stopped"
                  ? "stopped"
                  : `${last.count} suggestion${last.count === 1 ? "" : "s"}`
            }`
          : "Not run yet",
        st?.next ? `next ${when(st.next)}` : "",
      ]
        .filter(Boolean)
        .join(" · ");
  return (
    <div className={`job ${on && here ? "" : "off"}`}>
      <div className="t">
        <div className="pt">Prepare the weekly review</div>
        <div className="faint">
          {line}
          {on && !here ? " · runs on schedule only while Brainstead runs the summaries (below)" : ""}
        </div>
      </div>
      {last?.chat && !running && (
        <button
          type="button"
          className="btn sm ghost"
          title="Open the chat the last preparation had with the model, in Ask"
          onClick={() => void openReviewRun(last.chat!)}
        >
          Open
        </button>
      )}
      <button
        type="button"
        className="btn sm"
        title={running ? "Stop preparing the weekly review" : "Prepare this week's review now, whether or not it's scheduled"}
        onClick={() =>
          void (
            running
              ? api.weekprepStop()
              : api.weekprepRun().then((id) => {
                  selectReviewRun(id);
                  nav.go("ask");
                })
          )
            .then(load)
            .catch((e) => toast(String(e), undefined, "bad"))
        }
      >
        {running ? "Stop" : "Run now"}
      </button>
      <Switch label="Prepare the weekly review" on={on} onChange={(v) => settings.update({ weekprepEnabled: v })} />
    </div>
  );
}

/** HH:MM, 24-hour. Saved when it's a valid time. */
/** How far the daily check has got; a bar that just moves when the app can't say (a build from
 *  before the step was reported). */
function DailyCheckProgress({ doing, progress }: { doing?: string; progress?: number }) {
  const known = typeof progress === "number" && Number.isFinite(progress) && progress > 0;
  const pct = known ? Math.round(progress * 100) : 0;
  return (
    <div className="jprog pad">
      <div
        className={`bar ${known ? "" : "busy"}`}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={known ? pct : undefined}
      >
        <i style={known ? { width: `${Math.max(2, pct)}%` } : undefined} />
      </div>
      <span className="faint small">{known ? `${doing || "Starting"} · ${pct}%` : doing || "Running…"}</span>
    </div>
  );
}

/** The daily check: contradictions in the pages that changed, and the list of pages in index.md. */
function DailyCheck() {
  const s = useStore(settings);
  const [st, setSt] = useState<DailyCheckStatus | null>(null);
  const load = () =>
    void api
      .dailyCheckStatus()
      .then(setSt)
      .catch(() => {});
  useEffect(() => {
    load();
    const off = api.onDailyCheckChanged(() => load()).catch(() => () => {});
    // Its contradiction step reports each batch.
    const off2 = api.onContradictionsChanged(() => load()).catch(() => () => {});
    return () => {
      void off.then((f) => f());
      void off2.then((f) => f());
    };
  }, [s.dailyCheckEnabled, s.dailyCheckTime]);
  const on = !!s.dailyCheckEnabled;
  return (
    <>
      <div className="card">
        <div className={`job ${on ? "" : "off"}`}>
          <div className="t">
            <div className="pt">Daily check</div>
            <div className="faint small">
              {st?.running
                ? "Running now…"
                : [
                    st?.lastRun ? `Last ${st.lastRun.slice(0, 16).replace("T", " ")}${st.summary ? `: ${st.summary}` : ""}` : "Not run yet",
                    on && st?.next ? `next ${st.next.slice(0, 16).replace("T", " ")}` : "",
                  ]
                    .filter(Boolean)
                    .join(" · ")}
            </div>
          </div>
          <label className="jtime">
            <span className="muted">Every day at</span>
            <TimeField
              value={s.dailyCheckTime ?? "09:00"}
              onChange={(t) => settings.update({ dailyCheckTime: t })}
              label="Daily check time"
            />
          </label>
          <button
            type="button"
            className="btn sm"
            title={
              st?.running
                ? "Stop the check; what it has read so far is kept for next time"
                : "Run the daily check now: look for contradictions and update index.md"
            }
            onClick={() => void (st?.running ? api.dailyCheckStop() : api.dailyCheckRunNow()).then(load)}
          >
            {st?.running ? "Stop" : "Run now"}
          </button>
          <Switch label="Daily check" on={on} onChange={(v) => settings.update({ dailyCheckEnabled: v })} />
        </div>
        {st?.running && <DailyCheckProgress doing={st.doing} progress={st.progress} />}
        <p className="faint small pad">
          Checks the wiki pages that changed since the last run for contradictions, making the fixes (held in Changes when a check fails),
          and brings the list of pages in index.md up to date. It can also refresh pages whose sources changed: see Ingest in Settings › AI
          assistants.
        </p>
      </div>
    </>
  );
}

/** The day Run now summarises, chosen in the app's date picker: yesterday when none is picked. */
function DayButton({ value, max, onChange }: { value: string; max: string; onChange: (d: string) => void }) {
  const [at, setAt] = useState<DOMRect | null>(null);
  return (
    <>
      <button
        type="button"
        className="btn sm"
        aria-label="Day to summarise"
        title="Choose the day Run now summarises"
        onClick={(e) => setAt(e.currentTarget.getBoundingClientRect())}
      >
        <Icon name="calendar" size={12} />
        {value ? fmtShortDate(value) : "yesterday"}
      </button>
      {value && (
        <button
          type="button"
          className="ibtn"
          aria-label="Back to yesterday"
          title="Summarise yesterday again"
          onClick={() => onChange("")}
        >
          <Icon name="x" size={12} />
        </button>
      )}
      {at && (
        <Popover anchor={at} onClose={() => setAt(null)} width={252}>
          <DatePicker
            value={value || max}
            label="Day to summarise"
            onPick={(d) => {
              if (d > max) return toast("Pick yesterday or an earlier day: a day isn't over until it's ended.");
              onChange(d === max ? "" : d);
              setAt(null);
            }}
            onCancel={() => setAt(null)}
          />
        </Popover>
      )}
    </>
  );
}

function TimeField({ value, onChange, label }: { value: string; onChange: (v: string) => void; label: string }) {
  const [v, setV] = useState(value);
  useEffect(() => setV(value), [value]);
  const ok = /^([01]\d|2[0-3]):[0-5]\d$/.test(v);
  return (
    <input
      className={`tfield ${ok ? "" : "bad"}`}
      value={v}
      aria-label={label}
      placeholder="HH:MM"
      inputMode="numeric"
      maxLength={5}
      onChange={(e) => {
        setV(e.target.value);
        if (/^([01]\d|2[0-3]):[0-5]\d$/.test(e.target.value)) onChange(e.target.value);
      }}
      onBlur={() => !ok && setV(value)}
    />
  );
}
