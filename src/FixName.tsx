// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Fix a name everywhere (stage 7b): a name a notetaker got wrong, corrected in every note at once.
// Every file it's in is listed with what happens to it: notes are rewritten (their modification
// times kept), the right person's wiki page gets the wrong spelling as an alias, and sources are
// left alone as the raw record. A file whose name holds a guard (someone else with that
// spelling) is never touched. The correction goes into the list of name corrections
// (substitutions.json in the app data folder) so it stops coming back. One ⌘Z undoes it all.

import { useEffect, useState } from "react";
import { api, FixNameRequest, FixNameRow } from "./api";
import { Icon } from "./icons";
import { DiffText } from "./Review";
import { reportEditError, undoLast } from "./taskModel";
import { toast } from "./Toast";
import { Dialog, Switch, useDebounced } from "./ui";
import { Store, useStore } from "./store";
import { keyLabel } from "./platform";

/** Open with a name to start from, or null when closed. */
export const fixNameOpen = new Store<{ wrong?: string; right?: string } | null>(null);
export const openFixName = (wrong?: string) => fixNameOpen.set({ wrong });

/** What happens to each file, as the screen labels it. */
export const ACTION: Record<string, string> = {
  rewrite: "Rewrite",
  alias: "Add as alias",
  leave: "Left alone",
  guarded: "Guarded: someone else",
  conflict: "Skipped: a conflicted copy of this file exists",
};

/** Which rows the fix will change: rewrites and the alias, those still ticked. */
/** The files unticked to start with: with Ask about each file, every note to rewrite, so each is
 *  ticked once it's read (an assistant's fix_name starts from the same). */
export function startUnticked(rows: FixNameRow[], askAboutEach: boolean): Set<string> {
  return new Set(askAboutEach ? rows.filter((x) => x.action === "rewrite").map((x) => x.file) : []);
}

export function chosenRows(rows: FixNameRow[], off: Set<string>): FixNameRow[] {
  return rows.filter((r) => (r.action === "rewrite" || r.action === "alias") && !off.has(r.file));
}

/** A line with the wrong name struck and the right one shown after it; task fields as icons. A
 *  surname already after it (when the right name adds one) stays as it is. */
function Swap({ text, wrong, right }: { text: string; wrong: string; right: string }) {
  const extends_ = right.startsWith(`${wrong} `);
  const esc = wrong.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`(?<![\\p{L}\\p{N}_-])${esc}(?![\\p{L}\\p{N}_-])${extends_ ? "(?!\\s+[A-Z])" : ""}`, "gu");
  const parts: React.ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(re)) {
    parts.push(<DiffText key={`t${last}`} text={text.slice(last, m.index)} />);
    parts.push(
      <span key={`s${m.index}`}>
        <span className="fnold">{wrong}</span>
        <span className="fnnew">{right}</span>
      </span>,
    );
    last = m.index! + wrong.length;
  }
  parts.push(<DiffText key="end" text={text.slice(last)} />);
  return <>{parts}</>;
}

export function FixNameHost() {
  const open = useStore(fixNameOpen);
  if (!open) return null;
  return <FixName start={open.wrong ?? ""} startRight={open.right ?? ""} onClose={() => fixNameOpen.set(null)} />;
}

