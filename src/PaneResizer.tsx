// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The divider between two panes, dragged to resize the fixed one (src/panes.ts has the panes and
// their ranges). A screen calls usePanes with its panes and puts the hook's ref and style on its
// grid root and its resizers inside it: each is a 6px hit area over the 1px divider, nothing at rest
// and a 2px accent line while hovered, focused or dragged. A drag writes the pane's custom property
// on the root as the pointer moves (no re-render per move) and keeps the width on release; arrow
// keys move it PANE_STEP (Shift: PANE_STEP_BIG); Enter or a double-click puts the stylesheet's width
// back. A table's columns (src/FileList.tsx, the Trash, Activity's log, Doc check's register) use
// the same with mode "column": the resizer sits inside the column's header cell, on the edge away
// from the table's one flexible column (a column before it at its right edge, a column after it at
// its left edge), so the divider moves with the pointer whichever side of the flexible column it is.
// A type locked in Settings (src/panes.ts, resizeLocks) draws no resizers; its widths still apply.

import { CSSProperties, KeyboardEvent, PointerEvent, ReactNode, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  clampPaneWidth,
  fitPanes,
  PANE_STEP,
  PANE_STEP_BIG,
  PaneSpec,
  paneRoom,
  resizeLocked,
  resetPaneWidth,
  setPaneWidth,
  storedPaneWidth,
} from "./panes";
import { settings, useStore } from "./store";

interface Measured {
  /** The root's content width, for the room each pane has. */
  width: number;
  /** The root's padding and half its column gap, each side: where the divider is from the edge. */
  insetLeft: number;
  insetRight: number;
  /** The grid's column gap, taken from the room once per divider. */
  gap: number;
  /** The grid's columns as laid out, in px; null where the root isn't a grid. Fewer than the panes
   *  and the flexible one means a breakpoint dropped or stacked some (paneShown). */
  tracks: number[] | null;
  /** The stylesheet's width for each pane, by its custom property, at the breakpoint it's at;
   *  NaN where the stylesheet has none (a table's column, whose default is its spec's). */
  defaults: Record<string, number>;
}

const unmeasured: Measured = { width: Infinity, insetLeft: 0, insetRight: 0, gap: 0, tracks: null, defaults: {} };

function measure(el: HTMLElement, vars: string[]): Measured {
  // Each pane's own width in the stylesheet, read with the width usePanes put on the root taken off
  // for the moment (synchronously, so nothing is drawn without it).
  const defaults: Record<string, number> = {};
  for (const v of vars) {
    const inline = el.style.getPropertyValue(v);
    if (inline) el.style.removeProperty(v);
    defaults[v] = parseFloat(getComputedStyle(el).getPropertyValue(v));
    if (inline) el.style.setProperty(v, inline);
  }
  const cs = getComputedStyle(el);
  const px = (v: string) => parseFloat(v) || 0;
  const gap = px(cs.columnGap);
  // Line names ("[a]") and empty tracks drop out; what's left is each column's width.
  const tracks = cs.display.includes("grid")
    ? cs.gridTemplateColumns
        .split(" ")
        .map(parseFloat)
        .filter((t) => t > 0)
    : null;
  return {
    width: el.clientWidth - px(cs.paddingLeft) - px(cs.paddingRight),
    insetLeft: px(cs.paddingLeft) + gap / 2,
    insetRight: px(cs.paddingRight) + gap / 2,
    gap,
    tracks,
    defaults,
  };
}

/** Whether a screen's pane is still a column of its grid, by the tracks laid out: all there; or,
 *  where a breakpoint dropped some (the Weekly review's side column at 868px), the track on the
 *  pane's side is its width; none when the grid is one column (stacked). */
export function paneShown(tracks: number[] | null, panes: number, side: PaneSpec["side"], width: number): boolean {
  if (!tracks || tracks.length >= panes + 1) return true;
  if (tracks.length < 2) return false;
  return Math.abs((side === "left" ? tracks[0] : tracks[tracks.length - 1]) - width) < 1.5;
}

