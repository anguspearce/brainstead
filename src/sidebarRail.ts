// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The sidebar as an icon rail: its rows as icons with their badges, names and keys in their
// tooltips, the sections' headings as thin rules. On a narrow window (RAIL_BELOW CSS px or less,
// which Interface size counts in: the webview's own zoom makes the window narrower in CSS px) it
// starts as a rail; on a wider one it starts in full. ⌘\ (Ctrl+\ on Windows, the View menu's
// Toggle Sidebar) and the button at its top switch it for the width the window is at, and each
// width's choice is kept on its own (`sidebarRail.wide`, `sidebarRail.narrow`), so the rail a narrow
// window starts with never changes how a wide one shows (D-20261119-12).

import { api, Settings } from "./api";
import { settings, Store, useStore } from "./store";

/** A window this many CSS px wide or narrower is narrow: the narrow scenes (1000 px) and a
 *  1040 px window at 100% (the minimum is 800 px, D-20261120-01), and the usual 1320 px window from 120% up. */
export const RAIL_BELOW = 1100;

const query = `(max-width: ${RAIL_BELOW}px)`;
const media = typeof window !== "undefined" && window.matchMedia ? window.matchMedia(query) : null;

/** Whether the window is narrow now. */
export const narrowWindow = new Store(!!media?.matches);
media?.addEventListener?.("change", (e) => narrowWindow.set(e.matches));

export type RailWidth = "wide" | "narrow";

/** Whether the sidebar is a rail on a window of this width: a narrow one unless its choice is off,
 *  a wide one only when its choice is on. */
export function railOn(s: Pick<Settings, "sidebarRail">, narrow: boolean): boolean {
  return narrow ? s.sidebarRail?.narrow !== false : s.sidebarRail?.wide === true;
}

/** Keeps one width's choice; the usual one (full when wide, a rail when narrow) leaves no key, so
 *  neither choice made leaves no `sidebarRail` in settings.json. */
export function setRail(width: RailWidth, on: boolean) {
  const next = { ...settings.get().sidebarRail, [width]: on };
  if (next.wide === false) delete next.wide;
  if (next.narrow === true) delete next.narrow;
  settings.update({ sidebarRail: Object.keys(next).length ? next : undefined });
}

/** The width the window is at, as the toggle and the settings tool name it. */
export const railWidth = (): RailWidth => (narrowWindow.get() ? "narrow" : "wide");

/** ⌘\ and the sidebar's button: the other way for the width the window is at. */
export function toggleRail() {
  const w = railWidth();
  setRail(w, !railOn(settings.get(), w === "narrow"));
}

/** Whether the sidebar shows as a rail now. */
export function useRail(): boolean {
  const s = useStore(settings);
  const narrow = useStore(narrowWindow);
  return railOn(s, narrow);
}

/** The View menu's Toggle Sidebar (src-tauri/src/menu.rs; on Windows, its key through src/noteMenu.ts). */
export function startRailMenu() {
  return api.onMenu((id) => {
    if (id === "view:sidebar") toggleRail();
  });
}
