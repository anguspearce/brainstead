// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

/**
 * Guards one-key shortcuts that act on something (the Inbox's choices) against typing meant for
 * somewhere else: a password typed for a system prompt while the app had focus, or keys that reach
 * the window just as it comes back to the front.
 *
 * A key is stray while the window has had focus for less than `SETTLE_MS`, or when it comes within
 * `BURST_MS` of the key before it (someone typing a word, not choosing).
 */
export const SETTLE_MS = 1500;
export const BURST_MS = 300;

export type StrayState = { focusedAt: number; lastKey: number };

/** Whether a key pressed at `now` is stray; records it as the last key either way. */
export function strayKey(s: StrayState, now: number): boolean {
  const stray = now - s.focusedAt < SETTLE_MS || now - s.lastKey < BURST_MS;
  s.lastKey = now;
  return stray;
}

const state: StrayState = { focusedAt: -Infinity, lastKey: -Infinity };
const stray = new WeakSet<KeyboardEvent>();

// Tests press keys in quick succession on purpose; the rule itself is tested through strayKey.
if (typeof window !== "undefined" && !import.meta.env.VITEST) {
  const back = () => {
    state.focusedAt = performance.now();
  };
  window.addEventListener("focus", back);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") back();
  });
  // Every key is judged before any handler sees it, and counts towards a burst even if another takes it.
  window.addEventListener(
    "keydown",
    (e) => {
      if (e.repeat) return;
      if (strayKey(state, performance.now())) stray.add(e);
    },
    true,
  );
}

/** Whether this key press looks like typing meant for somewhere else, so a shortcut ignores it. */
export const isStrayKey = (e: KeyboardEvent): boolean => stray.has(e);
