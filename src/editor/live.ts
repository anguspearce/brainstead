// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Live mode: the markdown stays the document and is only drawn differently. Away
// from the cursor's lines the syntax marks are hidden, headings are sized, links show their label,
// images and checkboxes are drawn, and the properties fold into one line. Nothing here changes
// the text except a tick, which is an ordinary edit.

import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import { EditorState, Extension, Range, StateField } from "@codemirror/state";
import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate, WidgetType } from "@codemirror/view";
import { openUrl } from "@tauri-apps/plugin-opener";
import { iconElement } from "../icons";
import { QUERY_DOCS } from "../md/queryDocs";
import { openBuilderAt } from "./queryAssist";
import { fmtShortDate } from "../md/dates";
import { daysUntil } from "../md/taskQuery";
import { iconSvg } from "../icons";
import { IMAGE_EXT, imageSize, imgTag, parseWikilink, setImageWidth, wikiLabel } from "../md/wikilinks";
import { chipOf, FIELD_RE, splitTaskFields } from "../taskFields";
import type { EditorHooks } from "./Editor";
import { frontmatterEnd, frontmatterKeys, toggledLine } from "./text";
import { isMod, keyLabel } from "../platform";

const hide = Decoration.replace({});
/** Inline HTML pairs Edit draws as their formatting, tags hidden. */
const TAG_CLASS: Record<string, string> = {
  b: "cm-strong",
  strong: "cm-strong",
  i: "cm-em",
  em: "cm-em",
  s: "cm-strike",
  del: "cm-strike",
  strike: "cm-strike",
  u: "cm-u",
  sup: "cm-sup",
  sub: "cm-sub",
  code: "cm-icode",
};
/** Tags that stand for content of their own, left as written. */
const KEEP_TAGS = new Set(["img", "script", "style", "iframe", "video", "audio"]);
const line = (cls: string) => Decoration.line({ class: cls });
const mark = (cls: string, attrs?: Record<string, string>) => Decoration.mark({ class: cls, attributes: attrs });

/** Line numbers any selection touches: their syntax is shown. */
function activeLines(state: EditorState): Set<number> {
  const s = new Set<number>();
  if (!state.facet(EditorView.editable)) return s;
  for (const r of state.selection.ranges) {
    const a = state.doc.lineAt(r.from).number;
    const b = state.doc.lineAt(r.to).number;
    for (let n = a; n <= b; n++) s.add(n);
  }
  return s;
}

/** A drawn image. While the note can be edited, hovering it shows a frame with a handle at each
 *  corner, which resizes it (its width written into its markdown, `![[pic.png|400]]`), and a
 *  button that removes it from the note. A button shows it full size, in Edit or not. `text` is
 *  its markdown, to find it again. */
class ImageWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly label: string,
    readonly text: string,
    readonly hooks: EditorHooks,
  ) {
    super();
  }
  eq(o: ImageWidget) {
    return o.src === this.src && o.label === this.label && o.text === this.text;
  }
  toDOM(view: EditorView) {
    const { alt, width, height } = imageSize(this.label);
    const span = document.createElement("span");
    span.className = "cm-img";
    const img = document.createElement("img");
    img.alt = alt;
    if (width) img.style.width = `${width}px`;
    if (height && !width) img.style.height = `${height}px`;
    span.appendChild(img);
    void this.hooks.imageUrl?.(this.src).then((u) => {
      if (u) img.src = u;
      else {
        span.classList.add("missing");
        span.textContent = alt || this.src;
      }
    });
    if (this.hooks.openImage) {
      const full = document.createElement("button");
      full.type = "button";
      full.className = "cm-img-full";
      full.title = "Show the image full size";
      full.setAttribute("aria-label", "Show the image full size");
      full.innerHTML = iconSvg("expand", 14);
      full.addEventListener("pointerdown", (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      full.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        if (img.src) this.hooks.openImage?.(img.currentSrc || img.src, alt);
      });
      span.appendChild(full);
    }
    if (!view.state.facet(EditorView.editable) || view.state.readOnly) return span;
    span.classList.add("edit");
    // Where its markdown is now, or null when it's gone or changed.
    const range = () => {
      const from = view.posAtDOM(span);
      const to = from + this.text.length;
      return view.state.sliceDoc(from, to) === this.text ? { from, to } : null;
    };
    for (const corner of ["nw", "ne", "sw", "se"]) {
      const h = document.createElement("span");
      h.className = `cm-img-h ${corner}`;
      h.title = "Drag to resize the image";
      h.addEventListener("pointerdown", (e) => {
        e.preventDefault();
        e.stopPropagation();
        const sx = e.clientX;
        const w0 = img.getBoundingClientRect().width;
        const max = span.parentElement?.getBoundingClientRect().width || Infinity;
        // A handle on the left grows the image as it's dragged left.
        const dir = corner.endsWith("e") ? 1 : -1;
        let w = w0;
        span.classList.add("sizing");
        const move = (m: PointerEvent) => {
          w = Math.max(24, Math.min(max, w0 + dir * (m.clientX - sx)));
          img.style.width = `${Math.round(w)}px`;
          img.style.height = "";
        };
        const up = () => {
          window.removeEventListener("pointermove", move);
          window.removeEventListener("pointerup", up);
          span.classList.remove("sizing");
          const r = range();
          const next = r && Math.round(w) !== Math.round(w0) ? setImageWidth(this.text, w) : null;
          if (r && next) view.dispatch({ changes: { ...r, insert: next }, userEvent: "input.resize" });
        };
        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", up);
      });
      span.appendChild(h);
    }
    const del = document.createElement("button");
    del.type = "button";
    del.className = "cm-img-del";
    del.title = "Remove the image from the note (the file stays in the vault)";
    del.setAttribute("aria-label", "Remove the image");
    del.innerHTML = iconSvg("trash", 14);
    del.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    del.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      const r = range();
      if (!r) return;
      // On a line of its own, the line goes too.
      const l = view.state.doc.lineAt(r.from);
      const whole = l.text.trim() === this.text && l.number < view.state.doc.lines;
      const at = whole ? { from: l.from, to: l.to + 1 } : r;
      view.dispatch({ changes: { ...at, insert: "" }, userEvent: "delete.image" });
      view.focus();
    });
    span.appendChild(del);
    return span;
  }
  ignoreEvent(e: Event) {
    // The handles and the button handle their own.
    return !!(e.target as HTMLElement).closest?.(".cm-img-h, .cm-img-del, .cm-img-full");
  }
}

class LinkWidget extends WidgetType {
  constructor(
    readonly label: string,
    readonly target: string,
    readonly embed: boolean,
  ) {
    super();
  }
  eq(o: LinkWidget) {
    return o.label === this.label && o.target === this.target && o.embed === this.embed;
  }
  toDOM() {
    const a = document.createElement("span");
    a.className = `cm-wikilink${this.embed ? " embed" : ""}`;
    a.textContent = this.label;
    a.dataset.wiki = this.target;
    a.title = `${this.target} — ${keyLabel("⌘-click")} to open`;
    return a;
  }
  ignoreEvent() {
    return false;
  }
}

/** The docs link at the end of a ```tasks / ```dataview fence line, as in View's block header. */
class DocsWidget extends WidgetType {
  constructor(readonly lang: string) {
    super();
  }
  eq(o: DocsWidget) {
    return o.lang === this.lang;
  }
  toDOM(view: EditorView) {
    const [label, url] = QUERY_DOCS[this.lang];
    const box = document.createElement("span");
    box.className = "cm-qlinks";
    if (this.lang !== "dataviewjs") {
      const b = document.createElement("a");
      b.className = "qdocs cm-qbuild";
      b.href = "#";
      b.textContent = "Build query…";
      b.addEventListener("mousedown", (e) => e.preventDefault());
      b.addEventListener("click", (e) => {
        e.preventDefault();
        openBuilderAt(view, view.posAtDOM(box));
      });
      box.append(b);
    }
    const a = document.createElement("a");
    a.className = "qdocs cm-qdocs";
    a.href = url;
    a.textContent = `${label} ↗`;
    a.addEventListener("mousedown", (e) => e.preventDefault());
    a.addEventListener("click", (e) => {
      e.preventDefault();
      void openUrl(url);
    });
    box.append(a);
    return box;
  }
  ignoreEvent() {
    return true;
  }
}

class CheckWidget extends WidgetType {
  constructor(readonly done: boolean) {
    super();
  }
  eq(o: CheckWidget) {
    return o.done === this.done;
  }
  toDOM() {
    const b = document.createElement("span");
    b.className = `cm-check${this.done ? " on" : ""}`;
    b.setAttribute("role", "checkbox");
    b.setAttribute("aria-checked", String(this.done));
    b.dataset.check = "1";
    return b;
  }
  ignoreEvent() {
    return false;
  }
}

