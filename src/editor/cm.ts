// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The editor on CodeMirror 6 (§5.1). The markdown is the document, so text the user didn't touch
// comes back byte for byte: the file's line break is kept as CodeMirror's line separator, and
// anything typed or pasted is given the same line break.

import { systemLock } from "./systemLock";
import { spelling } from "./spelling";
import {
  acceptCompletion,
  autocompletion,
  closeCompletion,
  CompletionContext,
  CompletionResult,
  completionKeymap,
  completionStatus,
  selectedCompletion,
} from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorSelection, EditorState, Extension, Prec, StateEffect, StateField, Text, Transaction } from "@codemirror/state";
import { Decoration, DecorationSet, drawSelection, dropCursor, EditorView, keymap } from "@codemirror/view";
import { effortMinutes } from "../gtd";
import { pendingShorthand, resolveDateWord, shorthandEmoji } from "../md/dates";
import { headingSlug } from "../md/plugins";
import type { Editor, EditorHooks, EditorMode } from "./Editor";
import { Highlight } from "./highlightSyntax";
import { livePreview } from "./live";
import { fenceAt, queryAssist, queryCompletions } from "./queryAssist";
import { templaterCompletions } from "./templaterAssist";
import { taskHint } from "./taskHint";
import { frontmatterEnd, lineSeparator, TASK_LINE } from "./text";

/** Lines marked from outside (Knowledge health's uncited claims), as their text (trimmed, its
 *  first 160 characters). Marked again when the text is replaced from outside (the page reloaded);
 *  cleared by the user's own edit. */
const setLineHits = StateEffect.define<string[]>();
const lineHit = Decoration.line({ class: "cm-line-hit" });
const hitsIn = (doc: Text, want: Set<string>) => {
  const at = [];
  for (let n = 1; n <= doc.lines && want.size; n++) {
    const l = doc.line(n);
    if (want.has([...l.text.trim()].slice(0, 160).join(""))) at.push(lineHit.range(l.from));
  }
  return Decoration.set(at);
};
const lineHits = StateField.define<{ want: Set<string>; deco: DecorationSet }>({
  create: () => ({ want: new Set(), deco: Decoration.none }),
  update(v, tr) {
    for (const e of tr.effects) {
      if (e.is(setLineHits)) {
        const want = new Set(e.value.map((t) => t.trim()).filter(Boolean));
        return { want, deco: hitsIn(tr.state.doc, want) };
      }
    }
    if (!tr.docChanged || !v.want.size) return v;
    if (["input", "delete", "move", "undo", "redo"].some((u) => tr.isUserEvent(u))) return { want: new Set(), deco: Decoration.none };
    return { want: v.want, deco: hitsIn(tr.state.doc, v.want) };
  },
  provide: (f) => EditorView.decorations.from(f, (v) => v.deco),
});

/** `[[partial` or `#partial` right before the cursor. */
/** The images on the clipboard: its files, and the items that are images but aren't listed as
 *  files (WebKit gives a browser's Copy Image or a screenshot one way or the other). One each. */
export function clipboardImages(data: DataTransfer | null): File[] {
  if (!data) return [];
  const out = [...(data.files ?? [])].filter((f) => f.type.startsWith("image/"));
  if (out.length) return out;
  for (const it of [...(data.items ?? [])]) {
    if (it.kind !== "file" || !it.type.startsWith("image/")) continue;
    const f = it.getAsFile();
    if (f) out.push(f);
  }
  return out;
}

export function completionTrigger(before: string): { kind: "link" | "tag"; q: string; start: number } | null {
  const l = /\[\[([^\]\n]*)$/.exec(before);
  if (l) return { kind: "link", q: l[1], start: before.length - l[0].length };
  const t = /(^|[\s(])#([\p{L}\p{N}_/-]*)$/u.exec(before);
  // At the start of a line a lone `#` is a heading being typed.
  if (t && !(t[1] === "" && t[2] === "")) return { kind: "tag", q: t[2], start: before.length - t[2].length - 1 };
  return null;
}

function completions(hooks: EditorHooks) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!hooks.suggest) return null;
    // Inside a query block the query help answers instead (src/editor/queryAssist.ts).
    if (fenceAt(ctx.state, ctx.pos)) return null;
    const l = ctx.state.doc.lineAt(ctx.pos);
    const tr = completionTrigger(l.text.slice(0, ctx.pos - l.from));
    if (!tr) return null;
    const list = await hooks.suggest(tr.kind, tr.q);
    if (ctx.aborted || !list.length) return null;
    return {
      from: l.from + tr.start,
      options: list.map((s) => ({ label: s.label, detail: s.detail, apply: s.insert })),
      filter: false,
    };
  };
}

