// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Brainstead's Slack sync (D-20261118-01): hands the open Slack tab's sign-in (each workspace's
// client token, and the session cookie) to Brainstead's local helper every few minutes, so the app
// can read the user's own conversations with Slack's API. It reads nothing else and sends nothing
// anywhere but that helper on this machine. Works alongside the Slack desktop app: the tab only has
// to stay open.

importScripts("slack-reader.js");

const HOST = "com.wayned.brainstead";
const ALARM = "brainstead-slack-handover";
/** Minutes between hand-overs. */
const EVERY = 10;
/** What the last hand-over did, for the popup. Holds no secret. */
const LAST = "slack-last-handover";

async function handOver() {
  const at = new Date().toISOString();
  const say = (result, extra = {}) => chrome.storage.local.set({ [LAST]: { at, result, ...extra } });
  try {
    const tabs = await chrome.tabs.query({ url: ["https://app.slack.com/*"] });
    if (!tabs.length) return say("no Slack tab open");
    const pages = [];
    const problems = [];
    for (const tab of tabs) {
      try {
        const [r] = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func: globalThis.brainsteadReadSlack });
        if (r?.result) {
          pages.push(r.result);
          problems.push(...(r.result.problems || []));
        }
      } catch (e) {
        problems.push(`tab ${tab.id}: ${e?.message || e}`);
      }
    }
    const workspaces = globalThis.brainsteadSlackWorkspaces(pages);
    if (!workspaces.length) return say(`no sign-in found${problems.length ? ` (${problems.join("; ")})` : ""}`);
    const cookie = await chrome.cookies.get({ url: "https://app.slack.com/", name: "d" });
    if (!cookie?.value) return say("the Slack session cookie isn't there: sign in to Slack in this browser");
    const reply = await chrome.runtime.sendNativeMessage(HOST, {
      type: "sign-in",
      workspaces,
      cookie: cookie.value,
      version: chrome.runtime.getManifest().version,
    });
    return say(reply?.ok ? "handed over" : `Brainstead refused: ${reply?.message || "no answer"}`, {
      workspaces: workspaces.map((w) => w.name || w.id),
    });
  } catch (e) {
    return say(`failed: ${e?.message || e}`);
  }
}

chrome.alarms.create(ALARM, { periodInMinutes: EVERY });
chrome.alarms.onAlarm.addListener((a) => {
  if (a.name === ALARM) void handOver();
});
chrome.runtime.onStartup.addListener(() => void handOver());
chrome.runtime.onInstalled.addListener(() => void handOver());
chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (msg?.type !== "slack:hand-over") return false;
  handOver().then(() => chrome.storage.local.get(LAST).then((s) => reply(s[LAST] ?? null)));
  return true;
});
