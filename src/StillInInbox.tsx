// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A capture still in the Inbox, on a screen the Inbox sent it to (Draft reply, Ask about this
// source): the choices that take it out, there, so there's no going back to the Inbox for them.

import { useState } from "react";
import { api } from "./api";
import { useInbox } from "./gtd";
import { Icon } from "./icons";
import { ingest } from "./Ingest";
import { followThrough, isTranscriptPath, makeMeetingNotes } from "./meetingFlow";
import { Store } from "./store";
import { reportEditError } from "./taskModel";
import { toast } from "./Toast";

/** The capture the Inbox last sent to Ask about it, for Ask's strip (cleared once it's dealt with). */
export const askedFromInbox = new Store<string | null>(null);

/** Whether this file is a capture waiting in the Inbox. */
export function useInInbox(path: string | null | undefined): boolean {
  const items = useInbox();
  return !!path && !!items?.some((i) => i.kind === "capture" && i.path === path);
}

/** Takes the capture out of the Inbox, kept in Sources: what's left once a reply is copied. */
export const keepInSources = (path: string) =>
  api
    .inboxCaptureDone(path)
    .then(() => toast("Out of the Inbox, kept in Sources", undefined, "ok"))
    .catch(reportEditError);

/** "Still in the Inbox:" and the choices that take it out, the usual one first. */
export function StillInInbox({ path, onDone }: { path: string; onDone?: () => void }) {
  const inInbox = useInInbox(path);
  // Marking a capture done isn't a change to the vault, so the Inbox list doesn't reload: hidden here.
  const [settled, setSettled] = useState(false);
  if (!inInbox || settled) return null;
  const done = () => {
    setSettled(true);
    onDone?.();
  };
  const transcript = isTranscriptPath(path);
  return (
    <div className="stillin" role="status">
      <Icon name="inbox" size={14} />
      <span className="grow">Still in the Inbox.</span>
      {transcript && (
        <button
          type="button"
          className="btn sm pri"
          title="Write the meeting note from this transcript, as the Inbox does; it leaves once that's done"
          onClick={() => {
            done();
            void makeMeetingNotes([path]);
          }}
        >
          {followThrough().ingest ? "Make and ingest a meeting note" : "Make a meeting note"}
        </button>
      )}
      <button
        type="button"
        className={`btn sm ${transcript ? "" : "pri"}`}
        title="Nothing more to do with it: take it out of the Inbox and keep the file in Sources"
        onClick={() => void keepInSources(path).then(done)}
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
            void api.inboxCaptureDone(path).then(done).catch(reportEditError);
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
          void api
            .trashMove(path)
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
