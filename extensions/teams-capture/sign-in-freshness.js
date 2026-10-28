// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Whether the Teams tab's sign-in needs renewing, from the access tokens the page keeps (read by
// background.js's readSignIn). Brainstead's sync uses four of them; Teams only renews a token when
// it next needs it, so one left alone in a background tab runs out and the sync goes stale. When
// one Brainstead uses runs out soon (or isn't there), background.js reloads the tab, which makes
// Teams fetch fresh ones, then hands them over.
//
// Loaded into the service worker with importScripts, so it hangs its functions on
// `globalThis.brainsteadSignIn`. The tests (sign-in-freshness.test.js, run by Vitest) load it the
// same way.

(() => {
  // The services the sync uses, by what the token's scopes name (Brainstead's sync/auth.rs picks
  // the same ones by audience). Not mail: its token comes from the Outlook extension
  // (D-20261028-02), and reloading Teams doesn't reliably fetch one.
  const SERVICES = [
    { name: "chats", test: (t) => t.includes("api.spaces.skype.com") },
    { name: "transcripts", test: (t) => t.includes("substrate.office.com") },
    { name: "channel list", test: (t) => t.includes("chatsvcagg.teams.microsoft.com") },
  ];

  /** How long a reload is given before the sign-in is considered due again, in seconds. */
  const SOON_SECS = 10 * 60;

  /** Each service's longest-lived token expiry (seconds since 1970), or null when there's none. */
  function expiries(ats) {
    return SERVICES.map((s) => {
      const times = (ats || [])
        .filter((a) => s.test(String(a.target || "").toLowerCase()))
        .map((a) => Number(a.expiresOn))
        .filter((n) => Number.isFinite(n) && n > 0);
      return { name: s.name, expires: times.length ? Math.max(...times) : null };
    });
  }

  /** The services whose token is missing or runs out within SOON_SECS of `nowSecs`. */
  function due(ats, nowSecs) {
    return expiries(ats)
      .filter((e) => e.expires === null || e.expires < nowSecs + SOON_SECS)
      .map((e) => e.name);
  }

  globalThis.brainsteadSignIn = { expiries, due, SOON_SECS };
})();
