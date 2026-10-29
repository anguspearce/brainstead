// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The platform the page runs on, and the keyboard conventions that follow from it.
// Shortcuts are written with the macOS glyphs (⌘ ⌥ ⇧ ⌃) everywhere in the source; keyLabel turns
// them into words on other platforms: ⌘ is Ctrl, ⌥ is Alt, and a note's View, Edit and Source,
// ⌥⌘1 to ⌥⌘3, are Ctrl+Shift+digit (Ctrl+Alt is AltGr on many layouts, and Alt alone opens the menu bar).
// The capture shortcut default ⌃⌥Space is Ctrl+Shift+Space for the same reason.

export type Platform = "macos" | "windows" | "linux";

function detect(): Platform {
  if (typeof navigator === "undefined") return "macos";
  const ua = `${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`;
  if (/Mac|darwin/i.test(ua)) return "macos";
  if (/Win/i.test(ua)) return "windows";
  return "linux";
}

let current: Platform = detect();

/** The platform the page runs on. */
export const platform = (): Platform => current;
export let isMac = current === "macos";
export let isWindows = current === "windows";
/** Teams and Outlook sync works where there's a secret store for the sign-in: macOS (Keychain) and
 *  Windows (Credential Manager), not Linux yet. */
export let hasSync = isMac || isWindows;
/** ⌘ on macOS, Ctrl elsewhere. */
export let modLabel = isMac ? "⌘" : "Ctrl";
/** The file manager's name, for "Show in …". */
export let fileManager = isMac ? "Finder" : isWindows ? "Explorer" : "Files";
/** How prose names the command-line app: "Terminal" on macOS, "a terminal" elsewhere. */
export let terminalApp = isMac ? "Terminal" : "a terminal";

/** Overrides the detected platform. For tests and for the backend's own report. */
export function setPlatform(p: Platform) {
  current = p;
  isMac = p === "macos";
  isWindows = p === "windows";
  hasSync = isMac || isWindows;
  modLabel = isMac ? "⌘" : "Ctrl";
  fileManager = isMac ? "Finder" : isWindows ? "Explorer" : "Files";
  terminalApp = isMac ? "Terminal" : "a terminal";
  if (typeof document !== "undefined") document.documentElement.dataset.platform = p;
}

/**
 * A vault file's full path, for convertFileSrc and Templater's `tp.file.path`: `rel` (written with
 * `/`) under `root`, spelled with the root's own separator, so a Windows vault (`C:\Notes`) gets
 * `C:\Notes\img\a.png`, not a mix of `\` and `/`. A trailing separator on the root and leading
 * ones on `rel` don't double up.
 */
export function vaultFile(root: string, rel: string): string {
  const windows = /^([A-Za-z]:\\|\\\\)/.test(root) || (root.includes("\\") && !root.includes("/"));
  const r = rel.replace(/^[\\/]+/, "");
  const base = root.replace(/[\\/]+$/, "");
  return windows ? `${base}\\${r.replace(/\//g, "\\")}` : `${base}/${r}`;
}

/**
 * `data-tauri-drag-region`, spread onto an element, where the window has no title bar (macOS and
 * Windows; Linux keeps its own) and is dragged by these instead. Tauri's drag script acts on a
 * press on the element itself, not on its children, so a bar's buttons and fields keep their
 * clicks; the element itself no longer scrolls or takes clicks, so it goes on empty space only.
 */
export const dragRegion = (): { "data-tauri-drag-region"?: true } => (isMac || isWindows ? { "data-tauri-drag-region": true } : {});

/** Sets `data-platform` on the root element, which the stylesheet reads. */
export function applyPlatform() {
  if (typeof document !== "undefined") document.documentElement.dataset.platform = current;
}

type ModEvent = { metaKey: boolean; ctrlKey: boolean };

/** The platform's command modifier: ⌘ on macOS, Ctrl elsewhere. */
export const isMod = (e: ModEvent) => (isMac ? e.metaKey : e.ctrlKey);

/**
 * True when the event carries a Control press that is not the command modifier. On macOS that
 * is a real ⌃; elsewhere Ctrl is the command modifier, so there is no stray one.
 */
export const strayCtrl = (e: ModEvent) => isMac && e.ctrlKey;

/**
 * True for the ⌥⌘digit modifiers (a note's View, Edit and Source): ⌥⌘ on macOS, Ctrl+Shift
 * elsewhere. The caller reads the digit from `e.code`, because Shift changes `e.key` on most layouts.
 */
export const isOptCmd = (e: ModEvent & { altKey: boolean; shiftKey: boolean }) =>
  isMac ? e.metaKey && e.altKey && !e.ctrlKey && !e.shiftKey : e.ctrlKey && e.shiftKey && !e.altKey && !e.metaKey;