/** A task's date drawn as its icon and the day (the text keeps the emoji). */
class DateWidget extends WidgetType {
  constructor(
    readonly id: string,
    readonly day: string,
    readonly tone: string,
  ) {
    super();
  }
  eq(o: DateWidget) {
    return o.id === this.id && o.day === this.day && o.tone === this.tone;
  }
  toDOM() {
    const c = chipOf(this.id, this.day, fmtShortDate)!;
    const s = document.createElement("span");
    s.className = `cm-tdate ${this.id} ${this.tone}`;
    s.title = c.tip;
    s.append(iconElement(c.icon, 12), c.text);
    return s;
  }
}

class BulletWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    const s = document.createElement("span");
    s.className = "cm-bullet";
    s.textContent = "•";
    return s;
  }
}

class PropsWidget extends WidgetType {
  constructor(readonly keys: string[]) {
    super();
  }
  eq(o: PropsWidget) {
    return o.keys.join("\0") === this.keys.join("\0");
  }
  toDOM() {
    const d = document.createElement("div");
    d.className = "cm-props";
    d.dataset.props = "1";
    const t = document.createElement("span");
    t.className = "cm-props-t";
    t.textContent = `Properties · ${this.keys.length}`;
    d.appendChild(t);
    for (const k of this.keys) {
      const c = document.createElement("span");
      c.className = "chip";
      c.textContent = k;
      d.appendChild(c);
    }
    return d;
  }
  ignoreEvent() {
    return false;
  }
}

/** The properties block, folded unless the cursor is in it. Block widgets have to come from a field. */
const propsFold = StateField.define<DecorationSet>({
  create: (s) => foldProps(s),
  update: (v, tr) => (tr.docChanged || tr.selection ? foldProps(tr.state) : v),
  provide: (f) => EditorView.decorations.from(f),
});

/** HTML comments, hidden: only Source shows them. A comment on lines of its own takes its lines. */
const commentsHidden = StateField.define<DecorationSet>({
  create: (s) => hideComments(s),
  update: (v, tr) => (tr.docChanged || syntaxTree(tr.state) !== syntaxTree(tr.startState) ? hideComments(tr.state) : v),
  provide: (f) => EditorView.decorations.from(f),
});

function hideComments(state: EditorState): DecorationSet {
  const out: Range<Decoration>[] = [];
  const tree = ensureSyntaxTree(state, state.doc.length, 50) ?? syntaxTree(state);
  tree.iterate({
    enter: (n) => {
      if (n.name === "CommentBlock") {
        const a = state.doc.lineAt(n.from);
        const b = state.doc.lineAt(n.to);
        // With the line break after it, so no empty line is left where it was.
        const to = b.number < state.doc.lines ? b.to + 1 : b.to;
        out.push(Decoration.replace({ block: true }).range(a.from, to));
        return false;
      }
      if (n.name === "Comment") {
        out.push(hide.range(n.from, n.to));
        return false;
      }
    },
  });
  return Decoration.set(out, true);
}

function foldProps(state: EditorState): DecorationSet {
  const end = frontmatterEnd(state.doc.sliceString(0, Math.min(state.doc.length, 20000)));
  if (end === null) return Decoration.none;
  const inside = state.selection.ranges.some((r) => r.from <= end && r.to >= 0 && r.from <= end);
  if (inside && state.facet(EditorView.editable)) return Decoration.none;
  const keys = frontmatterKeys(state.doc.sliceString(0, end));
  return Decoration.set([Decoration.replace({ widget: new PropsWidget(keys), block: true }).range(0, end)]);
}

const IN_CODE = new Set(["InlineCode", "FencedCode", "CodeBlock", "CodeText", "HTMLBlock", "Comment", "URL", "Autolink"]);

function inCode(state: EditorState, pos: number): boolean {
  for (let n: ReturnType<typeof syntaxTree>["topNode"] | null = syntaxTree(state).resolveInner(pos, 1); n; n = n.parent)
    if (IN_CODE.has(n.name)) return true;
  return false;
}

