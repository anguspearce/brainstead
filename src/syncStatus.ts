// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What the Teams and Outlook sign-in's state means, in words: for the top bar's dot (SyncDot), the
// Settings Sign-in card and the MCP sync get, so all three say the same thing.

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
export function syncStatus(s: SyncSignIn | null): { tone: "ok" | "warn" | "off"; text: string } {
  if (!s?.signedIn) {
    return {
      tone: "off",
      text: "Teams and Outlook: not signed in. Open Teams and Outlook in Chrome so the extensions hand the sign-in over.",
    };
  }
  const problems: string[] = [];
  if (s.rejected.length) problems.push(rejectedText(s.rejected));
  if (s.stale.length) {
    const open = s.stale.every((x) => x === "mail") ? "Outlook" : s.stale.includes("mail") ? "Teams and Outlook" : "Teams";
    problems.push(`Gone stale: ${s.stale.join(", ")}. Keep ${open} open in Chrome.`);
  }
  if (s.handedOver && s.extensionOutdated !== null) problems.push(extensionOutdatedText(s));
  if (problems.length) return { tone: "warn", text: problems.join(" ") };
  return { tone: "ok", text: "Teams and Outlook connected." };
}
