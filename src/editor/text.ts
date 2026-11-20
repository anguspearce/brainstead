// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Plain-text rules the editor works by: which line break to keep, where the properties end, and
// the edits a tick or a date word makes to one task line.

import moment from "moment";
import { completion, inQuote } from "../tasksq/edits";
import { splitQuote } from "../tasksq/fields";

/** The line separator to give CodeMirror so the text comes back byte for byte. A file of CRLF
 *  lines keeps CRLF (Enter writes CRLF too); anything else splits on LF only, so a stray CR stays
 *  in the text as a character rather than being turned into a line break. */
export function lineSeparator(text: string): "\n" | "\r\n" {
  const crlf = (text.match(/\r\n/g) ?? []).length;
  if (!crlf) return "\n";
  const lf = (text.match(/\n/g) ?? []).length;
  return lf === crlf ? "\r\n" : "\n";
}

/** The YAML properties block at the top: its end offset (after the closing `---` line), or null. */
export function frontmatterEnd(text: string): number | null {
  const m = /^(?:\uFEFF)?---[ \t]*\r?\n(?:[\s\S]*?\r?\n)??(?:---|\.\.\.)[ \t]*(?=\r?\n|$)/.exec(text);
  return m ? m[0].length : null;
}

/** The property names in a properties block, for the folded summary. */
export function frontmatterKeys(block: string): string[] {
  return block
    .split(/\r?\n/)
    .slice(1, -1)
    .map((l) => /^([^\s:#][^:]*):/.exec(l)?.[1].trim())
    .filter((k): k is string => !!k);
}

const RANK = /\s*\^rank-(\d+)\s*$/;
const DONE = /\s*✅\s*\d{4}-\d{2}-\d{2}/g;

/** A task line ticked (`[x]` and ` ✅ today`, rank kept) or unticked (`[ ]`, ✅ and rank dropped),
 *  as Rust's `write::toggled` does it for the task screens. */
export function toggledLine(line: string, done: boolean, today: string): string {
  const [pre, rest] = splitQuote(line);
  if (pre) return pre + toggledLine(rest, done, today);
  const r = RANK.exec(line);
  let body = r ? line.slice(0, r.index) : line;
  body = body.replace(/^(\s*(?:[-*+]|\d+[.)])\s+\[)[ xX](\])/, `$1${done ? "x" : " "}$2`);
  body = body.replace(DONE, "").trimEnd();
  if (done) {
    body += ` ✅ ${today}`;
    if (r) body += ` ^rank-${r[1]}`;
  }
  return body;
}

export const TASK_LINE = /^(?:[ \t]{0,3}>[ \t]?)*\s*(?:[-*+]|\d+[.)])\s+\[[ xX]\]\s/;

/** A task's line ticked or unticked in the editor's text, as the task screens tick it: a recurring
 *  task's next occurrence goes above the done one and 🏁 delete takes the done one away, so the
 *  result can be two lines, or none (""); any other task is `toggledLine`. */
export function tickedText(line: string, done: boolean, today: string): string {
  const c = done ? inQuote(line, (l) => completion(l, moment(today, "YYYY-MM-DD"))) : null;
  return c ? c.lines.join("\n") : toggledLine(line, done, today);
}

/** The change that puts `next` (from `tickedText`) in a line's place; a line that goes ("") takes
 *  its line break with it. */
export function tickChange(doc: { length: number }, l: { from: number; to: number }, next: string) {
  if (next !== "") return { from: l.from, to: l.to, insert: next };
  return l.to < doc.length ? { from: l.from, to: l.to + 1, insert: "" } : { from: Math.max(0, l.from - 1), to: l.to, insert: "" };
}
