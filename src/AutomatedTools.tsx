// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Your own tools that start Claude Code sessions (Settings › Jobs & schedule): the daily summary
// counts their sessions as automated instead of writing them up as your work. Each tool is a name
// and what its sessions have in common: a folder they run in, the words their first message opens
// with, or both. Folders whose recent sessions look like a tool's are offered to add in one click.

import { useEffect, useState } from "react";
import { api, Automation, AutomationSuggestion } from "./api";
import { Icon } from "./icons";
import { reportEditError } from "./taskModel";
import { toast } from "./Toast";

/** A folder's last part, for a tool's name: `/work/digest` → `digest`. */
const folderName = (cwd: string) => cwd.split(/[\\/]/).filter(Boolean).pop() ?? cwd;

/** A suggested folder as a tool to list: named after the folder, its sessions matched by it. */
export const fromSuggestion = (g: AutomationSuggestion): Automation => ({ label: folderName(g.cwd), cwd_contains: g.cwd, opening: "" });

/** Whether a listed tool already takes in a suggested folder's sessions. */
export const covers = (t: Automation, g: AutomationSuggestion) => !!t.cwd_contains.trim() && g.cwd.includes(t.cwd_contains.trim());

// Saved only once a row has a name and something to match on: a half-typed one stays here.
const complete = (a: Automation) => !!a.label.trim() && !!(a.cwd_contains.trim() || a.opening.trim());

/** What to save, or null while the saved list hasn't loaded: saving then would overwrite every saved tool. */
export const toSave = (list: Automation[] | null): Automation[] | null => (list ? list.filter(complete) : null);

export function AutomatedTools() {
  // null until the saved list loads; it stays null when loading fails, and nothing is saved then.
  const [list, setList] = useState<Automation[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [sugg, setSugg] = useState<AutomationSuggestion[] | null>(null);
  const load = () => {
    setFailed(null);
    void api
      .automatedList()
      .then(setList)
      .catch((e) => {
        setList(null);
        setFailed(String(e));
      });
    void api
      .automatedSuggest()
      .then(setSugg)
      .catch(() => setSugg([]));
  };
  useEffect(load, []);

  const save = (next: Automation[], said?: string) => {
    if (!list) return;
    setList(next);
    return api
      .automatedSave(next.filter(complete))
      .then(() => {
        if (said) toast(said, undefined, "ok");
        void api.automatedSuggest().then(setSugg);
      })
      .catch(reportEditError);
  };
  // Typing edits the row here; it's saved when the field loses focus (a half-typed row may not check out yet).
  const edit = (i: number, f: Partial<Automation>) => setList((l) => l && l.map((a, j) => (j === i ? { ...a, ...f } : a)));
  const commit = () => {
    const next = toSave(list);
    if (!next) return;
    void api
      .automatedSave(next)
      .then(() => void api.automatedSuggest().then(setSugg))
      .catch(reportEditError);
  };

  // Not those covered by a tool listed here (even one not saved yet).
  const shown = (sugg ?? []).filter((g) => !(list ?? []).some((t) => covers(t, g)));
  return (
    <section className="sgroup">
      <h2 className="h3">Your tools that start Claude Code sessions</h2>
      <p className="faint">
        The daily summary writes up your Claude Code sessions as your work. A tool of yours that starts sessions itself (a script, another
        app) would show there too: list it, and its sessions are counted as automated instead. Brainstead&apos;s own jobs are recognised
        already.
      </p>
      <div className="card autotools">
        {failed && (
          <p className="qnone">
            <Icon name="info" size={14} />
            <span className="grow">Couldn&apos;t load your tools, so nothing here can be changed yet: {failed}</span>
            <button type="button" className="btn sm" title="Try loading your tools again" onClick={load}>
              Retry
            </button>
          </p>
        )}
        {list === null && !failed && <p className="faint pad">Loading…</p>}
        {list?.length === 0 && <p className="faint pad">No tools listed.</p>}
        {list?.map((a, i) => (
          <div key={i} className="autorow">
            <label className="inp">
              <input
                aria-label="Name"
                placeholder="Name"
                value={a.label}
                onChange={(e) => edit(i, { label: e.target.value })}
                onBlur={commit}
              />
            </label>
            <label className="inp mono" title="Its sessions run in a folder whose path contains this">
              <input
                aria-label="Folder contains"
                placeholder="Folder contains…"
                value={a.cwd_contains}
                onChange={(e) => edit(i, { cwd_contains: e.target.value })}
                onBlur={commit}
              />
            </label>
            <label className="inp" title="Its sessions' first message starts with these words (case doesn't matter)">
              <input
                aria-label="First message starts with"
                placeholder="First message starts with…"
                value={a.opening}
                onChange={(e) => edit(i, { opening: e.target.value })}
                onBlur={commit}
              />
            </label>
            <button
              type="button"
              className="ibtn"
              aria-label={`Remove ${a.label || "this tool"}`}
              title="Remove this tool: its sessions count as your work again"
              onClick={() => void save(list.filter((_, j) => j !== i))}
            >
              <Icon name="x" size={13} />
            </button>
          </div>
        ))}
        <div className="autofoot">
          <button
            disabled={!list}
            type="button"
            className="btn sm"
            title="Add a tool by hand: a name, and the folder or first message its sessions have"
            onClick={() => list && setList([...list, { label: "", cwd_contains: "", opening: "" }])}
          >
            <Icon name="plus" size={12} />
            Add a tool
          </button>
        </div>
      </div>
      {list && shown.length > 0 && (
        <>
          <h3 className="eyebrow">Folders that look like a tool&apos;s</h3>
          <p className="faint">
            In the last two weeks, nearly every session in these folders was one prompt and done, as a tool runs Claude Code.
          </p>
          <div className="card autotools">
            {shown.map((g) => (
              <div key={g.cwd} className="srow">
                <Icon name="folder" />
                <div className="t">
                  <div className="pt ell">{folderName(g.cwd)}</div>
                  <div className="faint small ell">
                    <span className="mono">{g.cwd}</span> · {g.count} single-prompt sessions
                  </div>
                </div>
                <button
                  type="button"
                  className="btn sm"
                  title="List this folder as a tool: its sessions count as automated, not as your work"
                  onClick={() => {
                    const t = fromSuggestion(g);
                    void save([...list, t], `Added ${t.label}: rename it if you like`);
                  }}
                >
                  It&apos;s a tool
                </button>
              </div>
            ))}
          </div>
        </>
      )}
    </section>
  );
}
