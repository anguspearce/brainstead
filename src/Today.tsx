// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Today: what's overdue, due and deferred until today, what you're waiting for, a capture box,
// and the notes you were in. No calendar: Microsoft Graph isn't available (§1).

import { changes, decisions, health, held, pageName, startKnowledge } from "./knowledge";
import { useEffect, useMemo, useState } from "react";
import { api, Draft, FileSummary, LatestSummary, ReviewsStatus, TaskRow } from "./api";
import { fmtAccel } from "./Shortcuts";
import { contextName, contextsInUse, unclarified, useInbox } from "./gtd";
import { CaptureBox } from "./Capture";
import { settings, useStore } from "./store";
import { isoWeek, keepsPaused, startWeekly } from "./Weekly";
import { Icon } from "./icons";
import { Markdown } from "./md/Markdown";
import { localToday } from "./md/taskQuery";
import { nav, openDoc, useViewState } from "./nav";
import { recentDocs } from "./recent";
import { DraftNudge, LoadFailed, TaskList, TasksFailed, useTaskListKeys } from "./TaskList";
import { deferredPast, tasksFailed, todayRows, useAllTasks, viewRows, VIEWS } from "./taskModel";
import { useVaultOpening, useVaultVersion, vaultStatus } from "./state";
import { TopBar } from "./TopBar";
import { ago, fmtCount, TableBand } from "./ui";
import { captureDefault, keyLabel } from "./platform";

export function greeting(h: number, name?: string) {
  const g = h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
  const first = name?.trim().split(/\s+/)[0];
  return first ? `${g}, ${first}` : g;
}

/** The time, redrawn on the hour (so the greeting and the date move on while the app stays open)
 *  and when the window comes back after the computer slept, as timers don't run during sleep. */
function useHourly(): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    let t = 0;
    const arm = () => {
      const d = new Date();
      setNow(d);
      const next = new Date(d);
      next.setHours(d.getHours() + 1, 0, 1, 0);
      t = window.setTimeout(arm, next.getTime() - d.getTime());
    };
    const wake = () => {
      if (document.visibilityState !== "visible") return;
      window.clearTimeout(t);
      arm();
    };
    t = window.setTimeout(arm, 0);
    document.addEventListener("visibilitychange", wake);
    window.addEventListener("focus", wake);
    return () => {
      window.clearTimeout(t);
      document.removeEventListener("visibilitychange", wake);
      window.removeEventListener("focus", wake);
    };
  }, []);
  return now;
}

