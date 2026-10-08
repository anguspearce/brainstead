// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// ⌘F on a note: find in View; find and replace in Edit and Source (src/editor/find.ts). ↩ the next
// match, ⇧↩ the one before, ↩ in the Replace box replaces the match, Esc closes.

import type { EditorView } from "@codemirror/view";
import { useEffect, useRef, useState } from "react";
import { clearView, editorFind, editorFindClose, editorStep, Found, paintView, rangesIn } from "../editor/find";
import { Icon } from "../icons";
import { keyLabel } from "../platform";

/** Scrolls the page to the editor's selection (the editor grows with its text, so the page scrolls,
 *  not the editor). */
function reveal(view: EditorView) {
  const at = view.state.selection.main.from;
  const c = view.coordsAtPos(at);
  const page = view.dom.closest(".docbody") as HTMLElement | null;
  if (!c || !page) return;
  const box = page.getBoundingClientRect();
  if (c.top < box.top + 60 || c.bottom > box.bottom - 40) page.scrollBy({ top: c.top - box.top - box.height / 2 });
}

export function FindBar({
  mode,
  getView,
  getRead,
  onClose,
}: {
  /** View (read), Edit (live) or Source: a change looks again. */
  mode: string;
  /** The editor in Edit and Source; null in View. */
  getView: () => EditorView | null;
  /** The drawn note, for View. */
  getRead: () => HTMLElement | null;
  onClose: () => void;
}) {
  const [view, setView] = useState<EditorView | null>(null);
  const [readEl, setReadEl] = useState<HTMLElement | null>(null);
  // The editor (or the drawn note) can come a moment after the bar: wait for it, up to 3 seconds.
  useEffect(() => {
    let n = 0;
    const look = () => {
      const v = getView();
      const r = getRead();
      const reading = mode.startsWith("read");
      if ((reading ? r : v) || ++n > 30) {
        setView(reading ? null : v);
        setReadEl(r);
        return true;
      }
      return false;
    };
    if (look()) return;
    const t = window.setInterval(() => look() && window.clearInterval(t), 100);
    return () => window.clearInterval(t);
  }, [mode]); // eslint-disable-line react-hooks/exhaustive-deps
  // A screenshot scene can open it with words in it.
  const [q, setQ] = useState(document.documentElement.dataset.find ?? "");
  const [rep, setRep] = useState("");
  const [found, setFound] = useState<Found>({ count: 0, at: 0 });
  const input = useRef<HTMLInputElement>(null);
  const ranges = useRef<Range[]>([]);

  useEffect(() => input.current?.focus(), [view]);

  // Look again whenever the words or the mode change.
  useEffect(() => {
    if (view) {
      clearView();
      setFound(q ? editorFind(view, q, rep) : { count: 0, at: 0 });
      if (q) {
        setFound(editorStep(view, "next"));
        reveal(view);
      }
    } else if (readEl) {
      ranges.current = rangesIn(readEl, q);
      const f = { count: ranges.current.length, at: ranges.current.length ? 1 : 0 };
      setFound(f);
      paintView(ranges.current, f.at);
    }
  }, [q, view, readEl]); // eslint-disable-line react-hooks/exhaustive-deps
  // The replacement text goes into the editor's query as it's typed.
  useEffect(() => {
    if (view && q) editorFind(view, q, rep);
  }, [rep]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(
    () => () => {
      clearView();
      if (view) editorFindClose(view);
    },
    [view],
  );

  // In View the boxes are placed from the text's position: drawn again when the window resizes.
  const atRef = useRef(0);
  useEffect(() => {
    atRef.current = found.at;
  });
  useEffect(() => {
    if (view) return;
    const again = () => paintView(ranges.current, atRef.current);
    window.addEventListener("resize", again);
    return () => window.removeEventListener("resize", again);
  }, [view]);

  const step = (dir: 1 | -1) => {
    if (view) {
      setFound(editorStep(view, dir === 1 ? "next" : "prev"));
      return reveal(view);
    }
    const n = ranges.current.length;
    if (!n) return;
    const at = ((found.at - 1 + dir + n) % n) + 1;
    setFound({ count: n, at });
    paintView(ranges.current, at);
  };
  const close = () => {
    clearView();
    if (view) {
      editorFindClose(view);
      view.focus();
    }
    onClose();
  };
  const key = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") step(e.shiftKey ? -1 : 1);
    else if (e.key === "Escape") close();
    else return;
    e.preventDefault();
  };

  return (
    <div className="findbar" data-no-print>
      <label className="inp">
        <Icon name="search" size={13} />
        <input
          ref={input}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={key}
          placeholder="Find in this note"
          aria-label="Find"
          spellCheck={false}
        />
        {q && <span className="faint small">{found.count ? `${found.at || "–"} of ${found.count}` : "None"}</span>}
      </label>
      <button
        type="button"
        className="ibtn sm"
        aria-label="Previous match"
        title={keyLabel("Previous (⇧↩)")}
        disabled={!found.count}
        onClick={() => step(-1)}
      >
        <Icon name="chevup" size={13} />
      </button>
      <button type="button" className="ibtn sm" aria-label="Next match" title="Next (↩)" disabled={!found.count} onClick={() => step(1)}>
        <Icon name="chevdown" size={13} />
      </button>
      {view && (
        <>
          <label className="inp">
            <input
              value={rep}
              onChange={(e) => setRep(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") close();
                else if (e.key === "Enter" && found.count) setFound(editorStep(view, "replace"));
                else return;
                e.preventDefault();
              }}
              placeholder="Replace with"
              aria-label="Replace with"
              spellCheck={false}
            />
          </label>
          <span title={found.count ? "Replace this match and move to the next (↩ in the Replace box)" : "Nothing to replace: no matches"}>
            <button type="button" className="btn sm" disabled={!found.count} onClick={() => setFound(editorStep(view, "replace"))}>
              Replace
            </button>
          </span>
          <span title={found.count ? "Replace every match in the note" : "Nothing to replace: no matches"}>
            <button type="button" className="btn sm" disabled={!found.count} onClick={() => setFound(editorStep(view, "all"))}>
              Replace all
            </button>
          </span>
        </>
      )}
      {!view && mode === "read:ready" && <span className="faint small">Switch to Edit to replace</span>}
      <span className="grow" />
      <button type="button" className="ibtn sm" aria-label="Close find" title="Close (Esc)" onClick={close}>
        <Icon name="x" size={12} />
      </button>
    </div>
  );
}
