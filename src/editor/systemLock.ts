// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A system note's callout ("This is a system note – …", at the top: the To Do list, the
// summaries' notes and the rest) can't be changed in the editor: other parts of Brainstead look
// for it. Typing, pasting and deleting across it leave it as it is, and a lock says why. The rule
// for where it is matches Rust's `rename::system_callout`.

import { EditorState, Extension, RangeSetBuilder, StateField, Transaction } from "@codemirror/state";
import { Decoration, DecorationSet, EditorView, WidgetType } from "@codemirror/view";
import { iconElement } from "../icons";

/** How much of the top of a note to look in: the callout is always near the start. */
const LOOK = 8000;

/** Where a note's system-note callout is, as [from, to] without its last line's ending; null when
 *  it has none. After a byte-order mark, properties and a `# Title` line. */
export function systemCallout(text: string): [number, number] | null {
  const lineEnd = (p: number) => {
    const i = text.indexOf("\n", p);
    return i < 0 ? text.length : i + 1;
  };
  const blank = (p: number) => text.slice(p, lineEnd(p)).trim() === "";
  let pos = text.startsWith("﻿") ? 1 : 0;
  if (/^---\r?\n/.test(text.slice(pos))) {
    let p = lineEnd(pos);
    while (p < text.length && !/^(---|\.\.\.)\s*$/.test(text.slice(p, lineEnd(p)))) p = lineEnd(p);
    if (p < text.length) pos = lineEnd(p);
  }
  while (pos < text.length && blank(pos)) pos = lineEnd(pos);
  if (/^#[ \t]/.test(text.slice(pos))) {
    pos = lineEnd(pos);
    while (pos < text.length && blank(pos)) pos = lineEnd(pos);
  }
  let end = pos;
  while (end < text.length && text[end] === ">") end = lineEnd(end);
  if (end === pos || !text.slice(pos, end).includes("This is a system note")) return null;
  return [pos, pos + text.slice(pos, end).replace(/[\r\n]+$/, "").length];
}

// The document's own text, a break one character as its positions count it (sliceDoc would give a
// CRLF file's breaks as two, and the range would drift a place right for each).
const rangeIn = (state: EditorState) => systemCallout(state.doc.sliceString(0, Math.min(state.doc.length, LOOK)));

class Lock extends WidgetType {
  toDOM() {
    const s = document.createElement("span");
    s.className = "cm-syslock";
    s.title = "This is a system note's header: other parts of Brainstead look for it, so it can't be changed here.";
    s.append(iconElement("lock", 12));
    return s;
  }
  ignoreEvent() {
    return true;
  }
}

function locks(state: EditorState): DecorationSet {
  const r = rangeIn(state);
  const b = new RangeSetBuilder<Decoration>();
  if (r) {
    // At its start, where it's always in view however long its first line is.
    b.add(r[0], r[0], Decoration.widget({ widget: new Lock(), side: -1 }));
  }
  return b.finish();
}

const lockMark = StateField.define<DecorationSet>({
  create: locks,
  update: (d, tr) => (tr.docChanged ? locks(tr.state) : d),
  provide: (f) => EditorView.decorations.from(f),
});

/** Changes that touch the callout are left out; the rest of the change goes ahead. */
const keep = EditorState.changeFilter.of((tr) => {
  if (!tr.docChanged) return true;
  const r = rangeIn(tr.startState);
  return r ? r : true;
});

/** Text put right against it, which the filter above lets through (it's at the range's edge),
 *  would join its first or last line: allowed only as a new line of its own. And the line breaks
 *  just before and after it are kept: deleting one (Backspace at its start under a heading, Delete
 *  at the end of its last line, Backspace on the blank line below) would join it to the next line.
 *  A deletion over one leaves that break and deletes the rest. */
const edges = EditorState.transactionFilter.of((tr) => {
  if (!tr.docChanged) return tr;
  const r = rangeIn(tr.startState);
  if (!r) return tr;
  const doc = tr.startState.doc;
  // The break before it, as [from, r[0]), and after it, as [r[1], to); none at the very start or end.
  const before =
    r[0] > 0 && doc.sliceString(r[0] - 1, r[0]) === "\n" ? (doc.sliceString(r[0] - 2, r[0] - 1) === "\r" ? r[0] - 2 : r[0] - 1) : r[0];
  const tail = doc.sliceString(r[1], r[1] + 2);
  const after = r[1] + (tail.startsWith("\r\n") ? 2 : tail.startsWith("\n") ? 1 : 0);
  let joins = false;
  let clipped = false;
  const specs: { from: number; to: number; insert: string }[] = [];
  tr.changes.iterChanges((fromA, toA, _fromB, _toB, text) => {
    const t = text.toString();
    if (fromA === toA && ((fromA === r[0] && t && !t.endsWith("\n")) || (fromA === r[1] && t && !t.startsWith("\n")))) joins = true;
    let [from, to] = [fromA, toA];
    if (from < r[0] && to > before) {
      to = before;
      clipped = true;
    }
    if (to > r[1] && from < after) {
      from = Math.max(from, after);
      clipped = true;
    }
    if (from < to || t) specs.push({ from, to: Math.max(from, to), insert: t });
  });
  if (joins) return [];
  if (!clipped) return tr;
  const userEvent = tr.annotation(Transaction.userEvent);
  return { changes: specs, ...(userEvent ? { annotations: Transaction.userEvent.of(userEvent) } : {}) };
});

export const systemLock: Extension = [keep, edges, lockMark];
