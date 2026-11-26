// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Small global stores read with useSyncExternalStore. Settings are loaded from Rust once and sent
// back debounced; nothing is sent until the first read has succeeded, so a failed read never
// overwrites settings.json with defaults. Pending saves are flushed when the window hides, loses
// focus or unloads.

import { useSyncExternalStore } from "react";
import { api, DEFAULT_SETTINGS, Settings } from "./api";

type Listener = () => void;

/** A value with subscribers. */
export class Store<T> {
  private listeners = new Set<Listener>();
  constructor(private value: T) {}
  get = (): T => this.value;
  set = (v: T) => {
    if (Object.is(v, this.value)) return;
    this.value = v;
    this.listeners.forEach((l) => l());
  };
  subscribe = (l: Listener) => {
    this.listeners.add(l);
    return () => {
      this.listeners.delete(l);
    };
  };
}

export function useStore<T>(s: Store<T>): T {
  return useSyncExternalStore(s.subscribe, s.get);
}

/** Settings and their save cycle. `write` is the Rust call; tests pass their own. */
export class SettingsStore extends Store<Settings> {
  loaded = false;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private due: Settings | null = null;
  /** The settings as Rust last took them: what the store goes back to when it refuses a change. */
  private saved: Settings | null = null;
  /** Set while the screenshot scene runs: changes show but aren't saved. */
  frozen = false;

  constructor(
    private write: (s: Settings) => Promise<Settings>,
    private delay = 300,
  ) {
    super(DEFAULT_SETTINGS);
  }

  loadFrom(s: Settings) {
    this.loaded = true;
    this.set({ ...DEFAULT_SETTINGS, ...s });
    this.saved = this.get();
  }

  update(patch: Partial<Settings>) {
    const next = { ...this.get(), ...patch };
    this.set(next);
    if (!this.loaded || this.frozen) return;
    this.due = next;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.flush(), this.delay);
  }

  /** Sends a pending change now. Rust's cleaned-up copy (trimmed folder names) comes back. */
  async flush(): Promise<void> {
    clearTimeout(this.timer);
    const d = this.due;
    this.due = null;
    if (!d) return;
    try {
      const saved = await this.write(d);
      this.saved = { ...DEFAULT_SETTINGS, ...saved };
      settingsError.set(null);
      // Only adopt it if nothing changed meanwhile.
      if (this.get() === d) this.set(saved);
    } catch (e) {
      settingsError.set(String(e));
      console.error("settings", e);
      // Rust took none of it: the screens go back to what it has, or the refused value would ride
      // along with every later change and each of those would be refused too.
      clearTimeout(this.timer);
      this.due = null;
      if (this.saved) this.set(this.saved);
    }
  }

  /** Saves at once, for changes that act straight away (choosing the vault). */
  async commit(patch: Partial<Settings>): Promise<void> {
    this.update(patch);
    await this.flush();
  }
}

export const settings = new SettingsStore((s) => api.settingsWrite(s));
/** The last error from saving settings (an invalid folder name), shown in Settings. */
export const settingsError = new Store<string | null>(null);

export function installFlushers() {
  const f = () => void settings.flush();
  window.addEventListener("blur", f);
  window.addEventListener("beforeunload", f);
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) f();
  });
}

export const READ_SIZE = { min: 12, max: 24, default: 15 };

/** The document text size, kept within range. */
export function applyReadSize(px: number | undefined) {
  const v = Math.max(READ_SIZE.min, Math.min(READ_SIZE.max, Math.round(px ?? READ_SIZE.default)));
  document.documentElement.style.setProperty("--read-size", `${v}px`);
  return v;
}

/** Applies the theme to the page (and remembers it for the splash, which paints before React). */
export function applyTheme(t: Settings["theme"]) {
  if (t === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.setAttribute("data-theme", t);
  try {
    localStorage.setItem("theme", t);
  } catch {
    // Disposable: the splash just follows the system then.
  }
}
