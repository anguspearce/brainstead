// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The app's chrome isn't selectable (src/styles.css sets user-select: none on the body), but
// what people may want to copy is: a task's line, an Inbox item, an assistant's answer, a quote.
// Much of that text sits in a row that does something on a click (a task row selects its task,
// a second click lets it go), so selecting it would also click the row: dragging across the text
// ends in a click on it, and a double-click to pick a word is two. A click that ends a drag, or
// one of a double-click, while text is selected is the selection's, not the row's.

/** How far the pointer moves between press and release before the click counts as a drag. */
const DRAG_PX = 4;

/** Whether a click is the end of selecting text rather than a click on what's under it. */
export function isSelectingClick(
  down: { x: number; y: number } | null,
  click: { x: number; y: number; detail: number },
  selected: string,
): boolean {
  if (!selected) return false;
  if (click.detail >= 2) return true;
  return !!down && Math.hypot(click.x - down.x, click.y - down.y) > DRAG_PX;
}

let started = false;

/** Keep the clicks that select text from also clicking the row the text is in. */
export function guardSelectingClicks(): void {
  if (started || typeof document === "undefined") return;
  started = true;
  let down: { x: number; y: number } | null = null;
  document.addEventListener("pointerdown", (e) => (down = { x: e.clientX, y: e.clientY }), true);
  document.addEventListener(
    "click",
    (e) => {
      const t = e.target as HTMLElement | null;
      // Buttons, links and fields keep their clicks: their text isn't what's being selected.
      if (t?.closest("button, a, input, textarea, select, [contenteditable]:not([contenteditable='false'])")) return;
      if (!isSelectingClick(down, { x: e.clientX, y: e.clientY, detail: e.detail }, window.getSelection()?.toString() ?? "")) return;
      e.stopPropagation();
    },
    true,
  );
}