/** `due: tomorrow ` on a task line becomes `📅 2026-10-03 ` as the word ends (and defer:, start:,
 *  created: their own emoji). */
const SHORT_END = /(^|[\s(])(due|defer|start|created):\s*(next week|\+\d{1,3}[dw]|\d{4}-\d{2}-\d{2}|[A-Za-z]+)([\s),.;])$/i;

/** `@calls ` becomes `#context/calls `, and `effort:15m ` becomes `[effort:: 15m] ` (stage 6, src/gtd.ts). */
const GTD_END = /(^|[\s(])(?:@([\p{L}\p{N}_/-]*[\p{L}_-][\p{L}\p{N}_/-]*)|effort:(\S+?))([\s),.;])$/iu;

export function shorthandEdit(lineText: string, caret: number, today: string): { from: number; to: number; insert: string } | null {
  if (!TASK_LINE.test(lineText)) return null;
  const before = lineText.slice(0, caret);
  const g = GTD_END.exec(before);
  if (g && (g[2] || effortMinutes(g[3]) !== null)) {
    const from = g.index + g[1].length;
    return { from, to: caret - g[4].length, insert: g[2] ? `#context/${g[2].toLowerCase()}` : `[effort:: ${g[3].toLowerCase()}]` };
  }
  const m = SHORT_END.exec(before);
  if (!m) return null;
  const d = resolveDateWord(m[3], today);
  if (!d) return null;
  const from = m.index + m[1].length;
  return { from, to: caret - m[4].length, insert: `${shorthandEmoji(m[2])} ${d}` };
}

/** Typed and pasted text gets the document's own line break. */
const sameLineBreaks = EditorState.transactionFilter.of((tr) => {
  if (!tr.docChanged || !(tr.isUserEvent("input") || tr.isUserEvent("paste") || tr.isUserEvent("drop"))) return tr;
  let odd = false;
  tr.changes.iterChanges((_a, _b, _c, _d, ins) => {
    for (let i = 1; i <= ins.lines; i++) if (/[\r\n]/.test(ins.line(i).text)) odd = true;
  });
  if (!odd) return tr;
  const changes: { from: number; to: number; insert: Text }[] = [];
  tr.changes.iterChanges((from, to, _c, _d, ins) => {
    const flat = ins.sliceString(0, ins.length, "\n").replace(/\r\n?/g, "\n");
    changes.push({ from, to, insert: Text.of(flat.split("\n")) });
  });
  return { changes, selection: tr.selection, userEvent: tr.annotation(Transaction.userEvent), scrollIntoView: tr.scrollIntoView };
});

/** ↩ with the list open on a word that's already the chosen suggestion (the caret put on it):
 *  the list closes and the line breaks, rather than taking the word again. */
function enterPastWholeWord(view: EditorView): boolean {
  if (completionStatus(view.state) !== "active") return false;
  const chosen = selectedCompletion(view.state);
  const head = view.state.selection.main.head;
  const w = view.state.wordAt(head);
  if (!chosen || !w || w.to !== head || view.state.sliceDoc(w.from, w.to) !== chosen.label) return false;
  closeCompletion(view);
  return false;
}

export class CMEditor implements Editor {
  readonly view: EditorView;
  private mode = new Compartment();
  private current: EditorMode;
  private listeners = new Set<(md: string) => void>();
  /** Where pasted images go once saved, kept in step with edits made meanwhile (null: the text
   *  was replaced, so at the caret). */
  private pastes = new Set<{ from: number; to: number } | { from: null; to: null }>();

  constructor(
    parent: HTMLElement,
    private hooks: EditorHooks = {},
    mode: EditorMode = "live",
  ) {
    this.current = mode;
    this.view = new EditorView({ parent, state: this.makeState("") });
  }

  get dom() {
    return this.view.dom;
  }

  private modeExt(m: EditorMode): Extension {
    // Source is the plain text in a monospaced font: nothing styled, so it reads as unlike Edit.
    return m === "live" ? livePreview(this.hooks) : EditorView.editorAttributes.of({ class: "cm-source" });
  }