function build(view: EditorView, hooks: EditorHooks): DecorationSet {
  const { state } = view;
  const act = activeLines(state);
  const fm = frontmatterEnd(state.doc.sliceString(0, Math.min(state.doc.length, 20000))) ?? -1;
  const out: Range<Decoration>[] = [];
  const lineOf = (pos: number) => state.doc.lineAt(pos);
  const isAct = (pos: number) => act.has(lineOf(pos).number);
  // An image stays drawn on the cursor's line: only a selection inside its markdown shows it.
  const inside = (a: number, b: number) => state.selection.ranges.some((r) => r.to > a && r.from < b);
  const lines = (from: number, to: number, cls: string) => {
    for (let p = from; p <= to;) {
      const l = lineOf(p);
      out.push(line(cls).range(l.from));
      p = l.to + 1;
    }
  };

  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from,
      to,
      enter: (n) => {
        if (n.from < fm) return n.name === "Document" ? undefined : false;
        const name = n.name;
        const h = /^(?:ATX|Setext)Heading(\d)$/.exec(name);
        if (h) {
          lines(n.from, n.to, `cm-h cm-h${h[1]}`);
          return;
        }
        switch (name) {
          case "HeaderMark": {
            if (isAct(n.from)) return;
            const l = lineOf(n.from);
            // `## ` at the start: the mark and its space; a setext underline: the whole line.
            const end = n.from === l.from && state.doc.sliceString(n.to, n.to + 1) === " " ? n.to + 1 : n.to;
            if (end > n.from) out.push(hide.range(n.from, end));
            return;
          }
          case "Emphasis":
            out.push(mark("cm-em").range(n.from, n.to));
            return;
          case "StrongEmphasis":
            out.push(mark("cm-strong").range(n.from, n.to));
            return;
          case "Strikethrough":
            out.push(mark("cm-strike").range(n.from, n.to));
            return;
          case "InlineCode":
            out.push(mark("cm-icode").range(n.from, n.to));
            return;
          case "Highlight":
            out.push(mark("cm-hl").range(n.from, n.to));
            return;
          case "HTMLTag": {
            // Inline HTML is drawn, never shown, even on the cursor's line (Source shows the tags):
            // a highlighter colour (src/editor/paint.ts) or a formatting pair styles its text.
            const tag = state.doc.sliceString(n.from, n.to);
            const im = imgTag(tag);
            if (im) {
              if (!inside(n.from, n.to))
                out.push(Decoration.replace({ widget: new ImageWidget(im.src, im.label, tag, hooks) }).range(n.from, n.to));
              return;
            }
            const t = /^<(\/?)([a-zA-Z][\w-]*)\b[^>]*?(\/?)>$/.exec(tag);
            if (!t || KEEP_TAGS.has(t[2].toLowerCase())) return;
            const name = t[2].toLowerCase();
            if (!t[1] && !t[3]) {
              const c = name === "mark" ? /\bclass="hl-(\w+)"/.exec(tag)?.[1] : undefined;
              const cls = c ? `cm-hl hl-${c}` : name === "mark" ? "cm-hl" : TAG_CLASS[name];
              const l = lineOf(n.to);
              const close = cls ? state.doc.sliceString(n.to, l.to).toLowerCase().indexOf(`</${name}>`) : -1;
              if (close > 0) out.push(mark(cls!).range(n.to, n.to + close));
            }
            out.push(hide.range(n.from, n.to));
            return;
          }
          case "EmphasisMark":
          case "StrikethroughMark":
          case "HighlightMark":
            if (!isAct(n.from)) out.push(hide.range(n.from, n.to));
            return;
          case "CodeMark": {
            const p = n.node.parent;
            if (p?.name === "InlineCode" && !isAct(n.from)) out.push(hide.range(n.from, n.to));
            return;
          }
          case "FencedCode": {
            lines(n.from, n.to, "cm-code");
            const top = lineOf(n.from);
            out.push(line("cm-code cm-code-top").range(top.from));
            const lang = /^\s*(?:`{3,}|~{3,})\s*([\w-]+)/.exec(top.text)?.[1]?.toLowerCase();
            if (lang && QUERY_DOCS[lang]) out.push(Decoration.widget({ widget: new DocsWidget(lang), side: 1 }).range(top.to));
            out.push(line("cm-code cm-code-end").range(lineOf(n.to).from));
            return;
          }
          case "CodeInfo":
          case "CodeBlock":
            return;
          case "Blockquote":
            lines(n.from, n.to, "cm-quote");
            return;
          case "QuoteMark": {
            if (isAct(n.from)) return;
            const end = state.doc.sliceString(n.to, n.to + 1) === " " ? n.to + 1 : n.to;
            out.push(hide.range(n.from, end));
            return;
          }
          case "HorizontalRule":
            out.push(line("cm-hr").range(lineOf(n.from).from));
            if (!isAct(n.from)) out.push(hide.range(n.from, n.to));
            return;
          case "Table":
            lines(n.from, n.to, "cm-table");
            return false;
          case "Comment":
          case "CommentBlock":
            return false;
          case "HTMLBlock": {
            // An `<img>` tag on a line of its own is drawn, as inline ones are.
            const block = state.doc.sliceString(n.from, n.to);
            const tag = block.trim();
            const im = !tag.includes("\n") ? imgTag(tag) : null;
            if (im) {
              const a = n.from + block.indexOf(tag);
              const b = a + tag.length;
              if (!inside(a, b)) out.push(Decoration.replace({ widget: new ImageWidget(im.src, im.label, tag, hooks) }).range(a, b));
              return false;
            }
            out.push(mark("cm-faint").range(n.from, n.to));
            return false;
          }
          case "ListMark": {
            if (isAct(n.from)) return;
            const after = state.doc.sliceString(n.to, n.to + 5);
            if (/^ \[[ xX]\]/.test(after)) {
              out.push(hide.range(n.from, n.to + 1));
            } else if (/^[-*+]$/.test(state.doc.sliceString(n.from, n.to))) {
              out.push(Decoration.replace({ widget: new BulletWidget() }).range(n.from, n.to));
            }
            return;
          }
          case "TaskMarker": {
            const done = /x/i.test(state.doc.sliceString(n.from, n.to));
            const cur = state.selection.ranges.some((r) => r.from > n.from && r.from < n.to);
            if (!cur) out.push(Decoration.replace({ widget: new CheckWidget(done) }).range(n.from, n.to));
            if (done) {
              const l = lineOf(n.from);
              if (n.to + 1 < l.to) out.push(mark("cm-done").range(n.to + 1, l.to));
            }
            return;
          }
          case "Image": {
            if (inside(n.from, n.to)) return false;
            const text = state.doc.sliceString(n.from, n.to);
            const m = /^!\[([^\]]*)\]\(\s*<?([^)\s>]+)>?/.exec(text);
            if (m) out.push(Decoration.replace({ widget: new ImageWidget(m[2], m[1], text, hooks) }).range(n.from, n.to));
            return false;
          }
          case "Link": {
            out.push(mark("cm-link").range(n.from, n.to));
            if (isAct(n.from)) return false;
            // `[label](url "title")`: everything but the label goes.
            const c = n.node.cursor();
            if (c.firstChild()) {
              let first = true;
              do {
                if (c.name === "LinkMark") {
                  if (first) out.push(hide.range(c.from, c.to));
                  else {
                    out.push(hide.range(c.from, n.to));
                    break;
                  }
                  first = false;
                }
              } while (c.nextSibling());
            }
            return false;
          }
          case "URL":
            if (n.node.parent?.name !== "Link") out.push(mark("cm-link").range(n.from, n.to));
            return;
        }
      },
    });

    // Wikilinks and tags aren't in the markdown grammar.
    const text = state.doc.sliceString(from, to);
    const re = /(!?)\[\[([^[\]\n]+?)\]\]|(^|[\s(])(#[\p{L}\p{N}_/-]*[\p{L}_/-][\p{L}\p{N}_/-]*)/gmu;
    for (let m; (m = re.exec(text));) {
      if (m[2] !== undefined) {
        const a = from + m.index;
        const b = a + m[0].length;
        if (a < fm || inCode(state, a)) continue;
        const t = parseWikilink(!!m[1], m[2]);
        if (!t) continue;
        const image = t.embed && IMAGE_EXT.test(t.target);
        if (image ? inside(a, b) : isAct(a)) {
          out.push(mark("cm-wikilink raw").range(a, b));
          continue;
        }
        if (image) out.push(Decoration.replace({ widget: new ImageWidget(`wiki:${t.target}`, t.alias ?? "", m[0], hooks) }).range(a, b));
        else out.push(Decoration.replace({ widget: new LinkWidget(wikiLabel(t), m[2].split("|")[0], t.embed) }).range(a, b));
      } else {
        const a = from + m.index + m[3].length;
        if (a < fm || inCode(state, a)) continue;
        const l = lineOf(a);
        // A heading's `#`, not a tag.
        if (a === l.from && /^#+\s/.test(l.text)) continue;
        out.push(mark("cm-tag").range(a, a + m[4].length));
      }
    }

    // A task's fields, away from the cursor's line: an icon and the value, never the emoji.
    const fields = new RegExp(FIELD_RE.source, "gu");
    for (let m; (m = fields.exec(text));) {
      const a = from + m.index;
      if (a < fm || isAct(a) || inCode(state, a)) continue;
      const l = lineOf(a);
      const task = /^(?:[ \t]{0,3}>[ \t]?)*\s*(?:[-*+]|\d+[.)])\s+\[([^\]])\]\s/.exec(l.text);
      const p = task && splitTaskFields(m[0]).find((x) => typeof x !== "string");
      if (!p || typeof p === "string") continue;
      const isDay = /^\d{4}-\d{2}-\d{2}$/.test(p.value);
      const n = isDay ? daysUntil(p.value, hooks.today?.()) : 0;
      const tone = p.field.id === "due" && isDay && task[1] === " " ? (n < 0 ? "is-late" : n <= 3 ? "is-soon" : "") : "";
      out.push(Decoration.replace({ widget: new DateWidget(p.field.id, p.value, tone) }).range(a, a + m[0].length));
    }
  }
  return Decoration.set(out, true);
}