export function TodayScreen() {
  const s = useStore(settings);
  const all = useAllTasks();
  const now = useHourly();
  const today = localToday(now);
  const t = useMemo(() => (all ? todayRows(all, today) : null), [all, today]);
  const waiting = useMemo(
    () =>
      all
        ? viewRows(
            all,
            VIEWS.find((v) => v.id === "waiting")!,
            today,
          ).filter((r) => !deferredPast(r, today))
        : [],
    [all, today],
  );
  const dateLine = now.toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" });
  // The @context chips: only contexts on today's tasks and Waiting for; a chosen one filters every band.
  const [ctx, setCtx] = useViewState<string | null>("today.context", null);
  const contexts = useMemo(() => (t ? contextsInUse([...t.overdue, ...t.due, ...t.scheduled, ...waiting]) : []), [t, waiting]);
  const inCtx = (r: TaskRow) => !ctx || (r.contexts ?? []).includes(ctx);
  const overdue = t ? t.overdue.filter(inCtx) : [];
  const dueToday = t ? t.due.filter(inCtx) : [];
  const deferred = t ? t.scheduled.filter(inCtx) : [];
  const waitingHere = waiting.filter(inCtx);
  const failed = useStore(tasksFailed);
  const nothingDue = !!t && !failed && !overdue.length && !dueToday.length && !deferred.length;
  useTaskListKeys();
  const tasks = (n: number) => (n === 1 ? "1 task" : `${fmtCount(n)} tasks`);
  return (
    <main className="main">
      <OpeningBanner />
      <TopBar title="Today" sub={dateLine} ask={{ about: "today: what's due, deferred and waiting", prompt: "About my day today: " }}>
        <button
          type="button"
          className="btn"
          title={`Quick capture, from any app (${fmtAccel(s.captureShortcut ?? captureDefault())})`}
          onClick={() => void api.captureShow()}
        >
          <Icon name="plus" size={14} />
          Capture
        </button>
      </TopBar>
      <div className="body today">
        <section className="tmain">
          <div className="hello">
            <h1 className="h1">{greeting(now.getHours(), s.ownerName)}.</h1>
            {t && !failed && (
              <div className="tstats">
                <Stat
                  to="today-overdue"
                  n={overdue.length}
                  label="Overdue"
                  tone="red"
                  tip={`${tasks(overdue.length)} past their due date`}
                />
                <Stat to="today-due" n={dueToday.length} label="Due today" tip={`${tasks(dueToday.length)} due today`} />
                <Stat
                  to="today-deferred"
                  n={deferred.length}
                  label="Deferred"
                  tip={`${tasks(deferred.length)} deferred until today or earlier`}
                />
                <Stat
                  to="today-waiting"
                  n={waitingHere.length}
                  label="Waiting"
                  tip={`${tasks(waitingHere.length)} waiting for someone else`}
                />
              </div>
            )}
          </div>
          <div className="card cardcap">
            <CaptureBox compact />
          </div>
          <section className="card ttable todaytable" role="table" aria-label="Today's tasks">
            <TasksFailed />
            {contexts.length > 0 && !failed && (
              <div className="tctx">
                <span className="faint">Context</span>
                <button
                  type="button"
                  className={`chip f ${ctx ? "" : "on"}`}
                  title="Show tasks from every context"
                  onClick={() => setCtx(null)}
                >
                  All
                </button>
                {contexts.map((c) => (
                  <button
                    key={c}
                    type="button"
                    className={`chip f ${ctx === c ? "on" : ""}`}
                    title={ctx === c ? "Show tasks from every context" : `Show only @${contextName(c)} tasks`}
                    onClick={() => setCtx(ctx === c ? null : c)}
                  >
                    @{contextName(c)}
                  </button>
                ))}
              </div>
            )}
            {nothingDue && (
              <div className="tcalm">
                <Icon name="check" size={16} />
                <div>
                  <div className="tcalm-t">{ctx ? `No @${contextName(ctx)} tasks due today` : "Nothing due today"}</div>
                  <div className="faint">Tasks due or deferred until today show here.</div>
                </div>
              </div>
            )}
            <Band
              id="today-overdue"
              icon="calendar"
              label="Overdue"
              tone="red"
              rows={overdue}
              tip={`${tasks(overdue.length)} past their due date`}
            />
            <Band id="today-due" icon="calendar" label="Due today" rows={dueToday} tip={`${tasks(dueToday.length)} due today`} />
            <Band
              id="today-deferred"
              icon="defer"
              label="Deferred until now"
              rows={deferred}
              tip={`${tasks(deferred.length)} deferred until today or earlier`}
            />
            <Band
              id="today-waiting"
              icon="waiting"
              label="Waiting for"
              rows={waitingHere}
              tip={`${tasks(waitingHere.length)} waiting for someone else`}
              hideTags={["waiting-for"]}
              rowAction={(w) => (
                <>
                  <WaitingSince t={w} today={today} />
                  <DraftNudge t={w} compact />
                </>
              )}
            />
            <div className="tfoot">
              <button
                type="button"
                className="btn sm ghost"
                title={keyLabel("Go to Tasks, every open task in the vault (⌥⌘3)")}
                onClick={() => nav.go("tasks")}
              >
                All tasks
                <Icon name="forward" size={12} />
              </button>
            </div>
          </section>
        </section>
        <aside className="tside">
          <ProcessCard />
          <SummaryCard kind="weekly" />
          <SummaryCard kind="daily" />
          <HeldCard />
          <ContinueCard />
        </aside>
      </div>
    </main>
  );
}

/** A count at the top of Today: jumps to its band below. */
function Stat({ to, n, label, tone, tip }: { to: string; n: number; label: string; tone?: "red"; tip: string }) {
  return (
    <button
      type="button"
      className={`tstat ${n ? (tone ?? "") : "zero"}`}
      aria-disabled={!n}
      title={n ? `${tip}: show them` : tip}
      onClick={() => n && document.getElementById(to)?.scrollIntoView({ block: "start", behavior: "smooth" })}
    >
      <b>{fmtCount(n)}</b>
      <span>{label}</span>
    </button>
  );
}

