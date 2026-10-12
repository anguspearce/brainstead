// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A meeting note in one step (D-20261006-03): a transcript Brainstead is sure about (its type,
// name and date read from its name, no note there yet) is drafted, made, ingested and moved to
// the Trash, with one toast at the end. One it isn't sure about opens Meeting note from a
// transcript at that question. Ingesting and trashing are the defaults (Settings › AI assistants);
// with both off, the note gets the old offer instead. Order is note, ingest, trash: a transcript
// whose note or ingest failed is kept, and stays in the Inbox to be chosen again; it leaves the
// Inbox once its note is made and ingested (D-20261012-10).

import { api, ChangeOrigin, ChangeOutcome, ChangeRow, IngestRun, NoteSpec, TrashEntry, Transcript } from "./api";
import { captureDone } from "./gtd";
import { pageName } from "./knowledge";
import { nav } from "./nav";
import { settings } from "./store";
import { reportEditError } from "./taskModel";
import { toast } from "./Toast";

/** The note a transcript becomes, from what was read from its name and what was changed. */
export function specOf(t: Transcript, edits: Partial<NoteSpec> = {}): NoteSpec {
  return {
    type: edits.type ?? t.inferred.type ?? "Meeting",
    name: edits.name ?? t.inferred.name ?? "",
    date: edits.date ?? t.inferred.date ?? "",
  };
}

export const noteFile = (s: NoteSpec) => `${s.type}. ${s.name.trim()} - ${s.date}.md`;
export const specOk = (s: NoteSpec) => !!s.name.trim() && /^\d{4}-\d{2}-\d{2}$/.test(s.date);

/** A Teams meeting transcript, as the extension names it, in sources/ or a folder in it. */
export const isTranscriptPath = (p: string) => /^sources\/(?:.+\/)?Teams\. Transcript\./.test(p);

/** Brainstead is sure what note it becomes: nothing to ask, so it can go in one step. */
export const sureOf = (t: Transcript) => t.inferred.confident && !t.inferred.dateCheck && !t.inferred.exists && specOk(specOf(t));

/** Whether a finished meeting run made its note, rather than holding it in Changes (its last step says which). */
export function meetingNoteMade(r: IngestRun): boolean {
  const last = r.steps[r.steps.length - 1];
  return !!last && last.status === "done" && !/held in changes/i.test(last.detail ?? "");
}

export const isActive = (r: IngestRun) => r.status === "running" || r.status === "queued";

/** The note a meeting run writes, by its file name. */
export const runNote = (m: IngestRun) => (m.note ? `${m.note.type.trim()}. ${m.note.name.trim()} - ${m.note.date.trim()}.md` : null);

/** What follows a note being made: ingest it, then move the transcript to the Trash. */
export function followThrough(): { ingest: boolean; trash: boolean } {
  const s = settings.get();
  return { ingest: s.meetingIngest !== false, trash: s.meetingTrash !== false };
}

interface Result {
  note: string;
  ingested: boolean;
  trashed: boolean;
  /** Why it stopped short (the transcript is then kept). */
  error?: string;
}

/** The transcripts sent together, so the end of them is one toast. */
const batches = new Map<number, { total: number; results: Result[] }>();
/** transcript → its batch. */
const batchOf = new Map<string, number>();
/** A note's ingest run → what's waiting on it. Only in memory: after a reload, `followedBy` finds
 *  it again from the runs list. */
const ingesting = new Map<string, Waiting>();
/** What a note's ingest is waiting to follow through: the transcript, its note, and the meeting run
 *  that made it (whose change in Changes says whether the note is still wanted). */
interface Waiting {
  transcript: string;
  note: string;
  run: string;
}

/** The transcript an ingest run follows through, found from the runs (`all`, newest first) when
 *  the window was reloaded since it started: the run is the first ingest of a note a meeting run
 *  made, started after it. A later ingest of the same note (Ingest again) isn't one. */
function followedBy(r: IngestRun, all: IngestRun[]): Waiting | undefined {
  if (r.kind === "meeting") return undefined;
  const name = r.source.split("/").pop() ?? "";
  const m = all.find((x) => x.kind === "meeting" && x.status === "done" && runNote(x) === name && x.started <= r.started);
  if (!m || !meetingNoteMade(m)) return undefined;
  return noteIngestOf(m, all)?.id === r.id ? { transcript: m.source, note: name, run: m.id } : undefined;
}

