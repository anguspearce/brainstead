// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Reads the sign-in the Slack web client keeps in its page: each workspace's client token, from
// `localConfig_v2` in localStorage (D-20261118-01). Loaded with importScripts and passed, as
// `globalThis.brainsteadReadSlack`, to chrome.scripting.executeScript, which runs it in the page.
// The session cookie (`d`) isn't readable from the page; background.js reads it with
// chrome.cookies. Nothing leaves the user's own machine: both go to Brainstead's local helper.

globalThis.brainsteadReadSlack = function readSlack() {
  // Runs in the Slack page, so it can't use anything outside this function.
  const out = { origin: location.origin, workspaces: [], problems: [] };
  try {
    const raw = localStorage.getItem("localConfig_v2");
    const cfg = raw ? JSON.parse(raw) : null;
    for (const t of Object.values((cfg && cfg.teams) || {})) {
      if (t && typeof t.token === "string" && t.token.startsWith("xoxc-") && t.url) {
        out.workspaces.push({ id: String(t.id || ""), name: String(t.name || ""), url: String(t.url), token: t.token });
      }
    }
    if (!out.workspaces.length) out.problems.push("no workspace sign-in in this tab");
  } catch (e) {
    out.problems.push(`reading the sign-in: ${(e && e.message) || e}`);
  }
  return out;
};

/** The workspaces worth handing over: one per id, each with a token and an https address. */
globalThis.brainsteadSlackWorkspaces = function workspaces(pages) {
  const seen = new Map();
  for (const p of pages) {
    for (const w of (p && p.workspaces) || []) {
      if (w.id && w.token && /^https:\/\/[^/]+\.slack\.com\/?$/.test(w.url) && !seen.has(w.id)) seen.set(w.id, w);
    }
  }
  return [...seen.values()];
};
