// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Shared pieces: tooltips, popovers, dialogs, switches and segmented controls (from a sibling app's ui.tsx).

import { ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./icons";

export const LAYER_LABEL: Record<string, string> = { note: "Note", wiki: "Wiki", source: "Source", template: "Template" };

/** A file's layer as Search shows it: a square in the layer's colour, then its name. After a file's
 *  name, in a list (`.lbt` holds the two). */
export function LayerTag({ layer }: { layer: string }) {
  return <span className={`lb ${layer}`}>{LAYER_LABEL[layer] ?? layer}</span>;
}

/** A bar pinned at the top of its scroller with position: sticky (a list's search and filters): a
 *  ref that keeps the scroller's --tb-head at the bar's height, so the list's bands stick under it
 *  however it wraps. */
export function usePinnedBar() {
  const [el, setEl] = useState<HTMLElement | null>(null);
  useLayoutEffect(() => {
    const box = el?.parentElement;
    if (!el || !box) return;
    const set = () => box.style.setProperty("--tb-head", `${el.offsetHeight}px`);
    set();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(set);
    ro.observe(el);
    return () => {
      ro.disconnect();
      box.style.removeProperty("--tb-head");
    };
  }, [el]);
  return setEl;
}

/** Search delay after the last keystroke: typing only updates the box, and the list redraws once. */
export const SEARCH_DEBOUNCE_MS = 250;

/** The search field used everywhere: a magnifier, the input, anything extra (a match counter), and ✕ to clear. */
export function SearchBox({
  value,
  onChange,
  placeholder,
  onKeyDown,
  inputRef,
  className = "",
  children,
  escAnywhere = true,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
  onKeyDown?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  inputRef?: React.Ref<HTMLInputElement>;
  className?: string;
  children?: ReactNode;
  /** Esc clears the box from anywhere on the page, not only while typing in it. */
  escAnywhere?: boolean;
}) {
  // Esc anywhere clears what's typed: not while typing in another field (or in the box, whose own
  // keys handle it), with a dialog open, or when something else already took the key.
  const latest = useRef(onChange);
  useEffect(() => {
    latest.current = onChange;
  });
  useEffect(() => {
    if (!value || !escAnywhere) return;
    const k = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;
      if ((e.target as HTMLElement)?.closest?.("input, textarea, select, [contenteditable]")) return;
      if (document.querySelector(".scrim, [role='dialog']")) return;
      latest.current("");
      e.preventDefault();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [value, escAnywhere]);
  return (
    <label className={`inp searchbox ${className}`}>
      <Icon name="search" size={className.includes("lg") ? 16 : 14} />
      <input
        ref={inputRef}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        aria-label={placeholder}
        spellCheck={false}
        autoCorrect="off"
        autoCapitalize="off"
      />
      {children}
      {value && (
        <button
          type="button"
          className="ibtn xs clear"
          aria-label="Clear search"
          title="Clear the search"
          onMouseDown={(e) => e.preventDefault()}
          onClick={(e) => {
            onChange("");
            e.currentTarget.parentElement?.querySelector("input")?.focus();
          }}
        >
          <Icon name="x" size={11} />
        </button>
      )}
    </label>
  );
}

/** `value` once it has stopped changing for `ms`. Emptying it takes effect at once (clearing a search). */
export function useDebounced<T>(value: T, ms = SEARCH_DEBOUNCE_MS): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    if (value === "" || ms <= 0) {
      setV(value);
      return;
    }
    const t = window.setTimeout(() => setV(value), ms);
    return () => window.clearTimeout(t);
  }, [value, ms]);
  return value === "" ? value : v;
}

/** Tooltips everywhere, styled like the app's popovers in place of macOS's plain ones. Any element
 *  with a title (or data-tip, or an icon-only button's aria-label) gets one: the title moves to
 *  data-tip so the native tooltip doesn't show as well. Shown after a short pause, below the
 *  element (above it near the bottom of the window; under the pointer on a wide one, such as a row), or beside it in the sidebar; gone on leaving,
 *  clicking or scrolling. */