/** Where a click landed: a checkbox to tick, a link to open, the properties to unfold. */
function clicks(hooks: EditorHooks) {
  return EditorView.domEventHandlers({
    mousedown(e, view) {
      const el = e.target as HTMLElement;
      if (el.closest("[data-check]")) {
        const pos = view.posAtDOM(el);
        const l = view.state.doc.lineAt(pos);
        const done = !el.classList.contains("on");
        const next = toggledLine(l.text, done, hooks.today?.() ?? new Date().toISOString().slice(0, 10));
        if (next !== l.text && view.state.facet(EditorView.editable))
          view.dispatch({ changes: { from: l.from, to: l.to, insert: next }, userEvent: "input.tick" });
        e.preventDefault();
        return true;
      }
      if (el.closest("[data-props]")) {
        const l2 = view.state.doc.line(Math.min(2, view.state.doc.lines));
        view.dispatch({ selection: { anchor: l2.from } });
        view.focus();
        e.preventDefault();
        return true;
      }
      if (!isMod(e)) {
        // A click on a drawn image leaves it drawn, for its handles; a double-click puts the
        // cursor in its markdown, which shows it.
        const im = el.closest(".cm-img");
        if (im) {
          if (e.detail >= 2) {
            view.dispatch({ selection: { anchor: view.posAtDOM(im) + 2 } });
            view.focus();
          }
          e.preventDefault();
          return true;
        }
        // A plain click on a drawn link puts the cursor there, which shows its markdown.
        const w = el.closest(".cm-wikilink:not(.raw)");
        if (w) {
          const pos = view.posAtDOM(w);
          view.dispatch({ selection: { anchor: pos } });
          view.focus();
          e.preventDefault();
          return true;
        }
        return false;
      }
      const wl = el.closest<HTMLElement>("[data-wiki]");
      if (wl) {
        hooks.openLink?.(wl.dataset.wiki!, true);
        e.preventDefault();
        return true;
      }
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos === null) return false;
      const target = linkAt(view.state, pos);
      if (!target) return false;
      hooks.openLink?.(target.target, target.wiki);
      e.preventDefault();
      return true;
    },
  });
}

