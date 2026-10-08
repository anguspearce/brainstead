// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The formatting toolbar over the editor in Edit and Source. Its buttons edit the markdown around
// the selection (src/editor/format.ts) and show what the caret is in. ⌘B, ⌘I, ⌘E and ⌘⇧X work
// while the editor has focus.

import { open } from "@tauri-apps/plugin-dialog";
import { EditorView } from "@codemirror/view";
import { Fragment, ReactNode, useEffect, useState } from "react";
import { api } from "../api";
import { localToday } from "../md/taskQuery";
import { reportEditError } from "../taskModel";
import { Popover } from "../ui";
import {
  Active,
  activeAt,
  Block,
  codeBlock,
  insertBlock,
  link,
  Mark,
  paint,
  setHeading,
  TABLE,
  toggleBlock,
  toggleMark,
  wikilink,
} from "./format";
import { imageEmbed } from "./hooks";
import { HL_COLOURS, HlColour } from "./paint";
import { settings, useStore } from "../store";
import { isMod, keyLabel } from "../platform";
import { openBuilderAt } from "./queryAssist";

// Line icons in the style of src/icons.tsx (24 box, 1.75 stroke).
const PATHS: Record<string, string> = {
  bullet: "M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01",
  number: "M10 6h10M10 12h10M10 18h10M4 4.5h1.5V9M3.8 9h3.2M4 14.3c.6-.6 2.8-.8 2.8.7 0 1.2-2.8 2-2.8 3.5h3",
  task: "M10 7h10M10 17h10M3.5 4.5h4v4h-4zM3.5 14.5h4v4h-4zM4.5 16.5l1 1 1.7-2",
  quote: "M5 7v10M9 8h10M9 12h10M9 16h6",
  codeblock: "M8 8l-4 4 4 4M16 8l4 4-4 4M13.5 6l-3 12",
  divider: "M3 12h18M7 7h10M7 17h10",
  link: "M10 14a4.5 4.5 0 0 0 6.4 0l2.8-2.8a4.5 4.5 0 0 0-6.4-6.4L11.5 6M14 10a4.5 4.5 0 0 0-6.4 0l-2.8 2.8a4.5 4.5 0 0 0 6.4 6.4l1.3-1.3",
  wikilink: "M8 4H5v16h3M16 4h3v16h-3M10 12h4",
  image: "M4 5h16v14H4zM4 16l4.5-4.5 3.5 3.5 2.5-2.5L20 18M15.5 9.5h.01",
  table: "M4 5h16v14H4zM4 10h16M4 15h16M10 5v14",
  date: "M5 6h14v14H5zM5 10h14M9 3v4M15 3v4",
  query: "M4 6h16M4 12h10M4 18h6M17 14l3 3-3 3M14 20h.01",
};

function Glyph({ name }: { name: string }) {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}

const HEADINGS: [number, string][] = [
  [0, "Body text"],
  [1, "Heading 1"],
  [2, "Heading 2"],
  [3, "Heading 3"],
  [4, "Heading 4"],
];

const KEYS: Record<string, [Mark, boolean]> = {
  b: ["bold", false],
  i: ["italic", false],
  e: ["code", false],
  x: ["strike", true],
};

const HL_NAMES: Record<HlColour, string> = {
  red: "Red",
  orange: "Orange",
  yellow: "Yellow",
  green: "Green",
  teal: "Teal",
  blue: "Blue",
  purple: "Purple",
  grey: "Grey",
};