/** Wider than this, a tooltip is placed under the pointer rather than at the element's middle. */
const WIDE = 240;

export function Tooltips() {
  const [tip, setTip] = useState<{ text: string; x: number; y: number; side: "below" | "above" | "right" } | null>(null);
  useEffect(() => {
    let timer: number | undefined;
    let el: HTMLElement | null = null;
    let pointer = 0;
    const move = (e: MouseEvent) => {
      pointer = e.clientX;
    };
    const hide = () => {
      window.clearTimeout(timer);
      el = null;
      setTip(null);
    };
    const over = (e: MouseEvent) => {
      pointer = e.clientX;
      const target = e.target as HTMLElement;
      const t = (target.closest?.("[title], [data-tip]") ??
        target.closest?.("button[aria-label], [role=button][aria-label]")) as HTMLElement | null;
      if (t === el) return;
      hide();
      if (!t) return;
      if (t.title) {
        t.dataset.tip = t.title;
        t.removeAttribute("title");
      }
      const text = t.dataset.tip || (!t.textContent?.trim() ? t.getAttribute("aria-label") : null);
      if (!text) return;
      el = t;
      timer = window.setTimeout(() => {
        if (el !== t || !t.isConnected || document.documentElement.dataset.scene) return; // none in screenshots
        const r = t.getBoundingClientRect();
        // A wide element (a row) gets its tip where the pointer is, not at its middle.
        const at = r.width > WIDE ? pointer : r.left + r.width / 2;
        const x = Math.max(150, Math.min(at, window.innerWidth - 150));
        if (t.closest(".side")) setTip({ text, side: "right", x: r.right + 8, y: r.top + r.height / 2 });
        else if (r.bottom + 44 > window.innerHeight) setTip({ text, side: "above", x, y: r.top - 6 });
        else setTip({ text, side: "below", x, y: r.bottom + 6 });
      }, 450);
    };
    // The pointer left the page: off the window, or onto a window over it (on Windows, the one over
    // the maximise button, src-tauri/src/platform/snap_win.rs).
    const out = (e: MouseEvent) => {
      if (!e.relatedTarget) hide();
    };
    document.addEventListener("mouseover", over);
    document.addEventListener("mouseout", out);
    document.addEventListener("mousemove", move, { passive: true });
    document.addEventListener("mousedown", hide, true);
    document.addEventListener("scroll", hide, true);
    window.addEventListener("blur", hide);
    return () => {
      hide();
      document.removeEventListener("mouseover", over);
      document.removeEventListener("mouseout", out);
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mousedown", hide, true);
      document.removeEventListener("scroll", hide, true);
      window.removeEventListener("blur", hide);
    };
  }, []);
  if (!tip) return null;
  const style: React.CSSProperties =
    tip.side === "right"
      ? { left: tip.x, top: tip.y, transform: "translateY(-50%)" }
      : { left: tip.x, top: tip.y, transform: tip.side === "above" ? "translate(-50%, -100%)" : "translateX(-50%)" };
  return (
    <div className="tip" role="tooltip" style={style}>
      {tip.text}
    </div>
  );
}

/** Closes on Escape or a click outside. */
export function useDismiss(ref: React.RefObject<HTMLElement | null>, onClose: () => void, active = true) {
  useEffect(() => {
    if (!active) return;
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    const down = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener("keydown", key, true);
    const t = window.setTimeout(() => window.addEventListener("mousedown", down), 0);
    return () => {
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("mousedown", down);
      window.clearTimeout(t);
    };
  }, [ref, onClose, active]);
}

