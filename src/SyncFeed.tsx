// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Activity › Sync & ingest (D-20261119-01): what the sync brought in and what each ingest did with
// it, newest first, from src/syncActivity.ts.

import { useEffect, useMemo, useState } from "react";
import { api, IngestRun, SyncLog } from "./api";
import { Icon } from "./icons";
import { openDoc, useViewState } from "./nav";
import { useGone } from "./docButton";
import { lastSeen } from "./state";
import { ago, FEED_FILTERS, FeedFilter, FeedRow, FeedStatus, nameOf, syncFeed, syncLine } from "./syncActivity";

const ICON: Record<FeedStatus, [string, string]> = {
  ok: ["check", "var(--green)"],
  nothing: ["minus", "var(--ink3)"],
  trashed: ["trash", "var(--ink3)"],
  running: ["refresh", "var(--amber)"],
  failed: ["alert", "var(--red)"],
  blocked: ["alert", "var(--amber)"],
  stopped: ["stop", "var(--ink3)"],
};

/** How many rows are drawn before Show more. */
const PAGE = 150;

export function useSyncFeedData(): { log: SyncLog; runs: IngestRun[] } | null {
  const [data, setData] = useState(() => lastSeen.get<{ log: SyncLog; runs: IngestRun[] }>("syncfeed"));
  useEffect(() => {
    let live = true;
    const load = () =>
      void Promise.all([api.syncLog(), api.ingestRuns()])
        .then(([log, runs]) => live && setData(lastSeen.set("syncfeed", { log, runs })))
        .catch(() => {});
    load();
    const t = window.setInterval(load, 30000);
    const off = api.onIngestChanged(() => load());
    return () => {
      live = false;
      window.clearInterval(t);
      void off.then((f) => f());
    };
  }, []);
  return data;
}

export function SyncFeed() {
  const data = useSyncFeedData();
  const [filter, setFilter] = useViewState<FeedFilter>("activity.sync", "all");
  const [shown, setShown] = useState(PAGE);
  const rows = useMemo(() => (data ? syncFeed(data.log, data.runs, filter) : []), [data, filter]);
  const line = data && syncLine(data.log, data.runs);
  const gone = useGone(rows.slice(0, shown).flatMap((r) => r.paths));
  if (!data) return <p className="faint">Loading…</p>;
  return (
    <div className="syncfeed">
      <div className="actfilters">
        {/* Only the problems are amber: when it last synced and what came in are just information. */}
        <span className="faint small">
          {line ? (
            line.problems ? (
              <>
                {line.text.slice(0, -line.problems.length)}
                <span className="amber">{line.problems}</span>
              </>
            ) : (
              line.text
            )
          ) : (
            "Nothing has synced or been ingested yet."
          )}
        </span>
        <div className="chips">
          {FEED_FILTERS.map(([v, label, tip]) => (
            <button
              key={v}
              type="button"
              className={`chip f ${filter === v ? "on" : ""}`}
              title={filter === v ? tip : `Show ${v === "all" ? "everything" : `only ${label}`}: ${tip.toLowerCase()}`}
              onClick={() => {
                setFilter(v);
                setShown(PAGE);
              }}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      <div className="actrows card">
        {!rows.length && (
          <p className="faint srow">
            {filter === "all" ? "No sync passes or ingests in the last two weeks." : "Nothing here in the last two weeks."}
          </p>
        )}
        {rows.slice(0, shown).map((r) => (
          <Row key={r.key} r={r} gone={gone} />
        ))}
        {rows.length > shown && (
          <div className="srow">
            <button type="button" className="btn sm" title="Show older entries" onClick={() => setShown((n) => n + PAGE)}>
              Show more
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function Row({ r, gone }: { r: FeedRow; gone: (p: string) => boolean }) {
  const [icon, colour] = ICON[r.status];
  return (
    <div className={`srow syncrow ${r.status}`}>
      <Icon name={icon} style={{ color: colour }} />
      <div className="t grow">
        <div className="pt">{r.title}</div>
        {r.groups.map(([label, paths]) => (
          <div key={label} className="synclinks">
            <span className="faint small">{label}:</span>
            {paths.slice(0, 8).map((p) => (
              <button
                key={p}
                type="button"
                className="tlink small"
                disabled={gone(p)}
                title={gone(p) ? `${p} isn't there any more` : `Open ${p}`}
                onClick={() => openDoc(p)}
              >
                {nameOf(p)}
              </button>
            ))}
            {paths.length > 8 && <span className="faint small">and {paths.length - 8} more</span>}
          </div>
        ))}
        {r.detail.map((d) => (
          <div key={d} className="faint small">
            {d}
          </div>
        ))}
      </div>
      <span className="faint small nowrap" title={r.at.replace("T", " ")}>
        {ago(r.at)}
      </span>
    </div>
  );
}