/** `bar` false (Source): no toolbar, but its shortcuts (⌘B and the rest) still work. */
export function Toolbar({ view, bar = true }: { view: EditorView | null; bar?: boolean }) {
  const [active, setActive] = useState<Active | null>(null);
  const [headAt, setHeadAt] = useState<DOMRect | null>(null);
  const [queryAt, setQueryAt] = useState<DOMRect | null>(null);
  const [hlAt, setHlAt] = useState<DOMRect | null>(null);
  // The highlighter keeps its colour until another is picked, as in the journal editor.
  const hl = useStore(settings).hlColour ?? "yellow";

  // Follow the caret: the editor reports edits, not moves, so listen on its element.
  useEffect(() => {
    if (!view) return;
    const update = () => setActive(activeAt(view.state));
    const key = (e: KeyboardEvent) => {
      const mod = isMod(e);
      const k = KEYS[e.key.toLowerCase()];
      if (mod && k && !e.altKey && e.shiftKey === k[1]) {
        e.preventDefault();
        e.stopPropagation();
        toggleMark(view, k[0]);
      } else if (mod && e.shiftKey && !e.altKey && e.key.toLowerCase() === "h") {
        e.preventDefault();
        e.stopPropagation();
        paint(view, settings.get().hlColour ?? "yellow");
      }
      requestAnimationFrame(update);
    };
    const el = view.dom;
    el.addEventListener("keydown", key, true);
    el.addEventListener("mouseup", update);
    el.addEventListener("focusin", update);
    document.addEventListener("selectionchange", update);
    update();
    return () => {
      el.removeEventListener("keydown", key, true);
      el.removeEventListener("mouseup", update);
      el.removeEventListener("focusin", update);
      document.removeEventListener("selectionchange", update);
    };
  }, [view]);

  if (!view || !bar) return null;
  const run = (f: () => unknown) => () => {
    f();
    setActive(activeAt(view.state));
  };
  const mark = (m: Mark, label: string, keys: string, glyph: ReactNode) => (
    <button
      type="button"
      className={`tbtn ${active?.marks.has(m) ? "on" : ""}`}
      aria-label={label}
      aria-pressed={!!active?.marks.has(m)}
      title={keys ? `${label} (${keys})` : label}
      onMouseDown={(e) => e.preventDefault()}
      onClick={run(() => toggleMark(view, m))}
    >
      {glyph}
    </button>
  );
  const block = (b: Block, label: string) => (
    <button
      type="button"
      className={`tbtn ${active?.block === b ? "on" : ""}`}
      aria-label={label}
      aria-pressed={active?.block === b}
      title={label}
      onMouseDown={(e) => e.preventDefault()}
      onClick={run(() => toggleBlock(view, b))}
    >
      <Glyph name={b} />
    </button>
  );
  const plain = (name: string, label: string, f: () => unknown) => (
    <button type="button" className="tbtn" aria-label={label} title={label} onMouseDown={(e) => e.preventDefault()} onClick={run(f)}>
      <Glyph name={name} />
    </button>
  );
  const image = async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "svg", "heic"] }],
    });
    const files = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
    try {
      const md: string[] = [];
      for (const f of files) md.push(imageEmbed(await api.imageImport(f)));
      if (md.length) view.dispatch(view.state.replaceSelection(md.join("\n")));
      view.focus();
    } catch (e) {
      reportEditError(e);
    }
  };
  const level = active?.heading ?? 0;
  return (
    <div className="etoolbar" role="toolbar" aria-label="Formatting" data-no-print>
      <button
        type="button"
        className="tbtn heading"
        aria-label="Text style"
        title="Text style"
        aria-haspopup="menu"
        onMouseDown={(e) => e.preventDefault()}
        onClick={(e) => setHeadAt(e.currentTarget.getBoundingClientRect())}
      >
        {HEADINGS[level]?.[1] ?? `Heading ${level}`}
        <svg width={10} height={10} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} aria-hidden="true">
          <path d="M6 9l6 6 6-6" />
        </svg>
      </button>
      <span className="tsep" />
      {mark("bold", "Bold", keyLabel("⌘B"), <b>B</b>)}
      {mark("italic", "Italic", keyLabel("⌘I"), <i>I</i>)}
      {mark("strike", "Strikethrough", keyLabel("⌘⇧X"), <s>S</s>)}
      <span className="tbsplit">
        <button
          type="button"
          className={`tbtn ${active?.marks.has("highlight") ? "on" : ""}`}
          aria-label={`Highlight ${HL_NAMES[hl].toLowerCase()}`}
          title={keyLabel(`Highlight ${HL_NAMES[hl].toLowerCase()} (⌘⇧H)`)}
          onMouseDown={(e) => e.preventDefault()}
          onClick={run(() => paint(view, hl))}
        >
          <span className={`tb-hl hl-${hl}`}>H</span>
        </button>
        <button
          type="button"
          className="tbtn tbmore"
          aria-label="Highlight colour"
          title="Highlight colour"
          aria-haspopup="menu"
          onMouseDown={(e) => e.preventDefault()}
          onClick={(e) => setHlAt(e.currentTarget.parentElement!.getBoundingClientRect())}
        >
          <svg width={9} height={9} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} aria-hidden="true">
            <path d="M6 9l6 6 6-6" />
          </svg>
        </button>
      </span>
      {mark("code", "Inline code", keyLabel("⌘E"), <code>{"<>"}</code>)}
      <span className="tsep" />
      {block("bullet", "Bulleted list")}
      {block("number", "Numbered list")}
      {block("task", "Task list")}
      {block("quote", "Quote")}
      {plain("codeblock", "Code block", () => codeBlock(view))}
      {plain("divider", "Divider", () => insertBlock(view, "---"))}
      <span className="tsep" />
      {plain("link", "Link", () => link(view))}
      {plain("wikilink", "Link to a note ([[)", () => wikilink(view))}
      {plain("image", "Image…", () => void image())}
      {plain("table", "Table", () => insertBlock(view, TABLE, 2))}
      {plain("date", "Today's date", () => view.dispatch(view.state.replaceSelection(localToday())))}
      <button
        type="button"
        className="tbtn"
        aria-label="Insert a query"
        title="Insert a query"
        aria-haspopup="menu"
        onMouseDown={(e) => e.preventDefault()}
        onClick={(e) => setQueryAt(e.currentTarget.getBoundingClientRect())}
      >
        <Glyph name="query" />
      </button>
      {queryAt && (
        <Popover anchor={queryAt} onClose={() => setQueryAt(null)} width={200}>
          <div className="menu" role="menu">
            {(
              [
                ["tasks", undefined, "Tasks query"],
                ["dataview", "TABLE", "Dataview table"],
                ["dataview", "LIST", "Dataview list"],
                ["dataview", "TASK", "Dataview tasks"],
              ] as const
            ).map(([lang, kind, label]) => (
              <Fragment key={label}>
                {/* The Tasks query apart from the three Dataview ones. */}
                {lang === "dataview" && kind === "TABLE" && <div className="sep" />}
                <button
                  type="button"
                  role="menuitem"
                  title={`Build a ${label.toLowerCase()} and insert it at the cursor`}
                  onClick={() => {
                    setQueryAt(null);
                    openBuilderAt(view, view.state.selection.main.head, lang, kind);
                  }}
                >
                  <Glyph name="query" />
                  {label}…
                </button>
              </Fragment>
            ))}
          </div>
        </Popover>
      )}
      {hlAt && (
        <Popover anchor={hlAt} onClose={() => setHlAt(null)} width={184}>
          <div className="hlpick" role="menu" onMouseDown={(e) => e.preventDefault()}>
            {HL_COLOURS.map((c) => (
              <button
                key={c}
                type="button"
                role="menuitemradio"
                aria-checked={c === hl}
                className={`dot hl-${c} ${c === hl ? "on" : ""}`}
                aria-label={`Highlight ${HL_NAMES[c].toLowerCase()}`}
                title={HL_NAMES[c]}
                onClick={() => {
                  setHlAt(null);
                  settings.update({ hlColour: c });
                  paint(view, c);
                  setActive(activeAt(view.state));
                }}
              />
            ))}
            <button
              type="button"
              className="hlnone"
              title="Remove the highlight from the selection"
              onClick={() => {
                setHlAt(null);
                paint(view, null);
                setActive(activeAt(view.state));
              }}
            >
              No highlight
            </button>
          </div>
        </Popover>
      )}
      {headAt && (
        <Popover anchor={headAt} onClose={() => setHeadAt(null)} width={180}>
          <div className="menu" role="menu">
            {HEADINGS.map(([n, label]) => (
              <button
                key={n}
                type="button"
                role="menuitemradio"
                aria-checked={n === level}
                className={`hpick h${n} ${n === level ? "on" : ""}`}
                title={n ? `Make this line a level ${n} heading` : "Make this line plain text"}
                onClick={() => {
                  setHeadAt(null);
                  setHeading(view, n);
                  setActive(activeAt(view.state));
                }}
              >
                {label}
              </button>
            ))}
          </div>
        </Popover>
      )}
    </div>
  );
}
