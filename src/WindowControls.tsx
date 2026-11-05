// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The window's own minimise, maximise and close buttons, at the top right on Windows, where the
// main window has no native title bar (src-tauri/tauri.windows.conf.json). They're where Windows
// puts its own and as wide, and as tall as the top bar, so their glyphs sit on its centre line.
// Close goes through the window's close request, the same path as the native X:
// src-tauri/src/lib.rs hides the window or quits (asking about unsaved edits first), never a hard
// close. Nothing here renders on macOS,
// where the window keeps its traffic lights, or on Linux, which keeps its native title bar.
//
// Windows 11 offers its snap layouts over the maximise button only to a window that answers the
// system's hit-test there, which the page can't: src-tauri/src/platform/snap_win.rs puts a window
// of its own over the button, told where it is here, and reports when the pointer is on it (the
// page gets no hover of its own there) and does the click.
//
// The glyphs are the ones Windows 11 draws in its own caption buttons: Segoe Fluent Icons at 10px
// (Segoe MDL2 Assets on Windows 10, the same code points), with drawn paths where neither is installed.

import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { isWindows } from "./platform";

// Whether either icon font is installed: a missing font measures as the fallback does.
let hasIconFont: boolean | null = null;
function iconFont(): boolean {
  if (hasIconFont !== null) return hasIconFont;
  const c = document.createElement("canvas").getContext("2d");
  if (!c) return (hasIconFont = false);
  const width = (font: string) => {
    c.font = font;
    return c.measureText("").width;
  };
  const fallback = width("10px monospace");
  hasIconFont = ["Segoe Fluent Icons", "Segoe MDL2 Assets"].some((f) => width(`10px "${f}", monospace`) !== fallback);
  return hasIconFont;
}

const Glyph = ({ code, d }: { code: string; d: string }) =>
  iconFont() ? (
    <span className="wglyph" aria-hidden="true">
      {code}
    </span>
  ) : (
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" shapeRendering="crispEdges">
      <path d={d} fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );

export function WindowControls() {
  const [maximised, setMaximised] = useState(false);
  const [hover, setHover] = useState(false);
  const max = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!isWindows) return;
    const w = getCurrentWindow();
    // The snap-layout window goes over the button, in the window's physical pixels.
    const place = () => {
      const b = max.current?.getBoundingClientRect();
      const r = window.devicePixelRatio || 1;
      void api
        .windowMaxButton(b ? [Math.round(b.left * r), Math.round(b.top * r), Math.round(b.width * r), Math.round(b.height * r)] : null)
        .catch(() => {});
    };
    const read = () => {
      void w.isMaximized().then(setMaximised, () => {});
      place();
    };
    read();
    window.addEventListener("resize", read);
    // The button's own size changes too (the bar's height, the page's zoom) without the window's.
    const sized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(place);
    if (max.current) sized?.observe(max.current);
    const off = [w.onResized(read), w.onScaleChanged(read), api.onMaxButtonHover(setHover)].map((o) => o.catch(() => () => {}));
    return () => {
      sized?.disconnect();
      window.removeEventListener("resize", read);
      off.forEach((o) => void o.then((f) => f()));
      void api.windowMaxButton(null).catch(() => {});
    };
  }, []);
  if (!isWindows) return null;
  const w = () => getCurrentWindow();
  return (
    <div className="winctl" role="group" aria-label="Window">
      <button type="button" tabIndex={-1} title="Minimise" aria-label="Minimise" onClick={() => void w().minimize()}>
        <Glyph code={""} d="M0 5.5h10" />
      </button>
      <button
        ref={max}
        type="button"
        tabIndex={-1}
        className={hover ? "hover" : undefined}
        title={maximised ? "Restore" : "Maximise"}
        aria-label={maximised ? "Restore" : "Maximise"}
        onClick={() => void w().toggleMaximize()}
      >
        {maximised ? <Glyph code={""} d="M0.5 2.5h7v7h-7zM2.5 2.5v-2h7v7h-2" /> : <Glyph code={""} d="M0.5 0.5h9v9h-9z" />}
      </button>
      <button type="button" tabIndex={-1} className="close" title="Close" aria-label="Close" onClick={() => void w().close()}>
        <Glyph code={""} d="M0 0l10 10M10 0L0 10" />
      </button>
    </div>
  );
}
