// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useState } from "react";
import { api } from "./api";
import { Icon } from "./icons";
import { reportEditError, undoLast } from "./taskModel";
import { toast } from "./Toast";
import { Dialog } from "./ui";

/** The initiatives an action can move to: every one but its own, active ones first. */
export function moveTargets(all: [string, string, string][], from: string): [string, string, string][] {
  const rank = (s: string) => (s === "active" ? 0 : s === "paused" ? 1 : 2);
  return all.filter(([, p]) => p !== from).sort((a, b) => rank(a[2]) - rank(b[2]));
}

/** Move to…: picks another initiative for an action and moves it there, whole (D-20261118-26).
 *  `line`/`lineText` are the action's line on `path`, its initiative's page. */
export function MoveAction({
  path,
  line,
  lineText,
  onClose,
  onDone,
}: {
  path: string;
  line: number;
  lineText: string;
  onClose: () => void;
  onDone?: () => void;
}) {
  const [all, setAll] = useState<[string, string, string][] | null>(null);
  const [q, setQ] = useState("");
  useEffect(() => {
    api
      .initiativesList()
      .then(setAll)
      .catch(() => setAll([]));
  }, []);
  const shown = moveTargets(all ?? [], path).filter(([n]) => n.toLowerCase().includes(q.trim().toLowerCase()));
  const move = async (to: string) => {
    try {
      const said = await api.initiativeActionMove(path, line, lineText, to);
      toast(said, { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok");
      onDone?.();
      onClose();
    } catch (e) {
      reportEditError(e);
    }
  };
  return (
    <Dialog onClose={onClose} width={460} label="Move to another initiative">
      <div className="confirm">
        <h2 className="h2">Move to…</h2>
        <p className="muted small">The action moves whole, with its quotes, and its task follows it.</p>
        <label className="inp">
          <Icon name="search" size={14} />
          <input autoFocus placeholder="Find an initiative" value={q} onChange={(e) => setQ(e.target.value)} />
        </label>
        <div className="ifile">
          {all === null && <p className="faint small">Loading…</p>}
          {shown.map(([name, p, status]) => (
            <button key={p} type="button" className="blink" title={`Move the action to ${name}`} onClick={() => void move(p)}>
              <span className="lbt">
                <span className="ell">{name}</span>
              </span>
              {status !== "active" && <span className="faint">{status}</span>}
            </button>
          ))}
          {all !== null && !shown.length && <p className="faint small">No other initiative matches.</p>}
        </div>
      </div>
    </Dialog>
  );
}
