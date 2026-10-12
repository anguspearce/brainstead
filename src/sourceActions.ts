// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What can be done with a source, the same wherever it's offered: the Inbox's buttons for a
// capture, a Sources row's button and its ▾ menu, and the file menu. The one it usually becomes
// leads: a transcript's meeting note, a thread's reply, else ingesting it.

import { askAboutNote } from "./Ask";
import { ingest } from "./Ingest";
import { canIngest } from "./Lists";
import { isTranscriptPath, makeMeetingNotes } from "./meetingFlow";
import { nav } from "./nav";
import { isThread } from "./skills/Reply";

export type SourceAction = "meeting" | "reply" | "ingest" | "ask";

/** The source's actions, the leading one first; one already ingested leads with Re-ingest. None for
 *  a file an ingest can't read (an SVG, a wiki page, a template). */
export function sourceActions(path: string, ingested = false): SourceAction[] {
  if (!canIngest(path)) return [];
  const all: SourceAction[] = isTranscriptPath(path)
    ? ["meeting", "ingest", "ask"]
    : isThread(path)
      ? ["reply", "ingest", "ask"]
      : ["ingest", "ask"];
  return ingested ? ["ingest", ...all.filter((a) => a !== "ingest")] : all;
}

/** Each action's key (in the Inbox), icon, label, short label (a row's button) and tip. */
export const SOURCE_ACTION: Record<
  SourceAction,
  {
    key: string;
    icon: string;
    label: (ingested: boolean) => string;
    short: (ingested: boolean) => string;
    tip: (ingested: boolean) => string;
  }
> = {
  meeting: {
    key: "M",
    icon: "note",
    label: () => "Make a meeting note",
    short: () => "Meeting note",
    tip: () => "Write a meeting or 1-1 note from this transcript, ingest it and move the transcript to the Trash",
  },
  reply: {
    key: "D",
    icon: "send",
    label: () => "Draft a reply",
    short: () => "Draft reply",
    tip: () => "Draft a reply to this thread",
  },
  ingest: {
    key: "I",
    icon: "wiki",
    label: (ing) => (ing ? "Ingest again" : "Ingest into the wiki"),
    short: (ing) => (ing ? "Re-ingest" : "Ingest"),
    tip: (ing) => (ing ? "Ingest this source again, to bring the wiki up to date with it" : "Have the AI read it into the wiki"),
  },
  ask: {
    key: "A",
    icon: "ask",
    label: () => "Ask about this source",
    short: () => "Ask",
    tip: () => "Start a chat in Ask about it",
  },
};

/** Does the action, as each screen offers it (the Inbox also takes a capture out once it's done). */
export function runSourceAction(a: SourceAction, path: string): void {
  if (a === "meeting") void makeMeetingNotes([path]);
  else if (a === "reply") nav.go({ screen: "reply", path });
  else if (a === "ingest") ingest([path]);
  else askAboutNote(path);
}
