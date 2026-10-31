// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The divider between two panes, dragged to resize the fixed one (src/panes.ts has the panes and
// their ranges). A screen calls usePanes with its panes and puts the hook's ref and style on its
// grid root and its resizers inside it: each is a 6px hit area over the 1px divider, nothing at rest
// and a 2px accent line while hovered, focused or dragged. A drag writes the pane's custom property
// on the root as the pointer moves (no re-render per move) and keeps the width on release; arrow
// keys move it PANE_STEP (Shift: PANE_STEP_BIG), Home and End to its least and most; Enter or a
// double-click puts the stylesheet's width back. A table's columns (src/FileList.tsx, the Trash,
// Activity's log, Doc check's register) use the same with mode "column": the resizer sits inside
// the column's header cell (role columnheader, named by its heading), on the edge away from the
// table's one flexible column (a column before it at its right edge, a column after it at
// its left edge), so the divider moves with the pointer whichever side of the flexible column it is.
// A type locked in Settings (src/panes.ts, resizeLocks) draws no resizers; its widths still apply.

import {
  CSSProperties,
  KeyboardEvent,
  PointerEvent,
  ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
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
  /** Each pane's width as the grid has it now, by its custom property (the one set on the root, else
   *  the stylesheet's): its track's width, where the grid still has its column (paneShown). */
  current: Record<string, number>;
}

const unmeasured: Measured = { width: Infinity, insetLeft: 0, insetRight: 0, gap: 0, tracks: null, defaults: {}, current: {} };

