// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The window's own minimise, maximise and close buttons, at the top right on Windows, where the
// main window has no native title bar (src-tauri/tauri.windows.conf.json). They're the size and
// place of the native ones, so the hand finds them where Windows puts them. Close goes through the
// window's close request, the same path as the native X: src-tauri/src/lib.rs hides the window or
// quits (asking about unsaved edits first), never a hard close. Nothing here renders on macOS,
// where the window keeps its traffic lights, or on Linux, which keeps its native title bar.

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isWindows } from "./platform";

const Glyph = ({ d }: { d: string }) => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" shapeRendering="crispEdges">
    <path d={d} fill="none" stroke="currentColor" strokeWidth="1" />
  </svg>
);

export function WindowControls() {
  const [maximised, setMaximised] = useState(false);
  useEffect(() => {
    if (!isWindows) return;
    const w = getCurrentWindow();
    const read = () => void w.isMaximized().then(setMaximised, () => {});
    read();
    const off = w.onResized(read).catch(() => () => {});
    return () => void off.then((f) => f());
  }, []);
  if (!isWindows) return null;
  const w = () => getCurrentWindow();
  return (
    <div className="winctl" role="group" aria-label="Window">
      <button type="button" tabIndex={-1} title="Minimise" aria-label="Minimise" onClick={() => void w().minimize()}>
        <Glyph d="M0 5.5h10" />
      </button>
      <button
        type="button"
        tabIndex={-1}
        title={maximised ? "Restore" : "Maximise"}
        aria-label={maximised ? "Restore" : "Maximise"}
        onClick={() => void w().toggleMaximize()}
      >
        {maximised ? <Glyph d="M0.5 2.5h7v7h-7zM2.5 2.5v-2h7v7h-2" /> : <Glyph d="M0.5 0.5h9v9h-9z" />}
      </button>
      <button type="button" tabIndex={-1} className="close" title="Close" aria-label="Close" onClick={() => void w().close()}>
        <Glyph d="M0 0l10 10M10 0L0 10" />
      </button>
    </div>
  );
}
