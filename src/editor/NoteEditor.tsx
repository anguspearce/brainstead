// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The editor in React: made once per file, fed its text, and told where Finder drops land.

import { Transaction } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import { api } from "../api";
import { IMAGE_EXT } from "../md/wikilinks";
import { reportEditError } from "../taskModel";
import { CMEditor } from "./cm";
import type { EditorHooks, EditorMode } from "./Editor";
import { imageEmbed } from "./hooks";
import { activeEditor, setActiveEditor } from "./queryBuilderStore";
import { Toolbar } from "./Toolbar";

export interface NoteEditorHandle {
  editor: CMEditor | null;
  /** Puts new text in with the smallest change, so the cursor and undo survive. */
  /** Sets the text, changing only what differs. `undoable` puts it in the editor's history (an edit
   *  made beside the editor, like a wiki page's fields); otherwise it isn't (an outside change). */
  replaceText(text: string, undoable?: boolean): void;
}

/** The changed middle between two texts. */
export function middleChange(a: string, b: string): { from: number; to: number; insert: string } | null {
  if (a === b) return null;
  let p = 0;
  while (p < a.length && p < b.length && a[p] === b[p]) p++;
  let s = 0;
  while (s < a.length - p && s < b.length - p && a[a.length - 1 - s] === b[b.length - 1 - s]) s++;
  // Never between the two halves of a CRLF.
  if (p > 0 && a[p - 1] === "\r" && a[p] === "\n") p--;
  if (s > 0 && a[a.length - s] === "\n" && a[a.length - s - 1] === "\r") s--;
  return { from: p, to: a.length - s, insert: b.slice(p, b.length - s) };
}

interface Props {
  initial: string;
  mode: EditorMode;
  hooks: EditorHooks;
  onChange: (md: string) => void;
  /** Where to put the caret once loaded (an offset in `initial`), as for a note just made from a template. */
  caret?: number | null;
}

export const NoteEditor = forwardRef<NoteEditorHandle, Props>(function NoteEditor({ initial, mode, hooks, onChange, caret }, ref) {
  const host = useRef<HTMLDivElement>(null);
  const ed = useRef<CMEditor | null>(null);
  /** The editor's view once it's made, for the formatting toolbar. */
  const [view, setView] = useState<EditorView | null>(null);
  const change = useRef(onChange);
  change.current = onChange;

  useEffect(() => {
    const e = new CMEditor(host.current!, hooks, mode);
    e.load(initial);
    ed.current = e;
    setView(e.view);
    setActiveEditor(e.view);
    if (caret != null) {
      // `initial` may have CRLFs; CodeMirror counts each as one position.
      const pos = Math.min(caret - (initial.slice(0, caret).match(/\r\n/g)?.length ?? 0), e.view.state.doc.length);
      e.view.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
      e.focus();
    }
    const off = e.onChange((md) => change.current(md));
    return () => {
      off();
      e.destroy();
      ed.current = null;
      setView(null);
      if (activeEditor() === e.view) setActiveEditor(null);
    };
    // Made once; the parent remounts it for another file.
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    ed.current?.setMode(mode);
  }, [mode]);

  // Images dropped from Finder (Tauri takes file drops before the page sees them).
  useEffect(() => {
    let off: (() => void) | undefined;
    let live = true;
    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent(async (ev) => {
          if (ev.payload.type !== "drop" || !host.current || !ed.current) return;
          const r = window.devicePixelRatio || 1;
          const x = ev.payload.position.x / r;
          const y = ev.payload.position.y / r;
          const box = host.current.getBoundingClientRect();
          if (x < box.left || x > box.right || y < box.top || y > box.bottom) return;
          const imgs = ev.payload.paths.filter((p) => IMAGE_EXT.test(p));
          const md: string[] = [];
          for (const p of imgs) {
            try {
              md.push(imageEmbed(await api.imageImport(p)));
            } catch (e) {
              reportEditError(e);
            }
          }
          if (md.length) ed.current?.insertAt(md.join("\n"), { x, y });
        }),
      )
      .then((u) => {
        if (live) off = u;
        else u();
      })
      .catch(() => {});
    return () => {
      live = false;
      off?.();
    };
  }, []);

  useImperativeHandle(ref, () => ({
    get editor() {
      return ed.current;
    },
    replaceText(text: string, undoable = false) {
      const e = ed.current;
      if (!e) return;
      const sep = e.view.state.lineBreak;
      const cur = e.getMarkdown();
      const c = middleChange(cur, text);
      if (!c) return;
      // Offsets in the file's text, to CodeMirror's (a CRLF is one position there).
      const toPos = (i: number) => (sep === "\n" ? i : cur.slice(0, i).split(sep).join("\n").length);
      e.view.dispatch({
        changes: { from: toPos(c.from), to: toPos(c.to), insert: c.insert },
        annotations: Transaction.addToHistory.of(undoable),
      });
    },
  }));

  return (
    <>
      <Toolbar view={view} bar={mode === "live"} />
      <div className="editor" ref={host} />
    </>
  );
});