  private makeState(doc: string): EditorState {
    const sep = lineSeparator(doc);
    const state = EditorState.create({
      doc,
      extensions: [
        EditorState.lineSeparator.of(sep),
        history(),
        drawSelection(),
        lineHits,
        dropCursor(),
        markdown({ base: markdownLanguage, addKeymap: true, extensions: [Highlight] }),
        highlightSelectionMatches(),
        // The find bar drives this search with CodeMirror's panel hidden (src/editor/find.ts). At the
        // bottom, a hidden panel counts as covering the window, so every keystroke scrolled the line
        // being typed up out of sight; at the top it counts as nothing.
        search({ top: true }),
        autocompletion({
          override: [completions(this.hooks), queryCompletions, templaterCompletions],
          activateOnTyping: true,
          icons: false,
        }),
        queryAssist(),
        // Tab takes a suggestion as ↩ does; ↩ on a word already written in full makes a new line.
        Prec.highest(
          keymap.of([
            { key: "Tab", run: acceptCompletion },
            { key: "Enter", run: enterPastWholeWord },
          ]),
        ),
        keymap.of([
          ...completionKeymap,
          // ⌘[ and ⌘] are back and forward in the app.
          ...defaultKeymap.filter((k) => k.key !== "Mod-[" && k.key !== "Mod-]"),
          ...historyKeymap,
          // ⌘F is the note's find bar (src/notes/FindBar.tsx), which drives this search.
          ...searchKeymap.filter((k) => k.key !== "Mod-f"),
        ]),
        EditorView.lineWrapping,
        EditorView.contentAttributes.of({ spellcheck: "false", autocorrect: "off", autocapitalize: "off", "aria-label": "Note text" }),
        sameLineBreaks,
        taskHint,
        spelling,
        systemLock,
        EditorView.domEventHandlers({ paste: (e, v) => this.paste(e, v) }),
        EditorView.updateListener.of((u) => {
          if (!u.docChanged) return;
          for (const p of this.pastes)
            if (p.from !== null) {
              p.from = u.changes.mapPos(p.from, -1);
              p.to = Math.max(p.from, u.changes.mapPos(p.to, 1));
            }
          const md = u.state.sliceDoc();
          this.listeners.forEach((f) => f(md));
          if (u.transactions.some((t) => t.isUserEvent("input.type"))) this.afterTyping(u.view);
        }),
        this.mode.of(this.modeExt(this.current)),
        ...(this.hooks.extensions ?? []),
      ],
    });
    // Start below the properties, so they open folded.
    const fm = frontmatterEnd(doc);
    if (fm === null) return state;
    const n = doc.slice(0, fm).split(sep).length;
    return n < state.doc.lines ? state.update({ selection: { anchor: state.doc.line(n + 1).from } }).state : state;
  }

  private afterTyping(view: EditorView) {
    const head = view.state.selection.main.head;
    const l = view.state.doc.lineAt(head);
    const caret = head - l.from;
    const today = this.hooks.today?.() ?? new Date().toISOString().slice(0, 10);
    const sh = shorthandEdit(l.text, caret, today);
    // Not inside the update: dispatching there isn't allowed.
    if (sh) {
      queueMicrotask(() =>
        view.dispatch({ changes: { from: l.from + sh.from, to: l.from + sh.to, insert: sh.insert }, userEvent: "input.date" }),
      );
      return;
    }
    const pend = TASK_LINE.test(l.text) ? pendingShorthand(l.text.slice(0, caret)) : null;
    if (pend && this.hooks.pickDate) {
      const c = view.coordsAtPos(head);
      const start = head - (pend.length + 1);
      this.hooks.pickDate(pend, c ? { left: c.left, top: c.top, bottom: c.bottom } : { left: 0, top: 0, bottom: 0 }, (date) => {
        // Only if the `due:` is still there.
        if (view.state.sliceDoc(start, head).toLowerCase() !== `${pend}:`) return;
        view.dispatch({ changes: { from: start, to: head, insert: `${shorthandEmoji(pend)} ${date}` }, userEvent: "input.date" });
        view.focus();
      });
    }
  }