function FixName({ start, startRight, onClose }: { start: string; startRight: string; onClose: () => void }) {
  const [wrong, setWrong] = useState(start);
  const [right, setRight] = useState(startRight);
  const [guard, setGuard] = useState("");
  const [ambiguous, setAmbiguous] = useState(false);
  const [remember, setRemember] = useState(true);
  const [note, setNote] = useState("");
  const [plan, setPlan] = useState<{ rightPage: string | null; rows: FixNameRow[] } | null>(null);
  const [off, setOff] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const w = useDebounced(wrong.trim(), 300);
  const r = useDebounced(right.trim(), 300);
  const g = useDebounced(guard, 300);

  const req = (): FixNameRequest => ({
    wrong: w,
    right: r,
    rightPage: plan?.rightPage ?? null,
    guards: g
      .split(",")
      .map((x) => x.trim())
      .filter(Boolean),
    ambiguous,
    note,
  });

  useEffect(() => {
    if (!w || !r || w === r) {
      setPlan(null);
      return;
    }
    let live = true;
    api
      .fixnamePlan({
        wrong: w,
        right: r,
        rightPage: null,
        guards: g
          .split(",")
          .map((x) => x.trim())
          .filter(Boolean),
        ambiguous,
        note,
      })
      .then((p) => {
        if (!live) return;
        setPlan(p);
        // A file that's ambiguous is unticked to start with: you tick it once you've read it.
        setOff(startUnticked(p.rows, ambiguous));
      })
      .catch((e) => live && reportEditError(e));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [w, r, g, ambiguous]);

  const take = plan ? chosenRows(plan.rows, off) : [];
  const occurrences = (plan?.rows ?? []).reduce((n, x) => n + x.count, 0);
  const apply = () => {
    if (!plan || busy) return;
    setBusy(true);
    const rq = req();
    api
      .fixnameApply(remember ? rq : { ...rq, skipSubstitution: true }, take)
      .then((label) => {
        toast(label, { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok");
        onClose();
      })
      .catch(reportEditError)
      .finally(() => setBusy(false));
  };
  const toggle = (f: string) =>
    setOff((s) => {
      const n = new Set(s);
      if (n.has(f)) n.delete(f);
      else n.add(f);
      return n;
    });

  return (
    <Dialog onClose={onClose} width={760} label="Fix a name everywhere">
      <div className="fixname">
        <div className="fnhead">
          <h2 className="h2 grow">Fix a name everywhere</h2>
          <button type="button" className="ibtn" aria-label="Close" title="Close without changing anything" onClick={onClose}>
            <Icon name="x" />
          </button>
        </div>
        <div className="fnnames">
          <label className="nnf">
            <span className="faint">Written as</span>
            <span className="inp">
              <input autoFocus value={wrong} onChange={(e) => setWrong(e.target.value)} placeholder="Lina" />
            </span>
          </label>
          <Icon name="forward" />
          <label className="nnf">
            <span className="faint">Correct spelling</span>
            <span className="inp">
              <input value={right} onChange={(e) => setRight(e.target.value)} placeholder="Lena" />
            </span>
          </label>
        </div>
        <label className="nnf">
          <span className="faint">Is “{w || "it"}” also a real name? Files to leave alone (part of the name, commas between)</span>
          <span className="inp">
            <input value={guard} onChange={(e) => setGuard(e.target.value)} placeholder="Lina Park" />
          </span>
        </label>
        {plan && (
          <div className="faint small">
            {occurrences} line{occurrences === 1 ? "" : "s"} in {plan.rows.length} file{plan.rows.length === 1 ? "" : "s"}
            {plan.rightPage ? ` · ${right.trim()} has a wiki page, which gets “${w}” as an alias` : ` · no wiki page for ${right.trim()}`}
          </div>
        )}
      </div>
      <div className="fnrows">
        {plan?.rows.map((x) => {
          const can = x.action === "rewrite" || x.action === "alias";
          return (
            <label key={x.file} className={`fnrow ${x.action}`}>
              <input
                type="checkbox"
                disabled={!can}
                checked={can && !off.has(x.file)}
                onChange={() => toggle(x.file)}
                aria-label={`Include ${x.file}`}
              />
              <span className="grow">
                <span className="fnfile">
                  {x.file}
                  {x.inFilename && <span className="chip amber sm">In the file name: rename it separately</span>}
                </span>
                {x.lines.slice(0, 2).map((l) => (
                  <span key={l.line} className="fnctx">
                    {x.action === "rewrite" ? <Swap text={l.text} wrong={w} right={r} /> : <DiffText text={l.text} />}
                  </span>
                ))}
              </span>
              <span className="faint small fnact">{ACTION[x.action] ?? x.action}</span>
            </label>
          );
        })}
        {plan && !plan.rows.length && <p className="faint pad">“{w}” isn't in any note.</p>}
      </div>
      <div className="fixname">
        <div className="row">
          <Switch label="Remember this correction" on={remember} onChange={setRemember} />
          <span className="grow">Remember this correction for future captures and transcripts</span>
          <Switch label="Ask about each file" on={ambiguous} onChange={setAmbiguous} />
          <span title="Start with every file unticked, so you tick each one after reading it">Ask about each file</span>
        </div>
        {remember && (
          <label className="inp">
            <input value={note} onChange={(e) => setNote(e.target.value)} placeholder="Where it's from: confirmed today via a 1-1" />
          </label>
        )}
        <div className="row">
          <span className="faint small grow">
            {take.length
              ? `${take.length} file${take.length === 1 ? "" : "s"} change. Modification times are kept; ${keyLabel("⌘Z")} undoes it.`
              : "Nothing ticked."}
          </span>
          <button type="button" className="btn lg" title="Close without changing anything" onClick={onClose}>
            Cancel
          </button>
          <span
            title={
              busy
                ? "Fixing the name…"
                : take.length
                  ? `Correct the name in the ${take.length} ticked file${take.length === 1 ? "" : "s"}; ${keyLabel("⌘Z")} undoes it`
                  : "Tick at least one file to change"
            }
          >
            <button type="button" className="btn pri lg" disabled={!take.length || busy} aria-busy={busy} onClick={apply}>
              {busy ? <span className="spin" /> : <Icon name="check" size={14} />}
              Apply
            </button>
          </span>
        </div>
      </div>
    </Dialog>
  );
}
