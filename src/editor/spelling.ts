// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Spelling and grammar in the editor, by macOS's own checker (platform::spell), as the sibling
// journal app does it, kept fast the same ways: only lines not checked before go to macOS, cached
// by their text (3000 at most, least recently used out); the new ones go in one spelling call and
// one grammar call, a line apiece, and the results are split back by where each line starts;
// 600ms after typing, sooner on opening; the word at the caret isn't marked; a result whose text
// changed meanwhile waits for the next check. Brainstead also checks only the lines near what's
// on screen. Misspellings get a red wavy underline, grammar a blue one; a click shows guesses or
// macOS's explanation, with Learn and Ignore, and a fix is an editor change, so ⌘Z undoes it.

import { RangeSetBuilder } from "@codemirror/state";
import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate } from "@codemirror/view";
import { api } from "../api";
import { settings, Store } from "../store";
import { maskLines } from "./spellMask";
import { isMod } from "../platform";
import { features } from "../features";

interface Issue {
  start: number;
  len: number;
  description: string;
  corrections: string[];
}
type Found = { spelling: [number, number][]; grammar: Issue[] | null };

const cache = new Map<string, Found>();
const remember = (text: string, f: Found) => {
  cache.delete(text);
  cache.set(text, f);
  if (cache.size > 3000) cache.delete(cache.keys().next().value!);
};

/** Words learned or ignored this session (macOS can be slow to take them in), and grammar
 *  problems ignored, by their text and explanation. */
const accepted = new Set<string>();
const acceptedGrammar = new Set<string>();
const grammarKey = (text: string, description: string) => `${text.toLowerCase()}|${description}`;

/** A marked word or phrase, for the click menu. */
export interface SpellHit {
  from: number;
  to: number;
  text: string;
  kind: "spelling" | "grammar";
  description?: string;
  corrections?: string[];
}

/** The open menu: what was clicked, in which editor, and where. */
export const spellMenu = new Store<(SpellHit & { view: EditorView; rect: DOMRect }) | null>(null);

const on = () => {
  const s = settings.get();
  const f = features.get();
  return { spelling: f.spelling && s.spellCheck !== false, grammar: f.grammar && s.grammarCheck !== false, lang: s.spellLanguage || null };
};

/** The language the cache was filled in: a new one starts it afresh. */
let cachedLang: string | null | undefined;

/** The language a check runs in now (Settings › Notes), for the menu's guesses too. */
export const spellLang = () => on().lang;

/** Lines either side of the screen that are checked too, so scrolling finds them done. */
const MARGIN = 60;

const spellMark = Decoration.mark({ class: "cm-spell" });
const grammarMark = Decoration.mark({ class: "cm-grammar" });

class Spelling {
  decorations: DecorationSet = Decoration.none;
  hits: SpellHit[] = [];
  private timer: number | undefined;
  private off: () => void;

  constructor(private view: EditorView) {
    this.schedule(50);
    this.off = settings.subscribe(() => this.schedule(0));
  }

  update(u: ViewUpdate) {
    if (u.docChanged) {
      this.decorations = this.decorations.map(u.changes);
      this.hits = this.hits.map((h) => ({ ...h, from: u.changes.mapPos(h.from), to: u.changes.mapPos(h.to, -1) }));
      this.schedule(600);
    } else if (u.viewportChanged) this.schedule(150);
    else if (u.selectionSet) this.build(false);
  }

  destroy() {
    window.clearTimeout(this.timer);
    this.off();
  }

