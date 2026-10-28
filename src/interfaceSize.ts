// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Interface size (Settings › General › Appearance): the whole main window at a percentage, 80 to
// 150 in steps of 10. It's the webview's own zoom (WebView2's zoom factor on Windows, WKWebView's
// page zoom on macOS), so the sidebar, lists, dialogs and note text grow together and canvases
// (the graph, PDFs) stay sharp; the stylesheet's px sizes are untouched. Text size (readSize) is
// separate and comes on top: 120% with 18 px text shows a 21.6 px body. Kept per machine in app
// settings, as readSize is. The capture and menu-bar windows are sized to their content and stay at 100%.

import { api } from "./api";
import { settings } from "./store";

export const INTERFACE_SIZE = { min: 80, max: 150, step: 10, default: 100 };

/** The nearest size the setting allows: a whole step, within range; the default when unset. */
export function clampInterfaceSize(pct: number | undefined): number {
  const n = Number(pct);
  if (!Number.isFinite(n)) return INTERFACE_SIZE.default;
  const step = Math.round(n / INTERFACE_SIZE.step) * INTERFACE_SIZE.step;
  return Math.max(INTERFACE_SIZE.min, Math.min(INTERFACE_SIZE.max, step));
}

/** Zooms this window to `pct`, kept within range, and returns the size in force. */
export async function applyInterfaceSize(pct: number | undefined): Promise<number> {
  const v = clampInterfaceSize(pct);
  try {
    const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    await getCurrentWebviewWindow().setZoom(v / 100);
  } catch (e) {
    console.error("interface size", e);
  }
  return v;
}

/** Sets the interface size: the window zooms at once and the setting is saved. */
export async function setInterfaceSize(pct: number): Promise<number> {
  const v = await applyInterfaceSize(pct);
  settings.update({ interfaceSize: v });
  return v;
}

/** One step bigger or smaller, or the usual size, as the View menu's items and their keys do. */
export function stepInterfaceSize(dir: 1 | -1 | 0): Promise<number> {
  const cur = clampInterfaceSize(settings.get().interfaceSize);
  return setInterfaceSize(dir === 0 ? INTERFACE_SIZE.default : cur + dir * INTERFACE_SIZE.step);
}

/** The View menu's items (src-tauri/src/menu.rs; on Windows, their keys through src/noteMenu.ts). */
export function startInterfaceSizeMenu() {
  return api.onMenu((id) => {
    if (id === "view:interface-bigger") void stepInterfaceSize(1);
    else if (id === "view:interface-smaller") void stepInterfaceSize(-1);
    else if (id === "view:interface-usual") void stepInterfaceSize(0);
  });
}