  private paste(e: ClipboardEvent, view: EditorView): boolean {
    const files = clipboardImages(e.clipboardData);
    if (!files.length || !this.hooks.pasteImages) return false;
    e.preventDefault();
    const sel = view.state.selection.main;
    const at: { from: number; to: number } | { from: null; to: null } = { from: sel.from, to: sel.to };
    this.pastes.add(at);
    void this.hooks
      .pasteImages(files)
      .then((md) => {
        if (!md) return;
        // Where the selection was, moved along by any typing while the images were saved.
        const len = view.state.doc.length;
        const cur = view.state.selection.main;
        const from = Math.min(at.from ?? cur.from, len);
        const to = Math.min(Math.max(from, at.to ?? cur.to), len);
        view.dispatch({ changes: { from, to, insert: md }, selection: { anchor: from + md.length }, userEvent: "input.paste" });
      })
      .catch((err) => console.error("Brainstead: pasting the image failed", err))
      .finally(() => this.pastes.delete(at));
    return true;
  }

  load(md: string) {
    for (const p of this.pastes) Object.assign(p, { from: null, to: null });
    // Lines marked from outside stay marked when the page is loaded again.
    const keep = [...this.view.state.field(lineHits).want];
    this.view.setState(this.makeState(md));
    if (keep.length) this.view.dispatch({ effects: setLineHits.of(keep) });
    const caret = document.documentElement.dataset.caret;
    if (caret) window.setTimeout(() => this.sceneCaret(caret, !!document.documentElement.dataset.hover), 800);
  }

  /** A screenshot scene's caret (src/scene.ts): put where `|` is in `text`, as a click puts it, or
   *  the mouse resting there. */
  private sceneCaret(text: string, hover: boolean) {
    const at = this.view.state.sliceDoc().indexOf(text.replace("|", ""));
    if (at < 0) return;
    const pos = at + text.indexOf("|");
    if (!hover) {
      this.view.focus();
      this.view.dispatch({
        selection: EditorSelection.cursor(pos),
        userEvent: "select.pointer",
        effects: EditorView.scrollIntoView(pos, { y: "center" }),
      });
      return;
    }
    this.view.dispatch({ effects: EditorView.scrollIntoView(pos, { y: "center" }) });
    window.setTimeout(() => {
      const c = this.view.coordsAtPos(pos, 1);
      const el = c && document.elementFromPoint(c.left + 2, (c.top + c.bottom) / 2);
      if (c && el) el.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: c.left + 2, clientY: (c.top + c.bottom) / 2 }));
    }, 300);
  }

  getMarkdown() {
    return this.view.state.sliceDoc();
  }

  onChange(f: (md: string) => void) {
    this.listeners.add(f);
    return () => {
      this.listeners.delete(f);
    };
  }

  insertAtCursor(text: string) {
    const r = this.view.state.selection.main;
    this.view.dispatch({
      changes: { from: r.from, to: r.to, insert: text },
      selection: EditorSelection.cursor(r.from + text.length),
      userEvent: "input",
    });
  }

  insertAt(text: string, coords: { x: number; y: number } | null) {
    const pos = coords ? this.view.posAtCoords(coords) : null;
    if (pos === null) return this.insertAtCursor(text);
    this.view.dispatch({
      changes: { from: pos, insert: text },
      selection: EditorSelection.cursor(pos + text.length),
      userEvent: "input.drop",
    });
  }

  focus() {
    this.view.focus();
  }

  /** Scrolls to the heading a link named (`[[Note#Heading]]`) and puts the cursor there. */
  revealHeading(heading: string): boolean {
    const want = headingSlug(heading);
    const doc = this.view.state.doc;
    for (let n = 1; n <= doc.lines; n++) {
      const m = /^#{1,6}\s+(.*?)\s*#*\s*$/.exec(doc.line(n).text);
      if (m && headingSlug(m[1]) === want) {
        const at = doc.line(n).from;
        this.view.dispatch({ selection: { anchor: at }, effects: EditorView.scrollIntoView(at, { y: "start", yMargin: 24 }) });
        return true;
      }
    }
    return false;
  }

  /** Highlights each line that reads one of `texts` (trimmed, its first 160 characters, as
   *  Knowledge health gives a line) until the next edit, and scrolls to the first. */
  highlightLines(texts: string[]): boolean {
    this.view.dispatch({ effects: setLineHits.of(texts) });
    const first = this.view.state.field(lineHits).deco.iter();
    if (first.value) this.view.dispatch({ effects: EditorView.scrollIntoView(first.from, { y: "center" }) });
    return !!first.value;
  }

  setMode(m: EditorMode) {
    if (m === this.current) return;
    this.current = m;
    this.view.dispatch({ effects: this.mode.reconfigure(this.modeExt(m)) });
  }

  destroy() {
    this.listeners.clear();
    this.view.destroy();
  }
}