/**
 * Stops WebView2's own handling of Ctrl shortcuts (find, print, reload, go to) when the app
 * handles the key. A no-op on macOS, where ⌘ has no page-level default the app does not own.
 */
export function claim(e: Event) {
  if (!isMac) e.preventDefault();
}

// WebView2 has browser shortcuts the app does not use (reload, print, find, go to, view source, zoom).
// Where Ctrl is the command modifier they would otherwise fire on a key press the app ignores.
const BROWSER_CTRL_KEYS = new Set(["r", "p", "g", "f", "u", "o", "s", "d", "j", "h", "k", "n", "=", "+", "-", "0"]);
const BROWSER_PLAIN_KEYS = new Set(["F3", "F5", "F7"]);

/**
 * Cancels the webview's own shortcuts so only the app's handlers act on them. Handlers still run:
 * this only prevents the default. Does nothing on macOS, where the webview has no such shortcuts.
 */
export function blockBrowserKeys(): () => void {
  if (isMac || typeof window === "undefined") return () => {};
  const k = (e: KeyboardEvent) => {
    if (BROWSER_PLAIN_KEYS.has(e.key) || (e.ctrlKey && !e.altKey && BROWSER_CTRL_KEYS.has(e.key.toLowerCase()))) e.preventDefault();
  };
  window.addEventListener("keydown", k);
  return () => window.removeEventListener("keydown", k);
}

const KEYS: Record<string, string> = { "↩": "Enter", "⌫": "Backspace", "⌦": "Delete", "⇥": "Tab", "⎋": "Esc", "␣": "Space" };
const GLYPH_KEYS = /[↩⌫⌦⇥⎋␣]/g;

/**
 * A shortcut written with macOS glyphs, as the current platform labels it. Unchanged on macOS.
 * Elsewhere "⇧⌘R" is "Ctrl+Shift+R", "⌥⌘1" is "Ctrl+Shift+1" (a note's View), "⌥⌘C" is
 * "Ctrl+Alt+C", "⌃⌥Space" is "Ctrl+Shift+Space" (the capture shortcut) and "⌥-click" is "Alt-click".
 * Return, delete and similar key glyphs become words wherever they stand.
 */
export function keyLabel(s: string): string {
  if (isMac) return s;
  return s
    .replace(/([⌃⌥⇧⌘]+)(-click|[A-Za-z0-9]+|[^\sA-Za-z0-9])?/g, (_m, mods: string, rest: string | undefined) => {
      const has = (g: string) => mods.includes(g);
      const ctrl = has("⌃") || has("⌘");
      const screen = has("⌥") && (has("⌃") || (has("⌘") && /^[0-9]$/.test(rest ?? "")));
      const words: string[] = [];
      if (ctrl) words.push("Ctrl");
      if (has("⌥") && !screen) words.push("Alt");
      if (has("⇧") || screen) words.push("Shift");
      const head = words.join("+");
      if (!rest) return head;
      if (rest === "-click") return `${head}-click`;
      return `${head}+${rest}`;
    })
    .replace(GLYPH_KEYS, (g) => KEYS[g]);
}

/** The capture shortcut a fresh install registers, in the global shortcut plugin's terms. */
export const captureDefault = () => (isMac ? "Control+Alt+Space" : "Control+Shift+Space");

const ACCEL_GLYPHS: Record<string, string> = {
  control: "⌃",
  ctrl: "⌃",
  alt: "⌥",
  option: "⌥",
  shift: "⇧",
  super: "⌘",
  command: "⌘",
  cmd: "⌘",
  meta: "⌘",
  cmdorctrl: "⌘",
};
const ACCEL_WORDS: Record<string, string> = { control: "Ctrl", ctrl: "Ctrl", alt: "Alt", option: "Alt", shift: "Shift", cmdorctrl: "Ctrl" };

/**
 * A global shortcut in the plugin's terms ("Control+Alt+Space") as the platform writes it:
 * "⌃⌥Space" on macOS, "Ctrl+Alt+Space" elsewhere, where Super is the Windows key.
 */
export function accelLabel(spec: string): string {
  if (isMac)
    return spec
      .split("+")
      .map((p) => ACCEL_GLYPHS[p.toLowerCase()] ?? p)
      .join("");
  const parts = spec.split("+").map((p) => (/^key./i.test(p) && p.length === 4 ? p.slice(3) : p));
  const superName = isWindows ? "Win" : "Super";
  return parts
    .map((p) => {
      const k = p.toLowerCase();
      return ACCEL_WORDS[k] ?? (["super", "command", "cmd", "meta"].includes(k) ? superName : p);
    })
    .join("+");
}
