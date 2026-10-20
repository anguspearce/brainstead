// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// How documents look: a theme (type and heading style) and a colour, light or dark on its own.
// Settings › Notes sets the defaults; a note can override them from its own screen (kept in
// settings by path, never in the note). Brainstead (the default) is the app's own reading style; Business is the previous app's (ui/src/hooks/
// use-doc-theme.ts, use-doc-accent.ts). Light or dark holds for every document and follows the
// app again when the app's theme changes. The CSS is "Document look" in src/styles.css; PDFs
// always print light.

import { useEffect, useState } from "react";
import type { Settings } from "./api";
import { settings, useStore } from "./store";

export const DOC_ACCENTS: [id: string, label: string, swatch: string][] = [
  ["brainstead", "Brainstead blue", "#1d5fd6"],
  ["blue", "Dark blue", "#143a5e"],
  ["lightblue", "Light blue", "#0e6ba8"],
  ["green", "Green", "#1e5631"],
  ["orange", "Orange", "#9a4b08"],
  ["yellow", "Yellow", "#7a5c00"],
  ["red", "Red", "#8a1f2b"],
  ["gray", "Gray", "#374151"],
  ["darkgray", "Dark gray", "#1f2937"],
];

/** id, name, what it's like, and the typeface its sample is set in. */
export const DOC_STYLES: [id: string, name: string, blurb: string, font: string][] = [
  [
    "brainstead",
    "Brainstead",
    "The app's own reading style.",
    '"Geist Variable", -apple-system, "Segoe UI Variable Text", "Segoe UI", sans-serif',
  ],
  ["business", "Business", "Coloured headings and rules, banded tables.", '"Avenir Next", "Helvetica Neue", sans-serif'],
  ["editorial", "Editorial", "A serif for long reading, quiet headings, generous spacing.", 'Charter, "Iowan Old Style", Georgia, serif'],
  [
    "modern",
    "Modern",
    "Large, tight headings, an accent bar, rounded tables.",
    '"Geist Variable", -apple-system, "Segoe UI Variable Text", "Segoe UI", sans-serif',
  ],
  [
    "minimal",
    "Minimal",
    "System type, colour only on links and the title rule.",
    '-apple-system, "SF Pro Text", "Helvetica Neue", "Segoe UI Variable Text", "Segoe UI", sans-serif',
  ],
  ["report", "Report", "Numbered sections, small-caps headings, filled table headers.", '"Helvetica Neue", Helvetica, Arial, sans-serif'],
  [
    "technical",
    "Technical",
    "Monospaced headings, gridded tables, code to the fore.",
    '-apple-system, "SF Pro Text", "Segoe UI Variable Text", "Segoe UI", sans-serif',
  ],
];

const known = (list: [string, ...unknown[]][], id: string | undefined, dflt: string) => (id && list.some(([x]) => x === id) ? id : dflt);

function useSystemDark() {
  const q = typeof matchMedia === "function" ? matchMedia("(prefers-color-scheme: dark)") : null;
  const [dark, setDark] = useState(!!q?.matches);
  useEffect(() => {
    if (!q) return;
    const f = () => setDark(q.matches);
    q.addEventListener("change", f);
    return () => q.removeEventListener("change", f);
  }, [q]);
  return dark;
}

type Look = { style?: string; accent?: string };

/** The defaults from Settings › Notes, in these settings. */
export const docDefaults = (s: Settings) => ({
  style: known(DOC_STYLES, s.docStyle, "brainstead"),
  accent: known(DOC_ACCENTS, s.docAccent, "brainstead"),
});

/** The notes' own looks with `patch` made to the one at `path`: a choice that matches the
 *  default isn't kept, and a note left with none is dropped. */
export function withOwnLook(all: Record<string, Look>, path: string, patch: Look, dflt: { style: string; accent: string }) {
  const next = { ...all[path], ...patch };
  if (next.style === dflt.style) delete next.style;
  if (next.accent === dflt.accent) delete next.accent;
  const out = { ...all };
  if (next.style || next.accent) out[path] = next;
  else delete out[path];
  return out;
}

/** How the note at `path` looks: its own theme and colour where it has them, else the defaults. */
export function lookOf(s: Settings, path: string) {
  const dflt = docDefaults(s);
  const own = s.docLooks?.[path];
  return {
    style: known(DOC_STYLES, own?.style, dflt.style),
    accent: known(DOC_ACCENTS, own?.accent, dflt.accent),
    own: !!(own?.style || own?.accent),
  };
}

/** The defaults from Settings › Notes. */
export function useDocDefaults() {
  return docDefaults(useStore(settings));
}

/** How the document at `path` looks: its own choice where it has one, else the defaults. */
export function useDocLook(path: string) {
  const s = useStore(settings);
  const dflt = useDocDefaults();
  const systemDark = useSystemDark();
  const appDark = s.theme === "dark" || (s.theme === "system" && systemDark);
  const own = s.docLooks?.[path];
  const setOwn = (patch: Look) => settings.update({ docLooks: withOwnLook(settings.get().docLooks ?? {}, path, patch, dflt) });
  const theme = (s.docTheme ?? (appDark ? "dark" : "light")) as "light" | "dark";
  return {
    /** Light or dark unlike the app, so the document needs its own background. */
    ownTheme: theme !== (appDark ? "dark" : "light"),
    theme: (s.docTheme ?? (appDark ? "dark" : "light")) as "light" | "dark",
    style: known(DOC_STYLES, own?.style, dflt.style),
    accent: known(DOC_ACCENTS, own?.accent, dflt.accent),
    /** The note has its own theme or colour. */
    overridden: !!(own?.style || own?.accent),
    toggleTheme: () => settings.update({ docTheme: (s.docTheme ?? (appDark ? "dark" : "light")) === "dark" ? "light" : "dark" }),
    setStyle: (style: string) => setOwn({ style }),
    setAccent: (accent: string) => setOwn({ accent }),
    useDefaults: () => setOwn({ style: dflt.style, accent: dflt.accent }),
  };
}

/** A note's own look follows it when it's renamed. */
export function moveDocLook(from: string, to: string) {
  const cur = settings.get().docLooks;
  if (!cur?.[from]) return;
  const { [from]: own, ...rest } = cur;
  settings.update({ docLooks: { ...rest, [to]: own } });
}

/** Whether the body opens with its own H1 (the previous app's bodyStartsWithH1): then the title
 *  above it isn't printed, so a PDF doesn't show it twice. Skips the properties; looks only at
 *  the first 20 lines. */
export function bodyStartsWithH1(src: string): boolean {
  let scan = src.replace(/^\uFEFF/, "");
  if (scan.startsWith("---")) {
    const end = scan.indexOf("\n---", 3);
    if (end >= 0) scan = scan.slice(end + 4);
  }
  for (const line of scan.split("\n").slice(0, 20)) {
    const t = line.trim();
    if (t) return /^#\s+\S/.test(t);
  }
  return false;
}
