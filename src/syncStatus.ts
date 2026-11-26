// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What the Teams and Outlook sign-in's state means, in words: for the top bar's dot (SyncDot), the
// Settings Teams and Outlook cards and the MCP sync get, so all three say the same thing.

import type { SyncSignIn } from "./api";

/** The line for when Microsoft turned the sign-in down for some services. */
export function rejectedText(rejected: string[]): string {
  const apps = [...new Set(rejected.map((r) => (r === "mail" ? "Outlook" : "Teams")))].join(" and ");
  return `Signed out: ${rejected.join(", ")}. Microsoft turned the sign-in down; if ${apps} in Chrome asks you to sign in, do so, and it's picked up within 15 minutes.`;
}

/** The line for when the last handover came from an older Teams extension than this build ships. */
export function extensionOutdatedText(s: Pick<SyncSignIn, "extensionOutdated" | "extensionShipped">): string {
  const from = s.extensionOutdated ? `version ${s.extensionOutdated}` : "an older version";
  return `The last sign-in came from ${from} of the Teams extension; this app ships ${s.extensionShipped}. Reload it at chrome://extensions if you haven't; this clears at its next hand-over, within 15 minutes.`;
}

/** The top bar's summary: connected, something needing attention, or not signed in. */
/** Whether Outlook mail syncs: its own switch, or before there was one, Teams'. */
export const mailOn = (s: { syncEnabled?: boolean; syncOutlook?: boolean }) => s.syncOutlook ?? !!s.syncEnabled;

/** What Teams covers, as the sign-in state names it; mail is Outlook's. */
const TEAMS_PARTS = ["chats", "transcripts", "channel list"];

/** The sync's status for the dot: about only the services switched on (Teams, Outlook mail). */
export function syncStatus(s: SyncSignIn | null, on = { teams: true, mail: true }): { tone: "ok" | "warn" | "off"; text: string } {
  const names = [on.teams && "Teams", on.mail && "Outlook"].filter(Boolean).join(" and ");
  const live = (on.teams && s?.teams !== false && s?.signedIn) || (on.mail && s?.outlook !== false && s?.signedIn);
  if (!s?.signedIn || !live) {
    return {
      tone: "off",
      text: `${names}: not signed in. Open ${names} in Chrome so the extension${on.teams && on.mail ? "s hand" : " hands"} the sign-in over.`,
    };
  }
  const mine = (x: string) => (x === "mail" ? on.mail : on.teams && TEAMS_PARTS.includes(x));
  const rejected = s.rejected.filter(mine);
  const stale = s.stale.filter(mine);
  const problems: string[] = [];
  if (rejected.length) problems.push(rejectedText(rejected));
  if (stale.length) {
    const open = stale.every((x) => x === "mail") ? "Outlook" : stale.includes("mail") ? "Teams and Outlook" : "Teams";
    problems.push(`Gone stale: ${stale.join(", ")}. Keep ${open} open in Chrome.`);
  }
  if (on.teams && s.handedOver && s.extensionOutdated !== null) problems.push(extensionOutdatedText(s));
  if (problems.length) return { tone: "warn", text: problems.join(" ") };
  return { tone: "ok", text: `${names} connected.` };
}
