// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Accent colour (Settings › General › Appearance): App keeps the stylesheet's own blue; System takes
// the Windows accent (platform/accent_win.rs), as Windows Settings and Terminal do, with the tints
// from src/accentTints.ts, and follows it at once when it changes in Windows. Off Windows there's no
// system colour (system_accent is null) and the setting isn't offered. Kept per machine in app
// settings, as the theme is, and applied in every window before it's drawn.

import { listen } from "@tauri-apps/api/event";
import { api, type Settings } from "./api";
import { accentCss, deriveAccent } from "./accentTints";
import { applyTheme, settings, Store } from "./store";

export type Accent = "app" | "system";

/** The system's accent colour, #rrggbb; null where there's none (off Windows) or until it's read. */
export const systemAccent = new Store<string | null>(null);

/** Puts `accent` in force: System with a system colour sets the accent tokens from it (a style
 *  element under data-accent="system"); anything else leaves the stylesheet's own. */
export function applyAccent(accent: Accent | undefined, colour = systemAccent.get()) {
  const root = document.documentElement;
  const tokens = accent === "system" && colour ? deriveAccent(colour) : null;
  let el = document.getElementById("system-accent");
  if (!tokens) {
    el?.remove();
    root.removeAttribute("data-accent");
    return;
  }
  if (!el) {
    el = document.createElement("style");
    el.id = "system-accent";
    document.head.append(el);
  }
  el.textContent = accentCss(tokens);
  // Set each time, unchanged or not: the graph's canvas repaints on the attribute (src/Graph.tsx).
  root.setAttribute("data-accent", "system");
}

/** Puts the theme and the accent colour from `s` in force. The menu-bar and quick capture windows
 *  live for the whole run, so they call this each time they open, with settings read again: a
 *  change made in the main window (or by an assistant) reaches them then. */
export function applyLook(s: Pick<Settings, "theme" | "accent">) {
  applyTheme(s.theme);
  applyAccent(s.accent);
}

/** Sets the accent colour and puts it in force, as the screen's App and System do. */
export function setAccent(accent: Accent) {
  settings.update({ accent });
  applyAccent(accent);
}

/** Reads the system's accent colour, then follows it: Windows tells every window when it changes. */
export async function startSystemAccent(): Promise<string | null> {
  const colour = await api.systemAccent().catch(() => null);
  systemAccent.set(colour);
  void listen<string>("system-accent-changed", (e) => {
    systemAccent.set(e.payload);
    applyAccent(settings.get().accent);
  }).catch(() => {});
  return colour;
}
