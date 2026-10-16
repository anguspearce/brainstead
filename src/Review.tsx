// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Changes (decided 2026-10-05): what assistants and Brainstead's runs did to the vault, newest
// first and grouped by run, each with Revert. Changes you started (Ask, a terminal, a run started
// by hand) are made at once; a scheduled run's change that fails a check is held at the top, with
// why, for you to accept or reject. Reading the feed is optional: only held changes are counted.

import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api, ChangeRow, ChangeView, Hunk } from "./api";
import { Icon } from "./icons";
import { fieldIcon, fieldTip, splitTaskFields } from "./taskFields";
import { byRun, changeMatches, changes, held, originLabel, pageName, reloadChanges, runChanges, startKnowledge } from "./knowledge";
import { nav, openDoc, useViewState } from "./nav";
import { reportEditError, undoLast } from "./taskModel";
import { toast } from "./Toast";
import { askLink, askQuote, TopBar } from "./TopBar";
import { settings, Store, useStore } from "./store";
import { Dialog, SearchBox, TableBand, useDebounced } from "./ui";
import { offerFollowUp } from "./Ingest";
import { DocButton, useGone } from "./docButton";
import { isMod, keyLabel } from "./platform";

/** A line of the page as the diff shows it: task fields as their icons, never the file's emoji. */
export function DiffText({ text }: { text: string }) {
  if (!text) return <> </>;
  return (
    <>
      {splitTaskFields(text).map((p, i) =>
        typeof p === "string" ? (
          p
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

const kindChip = (c: ChangeRow) =>
  c.kind === "new" ? (
    <span className="chip acc">New page</span>
  ) : c.kind === "task" ? (
    <span className="chip">Task</span>
  ) : c.kind === "rename" ? (
    <span className="chip">Rename</span>
  ) : c.kind === "trash" ? (
    <span className="chip amber">Moved to the Trash</span>
  ) : c.kind === "restore" ? (
    <span className="chip">Restored from the Trash</span>
  ) : c.kind === "image" ? (
    <span className="chip acc">Image added</span>
  ) : null;

function when(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const mins = Math.round((Date.now() - d.getTime()) / 60_000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  if (mins < 24 * 60) return `${Math.round(mins / 60)} h ago`;
  return d.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

const day = (iso: string) => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso.slice(0, 10) : d.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
};

const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

/** A run's band: "Daily check, 5 Oct: 6 changes to 4 pages". */
export function runHeading(label: string, rows: ChangeRow[]): string {
  const pages = new Set(rows.map((r) => r.page)).size;
  const first = rows[rows.length - 1];
  return `${label}, ${day(first.created)}: ${plural(rows.length, "change")}${pages > 1 ? ` to ${pages} pages` : ""}`;
}

/** A revert that couldn't be done line by line: the page as it was before the change, to copy from. */
export const revertConflict = new Store<{ message: string; before: string | null; page: string } | null>(null);

export function RevertConflict() {
  const r = useStore(revertConflict);
  if (!r) return null;
  const close = () => revertConflict.set(null);
  return (
    <Dialog onClose={close} width={640} label="Couldn't revert">
      <div className="confirm">
        <h2 className="h2">Couldn't revert this change</h2>
        <p className="muted">{r.message}</p>
        {r.before !== null && (
          <textarea className="inp mono revert-before" readOnly value={r.before} aria-label="The page before the change" />
        )}
        <div className="row">
          <button
            type="button"
            className="btn lg"
            title={`Open ${pageName(r.page)} to fix it by hand`}
            onClick={() => (close(), openDoc(r.page))}
          >
            Open the page
          </button>
          <span className="grow" />
          {r.before !== null && (
            <button
              type="button"
              className="btn lg"
              title="Copy the page as it was before the change"
              onClick={() => void navigator.clipboard.writeText(r.before ?? "").then(() => toast("Copied", undefined, "ok"))}
            >
              Copy
            </button>
          )}
          <button type="button" className="btn lg pri" title="Close" onClick={close}>
            Close
          </button>
        </div>
      </div>
    </Dialog>
  );
}

/** The changes being reverted now: their Revert buttons are off until it's done. */
export const reverting = new Store<ReadonlySet<string>>(new Set());

/** Reverts a change, with the toast or the conflict dialog; a second click while it's going does nothing. */
export function revert(c: ChangeRow) {
  if (reverting.get().has(c.id)) return Promise.resolve();
  reverting.set(new Set([...reverting.get(), c.id]));
  return api
    .changeRevert(c.id)
    .then((r) => {
      if (r.ok) toast(r.message, { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok");
      else revertConflict.set({ message: r.message, before: r.before, page: c.page });
    })
    .catch(reportEditError)
    .finally(() => {
      const next = new Set(reverting.get());
      next.delete(c.id);
      reverting.set(next);
    });
}

/** The run whose held changes are being accepted now, if any: Accept all and ⌘↩ wait for it. */
export const acceptingRun = new Store<string | null>(null);

/** Accepts every held change in a run (only those the search `q` shows, when there is one), one
 *  run at a time; each meeting note it made gets its follow-up offer. */
export function acceptRun(group: string, rows: ChangeRow[], q = "") {
  if (acceptingRun.get()) return Promise.resolve();
  acceptingRun.set(group);
  const todo = runChanges(rows, group, "held", q);
  const meetings = todo.filter((c) => c.origin.kind === "meeting" && c.origin.chat);
  return api
    .changesAcceptAll(group, false, q ? todo.map((c) => c.id) : undefined)
    .then(async (m) => {
      if (m.failed.length) toast(`Accepted ${m.done}; ${m.failed.length} couldn't be made: ${m.failed[0]}`, undefined, "bad");
      else toast(`Accepted ${plural(m.done, "change")}`, undefined, "ok");
      await reloadChanges();
      // Only the notes that were made: one that couldn't be is still held.
      const made = new Set((changes.get() ?? []).filter((c) => c.status === "applied").map((c) => c.id));
      for (const c of meetings) if (made.has(c.id)) offerFollowUp({ note: c.to ?? c.page, transcript: c.origin.chat ?? "" });
    })
    .catch(reportEditError)
    .finally(() => acceptingRun.set(null));
}

/** Whether a key press is for something else: typing in a field, or a dialog is open. */
export const keyIsElsewhere = (e: KeyboardEvent) =>
  !!(e.target as HTMLElement)?.closest?.("input, textarea, select, [contenteditable]") ||
  !!document.querySelector(".scrim, [role='dialog']");

/** How many runs of the feed show before "Show more". */
const RUNS_SHOWN = 30;

export function ReviewScreen() {
  useEffect(startKnowledge, []);
  const all = useStore(changes);
  // The search narrows held changes and the feed alike; ↑↓ in the box move through what it found.
  const [q, setQ] = useViewState("review:q", "");
  const dq = useDebounced(q).trim();
  const rows = useMemo(() => (all && dq ? all.filter((c) => changeMatches(c, dq)) : all), [all, dq]);
  const heldAll = useMemo(() => held(all ?? []).length, [all]);
  const waiting = useMemo(() => held(rows ?? []), [rows]);
  const heldRuns = useMemo(() => byRun(waiting), [waiting]);
  const [more, setMore] = useState(1);
  const feedRuns = useMemo(() => byRun((rows ?? []).filter((c) => c.status !== "held")), [rows]);
  const shownRuns = feedRuns.slice(0, RUNS_SHOWN * more);
  // J / K follow the list: held changes first, then the feed.
  const order = useMemo(() => [...heldRuns, ...shownRuns].flatMap((g) => g.rows), [heldRuns, shownRuns]);
  const [sel, setSel] = useViewState<string | null>("review:sel", null);
  const cur = order.find((c) => c.id === sel) ?? waiting[0] ?? order[0] ?? null;
  const idx = cur ? order.indexOf(cur) : -1;
  const move = (d: number) => {
    const next = order[Math.min(order.length - 1, Math.max(0, idx + d))];
    if (next) setSel(next.id);
  };
  const key = (e: React.KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "Escape") {
      if (q) setQ("");
      else return;
    } else if (!order.length) return;
    else if (e.key === "ArrowDown") setSel(order[(idx + 1) % order.length].id);
    else if (e.key === "ArrowUp") setSel(order[(idx - 1 + order.length) % order.length].id);
    else return;
    e.preventDefault();
  };
  const readOnly = useStore(settings).readOnly;
  const [confirm, setConfirm] = useState<{ group: string; label: string; n: number; revert?: boolean } | null>(null);
  const accepting = useStore(acceptingRun);
  // With a search on, a run's Reject all and Revert all act only on the changes it shows, as their
  // confirmations count (Accept all too).
  const runIds = (group: string, status: ChangeRow["status"]) =>
    dq ? runChanges(all ?? [], group, status, dq).map((c) => c.id) : undefined;
  const rejectRun = (group: string) =>
    api
      .changesRejectAll(group, runIds(group, "held"))
      .then((m) => toast(`Rejected ${plural(m.done, "change")}`))
      .then(reloadChanges)
      .catch(reportEditError);
  const revertRun = (group: string) =>
    api
      .changesRevertAll(group, runIds(group, "applied"))
      .then((m) =>
        m.failed.length
          ? toast(
              `Reverted ${plural(m.done, "change")}; ${plural(m.failed.length, "page")} edited since, left as they are`,
              undefined,
              "bad",
            )
          : toast(`Reverted ${plural(m.done, "change")}`, undefined, "ok"),
      )
      .then(reloadChanges)
      .catch(reportEditError);

  // The chosen change stays in sight: older runs are drawn until it's among them (when another
  // screen, such as an ingest run's See changes, chose it), and the list scrolls to its row.
  useEffect(() => {
    if (!sel) return;
    const at = feedRuns.findIndex((g) => g.rows.some((c) => c.id === sel));
    if (at >= RUNS_SHOWN * more) setMore(Math.floor(at / RUNS_SHOWN) + 1);
  }, [sel, feedRuns, more]);
  // Arriving (from See N changes, say), the change is brought to the middle of the list; after
  // that J / K only scroll as far as keeps it in sight.
  const arrived = useRef(false);
  useEffect(() => {
    // Not yet the change asked for: its run is older than those drawn, and is being drawn now.
    if (!cur || (sel && cur.id !== sel && rows?.some((c) => c.id === sel))) return;
    requestAnimationFrame(() => {
      const el = document.querySelector<HTMLElement>(`.qrow[data-id="${CSS.escape(cur.id)}"]`);
      const list = el?.closest<HTMLElement>(".ilist");
      if (!el || !list) return;
      if (arrived.current) el.scrollIntoView({ block: "nearest" });
      else {
        // To the middle of the list (or as near as its ends allow).
        const r = el.getBoundingClientRect();
        const lr = list.getBoundingClientRect();
        list.scrollTop += r.top - lr.top - (lr.height - r.height) / 2;
      }
      arrived.current = true;
    });
  }, [cur?.id, shownRuns.length]); // eslint-disable-line react-hooks/exhaustive-deps

  const gone = useGone(shownRuns.flatMap((g) => g.rows.map((r) => r.to ?? r.page)));

  return (
    <main className="main">
      <TopBar
        title="Changes"
        sub={heldAll ? `${plural(heldAll, "change")} held for you` : "Nothing held"}
        ask={
          cur
            ? { about: `the change ${askQuote(cur.title)}`, prompt: `About the change “${cur.title}” to ${askLink(cur.page)}: ` }
            : { about: "Changes", prompt: "About the changes assistants made: " }
        }
      >
        <span className="faint small">What assistants and runs changed; a scheduled change that fails a check waits here</span>
      </TopBar>
      {confirm?.revert && (
        <Dialog onClose={() => setConfirm(null)} width={440} label="Revert the run's changes">
          <div className="confirm">
            <h2 className="h2">Revert {plural(confirm.n, "change")}?</h2>
            <p className="muted">
              From {confirm.label}
              {dq ? ", only those the search shows" : ""}. Each page goes back as it was before; a page edited since keeps those edits and
              is listed.
            </p>
            <div className="row">
              <span className="grow" />
              <button type="button" className="btn lg" title="Keep the changes" onClick={() => setConfirm(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn lg danger-pri"
                title="Revert every change this run made"
                onClick={() => {
                  setConfirm(null);
                  void revertRun(confirm.group);
                }}
              >
                Revert all
              </button>
            </div>
          </div>
        </Dialog>
      )}
      {confirm && !confirm.revert && (
        <Dialog onClose={() => setConfirm(null)} width={440} label="Reject the run's changes">
          <div className="confirm">
            <h2 className="h2">Reject {plural(confirm.n, "held change")}?</h2>
            <p className="muted">
              From {confirm.label}
              {dq ? ", only those the search shows" : ""}. Nothing in the vault changes, and they can't be brought back.
            </p>
            <div className="row">
              <span className="grow" />
              <button type="button" className="btn lg" title="Keep them held" onClick={() => setConfirm(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn lg danger-pri"
                title="Reject every held change in this run"
                onClick={() => {
                  setConfirm(null);
                  void rejectRun(confirm.group);
                }}
              >
                Reject all
              </button>
            </div>
          </div>
        </Dialog>
      )}
      <div className="body queue">
        <div className="ilist tb" role="table" aria-label="Changes">
          <div className="qsearch">
            <SearchBox value={q} onChange={setQ} onKeyDown={key} placeholder="Search changes">
              {dq && (
                <span className="faint nav">
                  {order.length ? `${idx + 1}/${order.length}` : "0"}
                  <span className="kbd">↑↓</span>
                </span>
              )}
            </SearchBox>
          </div>
          {rows === null && <p className="faint pad">Loading…</p>}
          {heldRuns.length > 0 && (
            <div className="tb-group held" role="rowgroup">
              <TableBand
                icon="review"
                label="Held for you"
                n={waiting.length}
                tip={`${plural(waiting.length, "change")} waiting for you`}
              />
              {heldRuns.map((g) => (
                <div key={g.group} className="qrun">
                  <div className="qrunhead">
                    <span className="grow small">{runHeading(g.label, g.rows)}</span>
                    <span
                      title={
                        readOnly
                          ? "Read-only is on (Settings › Vault)"
                          : accepting
                            ? "Accepting a run's changes…"
                            : keyLabel(
                                dq
                                  ? "Make every held change in this run that the search shows, oldest first (⌘↩)"
                                  : "Make every held change in this run, oldest first (⌘↩)",
                              )
                      }
                    >
                      <button
                        type="button"
                        className="btn sm"
                        disabled={readOnly || !!accepting}
                        onClick={() => void acceptRun(g.group, all ?? [], dq)}
                      >
                        {accepting === g.group ? "Accepting…" : "Accept all"}
                      </button>
                    </span>
                    <button
                      type="button"
                      className="btn sm ghost"
                      title={
                        dq ? "Turn down every held change in this run that the search shows" : "Turn down every held change in this run"
                      }
                      disabled={accepting === g.group}
                      onClick={() => setConfirm({ group: g.group, label: g.label, n: g.rows.length })}
                    >
                      Reject all
                    </button>
                  </div>
                  {g.rows.map((c) => (
                    <Row key={c.id} c={c} sel={cur?.id === c.id} onSel={() => setSel(c.id)} />
                  ))}
                </div>
              ))}
            </div>
          )}
          {shownRuns.map((g) => (
            <div key={g.group} className="tb-group" role="rowgroup">
              <TableBand
                icon={g.rows[0].origin.kind === "chat" ? "ask" : "wiki"}
                label={runHeading(g.label, g.rows)}
                n={g.rows.length}
                tip={`${plural(g.rows.length, "change")} in this run`}
              />
              {g.rows.filter((c) => c.status === "applied").length > 1 && (
                <div className="qrunhead">
                  <span className="grow" />
                  <span
                    title={
                      readOnly
                        ? "Read-only is on (Settings › Vault)"
                        : dq
                          ? "Put back every page this run changed that the search shows"
                          : "Put back every page this run changed"
                    }
                  >
                    <button
                      type="button"
                      className="btn sm ghost"
                      disabled={readOnly}
                      onClick={() =>
                        setConfirm({ group: g.group, label: g.label, n: g.rows.filter((c) => c.status === "applied").length, revert: true })
                      }
                    >
                      Revert all
                    </button>
                  </span>
                </div>
              )}
              {g.rows.map((c) => (
                <Row key={c.id} c={c} sel={cur?.id === c.id} gone={gone(c.to ?? c.page)} onSel={() => setSel(c.id)} />
              ))}
            </div>
          ))}
          {feedRuns.length > shownRuns.length && (
            <button type="button" className="btn ghost pad" title="Show older runs" onClick={() => setMore((m) => m + 1)}>
              Show older changes
            </button>
          )}
          {rows !== null && !rows.length && <p className="faint pad">{dq ? `No changes match “${dq}”.` : "No changes yet."}</p>}
        </div>
        {cur ? (
          <Detail key={cur.id} row={cur} q={dq} onMove={move} />
        ) : (
          <div className="iopen">
            <div className="soon">
              <Icon name="review" size={28} />
              {dq ? (
                <h1 className="h2">No changes match</h1>
              ) : (
                <>
                  <h1 className="h2">No changes yet</h1>
                  <p className="muted">
                    What an assistant in Ask or a terminal changes, and what Brainstead's runs change, shows here, each with Revert.
                  </p>
                </>
              )}
            </div>
          </div>
        )}
      </div>
    </main>
  );
}

function Row({ c, sel, gone, onSel }: { c: ChangeRow; sel: boolean; gone?: boolean; onSel: () => void }) {
  return (
    <button
      type="button"
      role="row"
      data-id={c.id}
      className={`qrow tb-row ${sel ? "sel" : ""} ${c.status === "reverted" || c.status === "rejected" ? "done" : ""}`}
      title="Show this change (J / K)"
      onClick={onSel}
    >
      <span className="qtop">
        <span className="tt">{c.title}</span>
        {c.status === "reverted" ? (
          <span className="chip sm">Reverted</span>
        ) : c.status === "rejected" ? (
          <span className="chip sm">Rejected</span>
        ) : c.flags.length > 0 ? (
          <span className="chip amber sm" title={c.flags.join("\n")}>
            {c.status === "held" ? "Held" : "Flagged"}
          </span>
        ) : (
          kindChip(c)
        )}
      </span>
      <span className="faint">
        <span className={gone ? "strike" : ""}>{pageName(c.to ?? c.page)}</span> · {originLabel(c)}
        {c.model ? ` · ${c.model.replace(/^[a-z]+:/, "")}` : ""} · {when(c.created)}
      </span>
    </button>
  );
}

function Detail({ row, q, onMove }: { row: ChangeRow; q: string; onMove: (d: number) => void }) {
  const [v, setV] = useState<ChangeView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [tries, setTries] = useState(0);
  const [busy, setBusy] = useState(false);
  const readOnly = useStore(settings).readOnly;
  const accepting = useStore(acceptingRun);
  const pending = useStore(reverting);
  const all = useStore(changes);
  const gone = useGone([...(v?.quotes.map((q) => q.path) ?? []), row.to ?? row.page]);

  useEffect(() => {
    let live = true;
    setErr(null);
    api
      .changeGet(row.id)
      .then((r) => live && setV(r))
      .catch((e) => live && setErr(String(e)));
    return () => {
      live = false;
    };
  }, [row.id, row.status, tries]);

  const isHeld = row.status === "held";
  const accept = () => {
    if (!isHeld || busy || readOnly || v?.problem) return;
    setBusy(true);
    api
      .changeAccept(row.id)
      .then((o) => {
        toast(o.message.replace(/ Revert it in Changes\.$/, ""), { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok");
        if (row.origin.kind === "meeting" && row.origin.chat) offerFollowUp({ note: o.page, transcript: row.origin.chat });
        onMove(1);
      })
      .catch(reportEditError)
      .finally(() => setBusy(false));
  };
  const reject = () => {
    if (!isHeld) return;
    api
      .changeReject(row.id)
      .then(() => toast("Rejected"))
      .catch(reportEditError);
  };
  const edit = () =>
    api
      .changeForEditing(row.id)
      .then((page) => {
        openDoc(page);
        toast("Opened with the change as a draft: restore it, edit, then save.");
      })
      .catch(reportEditError);

  // J / K between changes; for a held one A accepts, R rejects, ⌘↩ accepts its run.
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.defaultPrevented || keyIsElsewhere(e)) return;
      if (isMod(e) && e.key === "Enter") {
        if (e.repeat || !isHeld || readOnly || accepting || !v || v.problem) return;
        void acceptRun(row.group, all ?? [], q);
      } else if (e.repeat && (e.key === "a" || e.key === "r")) return;
      else if (e.metaKey || e.ctrlKey || e.altKey) return;
      else if (e.key === "j") onMove(1);
      else if (e.key === "k") onMove(-1);
      else if (e.key === "a" && isHeld) accept();
      else if (e.key === "r" && isHeld) reject();
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  });

  if (!v)
    return err ? (
      <div className="iopen">
        <p className="qnone">
          <Icon name="info" size={14} />
          <span className="grow">Couldn&apos;t load this change: {err}</span>
          <button type="button" className="btn sm" title="Try loading this change again" onClick={() => setTries((n) => n + 1)}>
            Retry
          </button>
        </p>
      </div>
    ) : (
      <div className="iopen faint">Loading…</div>
    );
  const page = v.to ?? v.page;
  const moves = v.kind === "rename" || v.kind === "trash" || v.kind === "restore";
  return (
    <div className="iopen qopen selectable">
      <div className="qhead">
        <div className="grow">
          <div className="eyebrow">
            {page}
            {v.changes.length === 1 && v.changes[0].section ? ` · ${v.changes[0].section}` : ""}
          </div>
          <h1 className="h2">{v.title}</h1>
          {v.reason && <p className="muted small">{v.reason}</p>}
          <p className="faint small">
            {originLabel(v)}
            {v.model ? ` · ${v.model.replace(/^[a-z]+:/, "")}` : ""} · {when(v.created)}
            {v.origin.trigger === "scheduled" ? " · scheduled" : ""}
          </p>
        </div>
        {kindChip(v)}
      </div>
      {v.flags.length > 0 && (
        <div className={`card qflags ${isHeld ? "held" : ""}`}>
          <span className="eyebrow">{isHeld ? "Held because" : "Flagged"}</span>
          {v.flags.map((f) => (
            <p key={f} className="small">
              <Icon name="info" size={12} /> {f}
            </p>
          ))}
        </div>
      )}
      {v.problem && (
        <p className="qnone">
          <Icon name="info" size={14} />
          <span className="grow">{v.problem} It can't be made as it stands: reject it, or edit the page yourself.</span>
        </p>
      )}
      {readOnly && isHeld && (
        <p className="qnone">
          <Icon name="info" size={14} />
          <span className="grow">Read-only is on, so this can't be accepted.</span>
          <button
            type="button"
            className="btn sm"
            title="Open Settings › Vault, where read-only is turned off"
            onClick={() => nav.go({ screen: "settings", pane: "vault" })}
          >
            Settings › Vault
          </button>
        </p>
      )}
      {v.warnings.map((w) => (
        <p key={w} className="faint small">
          <Icon name="info" size={12} /> {w}
        </p>
      ))}
      {moves && (
        <div className="card qmove">
          <Icon name={v.kind === "rename" ? "rename" : v.kind === "restore" ? "back" : "trash"} size={16} />
          {v.kind === "rename" ? (
            <span>
              {isHeld ? "Renames" : "Renamed"} <b>{pageName(v.page)}</b> to <b>{pageName(v.to ?? "")}</b> in its folder, and rewrites the
              links to it.
            </span>
          ) : v.kind === "restore" ? (
            <span>
              {isHeld ? "Puts" : "Put"} <b>{pageName(v.page)}</b> back from Brainstead’s Trash, at {v.page}.
            </span>
          ) : (
            <span>
              {isHeld ? "Moves" : "Moved"} <b>{pageName(v.page)}</b> to Brainstead’s Trash, where it can be restored.
            </span>
          )}
        </div>
      )}
      {v.kind === "image" && (
        <div className="card qimage">
          <span className="qmove">
            <Icon name="image" size={16} />
            <span>
              Added <b>{pageName(v.page)}</b> to images/
              {v.status === "applied"
                ? ", for notes to embed. Revert moves it to Brainstead’s Trash."
                : "; it was moved to Brainstead’s Trash."}
            </span>
          </span>
          {v.status === "applied" && (
            <img src={convertFileSrc(`${(settings.get().vaultPath ?? "").replace(/\/$/, "")}/${v.page}`)} alt={pageName(v.page)} />
          )}
        </div>
      )}
      {v.changes.map((h) => (
        <HunkView key={h.index} h={h} total={v.changes.length} />
      ))}
      {v.quotes.length > 0 && (
        <div className="card qquotes">
          <span className="eyebrow">What it rests on</span>
          {v.quotes.map((q, i) => (
            <div key={i} className="qquote">
              <blockquote>“{q.text}”</blockquote>
              <span className="faint small">
                {q.path ? (
                  <DocButton path={q.path} gone={gone(q.path)} className="blink">
                    {pageName(q.path)}
                  </DocButton>
                ) : (
                  q.source
                )}
                {q.anchor ? ` · ${q.anchor}` : ""}
                {q.checked === true ? (
                  <span className="chip green sm">Found in the source</span>
                ) : q.checked === false ? (
                  <span className="chip amber sm">Not found in the source</span>
                ) : (
                  <span className="chip sm">Not checked: Brainstead doesn't read this file's text yet</span>
                )}
              </span>
            </div>
          ))}
        </div>
      )}
      <div className="qfoot">
        {isHeld ? (
          <>
            <span
              title={
                readOnly
                  ? "Read-only is on (Settings › Vault)"
                  : v.problem
                    ? "It can't be made as it stands"
                    : "Make this change on the page as it is now (A)"
              }
            >
              <button type="button" className="btn pri lg" disabled={busy || readOnly || !!v.problem} onClick={accept}>
                <Icon name="check" size={14} />
                Accept
                <span className="kbd">A</span>
              </button>
            </span>
            {v.kind !== "new" && !moves && (
              <span
                title={
                  readOnly
                    ? "Read-only is on (Settings › Vault)"
                    : "Open the page with the change as an unsaved draft to edit before saving"
                }
              >
                <button type="button" className="btn lg" disabled={readOnly || !!v.problem} onClick={edit}>
                  Edit before accepting
                </button>
              </span>
            )}
            <button type="button" className="btn lg ghost" title="Turn this change down; the page is left as it is (R)" onClick={reject}>
              Reject
              <span className="kbd">R</span>
            </button>
            <span className="grow" />
            <span className="faint small">J / K · A accept · R reject · {keyLabel("⌘↩")} accept the run</span>
          </>
        ) : (
          <>
            {v.status === "applied" && v.revertable && (
              <span
                title={
                  readOnly
                    ? "Read-only is on (Settings › Vault)"
                    : pending.has(v.id)
                      ? "Reverting…"
                      : "Undo this change on the page as it is now, keeping later edits"
                }
              >
                <button type="button" className="btn lg" disabled={readOnly || pending.has(v.id)} onClick={() => void revert(v)}>
                  <Icon name="history" size={14} />
                  Revert
                </button>
              </span>
            )}
            <DocButton path={page} gone={gone(page)} className="btn lg ghost" title={`Open ${pageName(page)}`}>
              Open the page
            </DocButton>
            <span className="grow" />
            <span className="faint small">J / K changes</span>
          </>
        )}
      </div>
    </div>
  );
}

function HunkView({ h, total }: { h: Hunk; total: number }) {
  return (
    <div className="diff">
      <div className="hunk">
        {h.section || "Start of the page"}
        {total > 1 ? ` · change ${h.index + 1} of ${total}` : ""}
      </div>
      {h.ctxBefore.map((l, i) => (
        <div key={`b${i}`} className="ctxl">
          <DiffText text={l} />
        </div>
      ))}
      {h.old.map((l, i) => (
        <div key={`o${i}`} className="del">
          <DiffText text={l} />
        </div>
      ))}
      {h.new.map((l, i) => (
        <div key={`n${i}`} className="add">
          <DiffText text={l} />
        </div>
      ))}
      {h.ctxAfter.map((l, i) => (
        <div key={`a${i}`} className="ctxl">
          <DiffText text={l} />
        </div>
      ))}
    </div>
  );
}

/** A page's side pane: what agents changed in it lately, each with Revert, and any change held for it. */
export function AgentChangesCard({ path }: { path: string }) {
  useEffect(startKnowledge, []);
  const all = useStore(changes);
  const readOnly = useStore(settings).readOnly;
  const pending = useStore(reverting);
  const mine = (all ?? []).filter((c) => (c.to ?? c.page) === path || c.page === path);
  if (!mine.length) return null;
  const shown = mine.slice(0, 5);
  return (
    <section className="card side-card agentch">
      <div className="row">
        <div className="eyebrow grow">Agent changes · {mine.length}</div>
      </div>
      {shown.map((c) => (
        <div key={c.id} className="agentrow">
          <button type="button" className="blink grow" title="Show this change in Changes" onClick={() => nav.go("review")}>
            <span className="ell">{c.title}</span>
            <span className="faint small">
              {originLabel(c)} · {when(c.created)}
              {c.status === "held"
                ? " · held for you"
                : c.status === "reverted"
                  ? " · reverted"
                  : c.status === "rejected"
                    ? " · rejected"
                    : ""}
            </span>
          </button>
          {c.status === "applied" && c.revertable && (
            <span
              title={
                readOnly ? "Read-only is on (Settings › Vault)" : pending.has(c.id) ? "Reverting…" : "Undo this change, keeping later edits"
              }
            >
              <button type="button" className="btn sm" disabled={readOnly || pending.has(c.id)} onClick={() => void revert(c)}>
                Revert
              </button>
            </span>
          )}
        </div>
      ))}
    </section>
  );
}
