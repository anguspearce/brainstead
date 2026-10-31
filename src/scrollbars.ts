// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// On Windows, scrollbars stay hidden until the pointer is over their pane or it scrolls
// (src/styles.css); a Mac keeps its native ones and none of this runs there. The stylesheet can't go by :hover alone: WebView2 matches it but doesn't repaint the bar until
// something else restyles the element. So the panes under the pointer carry data-pointer-in, and
// one that scrolls carries data-scrolling until it has been still for a second. Attributes rather
// than classes, which React would overwrite on its next render.
//
// Only elements with something to scroll are marked. Marking every element under the pointer
// would touch the editor's lines on each move, and CodeMirror watches attributes inside
// .cm-content: each one is a DOM sync for the editor, which then strips the attribute again.
//
// Where Windows is set to show scroll bars all the time (Settings › Accessibility › Visual
// effects › Always show scrollbars), the root carries data-scrollbars="always" and every thumb
// shows. That's read at start-up and again whenever the window takes the focus, as it does on
// coming back from Windows' settings.

import { api } from "./api";
import { isWindows } from "./platform";

/** The attributes this sets, which are window state: rich copy strips them (src/notes/actions.ts). */
export const SCROLL_STATE_ATTRS = ["data-pointer-in", "data-scrolling"] as const;

const HOLD_MS = 1000;
const timers = new WeakMap<Element, number>();
let under: Element[] = [];
let started = false;

/** Whether `el` has more content than it shows, across or down. */
const scrolls = (el: Element) => el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth;

/** Marks the panes among `path` (the pointer's event path, innermost first) that scroll, and
 *  unmarks the ones the pointer has left. */
export function pointerAt(path: readonly EventTarget[]) {
  const now = path.filter((n): n is Element => n instanceof Element && scrolls(n));
  for (const el of under) if (!now.includes(el)) el.removeAttribute("data-pointer-in");
  for (const el of now) if (!under.includes(el)) el.setAttribute("data-pointer-in", "");
  under = now;
}

/** Marks `el` as scrolling until it has been still for a second. */
export function scrolled(el: Element) {
  el.setAttribute("data-scrolling", "");
  window.clearTimeout(timers.get(el));
  timers.set(
    el,
    window.setTimeout(() => el.removeAttribute("data-scrolling"), HOLD_MS),
  );
}

/** Shows every scrollbar all the time (`always`), as Windows does, or only while in use. */
export function applySystemScrollbars(always: boolean) {
  if (always) document.documentElement.setAttribute("data-scrollbars", "always");
  else document.documentElement.removeAttribute("data-scrollbars");
}

/** Asks Windows whether it shows scroll bars all the time, and puts the answer in force. */
async function readSystemScrollbars() {
  try {
    applySystemScrollbars(await api.alwaysShowScrollbars());
  } catch {
    // No backend (a test, a plain browser): scrollbars hide until used.
  }
}

/** Starts following the pointer and scrolling in this window, and Windows' own preference; a
 *  second call does nothing, and off Windows the first does nothing either. */
export function showScrollbarsWhileScrolling() {
  if (started || !isWindows) return;
  started = true;
  void readSystemScrollbars();
  window.addEventListener("focus", () => void readSystemScrollbars());
  const opts = { capture: true, passive: true };
  document.addEventListener("pointerover", (e) => pointerAt(e.composedPath()), opts);
  // Not captured: pointerleave doesn't bubble, so without capture this hears only the pointer
  // leaving the window, not every element it crosses on the way.
  document.documentElement.addEventListener("pointerleave", () => pointerAt([]), { passive: true });
  // Captured, because scroll doesn't bubble from the pane that scrolled.
  document.addEventListener("scroll", (e) => scrolled(e.target instanceof Element ? e.target : document.documentElement), opts);
}
