// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterEach, describe, expect, it, vi } from "vitest";
import { clipboardImages, CMEditor, completionTrigger, shorthandEdit } from "./cm";
import { openTaskAt } from "./taskHint";
import { forceParsing } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { merge3 } from "./merge";
import { frontmatterEnd, frontmatterKeys, lineSeparator, toggledLine } from "./text";

let ed: CMEditor | null = null;
afterEach(() => {
  ed?.destroy();
  ed = null;
});

describe("Editor interface", () => {
  it("loads, reports changes and inserts at the cursor", () => {
    const parent = document.createElement("div");
    document.body.appendChild(parent);
    ed = new CMEditor(parent);
    ed.load("hello");
    const seen = vi.fn();
    const off = ed.onChange(seen);
    ed.view.dispatch({ selection: { anchor: 5 } });
    ed.insertAtCursor(" world");
    expect(ed.getMarkdown()).toBe("hello world");
    expect(seen).toHaveBeenLastCalledWith("hello world");
    off();
    ed.insertAtCursor("!");
    expect(seen).toHaveBeenCalledTimes(1);
    // Loading isn't a change.
    ed.load("other");
    expect(seen).toHaveBeenCalledTimes(1);
  });

  it("opens below the properties, folded into one block", () => {
    const parent = document.createElement("div");
    document.body.appendChild(parent);
    ed = new CMEditor(parent);
    ed.load("---\ntitle: x\ntags: [a]\n---\n# Heading\n\n- [ ] task\n");
    expect(ed.view.state.selection.main.head).toBe(ed.view.state.doc.line(5).from);
    const props = parent.querySelector(".cm-props");
    expect(props?.textContent).toContain("Properties · 2");
  });

  it("ticks a task through the text, with a done date", () => {
    const parent = document.createElement("div");
    document.body.appendChild(parent);
    ed = new CMEditor(parent, { today: () => "2026-10-02" });
    ed.load("# T\n\n- [ ] ship it ^rank-1024\n");
    const box = parent.querySelector<HTMLElement>("[data-check]");
    expect(box).toBeTruthy();
    box!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(ed.getMarkdown()).toBe("# T\n\n- [x] ship it ✅ 2026-10-02 ^rank-1024\n");
  });

  it("draws inline HTML in Edit, even on the cursor's line, and shows the tags only in Source", () => {
    const parent = document.createElement("div");
    document.body.appendChild(parent);
    ed = new CMEditor(parent);
    ed.load('A <mark class="hl-red">red</mark> and <u>under</u><br>');
    ed.view.dispatch({ selection: { anchor: 3 } });
    expect(parent.querySelector(".cm-content")?.textContent).toBe("A red and under");
    expect(parent.querySelector(".cm-hl.hl-red")?.textContent).toBe("red");
    expect(parent.querySelector(".cm-u")?.textContent).toBe("under");
    ed.setMode("source");
    expect(parent.querySelector(".cm-content")?.textContent).toContain('<mark class="hl-red">');
  });

  it("hides HTML comments in Edit, inline and on lines of their own", () => {
    const parent = document.createElement("div");
    document.body.appendChild(parent);
    ed = new CMEditor(parent);
    ed.load("a <!-- note --> b\n\n<!-- block\nmore -->\ntext");
    ed.view.dispatch({ selection: { anchor: 1 } });
    expect(parent.querySelector(".cm-content")?.textContent).not.toContain("<!--");
    ed.setMode("source");
    expect(parent.querySelector(".cm-content")?.textContent).toContain("<!-- block");
    expect(ed.getMarkdown()).toBe("a <!-- note --> b\n\n<!-- block\nmore -->\ntext");
  });

  it("switches mode without touching the text", () => {
    ed = new CMEditor(document.createElement("div"));
    ed.load("**b** [[Link|label]]");
    ed.setMode("source");
    ed.setMode("live");
    expect(ed.getMarkdown()).toBe("**b** [[Link|label]]");
  });
});

describe("autocomplete triggers", () => {
  it("finds [[ and #", () => {
    expect(completionTrigger("see [[Orbit")).toEqual({ kind: "link", q: "Orbit", start: 4 });
    expect(completionTrigger("a #pro")).toEqual({ kind: "tag", q: "pro", start: 2 });
    expect(completionTrigger("#")).toBeNull();
    expect(completionTrigger("#wo")).toEqual({ kind: "tag", q: "wo", start: 0 });
    expect(completionTrigger("[[done]] ok")).toBeNull();
  });
});

