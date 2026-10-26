// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Meeting note from a transcript (stage 7b): a Teams transcript in sources/ becomes a meeting or
// 1-1 note. Its type, name and date are read from the file name (the previous app's rules), and
// can be changed; the model fills in the note from the template and Brainstead makes it (in
// Changes, revertable), then offers to ingest it and move the transcript to the Trash. Several can
// go at once.

import { useEffect, useMemo, useState } from "react";
import { api, NoteSpec, Transcript } from "./api";
import { Icon } from "./icons";
import { isActive, meetingProgress, runs, RunsPane, startRuns } from "./Ingest";
import { settings, useStore } from "./store";
import { changes, pageName, startKnowledge } from "./knowledge";
import { Markdown } from "./md/Markdown";
import { reportEditError } from "./taskModel";
import { toast } from "./Toast";
import { ReadOnlyBanner, TopBar } from "./TopBar";
import { lastSeen, useVaultVersion } from "./state";
import { nav, openDoc, place, useViewState } from "./nav";
import { Dialog, Seg } from "./ui";
import { draftNotes, followThrough, keepNote, noteFile, specOf, specOk, trashTranscript } from "./meetingFlow";

export { noteFile, specOf, specOk };

/** The note types the screen offers (start_run meeting_note takes the same). */
export const TYPES = ["Meeting", "1-1", "Workshop", "Interview"];

/** The date is only the day the transcript was captured, and the user hasn't confirmed or changed
 *  it yet: the note waits until they do. */
export const needsDate = (t: Transcript, edits: Partial<NoteSpec> = {}) => t.inferred.dateCheck && edits.date === undefined;

/** Its note exists already (at the name read from the transcript, unchanged): there's nothing to
 *  draft, unless the name or date is changed. */
export const noteExists = (t: Transcript, edits: Partial<NoteSpec> = {}) =>
  t.inferred.exists && noteFile(specOf(t, edits)) === noteFile(specOf(t));

/** What the card's buttons do. */
export type CardAction = "make" | "confirmAndMake" | "ingestNote" | "trash" | "openNote" | "openChanges" | "openVault" | "openGeneral";

/** What the card says about a transcript and offers to do with it, the best first (the
 *  highlighted button). Every case the transcript can be in, in order: Read-only, its note under
 *  way, failed or held, the note there already, the transcript dealt with already, the details
 *  still to give, then the ordinary make. */
export function cardFor(
  t: Transcript,
  edits: Partial<NoteSpec> = {},
  o: { readOnly?: boolean; sending?: boolean; progress?: { label: string; busy: boolean; why?: string } | null; ingest?: boolean } = {},
): { say?: string; tone?: "busy" | "warn" | "ok"; actions: CardAction[]; makeDisabled?: string } {
  const p = o.progress;
  const spec = specOf(t, edits);
  if (o.readOnly)
    // The screen's banner has the switch that turns it off.
    return { say: "Read-only is on, so no note can be made.", tone: "warn", actions: [] };
  if (o.sending || p?.busy)
    return {
      say: `${p?.label ?? "Writing note…"} It carries on by itself; ${o.ingest ? "once the note is made and ingested" : "once the note is made"}, the transcript leaves this list.`,
      tone: "busy",
      actions: [],
    };
  if (p?.label === "Note held in Changes")
    return {
      say: "The note is held in Changes, so it isn't in the vault yet. Accept it there and it's made, then ingested.",
      tone: "warn",
      actions: ["openChanges"],
    };
  if (p && /^Note (failed|stopped)/.test(p.label))
    return {
      say: `${p.label === "Note stopped" ? "Making the note was stopped" : "Making the note failed"}${p.why ? `: ${p.why}` : ""}. Try again, or let the transcript go.`,
      tone: "warn",
      actions: ["make", "trash"],
    };
  if (p && /^Note (rejected|reverted) in Changes/.test(p.label))
    return {
      say: `Making the note was ${p.label.includes("reverted") ? "reverted" : "rejected"} in Changes, so it isn't in the vault. Make it again, or let the transcript go.`,
      tone: "warn",
      actions: ["make", "trash"],
    };
  if (p && /^Ingest (failed|stopped)/.test(p.label))
    return {
      say: `The note was made, but its ingest ${p.label === "Ingest stopped" ? "was stopped" : "failed"}${p.why ? `: ${p.why}` : ""}.`,
      tone: "warn",
      actions: ["ingestNote", "trash", "openNote"],
    };
  if (noteExists(t, edits))
    return t.noteIngested
      ? {
          say: `${noteFile(spec)} is made and ingested already (a wiki page cites it): only the transcript is left.`,
          tone: "ok",
          actions: ["trash", "openNote"],
        }
      : {
          say: `${noteFile(spec)} exists already. Ingest it as it is, or change the name or date above for a new note.`,
          tone: "warn",
          actions: ["ingestNote", "trash", "openNote"],
        };
  if (t.done === "ingested")
    return {
      say: "This transcript was ingested itself, so the wiki has it already. A note can still be made from it.",
      tone: "ok",
      actions: ["trash", "make"],
    };
  if (t.done === "linked")
    return {
      say: "A note links to this transcript already, most likely the meeting note made from it.",
      tone: "ok",
      actions: ["trash", "make"],
    };
  if (!specOk(spec))
    return {
      actions: ["make"],
      makeDisabled: !spec.name.trim()
        ? `Give the note ${spec.type === "1-1" ? "the person it was with" : "a name"} above`
        : "Give the note its date above, as YYYY-MM-DD",
    };
  if (needsDate(t, edits))
    return {
      say: `${spec.date} is the day the transcript was captured, not necessarily the meeting's. Check it, or type the right one above.`,
      tone: "warn",
      actions: ["confirmAndMake"],
    };
  const asks = t.inferred.ask.length > 0;
  return {
    say: asks ? "Check the details above and change any that are wrong, then make the note." : undefined,
    actions: t.inferred.ask.some((a) => /Settings › General/.test(a)) ? ["make", "openGeneral"] : ["make"],
  };
}