function measure(el: HTMLElement, vars: string[]): Measured {
  // Each pane's own width in the stylesheet, read with the width usePanes put on the root taken off
  // for the moment (synchronously, so nothing is drawn without it).
  const defaults: Record<string, number> = {};
  const current: Record<string, number> = {};
  for (const v of vars) {
    const inline = el.style.getPropertyValue(v);
    if (inline) el.style.removeProperty(v);
    defaults[v] = parseFloat(getComputedStyle(el).getPropertyValue(v));
    if (inline) el.style.setProperty(v, inline);
    current[v] = inline ? parseFloat(inline) : defaults[v];
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
    current,
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
  /** One pane's resizer, to place by hand: in a table, inside the column's own header cell. */
  resizer: (key: K) => ReactNode;
  /** Every pane's resizer, for the root. */
  resizers: ReactNode;
}

/** What a screen's panes show for one measure of its root: each pane's width and room, the panes a
 *  breakpoint dropped, and the custom properties to set on the root. */
interface Layout<K extends string> {
  shown: Record<K, number>;
  rooms: Record<K, number>;
  gone: Set<K>;
  style: CSSProperties;
}

const drawKey = <K extends string>(l: Layout<K>) => JSON.stringify([l.style, [...l.gone]]);

/** A root's latest measure, kept outside React state: each divider follows it on its own, and the
 *  screen re-renders only when what it draws (the root's properties, the panes shown) changes. */
class Live<K extends string> {
  m = unmeasured;
  private subs = new Set<() => void>();
  constructor(private layout: (m: Measured) => Layout<K>) {}
  subscribe = (f: () => void) => {
    this.subs.add(f);
    return () => void this.subs.delete(f);
  };
  get = () => this.m;
  /** The screen's layout as its last render has it (the widths stored then). */
  drawWith(layout: (m: Measured) => Layout<K>) {
    this.layout = layout;
  }
  /** Takes a new measure, tells the dividers, and says whether the screen draws something else. */
  take(next: Measured): boolean {
    if (JSON.stringify(next) === JSON.stringify(this.m)) return false;
    const before = drawKey(this.layout(this.m));
    this.m = next;
    this.subs.forEach((f) => f());
    return drawKey(this.layout(next)) !== before;
  }
}

/** A screen's panes: `specs` by key (from PANES). */
export function usePanes<K extends string>(specs: Record<K, PaneSpec>, opts: PanesOptions = {}): Panes<K> {
  const { paneWidths: widthsStored, resizeLocks } = useStore(settings);
  // The root as state, not a ref: the resizers take it as a prop, in render.
  const [root, setRoot] = useState<HTMLElement | null>(null);
  const [, redraw] = useReducer((n: number) => n + 1, 0);
  const keys = Object.keys(specs) as K[];
  const varList = keys.map((k) => specs[k].cssVar).join(" ");
  const mode = opts.mode ?? "pane";
  const fixed = opts.fixed ?? 0;
  const layout = (m: Measured): Layout<K> => {
    const widths = {} as Record<K, number | undefined>;
    const shown = {} as Record<K, number>;
    const rooms = {} as Record<K, number>;
    const style: Record<string, string> = { position: "relative" };
    // A pane a breakpoint dropped from the grid (the Weekly review's side column at 868px) has no
    // divider and takes no room from the panes still shown.
    const gone = new Set(
      keys.filter((k) => {
        if (mode !== "pane") return false;
        const now = m.current[specs[k].cssVar];
        return !paneShown(m.tracks, keys.length, specs[k].side, Number.isFinite(now) ? now : specs[k].def);
      }),
    );
    const here = keys.filter((k) => !gone.has(k));
    const want = (k: K) => {
      const stored = storedPaneWidth(widthsStored, k);
      const def = m.defaults[specs[k].cssVar];
      return { spec: specs[k], want: stored ?? (Number.isFinite(def) ? def : specs[k].def), stored: stored !== undefined };
    };
    // Every shown pane's width, stored or the stylesheet's, fitted to the room together: a default
    // too wide for a narrow screen (a wide sidebar beside it, say) is narrowed as a stored one is. A
    // grid has a gap beside each fixed pane it shows; a table's are in `fixed`.
    const room = paneRoom(m.width, fixed + (mode === "pane" ? m.gap * here.length : 0));
    const wants = here.map(want);
    const fit = fitPanes(wants, room);
    const total = fit.reduce((a, b) => a + b, 0);
    here.forEach((k, i) => {
      shown[k] = fit[i];
      rooms[k] = room - (total - fit[i]);
      // Set on the root where it's stored, or narrower than the stylesheet's; else the stylesheet's shows.
      widths[k] = wants[i].stored || fit[i] < wants[i].want ? fit[i] : undefined;
    });
    // A dropped pane keeps its stored width, within its range, for when its column comes back.
    gone.forEach((k) => {
      const w = want(k);
      shown[k] = clampPaneWidth(w.spec, w.want);
      rooms[k] = Infinity;
      widths[k] = w.stored ? shown[k] : undefined;
    });
    keys.forEach((k) => {
      if (widths[k] !== undefined) style[specs[k].cssVar] = `${widths[k]}px`;
    });
    return { shown, rooms, gone, style: style as CSSProperties };
  };
  const [live] = useState(() => new Live<K>(layout));
  useLayoutEffect(() => {
    live.drawWith(layout);
  });
  // A size change reaches the dividers at once; the screen re-renders only when what it draws
  // changes (a default narrowed to fit, a pane dropped), not on every frame of a window resize or a
  // sidebar drag, nor when the root only grows taller.
  const sync = useCallback(
    (el: HTMLElement) => {
      if (live.take(measure(el, varList.split(" ")))) redraw();
    },
    [live, varList],
  );
  useLayoutEffect(() => {
    if (!root) return;
    sync(root);
    if (typeof ResizeObserver === "undefined") return;
    let width = root.clientWidth;
    const ro = new ResizeObserver(() => {
      if (root.clientWidth === width) return;
      width = root.clientWidth;
      sync(root);
    });
    ro.observe(root);
    return () => ro.disconnect();
  }, [root, sync]);
  const { gone, style } = layout(live.m);
  // The grid's tracks change with the widths set here: measured again after each, so a pane's
  // divider shows exactly while its column does.
  const styleKey = JSON.stringify(style);
  useLayoutEffect(() => {
    if (root && mode === "pane") sync(root);
  }, [styleKey]); // eslint-disable-line react-hooks/exhaustive-deps
  const resizer = (key: K) =>
    gone.has(key) || resizeLocked(specs[key].type, resizeLocks) ? null : (
      <PaneResizer key={key} paneKey={key} spec={specs[key]} live={live} layout={layout} root={root} mode={mode} />
    );
  // Left to right, so Tab reaches a screen's dividers in the order they're drawn.
  const inOrder = [...keys.filter((k) => specs[k].side === "left"), ...keys.filter((k) => specs[k].side === "right")];
  return { root: { ref: setRoot, style }, resizer, resizers: <>{inOrder.map((k) => resizer(k))}</> };
}

interface ResizerProps<K extends string> {
  paneKey: K;
  spec: PaneSpec;
  /** The root's latest measure, followed as the root resizes, and the screen's layout of it. */
  live: Live<K>;
  layout: (m: Measured) => Layout<K>;
  root: HTMLElement | null;
  mode: "pane" | "column";
}

export function PaneResizer<K extends string>({ spec, paneKey, live, layout, root, mode }: ResizerProps<K>) {
  const m = useSyncExternalStore(live.subscribe, live.get);
  const l = layout(m);
  const p = {
    // The width the pane shows: the stored one or the stylesheet's (at its breakpoint), fitted.
    now: l.shown[paneKey],
    room: l.rooms[paneKey],
    // The root's padding and half gap on the pane's side: the divider's distance past the pane.
    inset: spec.side === "left" ? m.insetLeft : m.insetRight,
  };
  const { room } = p;
  // `was`: the root's own value for the pane's property when the drag began ("" for none).
  const drag = useRef<{ x: number; w: number; live: number; moved: boolean; was: string } | null>(null);
  const [dragging, setDragging] = useState(false);
  // A divider that goes mid-drag (a lock turned on, a breakpoint that drops its pane) never sees
  // its pointerup: the app leaves resize mode as it goes, not only on a release, and the width the
  // drag drew but never kept goes back to the one the root had.
  useEffect(() => {
    if (!dragging) return;
    return () => {
      document.body.classList.remove("presizing");
      const d = drag.current;
      if (!d || !root) return;
      if (d.was) root.style.setProperty(spec.cssVar, d.was);
      else root.style.removeProperty(spec.cssVar);
    };
  }, [dragging, root, spec.cssVar]);
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
    drag.current = { x: e.clientX, w: p.now, live: p.now, moved: false, was: root?.style.getPropertyValue(spec.cssVar) ?? "" };
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
    if (!["Enter", "ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Enter") {
      reset();
      return;
    }
    // Home and End: the pane at its least and at its most, as a separator's value goes.
    if (e.key === "Home" || e.key === "End") {
      commit(e.key === "Home" ? spec.min : spec.max);
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
