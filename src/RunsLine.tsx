// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The sidebar's Running line (D-20261120-03): while something runs, what it is and how far it has
// got; a click opens the list the menu-bar window shows, each run with its Stop and where it lives,
// and the runs that ended this session. For half an hour after the last one ends it says so.

import { useEffect, useRef, useState } from "react";
import { Icon } from "./icons";
import { nav } from "./nav";
import { Finished, Running, runsNow, sceneRuns, shownRuns } from "./runs";
import { useStore } from "./store";
import { ago, Popover } from "./ui";

/** How long the line says the last run ended, with nothing running. */
const RECENT_MS = 30 * 60_000;

const pct = (p: number | null) => (p === null ? "" : ` · ${Math.round(p * 100)}%`);

/** The line's words: the one run and its progress, or how many. */
export function runsLineText(running: Running[]): string {
  const { shown, queued } = shownRuns(running);
  if (shown.length === 1 && !queued) return `${shown[0].short}${pct(shown[0].progress)}`;
  const going = shown.length ? `${shown.length} running` : "";
  const waiting = queued ? `${queued} waiting` : "";
  return [going, waiting].filter(Boolean).join(", ");
}

export function RunsLine({ rail }: { rail: boolean }) {
  const { running, finished } = useStore(runsNow);
  const [at, setAt] = useState<DOMRect | null>(null);
  const btn = useRef<HTMLButtonElement>(null);
  // A scene that asks for the list open.
  useEffect(() => {
    if (sceneRuns.get() === "open" && btn.current) setAt(btn.current.getBoundingClientRect());
  }, []);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(t);
  }, []);
  const last = finished[0];
  const recent = !running.length && last && now - last.at < RECENT_MS;
  if (!running.length && !recent && !at) return null;
  const text = running.length
    ? runsLineText(running)
    : last
      ? `${last.failed ? "Failed" : "Finished"}: ${last.short} · ${ago(last.at, now)}`
      : "";
  const tip = running.length
    ? `${running.map((r) => r.label).join("\n")}\nClick for each run's progress and Stop`
    : `${last?.text ?? ""}\nClick for the runs that ended`;
  return (
    <>
      {(running.length > 0 || recent) && (
        <button
          ref={btn}
          type="button"
          className={`runsline${rail ? " rail" : ""}${running.length ? " going" : ""}${last?.failed && !running.length ? " bad" : ""}`}
          title={rail ? `${text}\n${tip}` : tip}
          aria-label={rail ? text : undefined}
          onClick={(e) => setAt(at ? null : e.currentTarget.getBoundingClientRect())}
        >
          {running.length ? <span className="spin" aria-hidden="true" /> : <Icon name={last?.failed ? "alert" : "check"} size={12} />}
          {rail ? running.length > 1 && <span className="runsn">{running.length}</span> : <span className="ell">{text}</span>}
        </button>
      )}
      {at && <RunsPopover at={at} running={running} finished={finished} now={now} onClose={() => setAt(null)} />}
    </>
  );
}

function RunsPopover({
  at,
  running,
  finished,
  now,
  onClose,
}: {
  at: DOMRect;
  running: Running[];
  finished: Finished[];
  now: number;
  onClose: () => void;
}) {
  const { shown, queued } = shownRuns(running);
  const go = (p: Running["place"]) => {
    onClose();
    nav.go(p);
  };
  return (
    <Popover anchor={at} onClose={onClose} width={340} place="above">
      <div className="runspop" role="dialog" aria-label="Running">
        <div className="mhead faint small">{running.length ? "Running" : "Nothing running"}</div>
        {shown.map((r) => (
          <div key={r.key} className="tray-run">
            <div className="tray-runhead">
              <span className="spin" aria-hidden="true" />
              <button type="button" className="linkish grow ell" title={`Open where this run is: ${r.label}`} onClick={() => go(r.place)}>
                {r.label}
              </button>
              <button type="button" className="btn sm ghost" title={`Stop: ${r.label.toLowerCase()}`} onClick={() => void r.stop()}>
                Stop
              </button>
            </div>
            <div className="tray-progress">
              <i style={{ width: `${Math.round((r.progress ?? 0.08) * 100)}%` }} className={r.progress === null ? "indef" : ""} />
            </div>
            <span className="faint small ell">{r.doing}</span>
          </div>
        ))}
        {queued > 0 && (
          <button
            type="button"
            className="tray-queued faint small"
            title="Open Sources, where the ingest runs list each one waiting, with its Stop"
            onClick={() => go({ screen: "sources" })}
          >
            {queued} more waiting their turn
          </button>
        )}
        {finished.length > 0 && (
          <>
            <div className="mhead faint small">Ended</div>
            {finished.map((f) => (
              <button
                key={f.key}
                type="button"
                className={`runsdone${f.failed ? " bad" : ""}`}
                title={`Open where it is: ${f.text}`}
                onClick={() => go(f.place)}
              >
                <Icon name={f.failed ? "alert" : "check"} size={12} />
                <span className="grow ell">{f.text}</span>
                <span className="faint small">{ago(f.at, now)}</span>
              </button>
            ))}
          </>
        )}
        <button
          type="button"
          className="linkish small"
          title="Open Settings › Automations: the scheduled runs, with Run now"
          onClick={() => go({ screen: "settings", pane: "jobs" })}
        >
          Automations
        </button>
      </div>
    </Popover>
  );
}
