// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Put in Teams's link key and its fill (reply-fill.js). Vitest runs it with the app's tests (`make check`).

import { afterEach, beforeAll, describe, expect, it } from "vitest";

let keyOf, fill;
beforeAll(async () => {
  await import("./reply-fill.js");
  ({ keyOf, fill } = globalThis.brainsteadReply);
});

describe("keyOf", () => {
  it("reads the key Brainstead put on the chat's link", () => {
    expect(keyOf("https://teams.microsoft.com/_?brainstead-reply=18a2f9c1#/conversations/19%3Aa")).toBe("18a2f9c1");
    expect(keyOf("https://teams.microsoft.com/v2/?x=1&brainstead-reply=ab12")).toBe("ab12");
  });
  it("takes nothing else", () => {
    expect(keyOf("https://teams.microsoft.com/_#/conversations/19%3Aa")).toBeNull();
    expect(keyOf("https://teams.microsoft.com/?brainstead-reply=../x")).toBeNull();
    expect(keyOf("https://teams.microsoft.com/?not-brainstead-reply=ab")).toBeNull();
    expect(keyOf(undefined)).toBeNull();
  });
});

describe("fill", () => {
  const ours = "19:04b42fdf-aaaa_0a1f956a-bbbb@unq.gbl.spaces";
  const other = "19:77c0ffee-cccc_0a1f956a-bbbb@unq.gbl.spaces";
  const quick = { tries: 3, every: 1, settle: 1 };
  // A Teams page with a message box, and the chat list's selected item naming `open`.
  const teams = (open) => {
    document.body.innerHTML = `
      <div role="tree">
        <div data-tid="chat-list-item" id="chat-list-item_${ours}" aria-selected="${open === ours}">Maya</div>
        <div data-tid="chat-list-item" id="chat-list-item_${other}" aria-selected="${open === other}">Lena</div>
      </div>
      <div data-tid="ckeditor" contenteditable="true"></div>`;
    return document.querySelector('[data-tid="ckeditor"]');
  };
  const typedIn = (box) => (box.textContent || "").trim();
  afterEach(() => {
    document.body.innerHTML = "";
    history.replaceState(null, "", "/");
  });

  it("types nothing into another chat's box", async () => {
    const box = teams(other);
    const r = await fill("<p>Hi Maya</p>", "Hi Maya", ours, quick);
    expect(r.ok).toBe(false);
    expect(r.why).toMatch(/didn't open the reply's chat/);
    expect(typedIn(box)).toBe("");
  });

  it("types nothing when it isn't told the chat", async () => {
    const box = teams(ours);
    expect((await fill("<p>Hi</p>", "Hi", undefined, quick)).ok).toBe(false);
    expect(typedIn(box)).toBe("");
  });

  it("types into the reply's own chat, found by the chat list", async () => {
    const box = teams(ours);
    document.execCommand = (c, _u, text) => c === "insertText" && ((box.textContent += text), true);
    expect(await fill("<p>Hi Maya</p>", "Hi Maya", ours, quick)).toEqual({ ok: true });
    expect(typedIn(box)).toBe("Hi Maya");
  });

  it("knows the chat from the address, encoded or not", async () => {
    const box = teams(null);
    history.replaceState(null, "", `/_#/conversations/${encodeURIComponent(ours)}`);
    document.execCommand = (c, _u, text) => c === "insertText" && ((box.textContent += text), true);
    expect((await fill("<p>Hi</p>", "Hi", ours, quick)).ok).toBe(true);
  });

  it("keeps the gaps between paragraphs when Teams takes only typing", async () => {
    const box = teams(ours);
    // An editor that ignores a paste and inserted HTML, and takes typed text and line breaks.
    const did = [];
    document.execCommand = (c, _u, v) => {
      did.push(c);
      if (c === "insertText") box.textContent += v;
      else if (c === "insertLineBreak") box.textContent += "\n";
      else return false;
      return true;
    };
    const r = await fill("<p>Hi Maya</p><p>Thanks,<br>Lena</p>", "Hi Maya\n\nThanks,\nLena", ours, quick);
    expect(r.ok).toBe(true);
    expect(box.textContent).toBe("Hi Maya\n\nThanks,\nLena");
    expect(did).toContain("insertHTML");
    expect(did).not.toContain("insertParagraph");
  });
});
