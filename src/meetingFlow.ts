// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A meeting note in one step (D-20261006-03): a transcript Brainstead is sure about (its type,
// name and date read from its name, no note there yet) is drafted, made, ingested and moved to
// the Trash, with one toast at the end. One it isn't sure about opens Meeting note from a
// transcript at that question. Ingesting and trashing are the defaults (Settings › AI assistants);
// with both off, the note gets the old offer instead. Order is note, ingest, trash: a transcript
// whose note or ingest failed is kept.

import { api, IngestRun, NoteSpec, Transcript } from "./api";
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
/** A note's ingest run → what's waiting on it. */
const ingesting = new Map<string, { transcript: string; note: string }>();
let nextBatch = 1;

/** Drafts these notes, as one batch: each is followed through when it's made. */
export async function draftNotes(pairs: [string, NoteSpec][], unattended = false): Promise<string[]> {
  const ids = await api.meetingDraft(pairs, unattended);
  const b = nextBatch++;
  batches.set(b, { total: pairs.length, results: [] });
  for (const [t] of pairs) batchOf.set(t, b);
  // A transcript captured from Teams leaves the Inbox once its note is being drafted.
  pairs.forEach(([p]) => void api.inboxCaptureDone(p).catch(() => {}));
  return ids;
}

/** Make meeting note on these transcripts: the sure ones in one step, the rest on the meeting screen. */
export async function makeMeetingNotes(paths: string[]) {
  try {
    const all = await api.meetingTranscripts();
    const asked = paths.map((p) => all.find((t) => t.path === p)).filter((t): t is Transcript => !!t);
    const sure = asked.filter(sureOf);
    const unsure = asked.filter((t) => !sureOf(t));
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
  if (!f.ingest && !f.trash) {
    batchOf.delete(r.source);
    return false;
  }
  // A note an unattended assistant started is ingested unattended too: its failing changes are held.
  if (f.ingest)
    void api
      .ingestStart([note], r.trigger === "scheduled")
      .then(([id]) => ingesting.set(id, { transcript: r.source, note }))
      .catch((e) => finish(r.source, { note, ingested: false, trashed: false, error: `couldn't ingest it: ${String(e)}` }));
  else void trash(r.source, note, false);
  return true;
}

/** An ingest run ended: if it was a new meeting note's, the transcript goes to the Trash (if it worked). */
export function ingestRunEnded(r: IngestRun) {
  const w = ingesting.get(r.id);
  if (!w) return;
  ingesting.delete(r.id);
  if (r.status !== "done") return finish(w.transcript, { note: w.note, ingested: false, trashed: false, error: `its ingest ${r.status}` });
  if (!followThrough().trash) return finish(w.transcript, { note: w.note, ingested: true, trashed: false });
  void trash(w.transcript, w.note, true);
}

async function trash(transcript: string, note: string, ingested: boolean) {
  try {
    await api.trashMove(transcript);
    finish(transcript, { note, ingested, trashed: true });
  } catch (e) {
    finish(transcript, { note, ingested, trashed: false, error: `couldn't move the transcript to the Trash: ${String(e)}` });
  }
}
