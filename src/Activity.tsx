// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Activity (§4): a heatmap of file changes by day, and Sync & ingest. A day in the heatmap lists
// the files that changed on it, with a link to what Brainstead itself changed that day in
// Changes › All, where the vault's log.md now shows (D-20261119-04).

import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ActivityDay, api, FileSummary } from "./api";
import { Icon } from "./icons";
import { nav, openDoc, useViewState } from "./nav";
import { lastSeen, useVaultVersion } from "./state";
import { TopBar } from "./TopBar";
import { fmtCount, LayerTag, Seg } from "./ui";
import { buildGrid, cellTip, fmtDate, iso } from "./activityModel";
import { SyncFeed } from "./SyncFeed";

/** Activity's views: files by the day they changed, or what the sync brought in and each ingest
 *  did (D-20261119-01). "log", Activity's log before it moved to Changes, reads as "files". */
export type ActivityView = "files" | "sync";

export function ActivityScreen() {
  const version = useVaultVersion();
  // The last load shows at once on reopening, while the vault is read again.
  const [data, setData] = useState(() => lastSeen.get<{ days: ActivityDay[] }>("activity"));
  const [error, setError] = useState("");
  const [day, setDay] = useViewState<string | null>("activity.day", null);
  const [stored, setView] = useViewState<ActivityView | "log">("activity.view", "files");
  const view: ActivityView = stored === "sync" ? "sync" : "files";

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

  // Stable, so the heatmap's cells aren't redrawn when anything else changes.
  const dayRef = useRef(day);
  useEffect(() => {
    dayRef.current = day;
  }, [day]);
  const setDayRef = useRef(setDay);
  useEffect(() => {
    setDayRef.current = setDay;
  });
  const pickDay = useCallback((d: string) => setDayRef.current(d === dayRef.current ? null : d), []);
  // With no day picked, the latest day anything changed.
  const latest = useMemo(() => data?.days.reduce<string | null>((m, d) => (!m || d.date > m ? d.date : m), null) ?? null, [data]);
  const shown = day ?? latest;
  const sub = view === "sync" ? "Sync & ingest" : day ? fmtDate(day) : undefined;

  return (
    <main className="main">
      <TopBar
        title="Activity"
        sub={sub}
        ask={
          view === "sync"
            ? { about: "what the sync brought in and what was ingested", prompt: "About what Brainstead synced and ingested lately: " }
            : shown
              ? { about: `what changed on ${shown}`, prompt: `About what changed in the vault on ${shown}: ` }
              : { about: "what changed in the vault lately", prompt: "About what changed in the vault lately: " }
        }
      >
        <Seg<ActivityView>
          label="Show"
          value={view}
          onChange={setView}
          options={[
            ["files", "Files by day", "The vault's files by the day they last changed"],
            ["sync", "Sync & ingest", "What the sync brought in from Teams, Outlook and Slack, and what each ingest did with it"],
          ]}
        />
      </TopBar>
      <div className="body activity">
        {view === "sync" ? (
          <div className="actmain">
            <SyncFeed />
          </div>
        ) : (
          <div className="actmain">
            {error && <p className="faint">{error}</p>}
            {data && <Heatmap days={data.days} day={shown} onDay={pickDay} />}
            {shown && <DayFiles day={shown} picked={!!day} onClear={() => setDay(null)} />}
            {data && !shown && <p className="faint actempty">Nothing has changed in the vault yet.</p>}
          </div>
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

/** The files that last changed on the chosen day (the latest day, when none is picked), and a link
 *  to what Brainstead changed that day in Changes › All. */
function DayFiles({ day, picked, onClear }: { day: string; picked: boolean; onClear: () => void }) {
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
        <button
          type="button"
          className="btn sm"
          title="Open Changes › All on this day: what assistants and Brainstead's runs changed, and the vault's log"
          onClick={() => nav.go({ screen: "review", view: { "review:view": "all", "review:day": day, "review:q": "" } })}
        >
          <Icon name="review" size={12} />
          Changes that day
        </button>
        {picked && (
          <button type="button" className="btn sm ghost" title="Stop showing this day; back to the latest day" onClick={onClear}>
            <Icon name="x" size={12} />
            Clear the day
          </button>
        )}
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
