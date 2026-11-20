// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Draft reply's Put in Teams (Brainstead D-20261120-05): Brainstead opens the chat's link with a
// one-time key on it (`brainstead-reply=<key>`); background.js sees the link open, collects the
// reply for that key from Brainstead's helper and runs `fill` in the tab, which types it into the
// chat's message box. It never sends: the reply waits in the box for the user to press Send.
//
// Loaded into the service worker with importScripts, so it hangs its functions on
// `globalThis.brainsteadReply`; `fill` runs in the page through chrome.scripting, so it uses
// nothing outside itself. The tests (reply-fill.test.js, run by Vitest) load it the same way.

(() => {
  /** The key on a link Brainstead opened, or null. */
  function keyOf(url) {
    const m = /[?&#]brainstead-reply=([A-Za-z0-9]{1,64})(?:[&#]|$)/.exec(String(url || ""));
    return m ? m[1] : null;
  }

  /** In the page: waits for the chat's message box, then types the reply into it, unsent. Leaves a
   *  box that already has something typed in it alone. */
  async function fill(html, text) {
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const find = () =>
      document.querySelector('[data-tid="ckeditor"][contenteditable="true"]') ||
      document.querySelector('div[contenteditable="true"][role="textbox"]');
    const typed = (el) => (el.innerText || el.textContent || "").trim();
    let box = null;
    for (let i = 0; i < 120 && !box; i++) {
      box = find();
      if (!box) await sleep(500);
    }
    if (!box) return { ok: false, why: "Teams didn't show the chat's message box" };
    // Teams can show the last chat's box before it opens the linked one: give it a moment to settle.
    await sleep(2000);
    box = find() || box;
    if (typed(box)) return { ok: false, why: "the message box already had something in it" };
    box.focus();
    try {
      const data = new DataTransfer();
      data.setData("text/html", html);
      data.setData("text/plain", text);
      box.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
    } catch {
      // A page without DataTransfer: typed in below.
    }
    await sleep(300);
    if (!typed(box)) document.execCommand("insertText", false, text);
    await sleep(300);
    return typed(box) ? { ok: true } : { ok: false, why: "Teams didn't take the text" };
  }

  globalThis.brainsteadReply = { keyOf, fill };
})();