export interface PanesOptions {
  /** "column": the resizers sit in table headers, not over the grid's dividers. */
  mode?: "pane" | "column";
  /** Px of the root that never move: gaps where the root isn't a grid, and columns with no divider
   *  (the Trash's checkbox and buttons). A grid's own column gaps are measured. */
  fixed?: number;
}

export interface Panes<K extends string> {
  /** The grid root's props (a .body element, a table), spread on it: its ref and the panes' custom properties
   *  (position: relative too, for the resizers). Spread, not a ref prop: the lint would take the whole result for a ref. */
  root: { ref: (el: HTMLElement | null) => void; style: CSSProperties };
  /** Each pane's width where usePanes sets it: the stored one, or the stylesheet's made narrower to
   *  fit; undefined where the stylesheet's own shows. */
  widths: Record<K, number | undefined>;
  /** One pane's resizer, to place by hand: in a table, inside the column's own header cell. */
  resizer: (key: K) => ReactNode;
  /** Every pane's resizer, for the root. */
  resizers: ReactNode;
}

/** A screen's panes: `specs` by key (from PANES). */
export function usePanes<K extends string>(specs: Record<K, PaneSpec>, opts: PanesOptions = {}): Panes<K> {
  const { paneWidths: widthsStored, resizeLocks } = useStore(settings);
  // The root as state, not a ref: the resizers take it as a prop, in render.
  const [root, setRoot] = useState<HTMLElement | null>(null);
  const [m, setM] = useState<Measured>(unmeasured);
  const keys = Object.keys(specs) as K[];
  const keyList = keys.join("\0");
  const varList = keys.map((k) => specs[k].cssVar).join(" ");
  useLayoutEffect(() => {
    if (!root) return;
    const sync = () => setM(measure(root, varList.split(" ")));
    sync();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(sync);
    ro.observe(root);
    return () => ro.disconnect();
  }, [root, varList]);
  const mode = opts.mode ?? "pane";
  const fixed = opts.fixed ?? 0;
  // A grid has a gap beside each fixed pane; a table's are in `fixed`.
  const gaps = mode === "pane" ? m.gap * keys.length : 0;
  const { widths, shown, rooms, style } = useMemo(() => {
    const widths = {} as Record<K, number | undefined>;
    const shown = {} as Record<K, number>;
    const rooms = {} as Record<K, number>;
    const style: Record<string, string> = { position: "relative" };
    // Every pane's width, stored or the stylesheet's, fitted to the room together: a default too
    // wide for a narrow screen (a wide sidebar beside it, say) is narrowed as a stored one is.
    const wants = keys.map((k) => {
      const stored = storedPaneWidth(widthsStored, k);
      const def = m.defaults[specs[k].cssVar];
      return { spec: specs[k], want: stored ?? (Number.isFinite(def) ? def : specs[k].def), stored: stored !== undefined };
    });
    const room = paneRoom(m.width, [], fixed + gaps);
    const fit = fitPanes(wants, room);
    const total = fit.reduce((a, b) => a + b, 0);
    keys.forEach((k, i) => {
      shown[k] = fit[i];
      rooms[k] = room - (total - fit[i]);
      // Set on the root where it's stored, or narrower than the stylesheet's; else the stylesheet's shows.
      widths[k] = wants[i].stored || fit[i] < wants[i].want ? fit[i] : undefined;
      if (widths[k] !== undefined) style[specs[k].cssVar] = `${widths[k]}px`;
    });
    return { widths, shown, rooms, style: style as CSSProperties };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [widthsStored, m, fixed, gaps, keyList]);
  // The grid's tracks change with the widths set here: measured again after each, so a pane's
  // divider shows exactly while its column does.
  const styleKey = JSON.stringify(style);
  useLayoutEffect(() => {
    if (root && mode === "pane") setM(measure(root, varList.split(" ")));
  }, [styleKey]); // eslint-disable-line react-hooks/exhaustive-deps
  const gone = (key: K) => mode === "pane" && !paneShown(m.tracks, keys.length, specs[key].side, shown[key]);
  const resizer = (key: K) =>
    gone(key) || resizeLocked(specs[key].type, resizeLocks) ? null : (
      <PaneResizer
        key={key}
        paneKey={key}
        spec={specs[key]}
        now={shown[key]}
        room={rooms[key]}
        root={root}
        mode={mode}
        inset={specs[key].side === "left" ? m.insetLeft : m.insetRight}
      />
    );
  return { root: { ref: setRoot, style }, widths, resizer, resizers: <>{keys.map((k) => resizer(k))}</> };
}

interface ResizerProps {
  paneKey: string;
  spec: PaneSpec;
  /** The width the pane shows: the stored one or the stylesheet's (at its breakpoint), fitted. */
  now: number;
  room: number;
  root: HTMLElement | null;
  mode: "pane" | "column";
  /** The root's padding and half gap on the pane's side: the divider's distance past the pane. */
  inset: number;
}

export function PaneResizer(p: ResizerProps) {
  const { spec, paneKey, room, root, mode } = p;
  const drag = useRef<{ x: number; w: number; live: number; moved: boolean } | null>(null);
  const [dragging, setDragging] = useState(false);
  // A divider that goes mid-drag (a lock turned on, a breakpoint that drops its pane) never sees
  // its pointerup: the app leaves resize mode as it goes, not only on a release.
  useEffect(() => {
    if (!dragging) return;
    return () => document.body.classList.remove("presizing");
  }, [dragging]);
  // A screen whose root scrolls (Today, Knowledge health) would scroll its dividers away with its
  // first screenful: each is placed at the scroll position, one view tall, as the root scrolls.
  const self = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!root || mode !== "pane") return;
    const follow = () => {
      if (!self.current) return;
      self.current.style.top = `${root.scrollTop}px`;
      self.current.style.bottom = `${-root.scrollTop}px`;
    };
    follow();
    root.addEventListener("scroll", follow, { passive: true });
    return () => root.removeEventListener("scroll", follow);
  }, [root, mode]);

  const show = (w: number) => root?.style.setProperty(spec.cssVar, `${w}px`);
  const commit = (w: number) => {
    const v = setPaneWidth(paneKey, w, room);
    show(v);
  };
  const reset = () => {
    resetPaneWidth(paneKey);
    root?.style.removeProperty(spec.cssVar);
  };

  const down = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { x: e.clientX, w: p.now, live: p.now, moved: false };
    setDragging(true);
    document.body.classList.add("presizing");
  };
  const move = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const dx = e.clientX - d.x;
    if (!d.moved && Math.abs(dx) < 2) return;
    d.moved = true;
    d.live = clampPaneWidth(spec, d.w + (spec.side === "left" ? dx : -dx), room);
    show(d.live);
  };
  const up = () => {
    const d = drag.current;
    drag.current = null;
    setDragging(false);
    document.body.classList.remove("presizing");
    if (d?.moved) commit(d.live);
  };
  // A key the divider acts on stops here: a screen's own keys (the Inbox's Enter applies the
  // suggestion) listen on the window, and must not act on it too.
  const key = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Enter" && e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Enter") {
      reset();
      return;
    }
    const step = e.shiftKey ? PANE_STEP_BIG : PANE_STEP;
    const dir = (e.key === "ArrowRight" ? 1 : -1) * (spec.side === "left" ? 1 : -1);
    commit(p.now + dir * step);
  };

  const at: CSSProperties =
    mode === "column"
      ? {}
      : spec.side === "left"
        ? { left: `calc(var(${spec.cssVar}) + ${p.inset}px - 3px)` }
        : { right: `calc(var(${spec.cssVar}) + ${p.inset}px - 3px)` };
  return (
    <div
      ref={self}
      role="separator"
      aria-orientation="vertical"
      aria-label={`Resize ${spec.label}`}
      aria-valuenow={Math.round(p.now)}
      aria-valuemin={spec.min}
      aria-valuemax={Math.round(Math.max(spec.min, Math.min(spec.max, room)))}
      tabIndex={0}
      title={`Drag to resize ${spec.label}. Arrow keys move it; Enter or a double-click puts it back.`}
      className={`presize ${mode} ${dragging ? "dragging" : ""}`}
      data-edge={spec.side === "left" ? "end" : "start"}
      style={at}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      onLostPointerCapture={up}
      onDoubleClick={reset}
      onKeyDown={key}
    />
  );
}