/** A floating card placed below (or above, or beside) an anchor rectangle and kept on screen. */
export function Popover({
  anchor,
  onClose,
  children,
  width = 320,
  place = "below",
}: {
  anchor: DOMRect;
  onClose: () => void;
  children: ReactNode;
  width?: number;
  place?: "below" | "above" | "right";
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number; maxHeight?: number }>({ left: anchor.left, top: anchor.bottom + 8 });
  useDismiss(ref, onClose);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const placeIt = () => {
      // Its whole height, even while it's capped and scrolling.
      const h = el.scrollHeight;
      const w = Math.min(width, window.innerWidth - 24);
      const H = window.innerHeight;
      let left = place === "right" ? anchor.right + 8 : anchor.left + anchor.width / 2 - w / 2;
      left = Math.max(12, Math.min(left, window.innerWidth - w - 12));
      if (place === "right") {
        // Beside the anchor, moved up as far as it needs to fit; scrolling when the window is shorter.
        const top = Math.max(12, Math.min(anchor.top - 12, H - 12 - h));
        return setPos({ left, top, maxHeight: h > H - 24 ? H - 24 : undefined });
      }
      // Below or above, on the side asked for if it fits, else the other, else the roomier one,
      // never past the window: what doesn't fit scrolls inside it.
      const below = H - anchor.bottom - 8 - 12;
      const above = anchor.top - 8 - 12;
      const under = place === "above" ? !(h <= above) && (h <= below || below > above) : h <= below || (!(h <= above) && below >= above);
      const room = under ? below : above;
      const fit = Math.min(h, room);
      setPos({ left, top: under ? anchor.bottom + 8 : anchor.top - 8 - fit, maxHeight: h > room ? room : undefined });
    };
    placeIt();
    // Placed again when what's inside grows or shrinks (a list that loads after it opened).
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(placeIt);
    for (const c of el.children) ro.observe(c);
    return () => ro.disconnect();
  }, [anchor, width, place, children]);
  // On the body: a fixed card inside a blurred bar (the editor's toolbar) would be placed
  // relative to the bar, not the window.
  return createPortal(
    <div
      ref={ref}
      className="popover"
      style={{
        left: pos.left,
        top: pos.top,
        width: Math.min(width, window.innerWidth - 24),
        maxHeight: pos.maxHeight,
        overflowY: pos.maxHeight ? "auto" : undefined,
      }}
    >
      {children}
    </div>,
    document.body,
  );
}

export function Dialog({ onClose, children, width, label }: { onClose: () => void; children: ReactNode; width: number; label: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useDismiss(ref, onClose);
  return (
    <div className="scrim">
      <div ref={ref} role="dialog" aria-label={label} className="dialog" style={{ width }}>
        {children}
      </div>
    </div>
  );
}

export function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      title={`${label}: ${on ? "on" : "off"}; click to turn it ${on ? "off" : "on"}`}
      className="tg"
      onClick={() => onChange(!on)}
    />
  );
}

export function Seg<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  /** Value, label, and a tooltip; without one the button's title is its label. */
  options: [T, string, string?][];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div className="seg" role="radiogroup" aria-label={label}>
      {options.map(([v, l, tip]) => (
        <button
          key={v}
          type="button"
          role="radio"
          aria-checked={v === value}
          title={tip ?? l}
          className={v === value ? "on" : ""}
          onClick={() => onChange(v)}
        >
          {l}
        </button>
      ))}
    </div>
  );
}

/** "2 min ago", "just now", "yesterday": for the index status line. */
export function ago(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 45) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  return d === 1 ? "yesterday" : `${d} days ago`;
}

export const fmtCount = (n: number) => n.toLocaleString("en-GB");