/** The ingest that follows a meeting run's note through: the first ingest of that note's file
 *  started after the run (`all` is newest first). */
function noteIngestOf(m: IngestRun, all: IngestRun[]): IngestRun | undefined {
  const name = runNote(m);
  return all.filter((x) => x.kind !== "meeting" && x.source.split("/").pop() === name && x.started >= m.started).pop();
}

/** Transcripts this window has moved to the Trash or kept the note of (keepNote): the follow-
 *  through leaves them alone after that, so the end of the note's ingest doesn't trash one again. */
const handled = new Set<string>();
let nextBatch = 1;

/** Drafts these notes, as one batch: each is followed through when it's made. */
export async function draftNotes(pairs: [string, NoteSpec][], unattended = false): Promise<string[]> {
  const ids = await api.meetingDraft(pairs, unattended);
  const b = nextBatch++;
  batches.set(b, { total: pairs.length, results: [] });
  for (const [t] of pairs) {
    batchOf.set(t, b);
    // A new note for it (restored from the Trash, say): followed through afresh.
    handled.delete(t);
  }
  return ids;
}

/** A transcript captured from Teams leaves the Inbox once its note is made and ingested (or just
 *  made, with ingesting off); until then, and if either fails, it stays there to be chosen again. */
const leaveInbox = (transcript: string) => captureDone(transcript).catch(() => {});

/** Transcripts whose Make meeting note was pressed and whose run hasn't ended: pressed again (the
 *  button twice, the button and M, the Inbox and its Still in the Inbox strip), nothing more starts. */
const starting = new Set<string>();

/** Make meeting note on these transcripts: the sure ones in one step, the rest on the meeting screen. */
export async function makeMeetingNotes(paths: string[]) {
  const fresh = [...new Set(paths)].filter((p) => !starting.has(p));
  if (!fresh.length) return;
  fresh.forEach((p) => starting.add(p));
  try {
    const all = await api.meetingTranscripts();
    const asked = fresh.map((p) => all.find((t) => t.path === p)).filter((t): t is Transcript => !!t);
    const sure = asked.filter(sureOf);
    const unsure = asked.filter((t) => !sureOf(t));
    // Only the sure ones run now; the rest can be pressed again once the meeting screen has asked.
    fresh.filter((p) => !sure.some((t) => t.path === p)).forEach((p) => starting.delete(p));
    if (sure.length) {
      await draftNotes(sure.map((t) => [t.path, specOf(t)]));
      const f = followThrough();
      const then = [f.ingest && "ingested", f.trash && "its transcript moved to the Trash"].filter(Boolean).join(", ");
      toast(
        sure.length === 1
          ? `Writing ${noteFile(specOf(sure[0])).replace(/\.md$/, "")}${then ? `: it'll be ${then.replace("its transcript", "the transcript")}` : ""}`
          : `Writing ${sure.length} meeting notes, one after another`,
      );
    }
    if (unsure.length) {
      nav.go({ screen: "meeting", path: unsure[0].path });
      if (sure.length || unsure.length > 1)
        toast(`${unsure.length === 1 ? "One transcript needs" : `${unsure.length} transcripts need`} a check before its note is written`);
    }
    if (!asked.length) toast("That isn't a transcript Brainstead can make a meeting note from", undefined, "bad");
  } catch (e) {
    fresh.forEach((p) => starting.delete(p));
    reportEditError(e);
  }
}

function finish(transcript: string, r: Result) {
  const b = batchOf.get(transcript);
  batchOf.delete(transcript);
  const batch = b !== undefined ? batches.get(b) : undefined;
  if (!batch) return say([r]);
  batch.results.push(r);
  if (batch.results.length < batch.total) return;
  batches.delete(b!);
  say(batch.results);
}

