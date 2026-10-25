// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What can be done with a source, the same wherever it's offered: the Inbox's buttons for a
// capture, a Sources row's button and its ▾ menu, and the file menu. The one it usually becomes
// leads: a transcript's meeting note, a thread's reply, else ingesting it.

import { askAboutNote } from "./Ask";
import { ingest } from "./Ingest";
import { canIngest } from "./Lists";
import { followThrough, isTranscriptPath, makeMeetingNotes } from "./meetingFlow";
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

/** What Make a meeting note does, as the Meeting notes switches have it: the note, then ingesting
 *  it and moving the transcript to the Trash when they're on. */
export function meetingTip(n = 1): string {
  const f = followThrough();
  const then = [
    f.ingest && (n === 1 ? "ingest it" : "ingest each"),
    f.trash && (n === 1 ? "move the transcript to the Trash" : "move the transcripts to the Trash"),
  ].filter(Boolean);
  const what = n === 1 ? "Write a meeting or 1-1 note from this transcript" : "Write a meeting or 1-1 note from each transcript";
  return then.length ? `${what}, then ${then.join(" and ")}` : what;
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
    // Says what follows the note, as Settings › AI assistants' Meeting notes switches have it.
    label: () => (followThrough().ingest ? "Make and ingest a meeting note" : "Make a meeting note"),
    short: () => "Meeting note",
    tip: () => meetingTip(),
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

/** "Don't sync this sender" choices from an address: the exact sender, and (when it has one) their
 *  whole domain. Each entry is what goes in Settings › Teams and Outlook sync's skip list. */
export function ignoreSenderItems(address: string): { entry: string; label: string; tip: string }[] {
  const addr = address.trim().toLowerCase();
  const items = [{ entry: addr, label: `Don't sync ${addr}`, tip: `Stop syncing mail from ${addr}` }];
  const domain = addr.split("@")[1];
  if (domain) {
    items.push({ entry: domain, label: `Don't sync anyone at ${domain}`, tip: `Stop syncing mail from any ${domain} address` });
  }
  return items;
}
