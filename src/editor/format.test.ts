// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import { Highlight } from "./highlightSyntax";
import { activeAt, codeBlock, insertBlock, link, setHeading, TABLE, toggleBlock, toggleMark, wikilink } from "./format";

/** A view over `doc`, `|` marking the caret, or `«`…`»` the selection. */
function view(doc: string) {
  const caret = doc.includes("|");
  const a = caret ? doc.indexOf("|") : doc.indexOf("«");
  const sel = caret ? EditorSelection.cursor(a) : EditorSelection.range(a, doc.indexOf("»") - 1);
  const text = caret ? doc.replace("|", "") : doc.replace("«", "").replace("»", "");
  return new EditorView({
    state: EditorState.create({ doc: text, selection: sel, extensions: [markdown({ base: markdownLanguage, extensions: [Highlight] })] }),
  });
}
const text = (v: EditorView) => v.state.doc.toString();

describe("formatting", () => {
  it("wraps and unwraps marks", () => {
    const v = view("say «hello» there");
    toggleMark(v, "bold");
    expect(text(v)).toBe("say **hello** there");
    toggleMark(v, "bold");
    expect(text(v)).toBe("say hello there");
  });
  it("wraps the word at the caret, and inserts a pair on empty space", () => {
    const v = view("one tw|o three");
    toggleMark(v, "italic");
    expect(text(v)).toBe("one *two* three");
    const e = view("end |");
    toggleMark(e, "code");
    expect(text(e)).toBe("end ``");
    expect(e.state.selection.main.head).toBe(5);
  });
  it("doesn't take bold for italic", () => {
    const v = view("**«x»**");
    toggleMark(v, "italic");
    expect(text(v)).toBe("***x***");
  });
  it("sets and clears headings, leaving other lines alone", () => {
    const v = view("## Old|\nnext");
    setHeading(v, 1);
    expect(text(v)).toBe("# Old\nnext");
    setHeading(v, 0);
    expect(text(v)).toBe("Old\nnext");
  });
  it("toggles lists over the selected lines", () => {
    const v = view("«a\nb»\nc");
    toggleBlock(v, "number");
    expect(text(v)).toBe("1. a\n2. b\nc");
    toggleBlock(v, "task");
    expect(text(v)).toBe("- [ ] a\n- [ ] b\nc");
    toggleBlock(v, "task");
    expect(text(v)).toBe("a\nb\nc");
    toggleBlock(v, "quote");
    expect(text(v)).toBe("> a\n> b\nc");
  });
  it("inserts blocks on their own lines", () => {
    const v = view("para|");
    insertBlock(v, TABLE);
    expect(text(v)).toBe(`para\n\n${TABLE}\n`);
    const c = view("|");
    codeBlock(c);
    expect(text(c)).toBe("```\n\n```\n");
    expect(c.state.selection.main.head).toBe(4);
  });
  it("makes links", () => {
    const v = view("see «docs»");
    link(v);
    expect(text(v)).toBe("see [docs](url)");
    expect(v.state.sliceDoc(v.state.selection.main.from, v.state.selection.main.to)).toBe("url");
    const w = view("go |");
    wikilink(w);
    expect(text(w)).toBe("go [[]]");
    expect(w.state.selection.main.head).toBe(5);
  });
  it("knows what the caret is in", () => {
    const a = activeAt(view("## Head **bo|ld**").state);
    expect(a.heading).toBe(2);
    expect([...a.marks]).toEqual(["bold"]);
    expect(activeAt(view("- [ ] ta|sk").state).block).toBe("task");
    expect(activeAt(view("an ==hi|gh== word").state).marks.has("highlight")).toBe(true);
    expect(activeAt(view("a == b| == c").state).marks.has("highlight")).toBe(false);
  });
});