/** The one toast at the end of a batch. */
function say(rs: Result[]) {
  const ok = rs.filter((r) => !r.error);
  const bad = rs.filter((r) => r.error);
  if (rs.length === 1) {
    const r = rs[0];
    if (r.error) return toast(`${pageName(r.note)}: ${r.error}`, { label: "Changes", run: () => nav.go("review") }, "bad");
    const then = [r.ingested && "ingested", r.trashed && "the transcript is in the Trash"].filter(Boolean).join("; ");
    return toast(
      `${pageName(r.note)} is in the vault${then ? `, ${then}` : ""}`,
      { label: "Open", run: () => nav.go({ screen: "doc", path: r.note }) },
      "ok",
    );
  }
  const parts = [
    ok.length && `Made ${ok.length} meeting ${ok.length === 1 ? "note" : "notes"}`,
    bad.length &&
      `${bad.length} stopped short (${bad.map((r) => pageName(r.note)).join(", ")}), keeping ${bad.length === 1 ? "its transcript" : "their transcripts"}`,
  ];
  toast(parts.filter(Boolean).join("; "), { label: "Changes", run: () => nav.go("review") }, bad.length ? "bad" : "ok");
}

/** A meeting run ended: follow its note through. True when it's handled here, so no offer is needed. */
export function meetingRunEnded(r: IngestRun, made: boolean): boolean {
  starting.delete(r.source);
  const note = r.note ? noteFile(r.note) : "";
  if (!made) {
    // Failed, stopped, or held in Changes (then offered when it's accepted there).
    if (batchOf.has(r.source))
      finish(r.source, {
        note,
        ingested: false,
        trashed: false,
        error: r.status === "done" ? "held in Changes for you" : (r.error ?? r.status),
      });
    return true;
  }
  const f = followThrough();
  if (!f.ingest) void leaveInbox(r.source);
  if (!f.ingest && !f.trash) {
    batchOf.delete(r.source);
    return false;
  }
  // A note an unattended assistant started is ingested unattended too: its failing changes are held.
  if (f.ingest)
    void api
      .ingestStart([note], r.trigger === "scheduled")
      .then(([id]) => ingesting.set(id, { transcript: r.source, note, run: r.id }))
      .catch((e) => finish(r.source, { note, ingested: false, trashed: false, error: `couldn't ingest it: ${String(e)}` }));
  else void trash(r.source, note, false);
  return true;
}

/** An ingest run ended: if it was a new meeting note's, the transcript goes to the Trash (if it
 *  worked). `all` is the runs list, to find a note's ingest started before the window reloaded. */
export async function ingestRunEnded(r: IngestRun, all: IngestRun[] = []) {
  const w = ingesting.get(r.id) ?? (followThrough().ingest ? followedBy(r, all) : undefined);
  ingesting.delete(r.id);
  if (!w || handled.has(w.transcript)) return;
  if (r.status !== "done") return finish(w.transcript, { note: w.note, ingested: false, trashed: false, error: `its ingest ${r.status}` });
  // The note reverted (or rejected) in Changes while it was being ingested: the transcript is
  // wanted for a note again, so it stays, as resumeMeetingNotes leaves it.
  if (await noteUndone(w.run))
    return finish(w.transcript, { note: w.note, ingested: true, trashed: false, error: "its note was reverted in Changes" });
  void leaveInbox(w.transcript);
  if (!followThrough().trash) return finish(w.transcript, { note: w.note, ingested: true, trashed: false });
  void trash(w.transcript, w.note, true);
}

/** Whether a meeting run's note has been rejected or reverted in Changes since (its change, by its
 *  origin's run). One gone from Changes' history counts as kept. */
async function noteUndone(run: string): Promise<boolean> {
  const rows = await api.changesList().catch(() => [] as ChangeRow[]);
  const c = rows.find((x) => x.origin.kind === "meeting" && x.origin.run === run);
  return !!c && (c.status === "rejected" || c.status === "reverted");
}

/** After the window loads (`all` the runs, newest first): a transcript whose own meeting run made
 *  its note, and whose note's ingest finished after it (while the window was reloading, say),
 *  leaves the Inbox, so it isn't offered a note again. Only that proof counts: another transcript
 *  that would make a note of the same name, or one kept with a note written by hand, is left alone,
 *  and so is one whose note was rejected or reverted in Changes. */
export async function resumeMeetingNotes(all: IngestRun[]) {
  const [transcripts, rows] = await Promise.all([
    api.meetingTranscripts().catch(() => [] as Transcript[]),
    api.changesList().catch(() => [] as ChangeRow[]),
  ]);
  for (const t of transcripts) {
    const m = all.find((x) => x.kind === "meeting" && x.source === t.path);
    if (!m || m.status !== "done" || !meetingNoteMade(m)) continue;
    const c = rows.find((x) => x.origin.kind === "meeting" && x.origin.run === m.id);
    if (c && (c.status === "rejected" || c.status === "reverted")) continue;
    if (noteIngestOf(m, all)?.status === "done") void leaveInbox(t.path);
  }
}