/** One of Today's bands, with its rows; nothing when it's empty. */
function Band({
  id,
  icon,
  label,
  rows,
  tip,
  tone,
  hideTags,
  rowAction,
}: {
  id: string;
  icon: string;
  label: string;
  rows: TaskRow[];
  tip: string;
  tone?: "red";
  hideTags?: string[];
  rowAction?: (t: TaskRow) => React.ReactNode;
}) {
  if (!rows.length) return null;
  return (
    <div id={id} className={`tb-group ${tone ?? ""}`} role="rowgroup">
      <TableBand icon={icon} label={label} n={rows.length} tip={tip} />
      <TaskList table keys rows={rows} manual={false} hideTags={hideTags} rowAction={rowAction} />
    </div>
  );
}

/** How long a Waiting for task has waited, from its created date (➕) when it has one. */
function WaitingSince({ t, today }: { t: TaskRow; today: string }) {
  if (!t.created) return null;
  const days = Math.round((Date.parse(today) - Date.parse(t.created)) / 86_400_000);
  if (!(days >= 0)) return null;
  const since = new Date(`${t.created}T12:00:00`).toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });
  return (
    <span className={`wsince ${days >= 7 ? "long" : ""}`} title={`Waiting since ${since}`}>
      {days === 0 ? "since today" : `${days} d waiting`}
    </span>
  );
}

/** A right-rail card: a band for its title, then rows with dividers. */
function SideCard({ title, right, children }: { title: string; right?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="card rcard" aria-label={title}>
      <div className="tb-band rhead">
        <span>{title}</span>
        {right}
      </div>
      {children}
    </section>
  );
}

const DAY_INDEX: Record<string, number> = { sun: 0, mon: 1, tue: 2, wed: 3, thu: 4, fri: 5, sat: 6 };

/** Whether Today reminds you of the weekly review: from its time on its day (Settings › Jobs &
 *  schedule, Friday 16:00 by default) and all the next day. */
export function isReviewDay(day: string | undefined, time: string | undefined, now = new Date()): boolean {
  const d = DAY_INDEX[day ?? "fri"] ?? 5;
  const [h, m] = /^\d\d:\d\d$/.test(time ?? "") ? time!.split(":").map(Number) : [16, 0];
  const wd = now.getDay();
  if (wd === (d + 1) % 7) return true;
  return wd === d && now.getHours() * 60 + now.getMinutes() >= h * 60 + m;
}

const hhmm = (iso: string) => new Date(iso).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });

/** "Next 18:00" today, "Next Fri 17:00" on another day. */
function nextLabel(iso: string): string {
  const d = new Date(iso);
  const sameDay = localToday(d) === localToday();
  return `Next ${sameDay ? "" : `${d.toLocaleDateString("en-GB", { weekday: "short" })} `}${hhmm(iso)}`;
}

/** A summary's date for its card: "Today", "Yesterday", "Thu 1 Oct"; "Week 40". */
function summaryDate(label: string, today: string): string {
  if (label.includes("W")) return `Week ${+label.slice(-2)}${label === isoWeek(today) ? "" : ", last week"}`;
  if (label === today) return "Today";
  const d = new Date(`${label}T12:00:00`);
  if (localToday(new Date(d.getTime() + 86_400_000)) === today) return "Yesterday";
  return d.toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });
}

/** The latest daily or weekly summary, folded to a line until opened: its date and whether the
 *  next run is due or one is running. Shown when
 *  Brainstead runs the summaries or a block is on file. */