/** Why a transcript is done, as the list says it (list_transcripts too). */
export const DONE: Record<NonNullable<Transcript["done"]>, string> = { ingested: "ingested", linked: "linked from a note" };

/** Ingest the existing note: what follows a made note (Settings › AI assistants' Meeting notes switches),
 *  for one that exists already: ingest it, and move the transcript to the Trash. */
function keepNoteTip(): string {
  const f = followThrough();
  const then = [f.ingest && "ingest the note", f.trash && "move the transcript to the Trash"].filter(Boolean);
  return then.length
    ? `No new note: keep the one that's there as it is, then ${then.join(" and ")}`
    : "No new note: keep the one that's there as it is (ingesting and trashing are off in Settings)";
}

async function ingestExisting(transcript: string, note: string) {
  try {
    const r = await keepNote(transcript, note);
    if (r.ingested) startRuns();
    toast(
      r.ingested ? `Ingesting ${pageName(note)}` : "Kept as it is",
      r.ingested ? { label: "Sources", run: () => nav.go("sources") } : undefined,
      "ok",
    );
  } catch (e) {
    reportEditError(e);
  }
}

export function MeetingScreen() {
  // The last list shows at once on reopening, while the transcripts are read again.
  const [all, setList] = useState(() => lastSeen.get<Transcript[]>("transcripts"));
  // Only the transcripts still to do, unless the switch says all.
  const [show, setShow] = useViewState<"todo" | "all">("meeting:show", "todo");
  const list = useMemo(() => (all && show === "todo" ? all.filter((t) => !t.done) : all), [all, show]);
  // A transcript opened from elsewhere (the Inbox) starts selected.
  const [sel, setSel] = useState<string | null>(() => place.get().place.path ?? null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [edits, setEdits] = useState<Record<string, Partial<NoteSpec>>>({});
  const [text, setText] = useState<string>("");
  const v = useVaultVersion();
  useEffect(() => {
    let live = true;
    api
      .meetingTranscripts()
      .then((l) => live && setList(lastSeen.set("transcripts", l)))
      .catch(() => live && setList((was) => was ?? []));
    return () => {
      live = false;
    };
  }, [v]);
  const cur = list?.find((t) => t.path === sel) ?? list?.[0] ?? null;
  useEffect(() => {
    if (!cur) return;
    let live = true;
    api
      .docRead(cur.path)
      .then((d) => live && setText(d.content))
      .catch(() => live && setText(""));
    return () => {
      live = false;
    };
  }, [cur?.path]); // eslint-disable-line react-hooks/exhaustive-deps
  const spec = cur ? specOf(cur, edits[cur.path]) : null;
  const set = (p: Partial<NoteSpec>) => cur && setEdits((e) => ({ ...e, [cur.path]: { ...e[cur.path], ...p } }));
  // Transcripts with a note being drafted (or sent to be): their buttons wait.
  useEffect(startRuns, []);
  useEffect(startKnowledge, []);
  const allRuns = useStore(runs);
  const changeRows = useStore(changes);
  const [sending, setSending] = useState<Set<string>>(new Set());
  const drafting = useMemo(
    () => new Set([...sending, ...(allRuns ?? []).filter((r) => r.kind === "meeting" && isActive(r)).map((r) => r.source)]),
    [allRuns, sending],
  );
  const batch = useMemo(
    () => (list ?? []).filter((t) => picked.has(t.path) && !drafting.has(t.path) && !noteExists(t, edits[t.path])),
    [list, picked, drafting, edits],
  );
  // Make all asks once for the dates of the ticked transcripts, rather than a Confirm on each.
  const [dating, setDating] = useState<Record<string, string> | null>(null);
  const tickAll = (on: boolean) => setPicked(on ? new Set((list ?? []).map((t) => t.path)) : new Set());
  const ticked = (list ?? []).filter((t) => picked.has(t.path)).length;
  const draft = (items: Transcript[], ed = edits) => {
    const pairs: [string, NoteSpec][] = items.map((t) => [t.path, specOf(t, ed[t.path])]);
    const bad = pairs.find(([, s]) => !specOk(s));
    if (bad) return toast(`${pageName(bad[0])} needs a name and a date.`, undefined, "bad");
    const unchecked = items.find((t) => needsDate(t, ed[t.path]));
    if (unchecked)
      return toast(
        `${pageName(unchecked.path)} is dated with the day it was captured: confirm or change its date first.`,
        undefined,
        "bad",
      );
    const paths = items.map((t) => t.path);
    setSending((x) => new Set([...x, ...paths]));
    draftNotes(pairs)
      .then((ids) => {
        toast(ids.length === 1 ? "Making the note: it'll be in the vault shortly" : `Making ${ids.length} notes, one after another`);
        setPicked(new Set());
      })
      .catch(reportEditError)
      .finally(() =>
        setSending((x) => {
          const n = new Set(x);
          paths.forEach((p) => n.delete(p));
          return n;
        }),
      );
  };

  // The card: what's to say about the chosen transcript, and its buttons (cardFor).
  const readOnly = useStore(settings).readOnly;
  const ingestOn = followThrough().ingest;
  const card = cur
    ? cardFor(cur, edits[cur.path], {
        readOnly,
        sending: sending.has(cur.path),
        progress: meetingProgress(allRuns ?? [], cur.path, changeRows),
        ingest: ingestOn,
      })
    : { actions: [] as CardAction[] };
  const failed = !!card.say && /^(Making the note|The note was made, but)/.test(card.say);
  const ACTION: Record<CardAction, { label: string; tip: string; icon: string }> = {
    make: {
      label: failed ? "Try again" : ingestOn ? "Make and ingest the note" : "Make the note",
      tip: "Have the AI write the note from this transcript and your template",
      icon: "note",
    },
    confirmAndMake: {
      label: `Use ${spec?.date ?? "this date"} and ${ingestOn ? "make and ingest" : "make"} the note`,
      tip: "The meeting was on this day: use it, and make the note",
      icon: "check",
    },
    ingestNote: {
      label: failed ? "Ingest the note again" : ingestOn ? "Ingest the existing note" : "Keep the existing note",
      tip: keepNoteTip(),
      icon: "wiki",
    },
    trash: {
      label: "Move the transcript to the Trash",
      tip: "Move the transcript to the vault's Trash (restore it from there)",
      icon: "trash",
    },
    openNote: { label: "Open the note", tip: `Open ${spec ? noteFile(spec) : "the note"} to check it or fill it in`, icon: "note" },
    openChanges: { label: "Open Changes", tip: "Accept or reject the held note in Changes", icon: "review" },
    openVault: { label: "Open Settings › Vault", tip: "Turn Read-only off there", icon: "settings" },
    openGeneral: {
      label: "Set your name in Settings › General",
      tip: "Your name tells Brainstead which of two people is you",
      icon: "settings",
    },
  };
  const act = (a: CardAction) => {
    if (!cur || !spec) return;
    if (a === "make") return draft([cur]);
    if (a === "confirmAndMake") {
      const ed = { ...edits, [cur.path]: { ...edits[cur.path], date: spec.date } };
      setEdits(ed);
      return draft([cur], ed);
    }
    if (a === "ingestNote") return void ingestExisting(cur.path, noteFile(spec));
    if (a === "trash")
      // Its links point at its note first, when it has one (the one shown here, if it's there).
      return void trashTranscript(cur.path, noteExists(cur, edits[cur.path]) ? noteFile(spec) : undefined)
        .then(() => toast("Transcript moved to the Trash", undefined, "ok"))
        .catch(reportEditError);
    if (a === "openNote") return openDoc(noteFile(spec));
    if (a === "openChanges") return nav.go("review");
    nav.go({ screen: "settings", pane: a === "openVault" ? "vault" : "general" });
  };

  return (
    <main className="main">
      <TopBar
        title="Meeting notes"
        sub={list ? `${list.length} transcript${list.length === 1 ? "" : "s"}${show === "todo" ? " to do" : ""}` : undefined}
      >
        <Seg
          label="Show"
          value={show}
          onChange={setShow}
          options={[
            ["todo", "To do", "Transcripts not yet ingested or linked from a note"],
            ["all", `All${all ? ` · ${all.length}` : ""}`],
          ]}
        />
        {batch.length > 1 && (
          <button
            type="button"
            className="btn"
            title={`Have the AI write a meeting note from each of the ${batch.length} ticked transcripts`}
            onClick={() => setDating(Object.fromEntries(batch.map((t) => [t.path, specOf(t, edits[t.path]).date])))}
          >
            {ingestOn ? "Make and ingest all" : "Make all"} {batch.length}
          </button>
        )}
      </TopBar>
      {readOnly && <ReadOnlyBanner blocks="no meeting note can be made" />}
      <div className="body meeting">
        <div className="ilist">
          {list?.length === 0 &&
            (all?.length ? (
              <p className="faint pad">Every transcript in sources/ has been dealt with. All shows them.</p>
            ) : (
              <p className="faint pad">No Teams transcripts in sources/. The Teams extension, or a file you drop there, puts them there.</p>
            ))}
          {!!list?.length && (
            <label
              className="mall faint small"
              title={ticked === list.length ? "Untick every transcript" : "Tick every transcript in the list"}
            >
              <input
                type="checkbox"
                aria-label="Tick all"
                checked={ticked === list.length}
                ref={(el) => {
                  if (el) el.indeterminate = ticked > 0 && ticked < list.length;
                }}
                onChange={(e) => tickAll(e.target.checked)}
              />
              {ticked ? `${ticked} of ${list.length} ticked` : "Tick all"}
            </label>
          )}
          {list?.map((t) => (
            <div key={t.path} className={`qrow mrow ${cur?.path === t.path ? "sel" : ""}`}>
              <input
                type="checkbox"
                aria-label={`Pick ${t.path}`}
                checked={picked.has(t.path)}
                onChange={() =>
                  setPicked((p) => {
                    const n = new Set(p);
                    if (n.has(t.path)) n.delete(t.path);
                    else n.add(t.path);
                    return n;
                  })
                }
              />
              <button
                type="button"
                className="mpick"
                title="Show this transcript and the note it will become"
                onClick={() => setSel(t.path)}
              >
                <span className="tt">{pageName(t.path).replace(/^Teams\. Transcript\. /, "")}</span>
                <span className="faint">
                  {t.inferred.type ?? "?"} · {t.inferred.date ?? "no date"}
                  {needsDate(t, edits[t.path]) ? " · check the date" : !t.inferred.confident && " · check"}
                  {t.done && ` · ${DONE[t.done]}`}
                  {!t.done && t.inferred.exists && " · note exists"}
                </span>
              </button>
            </div>
          ))}
        </div>
        <div className="mtext selectable">{cur && <Markdown content={text} path={cur.path} root="" />}</div>
        <div className="mside">
          {cur && spec && (
            <div className="mdetails">
              <span className="eyebrow">The note</span>
              <label className="nnf">
                <span className="faint">Type</span>
                <select
                  title="The kind of note: Meeting, 1-1, Workshop or Interview, each made from its template"
                  className="sel"
                  value={spec.type}
                  onChange={(e) => set({ type: e.target.value })}
                >
                  {TYPES.map((t) => (
                    <option key={t}>{t}</option>
                  ))}
                </select>
              </label>
              <label className="nnf">
                <span className="faint">{spec.type === "1-1" ? "With" : "Name"}</span>
                <span className="inp">
                  <input value={spec.name} onChange={(e) => set({ name: e.target.value })} />
                </span>
              </label>
              <label className="nnf">
                <span className="faint">Date</span>
                <span className="inp">
                  <input value={spec.date} onChange={(e) => set({ date: e.target.value })} placeholder="YYYY-MM-DD" />
                </span>
                {needsDate(cur, edits[cur.path]) && (
                  <button
                    type="button"
                    className="btn sm"
                    title="The meeting was on this day: use it for the note"
                    onClick={() => set({ date: spec.date })}
                  >
                    Confirm
                  </button>
                )}
              </label>
              {/* The names this person's notes use, when the transcript's is none of them: one click. */}
              {!!cur.inferred.nameForms?.length && (
                <div className="qchips">
                  {cur.inferred.nameForms.map((n) => (
                    <button
                      key={n}
                      type="button"
                      className={`chip f ${spec.name === n ? "on" : ""}`}
                      aria-pressed={spec.name === n}
                      title={`Name the note for ${n}, as the existing 1-1 notes do`}
                      onClick={() => set({ name: n })}
                    >
                      {n}
                    </button>
                  ))}
                </div>
              )}
              <span className="faint mono small">{noteFile(spec)}</span>
              {/* The questions about its details (an existing note is the card's own message). */}
              {cur.inferred.ask
                .filter((a) => !/exists already/.test(a))
                .map((a) => (
                  <p key={a} className="faint small">
                    <Icon name="info" size={12} /> {a}
                  </p>
                ))}
              {card.say && (
                <div className={`doc-banner ${card.tone === "warn" ? "warn" : ""}`} role="status">
                  {card.tone === "busy" ? <span className="spin" /> : <Icon name={card.tone === "ok" ? "check" : "info"} size={14} />}
                  <span>{card.say}</span>
                </div>
              )}
              <div className="mexists">
                {card.actions.map((a, n) => {
                  const b = ACTION[a];
                  const off = a === "make" && card.makeDisabled;
                  return (
                    <span key={a} title={off ? card.makeDisabled : b.tip}>
                      <button
                        type="button"
                        className={`btn ${n === 0 ? "pri" : a === "trash" ? "ghost" : ""}`}
                        disabled={!!off}
                        onClick={() => act(a)}
                      >
                        <Icon name={b.icon} size={14} />
                        {b.label}
                      </button>
                    </span>
                  );
                })}
              </div>
              {(card.actions[0] === "make" || card.actions[0] === "confirmAndMake") && !card.makeDisabled && (
                <p className="faint small">
                  {(() => {
                    const f = followThrough();
                    const then = [f.ingest && "ingested", f.trash && "the transcript moved to the Trash"].filter(Boolean).join(", then ");
                    return `The note is made (revertable in Changes)${then ? `, then ${then}` : ""}.`;
                  })()}
                </p>
              )}
            </div>
          )}
          <RunsPane />
        </div>
      </div>
      {dating && (
        <Dialog onClose={() => setDating(null)} width={520} label="Check the dates">
          <div className="confirm">
            <h2 className="h2">Make {batch.length} notes with these dates?</h2>
            <p className="muted">Each note is named and dated with the day below. Change any that's wrong.</p>
            <div className="mdates">
              {batch.map((t) => (
                <label key={t.path} className="nnf">
                  <span className="tt">
                    {pageName(t.path).replace(/^Teams\. Transcript\. /, "")}
                    {needsDate(t, edits[t.path]) && <span className="faint small"> · the day it was captured</span>}
                  </span>
                  <span className="inp">
                    <input
                      value={dating[t.path] ?? ""}
                      placeholder="YYYY-MM-DD"
                      onChange={(e) => setDating((d) => d && { ...d, [t.path]: e.target.value })}
                    />
                  </span>
                </label>
              ))}
            </div>
            <div className="row">
              <span className="grow" />
              <button type="button" className="btn lg" title="Go back without making them" onClick={() => setDating(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn lg pri"
                title="The dates are right: make a note from each ticked transcript"
                onClick={() => {
                  const ed = { ...edits };
                  batch.forEach((t) => (ed[t.path] = { ...ed[t.path], date: dating[t.path] ?? "" }));
                  setEdits(ed);
                  setDating(null);
                  draft(batch, ed);
                }}
              >
                {ingestOn ? "Make and ingest all" : "Make all"} {batch.length}
              </button>
            </div>
          </div>
        </Dialog>
      )}
    </main>
  );
}