/** Sizes in binary units, as the previous app shows them: "1.5 KB", "105.4 MB". */
export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const u = ["KB", "MB", "GB", "TB"];
  let v = n / 1024,
    i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${u[i]}`;
}

/** A table's group band (.tb-band): an icon, the group's name and a pill with its count. Sticks
 *  under the table's header while the group's rows scroll. `none` is the catch-all group. */
export function TableBand({
  icon,
  label,
  n,
  tip,
  none,
  lead,
}: {
  /** Before the icon: a select-all checkbox, say. */
  lead?: ReactNode;
  icon?: string;
  label: ReactNode;
  n: number;
  /** What the count counts, for its tooltip: "3 tasks in this project". */
  tip: string;
  none?: boolean;
}) {
  return (
    <div className={`tb-band ${none ? "none" : ""}`} role="row">
      <span role="rowheader">
        {lead}
        {icon && <Icon name={icon} size={13} />}
        {label}
      </span>
      <span className="n" title={tip}>
        {fmtCount(n)}
      </span>
    </div>
  );
}

/** A choice from a long list, found by typing: a button showing what's chosen (like a select),
 *  that opens a search box over the list. ↑↓ move, ↩ picks, Esc closes. */
export function SearchPick({
  items,
  value,
  onChange,
  placeholder,
  label,
  className = "",
}: {
  items: { value: string; label: string; hint?: string }[];
  value: string | null;
  onChange: (v: string) => void;
  /** Shown when nothing is chosen, and in the search box. */
  placeholder: string;
  /** What it chooses, for its tooltip and screen readers: "Document to check". */
  label: string;
  className?: string;
}) {
  const [at, setAt] = useState<DOMRect | null>(null);
  const [q, setQ] = useState("");
  const [i, setI] = useState(0);
  const list = useRef<HTMLDivElement>(null);
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  const hits = items.filter((it) => words.every((w) => `${it.label} ${it.hint ?? ""}`.toLowerCase().includes(w)));
  const cur = items.find((it) => it.value === value);
  const close = () => {
    setAt(null);
    setQ("");
    setI(0);
  };
  const choose = (v: string) => {
    onChange(v);
    close();
  };
  useEffect(() => {
    list.current?.querySelector<HTMLElement>(`[data-i="${i}"]`)?.scrollIntoView({ block: "nearest" });
  }, [i]);
  return (
    <>
      <button
        type="button"
        className={`sel spick ${className}`}
        aria-label={label}
        title={cur ? `${label}: ${cur.label}` : `${label}: type to find one`}
        onClick={(e) => setAt(e.currentTarget.getBoundingClientRect())}
      >
        <span className={`ell ${cur ? "" : "faint"}`}>{cur?.label ?? placeholder}</span>
        <Icon name="chevdown" size={12} />
      </button>
      {at && (
        <Popover anchor={at} onClose={close} width={Math.max(320, at.width)}>
          <div className="spickbox">
            <SearchBox
              value={q}
              onChange={(v) => {
                setQ(v);
                setI(0);
              }}
              placeholder={placeholder}
              escAnywhere={false}
              inputRef={(el) => el?.focus()}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") setI((x) => Math.min(hits.length - 1, x + 1));
                else if (e.key === "ArrowUp") setI((x) => Math.max(0, x - 1));
                else if (e.key === "Enter" && hits[i]) choose(hits[i].value);
                else if (e.key === "Escape") close();
                else return;
                e.preventDefault();
              }}
            />
            <div className="menu spicklist" ref={list} role="listbox" aria-label={label}>
              {hits.map((it, n) => (
                <button
                  key={it.value}
                  type="button"
                  role="option"
                  data-i={n}
                  aria-selected={it.value === value}
                  className={`${n === i ? "kcur" : ""} ${it.value === value ? "on" : ""}`}
                  title={it.hint ?? it.label}
                  onMouseEnter={() => setI(n)}
                  onClick={() => choose(it.value)}
                >
                  <span className="ell">{it.label}</span>
                  {it.hint && <span className="faint small ell">{it.hint}</span>}
                </button>
              ))}
              {!hits.length && <p className="faint small mpad">Nothing matches “{q}”.</p>}
            </div>
          </div>
        </Popover>
      )}
    </>
  );
}