describe("date shorthand while typing", () => {
  it("turns a finished date word into a date on a task line", () => {
    const l = "- [ ] call due: tomorrow ";
    expect(shorthandEdit(l, l.length, "2026-10-02")).toEqual({ from: 11, to: 24, insert: "📅 2026-10-03" });
    const d = "- [ ] x defer: +3d.";
    expect(shorthandEdit(d, d.length, "2026-10-02")?.insert).toBe("⏳ 2026-10-05");
    const g = "- [ ] Call Lena @calls ";
    expect(shorthandEdit(g, g.length, "2026-10-02")).toEqual({ from: 16, to: 22, insert: "#context/calls" });
    const e = "- [ ] Call Lena effort:15m ";
    expect(shorthandEdit(e, e.length, "2026-10-02")?.insert).toBe("[effort:: 15m]");
    const mail = "- [ ] Mail bob@example.com ";
    expect(shorthandEdit(mail, mail.length, "2026-10-02")).toBeNull();
    expect(shorthandEdit("Not a task @calls ", 18, "2026-10-02")).toBeNull();
  });
  it("leaves other lines and non-dates alone", () => {
    expect(shorthandEdit("due: tomorrow ", 14, "2026-10-02")).toBeNull();
    expect(shorthandEdit("- [ ] due: banana ", 18, "2026-10-02")).toBeNull();
  });
});

describe("text rules", () => {
  it("picks the line separator", () => {
    expect(lineSeparator("a\nb")).toBe("\n");
    expect(lineSeparator("a\r\nb\r\n")).toBe("\r\n");
    expect(lineSeparator("a\r\nb\n")).toBe("\n");
  });
  it("finds the properties", () => {
    const t = "---\ntitle: x\ntype: Meeting\n---\nbody";
    expect(t.slice(0, frontmatterEnd(t)!)).toBe("---\ntitle: x\ntype: Meeting\n---");
    expect(frontmatterKeys(t.slice(0, frontmatterEnd(t)!))).toEqual(["title", "type"]);
    expect(frontmatterEnd("---\n---\nx")).toBe(7);
    expect(frontmatterEnd("# no\n---\n")).toBeNull();
  });
  it("ticks and unticks as the task screens do", () => {
    expect(toggledLine("- [ ] a ^rank-5", true, "2026-10-02")).toBe("- [x] a ✅ 2026-10-02 ^rank-5");
    expect(toggledLine("- [x] a ✅ 2026-10-01 ^rank-5", false, "2026-10-02")).toBe("- [ ] a");
    expect(toggledLine("> > - [ ] a ^rank-5", true, "2026-10-02")).toBe("> > - [x] a ✅ 2026-10-02 ^rank-5");
    expect(toggledLine("  * [ ] nested", true, "2026-10-02")).toBe("  * [x] nested ✅ 2026-10-02");
  });
});

describe("three-way merge", () => {
  const base = "a\nb\nc\nd\n";
  it("takes both sides when they touch different lines", () => {
    expect(merge3(base, "a\nB\nc\nd\n", "a\nb\nc\nD\n")).toEqual({ ok: true, text: "a\nB\nc\nD\n" });
    expect(merge3(base, "a\nb\nc\nd\ne\n", "z\na\nb\nc\nd\n")).toEqual({ ok: true, text: "z\na\nb\nc\nd\ne\n" });
  });
  it("refuses when both change the same lines", () => {
    expect(merge3(base, "a\nX\nc\nd\n", "a\nY\nc\nd\n")).toEqual({ ok: false });
    expect(merge3(base, "a\nX\nc\nd\n", "a\nb\nY\nd\n")).toEqual({ ok: false });
  });
  it("is fine with the same change on both sides, or one side unchanged", () => {
    expect(merge3(base, "a\nX\nc\nd\n", "a\nX\nc\nd\n")).toEqual({ ok: true, text: "a\nX\nc\nd\n" });
    expect(merge3(base, base, "q\n")).toEqual({ ok: true, text: "q\n" });
  });
  it("keeps each line's own ending", () => {
    expect(merge3("a\r\nb\r\nc\r\n", "a\r\nB\r\nc\r\n", "a\r\nb\r\nc\r\nd\r\n")).toEqual({ ok: true, text: "a\r\nB\r\nc\r\nd\r\n" });
  });
  it("hints at the shorthand while an open task is edited, pinned to its text", () => {
    const at = (doc: string, caret = doc.length) => openTaskAt(EditorState.create({ doc, selection: { anchor: caret } }));
    expect(at("- [ ] ")).toBe(6);
    expect(at("  - [ ]")).toBe(7);
    expect(at("- [ ] buy milk")).toBe(6);
    expect(at("- [ ] buy milk", 9)).toBe(6);
    expect(at("- [x] done")).toBeNull();
    expect(at("- [ ] \nnext", 6)).toBe(6);
    expect(at("- [ ] \nnext")).toBeNull();
    expect(at("- [ ] ", 2)).toBeNull();
    expect(at("plain line")).toBeNull();
    expect(at("> - [ ] in a callout")).toBe(8);
  });
  it("turns start: and created: words into dates as you type", () => {
    const l = "- [ ] plan start: fri ";
    expect(shorthandEdit(l, l.length, "2026-10-02")?.insert).toBe("🛫 2026-10-09");
    const c = "- [ ] plan created: today ";
    expect(shorthandEdit(c, c.length, "2026-10-02")?.insert).toBe("➕ 2026-10-02");
  });
});

