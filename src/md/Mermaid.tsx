// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Mermaid diagrams, loaded only when a document has one. Strict security level, renders one at a
// time and cached by source, pan by dragging, zoom with ⌘-scroll or the buttons, double-click to
// fit (the previous app's MermaidDiagram.tsx). Full screen shows it over the whole window; Escape
// or its button closes it. On Windows it starts below the window's own title bar (styles.css,
// .scrim.mfull), whose buttons would otherwise sit on the diagram's, and a press on those buttons
// or on the bar leaves it open.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Icon } from "../icons";
import { keepOpen, useDismiss } from "../ui";

type MermaidApi = typeof import("mermaid").default;
let loading: Promise<MermaidApi> | null = null;
let queue: Promise<unknown> = Promise.resolve();
const cache = new Map<string, string>();
let seq = 0;

function dark() {
  const t = document.documentElement.dataset.theme;
  return t === "dark" || (t !== "light" && window.matchMedia?.("(prefers-color-scheme: dark)").matches);
}

async function render(src: string): Promise<string> {
  const key = `${dark() ? "d" : "l"}:${src}`;
  const hit = cache.get(key);
  if (hit) return hit;
  loading ??= import("mermaid").then((m) => m.default);
  const mermaid = await loading;
  // One at a time: mermaid keeps global state while it renders.
  const job = queue.then(async () => {
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: dark() ? "dark" : "neutral",
      fontFamily: "Geist Variable, sans-serif",
    });
    const { svg } = await mermaid.render(`mmd-${++seq}`, src);
    if (cache.size > 200) cache.delete(cache.keys().next().value!);
    cache.set(key, svg);
    return svg;
  });
  queue = job.catch(() => {});
  return job;
}

export function Mermaid({ src, full, onClose }: { src: string; full?: boolean; onClose?: () => void }) {
  const [svg, setSvg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [view, setView] = useState({ x: 0, y: 0, k: 1 });
  /** The box's height: the diagram's own once it's drawn, up to 480px, so a small one leaves no gap. */
  const [height, setHeight] = useState(320);
  const box = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    let live = true;
    setErr(null);
    if (!src.trim()) {
      setErr("Empty diagram");
      return;
    }
    render(src)
      .then((s) => live && setSvg(s))
      .catch((e) => live && setErr(String(e?.message ?? e)));
    return () => {
      live = false;
    };
  }, [src]);

  const fit = () => {
    const b = box.current,
      i = inner.current?.firstElementChild as SVGSVGElement | null;
    if (!b || !i) return;
    const r = i.getBoundingClientRect();
    const w = r.width / view.k,
      h = r.height / view.k;
    // Full screen fits the window, and may grow a small diagram to fill it.
    const k = full
      ? Math.min(3, (b.clientWidth - 48) / Math.max(1, w), (b.clientHeight - 48) / Math.max(1, h))
      : Math.min(1, b.clientWidth / Math.max(1, w), 480 / Math.max(1, h));
    setView({ x: (b.clientWidth - w * k) / 2, y: full ? (b.clientHeight - h * k) / 2 : 12, k });
    if (!full) setHeight(Math.max(80, Math.round(h * k) + 24));
  };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useLayoutEffect(fit, [svg]);

  const zoom = (f: number, cx?: number, cy?: number) =>
    setView((v) => {
      const k = Math.max(0.2, Math.min(5, v.k * f));
      const b = box.current!;
      const px = cx ?? b.clientWidth / 2,
        py = cy ?? b.clientHeight / 2;
      return { k, x: px - ((px - v.x) * k) / v.k, y: py - ((py - v.y) * k) / v.k };
    });

  if (err)
    return (
      <div className="mermaid err">
        <div className="faint">Couldn't draw this diagram: {err}</div>
        <pre className="mermaid-source">{src}</pre>
      </div>
    );
  // The full-screen copy sits beside the box, not in it: React events bubble through a portal,
  // so panning there would pan this one too.
  return (
    <>
      <div
        ref={box}
        className={`mermaid${full ? " full" : ""}`}
        style={full ? undefined : { height }}
        onWheel={(e) => {
          if (!e.metaKey && !e.ctrlKey) return;
          e.preventDefault();
          const r = box.current!.getBoundingClientRect();
          zoom(e.deltaY < 0 ? 1.15 : 1 / 1.15, e.clientX - r.left, e.clientY - r.top);
        }}
        onPointerDown={(e) => {
          if ((e.target as HTMLElement).closest("button")) return;
          const sx = e.clientX,
            sy = e.clientY,
            v0 = view;
          const move = (m: PointerEvent) => setView({ ...v0, x: v0.x + m.clientX - sx, y: v0.y + m.clientY - sy });
          const up = () => {
            window.removeEventListener("pointermove", move);
            window.removeEventListener("pointerup", up);
          };
          window.addEventListener("pointermove", move);
          window.addEventListener("pointerup", up);
        }}
        onDoubleClick={fit}
      >
        <div
          ref={inner}
          className="mmd"
          style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.k})` }}
          dangerouslySetInnerHTML={{ __html: svg ?? "" }}
        />
        <div className="mzoom">
          <button type="button" className="ibtn" aria-label="Zoom in" title="Zoom in" onClick={() => zoom(1.2)}>
            <Icon name="plus" size={14} />
          </button>
          <button type="button" className="ibtn" aria-label="Zoom out" title="Zoom out" onClick={() => zoom(1 / 1.2)}>
            <Icon name="minus" size={14} />
          </button>
          <button type="button" className="ibtn" aria-label="Fit" title="Fit the whole diagram in view" onClick={fit}>
            <Icon name="fit" size={14} />
          </button>
          {full ? (
            <button type="button" className="ibtn" aria-label="Close full screen" title="Close full screen (Escape)" onClick={onClose}>
              <Icon name="x" size={14} />
            </button>
          ) : (
            <button
              type="button"
              className="ibtn"
              aria-label="Full screen"
              title="Show the diagram full screen"
              onClick={() => setExpanded(true)}
            >
              <Icon name="expand" size={14} />
            </button>
          )}
        </div>
      </div>
      {expanded && <FullScreen src={src} onClose={() => setExpanded(false)} />}
    </>
  );
}

function FullScreen({ src, onClose }: { src: string; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useDismiss(ref, onClose, true, keepOpen);
  return createPortal(
    <div className="scrim mfull">
      <div ref={ref} role="dialog" aria-label="Diagram" className="mfullbox">
        <Mermaid src={src} full onClose={onClose} />
      </div>
    </div>,
    document.body,
  );
}