/** Who's trashing a transcript, when it isn't the user on a screen: an assistant or a run, whose
 *  writes are recorded in Changes as theirs (`origin`; a "scheduled" trigger is nobody watching). */
export interface TrashBy {
  origin: ChangeOrigin;
  reason?: string;
  model?: string | null;
}

/** Moves a transcript to the Trash, its links (a wiki page written from it before the note, say)
 *  first pointed at its meeting note, with the claims made from it, so trashing it leaves no
 *  missing page behind. Every way a transcript is trashed goes through here: this flow, the
 *  meeting screen, the Inbox, its Still in the Inbox strip, the after-the-note offer, the file
 *  menu (and so Sources and the command palette) and the trash_note tool. `note` left out is the
 *  one its name says, if that's in the vault; null is none. With `by`, every write is recorded as
 *  that assistant's: the link rewrites unattended when it is, and the move as its change in
 *  Changes (`outcome`) rather than the user's (`entry`). */
export async function trashTranscript(
  transcript: string,
  note?: string | null,
  by?: TrashBy,
): Promise<{ entry?: TrashEntry; outcome?: ChangeOutcome }> {
  const unattended = by?.origin.trigger === "scheduled";
  const to =
    note !== undefined
      ? note
      : await api
          .meetingTranscripts()
          .then((all) => {
            const t = all.find((x) => x.path === transcript);
            return t?.inferred.exists ? noteFile(specOf(t)) : null;
          })
          .catch(() => null);
  if (to) {
    const meta = await api
      .docRead(transcript)
      .then((d) => d.meta)
      .catch(() => null);
    const pages = [...new Set((meta?.backlinks ?? []).map((b) => b.path))].filter((p) => p !== to);
    if (pages.length) await api.healthLinkGhost(transcript.split("/").pop()!.replace(/\.md$/, ""), to, pages, unattended).catch(() => 0);
  }
  if (!by) return { entry: await api.trashMove(transcript) };
  const name = (transcript.split("/").pop() ?? transcript).replace(/\.md$/, "");
  return {
    outcome: await api.changeSubmit({
      page: transcript,
      kind: "trash",
      title: `Move ${name} to the Trash`,
      reason: by.reason ?? "",
      instruction: { op: "trash" },
      origin: by.origin,
      model: by.model ?? null,
    }),
  };
}

async function trash(transcript: string, note: string, ingested: boolean) {
  if (handled.has(transcript)) return finish(transcript, { note, ingested, trashed: true });
  try {
    await trashTranscript(transcript, note);
    handled.add(transcript);
    finish(transcript, { note, ingested, trashed: true });
  } catch (e) {
    // Gone already (trashed another way meanwhile): that's what was wanted, not a failure.
    const gone = await api.docRead(transcript).then(
      () => false,
      () => true,
    );
    finish(
      transcript,
      gone
        ? { note, ingested, trashed: true }
        : { note, ingested, trashed: false, error: `couldn't move the transcript to the Trash: ${String(e)}` },
    );
  }
}

/** Ingest the existing note, on the meeting card and start_run meeting_note's ingest_existing: no
 *  new note, the one there kept as it is, then what follows a made note (`followThrough`): ingest
 *  it, then move the transcript to the Trash. An ingest that can't start keeps the transcript.
 *  Says which of the two it did. */
export async function keepNote(transcript: string, note: string, by?: TrashBy): Promise<{ ingested: boolean; trashed: boolean }> {
  const f = followThrough();
  // Dealt with here: the end of the ingest started below isn't followed through again.
  handled.add(transcript);
  if (f.ingest) await api.ingestStart([note], by?.origin.trigger === "scheduled");
  // Its note is there (and being ingested): the transcript leaves the Inbox, trashed or not.
  void leaveInbox(transcript);
  // Held in Changes (an assistant's move, nobody watching), it isn't trashed yet.
  const t = f.trash ? await trashTranscript(transcript, note, by) : null;
  return { ingested: f.ingest, trashed: !!t && (t.outcome?.applied ?? true) };
}