describe("images in Edit", () => {
  const make = (md: string) => {
    const parent = document.createElement("div");
    parent.className = "editor";
    document.body.appendChild(parent);
    ed = new CMEditor(parent, { imageUrl: async () => "blob:x" });
    ed.load(md);
    // The first parse gets 20 ms and the rest is left to the background; on a busy machine that can
    // stop short of the last lines, whose images then aren't drawn yet. Finish it now.
    forceParsing(ed.view, ed.view.state.doc.length, 5000);
    return parent;
  };

  it("keeps an image drawn on the cursor's line, at the width its markdown gives", () => {
    const parent = make("Before\n![[pic.png|300]]\nAfter");
    ed!.view.dispatch({ selection: { anchor: ed!.view.state.doc.line(2).to } });
    const img = parent.querySelector<HTMLImageElement>(".cm-img img");
    expect(img?.style.width).toBe("300px");
    expect(parent.querySelectorAll(".cm-img .cm-img-h")).toHaveLength(4);
  });

  it("draws a raw <img> tag, on a line of its own or in a sentence", () => {
    const parent = make('Before\n<img height="287" width="717" src="/api/vault-assets/images/p.png" />\n\nSee <img src="c.png"> here');
    const imgs = parent.querySelectorAll<HTMLImageElement>(".cm-img img");
    expect(imgs).toHaveLength(2);
    expect(imgs[0].style.width).toBe("717px");
    parent.querySelector<HTMLButtonElement>(".cm-img-del")!.click();
    expect(ed!.getMarkdown()).toBe('Before\n\nSee <img src="c.png"> here');
  });

  it("removes an image with its line", () => {
    const parent = make("Before\n![[pic.png]]\nAfter");
    parent.querySelector<HTMLButtonElement>(".cm-img-del")!.click();
    expect(ed!.getMarkdown()).toBe("Before\nAfter");
  });

  it("removes only the image when it shares its line", () => {
    const parent = make("See ![chart|200](images/c.png) here");
    parent.querySelector<HTMLButtonElement>(".cm-img-del")!.click();
    expect(ed!.getMarkdown()).toBe("See  here");
  });

  it("writes the new width when a handle is dragged", () => {
    const parent = make("![[pic.png]]\n");
    const img = parent.querySelector<HTMLImageElement>(".cm-img img")!;
    img.getBoundingClientRect = () => ({ width: 200 }) as DOMRect;
    const h = parent.querySelector<HTMLElement>(".cm-img-h.se")!;
    h.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 100 }));
    window.dispatchEvent(new MouseEvent("pointermove", { clientX: 150 }));
    window.dispatchEvent(new MouseEvent("pointerup", {}));
    expect(ed!.getMarkdown()).toBe("![[pic.png|250]]\n");
  });
});

describe("pasting images", () => {
  const file = (type: string) => new File(["x"], "image.png", { type });
  it("takes images from the clipboard's files, else from its items", () => {
    const png = file("image/png");
    expect(clipboardImages({ files: [png, file("text/plain")], items: [] } as unknown as DataTransfer)).toEqual([png]);
    const item = { kind: "file", type: "image/png", getAsFile: () => png };
    const text = { kind: "string", type: "text/html", getAsFile: () => null };
    expect(clipboardImages({ files: [], items: [text, item] } as unknown as DataTransfer)).toEqual([png]);
    expect(clipboardImages(null)).toEqual([]);
  });
});