function SummaryCard({ kind }: { kind: "daily" | "weekly" }) {
  const s = useStore(settings);
  const v = useVaultVersion();
  const [open, setOpen] = useViewState<boolean>(`today.${kind}Summary`, false);
  const [block, setBlock] = useState<LatestSummary | null>(null);
  const [status, setStatus] = useState<ReviewsStatus | null>(null);
  const [failed, setFailed] = useState(false);
  const [tries, setTries] = useState(0);
  const [changed, setChanged] = useState(0);
  const today = localToday();
  useEffect(() => {
    const off = api.onReviewsChanged(() => setChanged((n) => n + 1));
    return () => void off.then((f) => f());
  }, []);
  useEffect(() => {
    let live = true;
    // Each load starts clean: one that failed while the vault was still opening clears.
    setFailed(false);
    api
      .reviewLatest(kind)
      .then((b) => live && setBlock(b))
      .catch(() => live && setFailed(true));
    if (s.summariesHere)
      api
        .reviewsStatus()
        .then((r) => live && setStatus(r))
        .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [kind, s.summariesHere, v, tries, changed]);
  const name = kind === "daily" ? "Daily summary" : "Weekly summary";
  if (!s.summariesHere && !block && !failed) return null;
  // This period's run, for whether it failed.
  const target = kind === "daily" ? today : isoWeek(today);
  const run = status?.runs.find((r) => r.kind === kind && r.target === target);
  const next = status?.next[kind];
  const line = status?.running.includes(kind)
    ? "Running…"
    : run?.status === "error"
      ? "Didn't finish"
      : s.summariesHere && next
        ? nextLabel(next)
        : "";
  const when = block ? summaryDate(block.label, today) : "Not written yet";
  const anchor = block?.heading.replace(/^#+\s*/, "");
  return (
    <section className="card rcard sumcard" aria-label={name}>
      <button
        type="button"
        className="tb-band rhead sumhead"
        aria-expanded={open}
        title={open ? `Fold the ${name.toLowerCase()} away` : `Show the ${name.toLowerCase()}`}
        onClick={() => setOpen(!open)}
      >
        <span>
          <Icon name={open ? "chevdown" : "chevright"} size={12} />
          {name}
          <span className="sumwhen">{when}</span>
        </span>
        {line && <span className="sumline">{line}</span>}
      </button>
      {failed && (
        <LoadFailed
          compact
          what={`the ${name.toLowerCase()}`}
          onRetry={() => {
            setFailed(false);
            setTries((n) => n + 1);
          }}
        />
      )}
      {open &&
        (block ? (
          <>
            <div className="sumbody">
              <Markdown content={block.text.replace(/^## .*\n/, "")} path={block.file} root={s.vaultPath ?? ""} />
            </div>
            <div className="rfoot">
              <button
                type="button"
                className="btn sm ghost ract"
                title={`Open ${block.file} at ${anchor}`}
                onClick={() => openDoc(block.file, { anchor })}
              >
                Open note
                <Icon name="forward" size={12} />
              </button>
            </div>
          </>
        ) : (
          !failed && (
            <p className="faint rempty">
              {kind === "daily" ? "Today's summary isn't written yet." : "This week's summary isn't written yet."}
            </p>
          )
        ))}
    </section>
  );
}

/** While the vault is being read (the first time can take a minute), Today says so: its cards fill
 *  in as it's ready. */
function OpeningBanner() {
  const opening = useVaultOpening();
  const files = useStore(vaultStatus).stats.files;
  if (!opening) return null;
  return (
    <div className="opening" role="status">
      <span className="spin" />
      <span>Reading your vault{files ? `: ${fmtCount(files)} files so far` : ""}. Today fills in when it&apos;s done.</span>
    </div>
  );
}

/** What there is to process: the Inbox, bookmarks, wiki issues, and the weekly review on its day
 *  (or while one is paused). What couldn't be checked says so, with Retry. */
function ProcessCard() {
  const inbox = useInbox();
  const s = useStore(settings);
  const [paused, setPaused] = useState(false);
  // What couldn't be read, and a count that Retry bumps to read it all again.
  const [failed, setFailed] = useState<string[]>([]);
  const [tries, setTries] = useState(0);
  const fail = (what: string) => () => setFailed((f) => (f.includes(what) ? f : [...f, what]));
  const ok = (what: string) => setFailed((f) => f.filter((x) => x !== what));
  const v = useVaultVersion();
  const retry = () => {
    setFailed([]);
    setTries((n) => n + 1);
  };
  useEffect(() => {
    void api
      .weeklyStateRead()
      .then((w) => {
        ok("the weekly review");
        setPaused(keepsPaused(w, localToday()));
      })
      .catch(fail("the weekly review"));
  }, [tries, v]);
  const n = (inbox ?? []).filter(unclarified).length;
  useEffect(startKnowledge, []);
  // Bookmarks gone stale (two weeks untouched) or missing: worth a triage.
  const [bookmarks, setBookmarks] = useState(0);
  useEffect(() => {
    void api
      .bookmarksStatus()
      .then((b) => {
        ok("the bookmarks");
        setBookmarks(b.filter((x) => x.stale || x.missing).length);
      })
      .catch(fail("the bookmarks"));
    // Read again as the vault changes: on first run it's still being indexed when Today opens.
  }, [tries, v]);
  const need = decisions(useStore(health)?.report);
  const reviewDay = isReviewDay(s.weeklyReview?.day, s.weeklyReview?.time);
  return (
    <SideCard title="To process">
      {failed.length > 0 && <LoadFailed compact what={failed.join(" and ")} onRetry={retry} />}
      <div className="rrow">
        <Icon name={n ? "inbox" : "check"} size={14} />
        <span className="grow ell">{n ? "Inbox to clarify" : "The Inbox is clear"}</span>
        {n > 0 && (
          <>
            <span className="n hot" title={`${fmtCount(n)} in the Inbox`}>
              {fmtCount(n)}
            </span>
            <button
              type="button"
              className="btn sm ghost ract"
              title={keyLabel("Go to the Inbox to decide what each item is (⌥⌘2)")}
              onClick={() => nav.go("inbox")}
            >
              Clarify
            </button>
          </>
        )}
      </div>
      {bookmarks > 0 && (
        <div className="rrow">
          <Icon name="pin" size={14} />
          <span className="grow ell">Bookmarks to triage</span>
          <span className="n" title={`${fmtCount(bookmarks)} stale or missing bookmarks`}>
            {fmtCount(bookmarks)}
          </span>
          <button
            type="button"
            className="btn sm ghost ract"
            title="Go through the bookmarks waiting to be sorted"
            onClick={() => nav.go("triage")}
          >
            Triage
          </button>
        </div>
      )}
      {need > 0 && (
        <div className="rrow">
          <Icon name="health" size={14} />
          <span className="grow ell">Wiki issues to decide</span>
          <span className="n" title={`${fmtCount(need)} wiki issues need a decision`}>
            {fmtCount(need)}
          </span>
          <button
            type="button"
            className="btn sm ghost ract"
            title="Open Knowledge health to decide the wiki issues"
            onClick={() => nav.go("health")}
          >
            Check
          </button>
        </div>
      )}
      {(reviewDay || paused) && (
        <div className="rrow">
          <Icon name="calendar" size={14} />
          <span className="grow ell">{paused ? "Weekly review, paused" : "Weekly review day"}</span>
          <button
            type="button"
            className="btn sm ghost ract"
            title={paused ? "Pick up the weekly review where you left it" : "Start the weekly review"}
            onClick={startWeekly}
          >
            {paused ? "Carry on" : "Start"}
          </button>
        </div>
      )}
    </SideCard>
  );
}

/** Scheduled runs' changes held for you in Changes. */
function HeldCard() {
  useEffect(startKnowledge, []);
  const rows = held(useStore(changes) ?? []);
  if (!rows.length) return null;
  return (
    <SideCard
      title="Changes held for you"
      right={
        <span className="n" title={rows.length === 1 ? "1 change held" : `${fmtCount(rows.length)} changes held`}>
          {fmtCount(rows.length)}
        </span>
      }
    >
      {rows.slice(0, 4).map((p) => (
        <button
          key={p.id}
          type="button"
          className="rrow rlink"
          title={keyLabel(`${pageName(p.page)}: ${p.title}\n${p.flags[0] ?? ""}\nOpen Changes to accept or reject it (⌥⌘5)`)}
          onClick={() => nav.go("review")}
        >
          <span className={`lb ${p.page.startsWith("wiki/") ? "wiki" : "note"}`}>
            <span className="ell">{pageName(p.page)}</span>
          </span>
          <span className="ell grow">{p.title}</span>
        </button>
      ))}
      <div className="rfoot">
        <button
          type="button"
          className="btn sm ghost ract"
          title={keyLabel("Open Changes to accept or reject the held changes (⌥⌘5)")}
          onClick={() => nav.go("review")}
        >
          Review {fmtCount(rows.length)}
          <Icon name="forward" size={12} />
        </button>
      </div>
    </SideCard>
  );
}

/** Unsaved drafts, then notes you opened lately, then notes changed lately (in another editor or here). */
function ContinueCard() {
  const [changed, setChanged] = useState<FileSummary[]>([]);
  const [drafts, setDrafts] = useState<Draft[]>([]);
  const v = useVaultVersion();
  const opened = recentDocs();
  const [failed, setFailed] = useState(false);
  const [tries, setTries] = useState(0);
  const loadDrafts = () =>
    void api
      .draftsList()
      .then((d) => setDrafts(d.sort((a, b) => b.at - a.at)))
      .catch(() => setFailed(true));
  useEffect(loadDrafts, [v, tries]);
  useEffect(() => {
    let live = true;
    setFailed(false);
    api
      .filesList("note")
      .then((r) => live && setChanged(r.sort((a, b) => b.mtime - a.mtime).slice(0, 8)))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [v, tries]);
  const drafted = new Set(drafts.map((d) => d.path));
  const shownDrafts = drafts.slice(0, 4);
  // Six rows at most: drafts first, then opened, then changed.
  const room = Math.max(0, CONTINUE_MAX - shownDrafts.length);
  const openedRows = opened.filter((o) => !drafted.has(o.path)).slice(0, room);
  const changedRows = changed
    .filter((f) => !opened.some((o) => o.path === f.path) && !drafted.has(f.path))
    .slice(0, room - openedRows.length);
  // A label per kind only when there's more than one kind to tell apart.
  const kinds = [shownDrafts.length, openedRows.length, changedRows.length].filter(Boolean).length;
  const empty = !shownDrafts.length && !openedRows.length && !changedRows.length;
  return (
    <SideCard title="Continue where you left off">
      {failed && (
        <LoadFailed
          compact
          what="your recent notes"
          onRetry={() => {
            setFailed(false);
            setTries((n) => n + 1);
          }}
        />
      )}
      {empty && !failed && <p className="faint rempty">Notes you open or change show here.</p>}
      {shownDrafts.map((d) => (
        <div key={d.path} className="rrow draftrow">
          <button
            type="button"
            className="rlink grow"
            title={`Open ${d.path} with its unsaved changes (from ${ago(d.at)})`}
            onClick={() => openDoc(d.path)}
          >
            <span className="ell">{d.path.split("/").pop()?.replace(/\.md$/, "")}</span>
          </button>
          <span className="amber rstat" title={`Unsaved changes from ${ago(d.at)}`}>
            Unsaved
          </span>
          <button
            type="button"
            className="btn sm ghost"
            title="Throw the unsaved changes away"
            onClick={() => void api.draftDiscard(d.path).then(loadDrafts)}
          >
            Discard
          </button>
        </div>
      ))}
      {kinds > 1 && openedRows.length > 0 && <div className="rsub">Opened</div>}
      {openedRows.map((o) => (
        <DocRow key={o.path} path={o.path} name={o.title} when={`Opened ${ago(o.at)}`} age={ago(o.at)} />
      ))}
      {kinds > 1 && changedRows.length > 0 && <div className="rsub">Changed lately</div>}
      {changedRows.map((f) => (
        <DocRow key={f.path} path={f.path} name={f.title} when={`Changed ${ago(f.mtime)}`} age={ago(f.mtime)} />
      ))}
      {!empty && (
        <div className="rfoot">
          <button
            type="button"
            className="btn sm ghost ract"
            title="Go to Activity, everything that changed in the vault"
            onClick={() => nav.go("activity")}
          >
            More in Activity
            <Icon name="forward" size={12} />
          </button>
        </div>
      )}
    </SideCard>
  );
}

const CONTINUE_MAX = 6;

function DocRow({ path, name, when, age }: { path: string; name: string; when: string; age: string }) {
  return (
    <button type="button" className="rrow rlink" title={`Open ${path}\n${when}`} onClick={() => openDoc(path)}>
      <Icon name="note" size={13} />
      <span className="ell grow">{name}</span>
      <span className="faint rstat">{age}</span>
    </button>
  );
}
