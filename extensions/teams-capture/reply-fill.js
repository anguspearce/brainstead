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

  /** In the page: waits until Teams shows `conversation` (the chat the reply belongs to, e.g.
   *  `19:…@unq.gbl.spaces`) with its message box, then types the reply into it, unsent. Types
   *  nothing unless the page shows that chat is the one open: Teams can show the last chat's box
   *  instead (slow routing, an expired sign-in dropping the link's route), and the reply must never
   *  go into another chat. Leaves a box that already has something typed in it alone. `wait` is for
   *  the tests: how many times to look, and how long between looks and before typing. */
  async function fill(html, text, conversation, wait) {
    const { tries = 120, every = 500, settle = 2000 } = wait || {};
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const id = String(conversation || "").toLowerCase();
    if (!id.includes(":")) return { ok: false, why: "Brainstead didn't say which chat the reply is for" };
    const forms = [id, encodeURIComponent(id).toLowerCase()];
    const names = (s) =>
      forms.some((f) =>
        String(s || "")
          .toLowerCase()
          .includes(f),
      );
    // The chat open now is this one when the address names it, or the chat list's selected item does.
    const isOpen = () => {
      let at = location.href;
      try {
        at = decodeURIComponent(at);
      } catch {
        // A malformed escape: the raw address is checked.
      }
      if (names(at) || names(location.href)) return true;
      const picked = document.querySelectorAll('[aria-selected="true"], [aria-current="page"], [aria-current="true"]');
      for (const el of picked) {
        for (const e of [el, ...el.querySelectorAll("*")].slice(0, 200)) {
          for (const a of e.attributes) if (names(a.value)) return true;
        }
      }
      return false;
    };
    const find = () =>
      document.querySelector('[data-tid="ckeditor"][contenteditable="true"]') ||
      document.querySelector('div[contenteditable="true"][role="textbox"]');
    const typed = (el) => (el.innerText || el.textContent || "").trim();
    let box = null;
    let open = false;
    for (let i = 0; i < tries && !(box && open); i++) {
      box = find();
      open = isOpen();
      if (!(box && open)) await sleep(every);
    }
    if (!box) return { ok: false, why: "Teams didn't show the chat's message box" };
    if (!open) return { ok: false, why: "Teams didn't open the reply's chat, so nothing was typed into the chat it showed" };
    // Teams can show the last chat's box before it opens the linked one: give it a moment to settle,
    // then check again that it's still this chat.
    await sleep(settle);
    box = find() || box;
    if (!isOpen()) return { ok: false, why: "Teams moved to another chat, so nothing was typed" };
    if (typed(box)) return { ok: false, why: "the message box already had something in it" };
    // The cursor at the end of the box, where the editor looks for it when text comes in.
    const toEnd = () => {
      box.focus();
      const range = document.createRange();
      range.selectNodeContents(box);
      range.collapse(false);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    };
    // 1. Pasted as HTML, so the paragraphs keep their gaps.
    toEnd();
    try {
      const data = new DataTransfer();
      data.setData("text/html", html);
      data.setData("text/plain", text);
      box.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
    } catch {
      // A page without DataTransfer: put in below.
    }
    await sleep(300);
    // 2. The editor ignored the paste: the HTML put in as an edit.
    if (!typed(box)) {
      toEnd();
      document.execCommand("insertHTML", false, html);
      await sleep(300);
    }
    // 3. It takes only typing: typed a paragraph at a time, a blank line between them made with line
    //    breaks (Shift+Enter's), never a new paragraph, which Teams can take as Enter, its Send.
    if (!typed(box)) {
      toEnd();
      const paras = text
        .split(/\n\s*\n/)
        .map((p) => p.trim())
        .filter(Boolean);
      paras.forEach((para, i) => {
        if (i) {
          document.execCommand("insertLineBreak");
          document.execCommand("insertLineBreak");
        }
        para.split("\n").forEach((line, j) => {
          if (j) document.execCommand("insertLineBreak");
          document.execCommand("insertText", false, line);
        });
      });
      await sleep(300);
    }
    return typed(box) ? { ok: true } : { ok: false, why: "Teams didn't take the text" };
  }

  globalThis.brainsteadReply = { keyOf, fill };
})();
