// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Activity (§4): `log.md`'s entries newest first with action filters and a search, beside a heatmap
// of file changes by day. A day in the heatmap, or an entry's date, filters the log to that day and
// lists the files that changed on it.

import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ActivityDay, api, FileSummary, LogEntry } from "./api";
import { Icon } from "./icons";
import { fieldIcon, fieldsAsWords, fieldTip, splitTaskFields } from "./taskFields";
import { openDoc, useViewState } from "./nav";
import { lastSeen, useVaultVersion } from "./state";
import { useGlance, VaultGlance } from "./Glance";
import { TopBar } from "./TopBar";
import { fmtCount, LayerTag, SearchBox, Seg, TableBand, useDebounced } from "./ui";
import { settings, useStore } from "./store";
import { actionLabel, actionsIn, buildGrid, cellTip, filterLog, fmtDate, iso, linkTarget, marks, rankLog } from "./activityModel";

const PAGE = 200;

export function ActivityScreen() {
  const version = useVaultVersion();
  const glance = useGlance();
  // The last load shows at once on reopening, while the log is read again.
  const [data, setData] = useState(() => lastSeen.get<{ log: LogEntry[]; days: ActivityDay[] }>("activity"));
  const [error, setError] = useState("");
  const [q, setQ] = useViewState("activity.q", "");
  const [action, setAction] = useViewState<string | null>("activity.action", null);
  const [day, setDay] = useViewState<string | null>("activity.day", null);
  const [shown, setShown] = useState(PAGE);
  const [active, setActive] = useState(0);
  const dq = useDebounced(q);
  const searching = dq.trim().length >= 2;
  // While searching: best matches first, or the latest first (the switch by the box, as on Notes).
  const latest = useStore(settings).searchOrder?.activity === "latest";

  useEffect(() => {
    let live = true;
    api
      .activity()
      .then((d) => {
        if (!live) return;
        setData(lastSeen.set("activity", d));
        setError("");
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [version]);

  useEffect(() => {
    setShown(PAGE);
    setActive(0);
  }, [dq, action, day, latest]);

  // Stable, so the heatmap's cells aren't redrawn when anything else changes.
  const dayRef = useRef(day);
  useEffect(() => {
    dayRef.current = day;
  }, [day]);
  const setDayRef = useRef(setDay);
  useEffect(() => {
    setDayRef.current = setDay;
  });
  const showDay = useCallback((d: string) => setDayRef.current(d), []);
  const pickDay = useCallback((d: string) => setDayRef.current(d === dayRef.current ? null : d), []);

  const log = useMemo(() => {
    if (!data) return [];
    const m = filterLog(data.log, { action, day, q: searching ? dq : "" });
    return searching && !latest ? rankLog(m, dq) : m;
  }, [data, action, day, dq, searching, latest]);
  const idx = Math.min(active, Math.max(0, log.length - 1));
  if (idx >= shown) setShown(idx + PAGE);
  const visible = useMemo(() => log.slice(0, shown), [log, shown]);
  // Entries come under a band per day, unless a search ranks them by how well they match.
  const banded = !(searching && !latest);
  const perDay = useMemo(() => {
    const m = new Map<string, number>();
    for (const e of log) m.set(e.date, (m.get(e.date) ?? 0) + 1);
    return m;
  }, [log]);

  // Each title that names a file links to it.
  const [paths, setPaths] = useState<Map<string, string | null>>(new Map());
  useEffect(() => {
    setPaths(new Map());
  }, [version]);
  useEffect(() => {
    const want = [...new Set(visible.map((e) => linkTarget(e.title)))].filter((t) => !paths.has(t));
    if (!want.length) return;
    let live = true;
    api
      .linksResolve(want)
      .then((r) => live && setPaths((m) => new Map([...m, ...want.map((t, i) => [t, r[i]] as [string, string | null])])))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [visible, paths]);

  // The arrow keys move through the matches, Enter opens one (or shows its day), Escape clears.
  const key = (e: React.KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const n = log.length;
    if (e.key === "Escape") setQ("");
    else if (!n) return;
    else if (e.key === "ArrowDown") setActive((idx + 1) % n);
    else if (e.key === "ArrowUp") setActive((idx - 1 + n) % n);
    else if (e.key === "Enter") {
      const it = log[idx];
      const path = paths.get(linkTarget(it.title));
      if (path) openDoc(path);
      else showDay(it.date);
    } else return;
    e.preventDefault();
  };

  // Entries are drawn PAGE at a time, more as the end comes into view (as the notes list does).
  const end = useRef<HTMLDivElement>(null);
  const more = shown < log.length;
  useEffect(() => {
    const el = end.current;
    if (!el || !more) return;
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && setShown((n) => n + PAGE), { rootMargin: "400px" });
    io.observe(el);
    return () => io.disconnect();
  }, [more, shown]);
  const actions = useMemo(() => (data ? actionsIn(data.log) : []), [data]);
  const sub = day ? fmtDate(day) : undefined;

  return (
    <main className="main">
      <TopBar
        title="Activity"
        sub={sub}
        ask={
          day
            ? { about: `what changed on ${day}`, prompt: `About what changed in the vault on ${day}: ` }
            : { about: "what changed in the vault lately", prompt: "About what changed in the vault lately: " }
        }
      />
      <div className="body activity">
        <div className="actmain">
          {error && <p className="faint">{error}</p>}
          {data && <Heatmap days={data.days} day={day} onDay={pickDay} />}
          {day && <DayFiles day={day} onClear={() => setDay(null)} />}
          {data && (
            <div className="actfilters">
              <SearchBox value={q} onChange={setQ} onKeyDown={key} placeholder="Search activity" className="listq">
                {searching && (
                  <span className="faint nav">
                    {log.length ? `${idx + 1}/${log.length}` : "0"}
                    <span className="kbd">↑↓</span>
                  </span>
                )}
              </SearchBox>
              {/* The order only means something while there are search results. */}
              {searching && (
                <Seg
                  label="Search results order"
                  value={latest ? "latest" : "ranked"}
                  options={[
                    ["ranked", "Best match", "Search results with the best matches first"],
                    ["latest", "Latest", "Search results with the newest first"],
                  ]}
                  onChange={(v) => settings.update({ searchOrder: { ...settings.get().searchOrder, activity: v } })}
                />
              )}
              {actions.length > 0 && (
                <div className="chips">
                  <button
                    type="button"
                    className={`chip f ${action ? "" : "on"}`}
                    title="Show every kind of change"
                    onClick={() => setAction(null)}
                  >
                    All
                  </button>
                  {actions.map((a) => (
                    <button
                      key={a}
                      type="button"
                      className={`chip f ${action === a ? "on" : ""}`}
                      title={action === a ? "Show every kind of change again" : `Show only “${actionLabel(a)}” changes`}
                      onClick={() => setAction(action === a ? null : a)}
                    >
                      {actionLabel(a)}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
          {data && (
            <LogTable
              count={
                log.length === data.log.length
                  ? `${fmtCount(log.length)} ${log.length === 1 ? "entry" : "entries"}`
                  : `${fmtCount(log.length)} of ${fmtCount(data.log.length)} entries`
              }
              entries={visible}
              paths={paths}
              q={searching ? dq.trim() : ""}
              active={searching ? idx : -1}
              perDay={banded ? perDay : null}
              empty={!data.log.length ? "log.md has no entries yet." : searching ? `Nothing matches “${dq.trim()}”.` : "Nothing matches."}
              onDay={showDay}
            >
              {more && (
                <div ref={end} className="faint actmore">
                  Loading more…
                </div>
              )}
            </LogTable>
          )}
        </div>
        {glance && (
          <aside className="actside">
            <VaultGlance g={glance} />
          </aside>
        )}
      </div>
    </main>
  );
}

function Heatmap({ days, day, onDay }: { days: ActivityDay[]; day: string | null; onDay: (d: string) => void }) {
  const files = useMemo(() => days.reduce((n, d) => n + d.notes + d.wiki + d.sources, 0), [days]);
  return (
    <div className="actheat">
      <div className="acthead">
        <span className="eyebrow">Files by the day they last changed</span>
        {/* Templates aren't counted, so this can be fewer than the sidebar's count of files. */}
        <span className="faint" title="Templates aren't counted here">
          {fmtCount(files)} notes, wiki pages and sources · {fmtCount(days.length)} days
        </span>
        <div className="sp" />
        <div className="actkey faint">
          Less
          {[0, 1, 2, 3, 4].map((l) => (
            <i key={l} className={`hc l${l}`} />
          ))}
          More
        </div>
      </div>
      {/* The chosen day is marked by one rule, so choosing one doesn't redraw years of cells. */}
      {day && /^\d{4}-\d{2}-\d{2}$/.test(day) && (
        <style>{`.acthm [data-d="${day}"] { outline: 2px solid var(--ink); outline-offset: 1px; }`}</style>
      )}
      <HeatCells days={days} onDay={onDay} />
    </div>
  );
}

const HeatCells = memo(function HeatCells({ days, onDay }: { days: ActivityDay[]; onDay: (d: string) => void }) {
  const today = iso(new Date());
  const grid = useMemo(() => buildGrid(days, today), [days, today]);
  const scroller = useRef<HTMLDivElement>(null);
  const [can, setCan] = useState({ back: false, on: false });
  const sync = useCallback(() => {
    const el = scroller.current;
    if (!el) return;
    const max = el.scrollWidth - el.clientWidth;
    setCan((c) => {
      const n = { back: el.scrollLeft > 1, on: el.scrollLeft < max - 1 };
      return c.back === n.back && c.on === n.on ? c : n;
    });
  }, []);
  // Opens on the latest weeks; scrolling back stays where it is until the number of weeks changes.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el) el.scrollLeft = el.scrollWidth;
    sync();
  }, [grid.weeks.length, sync]);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(sync);
    ro.observe(el);
    return () => ro.disconnect();
  }, [sync]);
  const page = (dir: number) => {
    const el = scroller.current;
    if (el) el.scrollBy({ left: dir * Math.max(120, el.clientWidth - 80), behavior: "smooth" });
  };
  return (
    <div className="actstrip">
      <button
        type="button"
        className="ibtn sm actpage"
        aria-label="Earlier weeks"
        title="Earlier"
        disabled={!can.back}
        onClick={() => page(-1)}
      >
        <Icon name="back" size={14} />
      </button>
      <div className="actscroll" ref={scroller} onScroll={sync}>
        <div
          className="acthm"
          style={{ gridTemplateColumns: `repeat(${grid.weeks.length}, 13px)` }}
          onClick={(e) => {
            const d = (e.target as HTMLElement).dataset?.d;
            if (d) onDay(d);
          }}
        >
          {grid.months.map((m) => (
            <span key={m.week} className="actmonth" style={{ gridColumn: m.week + 1 }}>
              {m.label}
              {m.year && <b> {m.year}</b>}
            </span>
          ))}
          {grid.weeks.map((w, i) =>
            w.map((c, j) =>
              c.future ? (
                <i key={c.date} className="hc future" style={{ gridColumn: i + 1, gridRow: j + 2 }} />
              ) : (
                <button
                  key={c.date}
                  type="button"
                  data-d={c.date}
                  className={`hc l${c.level}${c.date === today ? " today" : ""}`}
                  style={{ gridColumn: i + 1, gridRow: j + 2 }}
                  title={cellTip(c)}
                />
              ),
            ),
          )}
        </div>
      </div>
      <button type="button" className="ibtn sm actpage" aria-label="Later weeks" title="Later" disabled={!can.on} onClick={() => page(1)}>
        <Icon name="forward" size={14} />
      </button>
    </div>
  );
});

/** The files that last changed on the chosen day. */
function DayFiles({ day, onClear }: { day: string; onClear: () => void }) {
  const version = useVaultVersion();
  const [files, setFiles] = useState<FileSummary[] | null>(null);
  useEffect(() => {
    let live = true;
    api.activityDay(day).then((f) => live && setFiles(f));
    return () => {
      live = false;
    };
  }, [day, version]);
  return (
    <div className="actday">
      <div className="acthead">
        <b>{fmtDate(day)}</b>
        <span className="faint">
          {files ? `${fmtCount(files.length)} ${files.length === 1 ? "file" : "files"} last changed that day` : ""}
        </span>
        <div className="sp" />
        <button type="button" className="btn sm" title="Stop showing this day; back to the whole log" onClick={onClear}>
          <Icon name="x" size={12} />
          Clear the day
        </button>
      </div>
      {files && files.length > 0 && (
        <div className="actfiles">
          {files.map((f) => (
            <button key={f.path} type="button" className="actfile" onClick={() => openDoc(f.path)} title={f.path}>
              <LayerTag layer={f.layer} />
              <span className="t">{f.title}</span>
              <span className="faint tnum">{new Date(f.mtime).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** "Wed 30 Sep", with the year only when it isn't this one. */
const dayLabel = (d: string) => {
  const s = fmtDate(d).replace(",", "");
  return d.startsWith(String(new Date().getFullYear())) ? s.replace(/ \d{4}$/, "") : s;
};

/** The log: the header stays put and the rows scroll under it, under a band per day when `perDay`
 *  is given (each day's count). `children` goes after the rows. */
function LogTable({
  count,
  entries,
  paths,
  q,
  active,
  perDay,
  empty,
  onDay,
  children,
}: {
  /** How many entries are shown, of how many, for the header. */
  count: string;
  entries: LogEntry[];
  paths: Map<string, string | null>;
  q: string;
  active: number;
  perDay: Map<string, number> | null;
  empty: string;
  onDay: (d: string) => void;
  children?: React.ReactNode;
}) {
  const rows = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (active >= 0) rows.current?.querySelector(`[data-row="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);
  if (!entries.length) return <p className="faint actempty">{empty}</p>;
  const seen = new Map<string, number>();
  const row = (e: LogEntry, i: number) => {
    const key = `${e.date}|${e.time}|${e.action}|${e.title}`;
    // Same-looking entries get a count, so keys stay stable as filters change.
    const n = (seen.get(key) ?? 0) + 1;
    seen.set(key, n);
    return (
      <LogRow
        key={`${key}|${n}`}
        e={e}
        row={i}
        on={i === active}
        q={q}
        path={paths.get(linkTarget(e.title)) ?? null}
        onDay={perDay ? null : onDay}
      />
    );
  };
  // Runs of entries on the same day, in the order shown.
  const days: { date: string; at: number; entries: LogEntry[] }[] = [];
  if (perDay)
    entries.forEach((e, i) => {
      const last = days[days.length - 1];
      if (last && last.date === e.date) last.entries.push(e);
      else days.push({ date: e.date, at: i, entries: [e] });
    });
  return (
    <div className={`actlog ${perDay ? "banded" : ""}`} role="table" aria-label="The vault's log">
      <div className="ar tb-head" role="row">
        <span role="columnheader">{perDay ? "Time" : "When"}</span>
        <span role="columnheader">Item</span>
        <span role="columnheader" className="itemh">
          Action
          <span className="count">{count}</span>
        </span>
      </div>
      <div className="actrows" role="rowgroup" ref={rows}>
        {perDay
          ? days.map((d) => {
              const n = perDay.get(d.date) ?? d.entries.length;
              return (
                <div key={`${d.date}|${d.at}`} className="tb-group" role="rowgroup">
                  <TableBand
                    icon="calendar"
                    label={
                      <button
                        type="button"
                        className="linkish"
                        title="Show this day and the files that changed on it"
                        onClick={() => onDay(d.date)}
                      >
                        {dayLabel(d.date)}
                      </button>
                    }
                    n={n}
                    tip={`${n} ${n === 1 ? "entry" : "entries"} that day`}
                  />
                  {d.entries.map((e, j) => row(e, d.at + j))}
                </div>
              );
            })
          : entries.map(row)}
        {children}
      </div>
    </div>
  );
}

/** Text with its task fields (a log line quoting a task) drawn as their icons, never the file's emoji. */
function FieldText({ text, q }: { text: string; q: string }) {
  return (
    <>
      {splitTaskFields(text).map((p, i) =>
        typeof p === "string" ? (
          <Marked key={i} text={p} q={q} />
        ) : (
          <span key={i} className="dfield" title={fieldTip(p, p.value)}>
            <Icon name={fieldIcon(p)} size={12} />
            {p.value}
          </span>
        ),
      )}
    </>
  );
}

function Marked({ text, q }: { text: string; q: string }) {
  if (!q) return <>{text}</>;
  return (
    <>
      {marks(text, q).map((m, i) =>
        m.hit ? (
          <mark key={i} className="search-hit">
            {m.text}
          </mark>
        ) : (
          <span key={i}>{m.text}</span>
        ),
      )}
    </>
  );
}

const LogRow = memo(function LogRow({
  e,
  row,
  on,
  q,
  path,
  onDay,
}: {
  e: LogEntry;
  row: number;
  on: boolean;
  q: string;
  path: string | null;
  /** Without a band per day, the row says its day, which shows that day. */
  onDay: ((d: string) => void) | null;
}) {
  const first = e.description.split("\n").find((l) => l.trim()) ?? "";
  return (
    <div className={`ar tb-row ${on ? "sel" : ""}`} role="row" data-row={row}>
      <span className="when">
        {onDay && (
          <button type="button" className="linkish" title="Show this day" onClick={() => onDay(e.date)}>
            {dayLabel(e.date)}
          </button>
        )}
        {e.time && <span className="faint tnum">{e.time}</span>}
      </span>
      <span className="item" title={e.description ? fieldsAsWords(e.description) : undefined}>
        {path ? (
          <button type="button" className="linkish strong" title={`Open ${path}`} onClick={() => openDoc(path)}>
            <Marked text={linkTarget(e.title)} q={q} />
          </button>
        ) : (
          <span className="strong">
            <FieldText text={e.title} q={q} />
          </span>
        )}
        {first && (
          <span className="faint">
            {" · "}
            <FieldText text={first.replace(/^[-*]\s+/, "")} q={q} />
          </span>
        )}
      </span>
      <span>
        <span className="chip">{actionLabel(e.action)}</span>
      </span>
    </div>
  );
});