/** The link under a position: a wikilink's target, a markdown link's URL, or a bare URL. */
export function linkAt(state: EditorState, pos: number): { target: string; wiki: boolean } | null {
  const l = state.doc.lineAt(pos);
  const re = /\[\[([^[\]\n]+?)\]\]/g;
  for (let m; (m = re.exec(l.text));) {
    const a = l.from + m.index;
    if (pos >= a && pos <= a + m[0].length) return { target: m[1].split("|")[0], wiki: true };
  }
  for (let n: ReturnType<typeof syntaxTree>["topNode"] | null = syntaxTree(state).resolveInner(pos, 1); n; n = n.parent) {
    if (n.name === "URL") return { target: state.doc.sliceString(n.from, n.to), wiki: false };
    if (n.name === "Link") {
      const u = n.getChild("URL");
      return u ? { target: state.doc.sliceString(u.from, u.to), wiki: false } : null;
    }
  }
  return null;
}

export function livePreview(hooks: EditorHooks): Extension {
  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = build(view, hooks);
      }
      update(u: ViewUpdate) {
        if (u.docChanged || u.viewportChanged || u.selectionSet || syntaxTree(u.state) !== syntaxTree(u.startState))
          this.decorations = build(u.view, hooks);
      }
    },
    { decorations: (v) => v.decorations },
  );
  return [plugin, propsFold, commentsHidden, clicks(hooks), EditorView.editorAttributes.of({ class: "cm-live" })];
}
