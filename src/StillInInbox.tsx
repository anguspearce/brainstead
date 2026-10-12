// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A capture still in the Inbox, on a screen the Inbox sent it to (Draft reply, Ask about this
// source): the choices that take it out, there, so there's no going back to the Inbox for them.

import { useEffect, useState } from "react";
import { api } from "./api";
import { captureDone, useInbox } from "./gtd";
import { Icon } from "./icons";
import { ingest, meetingProgress, runs, startRuns } from "./Ingest";
import { changes, startKnowledge } from "./knowledge";
import { followThrough, isTranscriptPath, makeMeetingNotes, trashTranscript } from "./meetingFlow";
import { useStore } from "./store";
import { reportEditError } from "./taskModel";
import { toast } from "./Toast";

/** Whether this file is a capture waiting in the Inbox. */
export function useInInbox(path: string | null | undefined): boolean {
  const items = useInbox();
  return !!path && !!items?.some((i) => i.kind === "capture" && i.path === path);
}

/** Takes the capture out of the Inbox, kept in Sources: what's left once a reply is copied. True
 *  once it's out; on an error it's said and the capture stays. */
export const keepInSources = (path: string): Promise<boolean> =>
  captureDone(path)
    .then(() => {
      toast("Out of the Inbox, kept in Sources", undefined, "ok");
      return true;
    })
    .catch((e) => {
      reportEditError(e);
      return false;
    });

/** "Still in the Inbox:" and the choices that take it out, the usual one first. Render it keyed by
 *  the file, so a choice made for one capture never hides the strip for the next. */
export function StillInInbox({ path, onDone }: { path: string; onDone?: () => void }) {
  const inInbox = useInInbox(path);
  useEffect(startRuns, []);
  useEffect(startKnowledge, []);
  const runList = useStore(runs);
  const changeRows = useStore(changes);
  // Chosen, it's hidden at once rather than when the Inbox next loads; a transcript whose meeting
  // note is under way stays in the Inbox until it's made and ingested, so it stays hidden meanwhile.
  const [settled, setSettled] = useState(false);
  const transcript = isTranscriptPath(path);
  if (!inInbox || settled || (transcript && meetingProgress(runList ?? [], path, changeRows)?.busy)) return null;
  const done = () => {
    setSettled(true);
    onDone?.();
  };
  return (
    <div className="stillin" role="status">
      <Icon name="inbox" size={14} />
      <span className="grow">Still in the Inbox.</span>
      {transcript && (
        <button
          type="button"
          className="btn sm pri"
          title="Write the meeting note from this transcript, as the Inbox does; it leaves once that's done"
          // Not hidden for good: while the note is under way the strip waits, and if it fails it's back.
          onClick={() => void makeMeetingNotes([path])}
        >
          {followThrough().ingest ? "Make and ingest a meeting note" : "Make a meeting note"}
        </button>
      )}
      <button
        type="button"
        className={`btn sm ${transcript ? "" : "pri"}`}
        title="Nothing more to do with it: take it out of the Inbox and keep the file in Sources"
        onClick={() => void keepInSources(path).then((ok) => ok && done())}
      >
        Done, keep in Sources
      </button>
      {!transcript && (
        <button
          type="button"
          className="btn sm"
          title="Have the AI read it into the wiki, and take it out of the Inbox"
          onClick={() => {
            ingest([path]);
            void captureDone(path).then(done).catch(reportEditError);
          }}
        >
          Ingest and take out
        </button>
      )}
      <button
        type="button"
        className="btn sm ghost"
        title="Move the file to the vault's Trash; restored from there, it's back in the Inbox"
        onClick={() =>
          void (transcript ? trashTranscript(path) : api.trashMove(path))
            .then(() => {
              toast("Moved to the Trash", undefined, "ok");
              done();
            })
            .catch(reportEditError)
        }
      >
        Move to the Trash
      </button>
    </div>
  );
}