  schedule(ms: number) {
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.check(), ms);
  }

  /** The lines to check: what's on screen and a margin either side. */
  private span(): [number, number] {
    const d = this.view.state.doc;
    const { from, to } = this.view.viewport;
    return [Math.max(1, d.lineAt(from).number - MARGIN), Math.min(d.lines, d.lineAt(to).number + MARGIN)];
  }

  private maskedFor: { doc: unknown; lines: string[] } | null = null;

  /** Every line to the end of the span, masked; from the top, so fenced code and frontmatter are
   *  known wherever the span starts. Kept while the text is the same. */
  private masked(): string[] {
    const d = this.view.state.doc;
    const [, last] = this.span();
    if (this.maskedFor?.doc === d && this.maskedFor.lines.length >= last) return this.maskedFor.lines;
    const lines: string[] = [];
    for (let n = 1; n <= last; n++) lines.push(d.line(n).text);
    this.maskedFor = { doc: d, lines: maskLines(lines) };
    return this.maskedFor.lines;
  }

  private async check() {
    const want = on();
    if (!want.spelling && !want.grammar) {
      this.hits = [];
      this.decorations = Decoration.none;
      this.view.dispatch({});
      return;
    }
    if (want.lang !== cachedLang) {
      cache.clear();
      cachedLang = want.lang;
    }
    const [first] = this.span();
    const masked = this.masked().slice(first - 1);
    const need = [...new Set(masked.filter((t) => t.trim() && (!cache.has(t) || (want.grammar && !cache.get(t)!.grammar))))];
    if (need.length) {
      const joined = need.join("\n");
      const starts: number[] = [];
      need.reduce((n, t) => (starts.push(n), n + t.length + 1), 0);
      const line = (a: number) => {
        let k = starts.length - 1;
        while (k > 0 && starts[k] > a) k--;
        return k;
      };
      let sp: [number, number][], gr: Issue[] | null;
      try {
        [sp, gr] = await Promise.all([
          want.spelling ? api.spellCheck(joined, want.lang) : Promise.resolve([] as [number, number][]),
          want.grammar ? api.spellGrammar(joined, want.lang) : Promise.resolve(null),
        ]);
      } catch {
        return;
      }
      const got = need.map((t): Found => ({ spelling: [], grammar: gr ? [] : (cache.get(t)?.grammar ?? null) }));
      for (const [a, len] of sp) {
        const k = line(a);
        got[k].spelling.push([a - starts[k], len]);
      }
      for (const g of gr ?? []) {
        const k = line(g.start);
        got[k].grammar!.push({ ...g, start: g.start - starts[k] });
      }
      need.forEach((t, k) => remember(t, got[k]));
    }
    if (this.view.dom.isConnected) this.build();
  }

  /** Marks from the cache for the lines as they are now; `redraw` when not inside an update. */
  private build(redraw = true) {
    const want = on();
    const st = this.view.state;
    const d = st.doc;
    const [first, last] = this.span();
    const masked = this.masked();
    const caret = st.selection.main.empty ? st.selection.main.head : -1;
    const hits: SpellHit[] = [];
    for (let n = first; n <= last; n++) {
      const l = d.line(n);
      const f = cache.get(masked[n - 1]);
      if (!f) continue;
      if (want.spelling)
        for (const [a, len] of f.spelling) {
          const text = l.text.slice(a, a + len);
          const from = l.from + a;
          // The word being typed isn't marked yet.
          if (!text || accepted.has(text.toLowerCase()) || caret === from + len) continue;
          hits.push({ from, to: from + len, text, kind: "spelling" });
        }
      if (want.grammar)
        for (const g of f.grammar ?? []) {
          const text = l.text.slice(g.start, g.start + g.len);
          const from = l.from + g.start;
          if (!text.trim() || acceptedGrammar.has(grammarKey(text, g.description)) || (caret > from && caret <= from + g.len)) continue;
          hits.push({ from, to: from + g.len, text, kind: "grammar", description: g.description, corrections: g.corrections });
        }
    }
    hits.sort((a, b) => a.from - b.from || a.to - b.to);
    const b = new RangeSetBuilder<Decoration>();
    let end = -1;
    for (const h of hits) {
      if (h.from < end) continue; // no overlaps: the first one wins
      b.add(h.from, h.to, h.kind === "spelling" ? spellMark : grammarMark);
      end = h.to;
    }
    this.hits = hits;
    this.decorations = b.finish();
    if (redraw) this.view.dispatch({});
  }
}

const plugin = ViewPlugin.fromClass(Spelling, {
  decorations: (p) => p.decorations,
  eventHandlers: {
    click(e, view) {
      if (isMod(e) || e.altKey || e.shiftKey || e.button !== 0) return false;
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos == null) return false;
      const hit = this.hits.find((h) => pos >= h.from && pos <= h.to);
      if (!hit) return false;
      const c = view.coordsAtPos(hit.from);
      const rect = c ? new DOMRect(c.left, c.top, 0, c.bottom - c.top) : new DOMRect(e.clientX, e.clientY, 0, 0);
      spellMenu.set({ ...hit, view, rect });
      return false;
    },
  },
});

/** Checks again now (after a word is learned or ignored, say). */
export function respell(view: EditorView) {
  view.plugin(plugin)?.schedule(0);
}

/** Replaces the marked text, as an editor change (⌘Z undoes it). */
export function applyFix(view: EditorView, h: SpellHit, to: string) {
  if (view.state.sliceDoc(h.from, h.to) !== h.text) return;
  view.dispatch({ changes: { from: h.from, to: h.to, insert: to }, userEvent: "input.spell" });
  view.focus();
}

export function acceptWord(view: EditorView, h: SpellHit, learn: boolean) {
  if (h.kind === "spelling") {
    accepted.add(h.text.toLowerCase());
    void api.spellAccept(h.text, learn).catch(() => {});
  } else acceptedGrammar.add(grammarKey(h.text, h.description ?? ""));
  respell(view);
}

export const spelling = [
  plugin,
  EditorView.baseTheme({
    ".cm-spell": { textDecoration: "underline wavy var(--red)", textDecorationSkipInk: "none", textUnderlineOffset: "3px" },
    ".cm-grammar": { textDecoration: "underline wavy var(--accent)", textDecorationSkipInk: "none", textUnderlineOffset: "3px" },
  }),
];
