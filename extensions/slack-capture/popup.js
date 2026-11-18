// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The popup: what the last hand-over did, and a button to hand over now (background.js does it).

const statusEl = /** @type {HTMLParagraphElement} */ (document.getElementById("status"));
const go = /** @type {HTMLButtonElement} */ (document.getElementById("go"));

/** @param {{ at: string, result: string, workspaces?: string[] } | null} last */
function show(last) {
  if (!last) {
    statusEl.textContent = "Not handed over yet.";
    return;
  }
  const when = new Date(last.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const ws = last.workspaces?.length ? ` (${last.workspaces.join(", ")})` : "";
  statusEl.textContent = `${when}: ${last.result}${ws}.`;
}

chrome.storage.local.get("slack-last-handover").then((s) => show(s["slack-last-handover"] ?? null));
go.addEventListener("click", () => {
  go.disabled = true;
  statusEl.textContent = "Handing over…";
  chrome.runtime.sendMessage({ type: "slack:hand-over" }).then((last) => {
    show(last);
    go.disabled = false;
  });
});
