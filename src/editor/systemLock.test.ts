// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { systemCallout, systemLock } from "./systemLock";

const NOTE = "# To Do\n\n> **This is a system note** - Tasks.\n> More.\n\n- [ ] a\n";

describe("a system note's header", () => {
  it("is found where Rust finds it", () => {
    const [a, b] = systemCallout(NOTE)!;
    expect(NOTE.slice(a, b)).toBe("> **This is a system note** - Tasks.\n> More.");
    expect(systemCallout("---\ntags: [x]\n---\n> **This is a system note** - Log.\n")).toEqual([18, 52]);
    expect(systemCallout("> Just a quote.\n")).toBeNull();
  });

  it("can't be changed in the editor; the rest can", () => {
    const st = EditorState.create({ doc: NOTE, extensions: [systemLock] });
    const [a, b] = systemCallout(NOTE)!;
    const edit = (from: number, to: number, insert: string) => st.update({ changes: { from, to, insert } }).state.doc.toString();
    expect(edit(a + 5, a + 10, "x")).toBe(NOTE); // typing in it
    expect(edit(b, b, " extra")).toBe(NOTE); // at its end
    expect(edit(a, a, "x")).toBe(NOTE); // at its start
    expect(edit(a, a, "New line\n")).toBe(NOTE.slice(0, a) + "New line\n" + NOTE.slice(a)); // a line above it is fine
    expect(edit(b, b, "\n> more")).toBe(NOTE.slice(0, b) + "\n> more" + NOTE.slice(b)); // so is a line below
    expect(edit(0, NOTE.length, "")).toContain("> **This is a system note** - Tasks.\n> More."); // select all, delete
    expect(edit(NOTE.length, NOTE.length, "- [ ] b\n")).toBe(`${NOTE}- [ ] b\n`); // below it
    // Not a system note: nothing held.
    const plain = EditorState.create({ doc: "> quote\n", extensions: [systemLock] });
    expect(plain.update({ changes: { from: 2, to: 7, insert: "x" } }).state.doc.toString()).toBe("> x\n");
  });

  it("keeps the line breaks either side of it", () => {
    const st = EditorState.create({ doc: NOTE, extensions: [systemLock] });
    const [a, b] = systemCallout(NOTE)!;
    const edit = (from: number, to: number, insert = "") => st.update({ changes: { from, to, insert } }).state.doc.toString();
    expect(edit(b, b + 1)).toBe(NOTE); // Backspace on the blank line below it
    expect(edit(a - 1, a)).toBe(NOTE); // Backspace at its start, with the heading right above
    // Delete at the end of its last line, with text right below.
    const tight = "# To Do\n> **This is a system note** - Tasks.\nNext line\n";
    const ts = EditorState.create({ doc: tight, extensions: [systemLock] });
    const [, tb] = systemCallout(tight)!;
    expect(ts.update({ changes: { from: tb, to: tb + 1 } }).state.doc.toString()).toBe(tight);
    // A deletion over a break leaves the break and deletes the rest.
    expect(edit(b, b + 2)).toBe(NOTE.slice(0, b + 1) + NOTE.slice(b + 2));
    expect(edit(0, a)).toBe("\n" + NOTE.slice(a));
    expect(edit(0, NOTE.length)).toBe("\n> **This is a system note** - Tasks.\n> More.\n");
  });

  it("is in the same place in a file with CRLF line breaks", () => {
    const st = EditorState.create({ doc: NOTE, extensions: [systemLock, EditorState.lineSeparator.of("\r\n")] });
    const [a, b] = systemCallout(NOTE)!;
    const edit = (from: number, to: number, insert: string) => st.update({ changes: { from, to, insert } }).state.doc.toString();
    expect(edit(a, a + 1, "x")).toBe(NOTE); // its first character is held
    expect(edit(b + 2, b + 2, "x")).toBe(NOTE.slice(0, b + 2) + "x" + NOTE.slice(b + 2)); // the line after it isn't
  });
});
