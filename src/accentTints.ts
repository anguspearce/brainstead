// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The accent tokens for Appearance › Accent colour's System (src/accent.ts), from one colour (the
// Windows accent). The accent is that colour itself; the tints the stylesheet uses with it
// (--accent-ink for links and hovers, --accent-soft for the selected row, --ring for the focus ring)
// take its hue at the luminance the app's own tints have, so every contrast against text and the page is
// the one the app's own blue gives. Where a pair still falls short of WCAG AA (4.5:1 for text, 3:1
// for the accent against the page in dark), only the lightness moves: hue and chroma are kept
// (chroma lowered only to stay inside sRGB). Worked in OKLCH, where a lightness change doesn't drift
// the hue.

/** One theme's accent tokens, as styles.css names them, with the backgrounds they sit on. */
export interface AccentTokens {
  accent: string;
  accentInk: string;
  accentSoft: string;
  onAccent: string;
  ring: string;
}

/** The app's own tokens (styles.css :root and its dark block; a test keeps them in step), and the
 *  page colours the accent sits on: ground, side, surface and sunk. */
export const APP_ACCENT: Record<"light" | "dark", AccentTokens & { ink: string; grounds: string[] }> = {
  light: {
    accent: "#1d5fd6",
    accentInk: "#174fb3",
    accentSoft: "#e8f0fd",
    onAccent: "#fff",
    ring: "#a9c4f5",
    ink: "#151a22",
    grounds: ["#fafbfc", "#f2f4f7", "#fff", "#f4f6f9"],
  },
  dark: {
    accent: "#2f6fdb",
    accentInk: "#5f96f2",
    accentSoft: "#1b2535",
    onAccent: "#fff",
    ring: "#3a6bc0",
    ink: "#e8ebf0",
    grounds: ["#16181c", "#1b1e23", "#1f2228", "#1a1d21"],
  },
};

type Rgb = [number, number, number];
interface Lch {
  l: number;
  c: number;
  h: number;
}

/** A `#rgb` or `#rrggbb` colour as linear sRGB, or null. */
export function parseHex(hex: string): Rgb | null {
  let m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim())?.[1];
  if (!m) return null;
  if (m.length === 3) m = [...m].map((d) => d + d).join("");
  const lin = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
  return [0, 2, 4].map((i) => lin(parseInt(m!.slice(i, i + 2), 16) / 255)) as Rgb;
}

function toHex(rgb: Rgb): string {
  const enc = (v: number) => {
    const x = Math.max(0, Math.min(1, v));
    return Math.round((x <= 0.0031308 ? x * 12.92 : 1.055 * x ** (1 / 2.4) - 0.055) * 255);
  };
  return "#" + rgb.map((v) => enc(v).toString(16).padStart(2, "0")).join("");
}

const lumOf = (rgb: Rgb) => 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];

/** WCAG relative luminance of a hex colour. */
export function luminance(hex: string): number {
  return lumOf(parseHex(hex) ?? [0, 0, 0]);
}

/** WCAG contrast ratio between two hex colours, 1 to 21. */
export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** OKLCH of linear sRGB (Björn Ottosson's OKLab), the hue in radians. */
function toLch([r, g, b]: Rgb): Lch {
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return { l: L, c: Math.hypot(A, B), h: Math.atan2(B, A) };
}

function fromLch({ l: L, c, h }: Lch): Rgb {
  const A = c * Math.cos(h);
  const B = c * Math.sin(h);
  const l = (L + 0.3963377774 * A + 0.2158037573 * B) ** 3;
  const m = (L - 0.1055613458 * A - 0.0638541728 * B) ** 3;
  const s = (L - 0.0894841775 * A - 1.291485548 * B) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

/** The hue of a hex colour in degrees, 0 to 360 (OKLCH). */
export function hueOf(hex: string): number {
  const h = (toLch(parseHex(hex) ?? [0, 0, 0]).h * 180) / Math.PI;
  return (h + 360) % 360;
}

const inGamut = (rgb: Rgb) => rgb.every((v) => v >= -1e-4 && v <= 1 + 1e-4);

/** The colour at lightness `l` and hue `h`, its chroma lowered from `c` only as far as sRGB needs. */
function inSrgb(l: number, c: number, h: number): Rgb {
  const want = fromLch({ l, c, h });
  if (inGamut(want)) return want;
  let [lo, hi] = [0, c];
  for (let i = 0; i < 24; i++) {
    const mid = (lo + hi) / 2;
    if (inGamut(fromLch({ l, c: mid, h }))) lo = mid;
    else hi = mid;
  }
  return fromLch({ l, c: lo, h });
}

/** The colour of hue `h` and chroma up to `c` whose luminance is `y`: lightness found by halving. */
function atLuminance(y: number, c: number, h: number): string {
  let [lo, hi] = [0, 1];
  for (let i = 0; i < 32; i++) {
    const mid = (lo + hi) / 2;
    if (lumOf(inSrgb(mid, c, h)) < y) lo = mid;
    else hi = mid;
  }
  return toHex(inSrgb((lo + hi) / 2, c, h));
}

/** A contrast `hex` must keep with `bg`: at least `min`, from the side given (`below`: darker). */
interface Keep {
  bg: string;
  min: number;
  side: "below" | "above";
}

/** `hex` itself when it keeps every contrast, else the same hue and chroma at the nearest
 *  luminance that does, a margin beyond the bound so the rounding to #rrggbb can't undo it. */
function fit(hex: string, keeps: Keep[]): string {
  const ok = (x: string) => keeps.every((k) => contrast(x, k.bg) >= k.min && luminance(x) < luminance(k.bg) === (k.side === "below"));
  if (ok(hex)) return hex;
  const { c, h } = toLch(parseHex(hex)!);
  let lo = 0;
  let hi = 1;
  for (const k of keeps) {
    const yb = luminance(k.bg);
    if (k.side === "below") hi = Math.min(hi, (yb + 0.05) / k.min - 0.05);
    else lo = Math.max(lo, k.min * (yb + 0.05) - 0.05);
  }
  const y = luminance(hex);
  let out = hex;
  for (let margin = 0.0005; margin < 0.05; margin *= 2) {
    // With no luminance keeping both, halfway between misses each by least.
    const target = lo > hi ? (lo + hi) / 2 : Math.min(Math.max(y, lo + margin), hi - margin);
    out = atLuminance(target, c, h);
    if (ok(out) || lo > hi) return out;
  }
  return out;
}

/** One theme's tokens from `colour`. */
function deriveTheme(colour: string, theme: "light" | "dark"): AccentTokens {
  const app = APP_ACCENT[theme];
  const win = toLch(parseHex(colour)!);
  const ref = toLch(parseHex(app.accent)!);
  // Each tint keeps the app's own share of the accent's chroma: a muted accent gives muted tints.
  const scale = ref.c > 0 ? win.c / ref.c : 0;
  const tint = (own: string) => atLuminance(luminance(own), toLch(parseHex(own)!).c * scale, win.h);
  // Text is darker than its page in light, lighter in dark.
  const text = theme === "light" ? "below" : "above";
  const page = theme === "light" ? "above" : "below";
  const accent = fit(colour, [
    { bg: app.onAccent, min: 4.5, side: "below" },
    // A selected row's bar and a checked box on the page; in light, white text on it covers this.
    ...(theme === "dark" ? app.grounds.map((bg): Keep => ({ bg, min: 3, side: "above" })) : []),
  ]);
  const accentSoft = fit(tint(app.accentSoft), [{ bg: app.ink, min: 4.5, side: page }]);
  const accentInk = fit(
    tint(app.accentInk),
    [...app.grounds, accentSoft].map((bg): Keep => ({ bg, min: 4.5, side: text })),
  );
  return { accent, accentInk, accentSoft, onAccent: app.onAccent, ring: tint(app.ring) };
}

/** The light and dark tokens for `colour` (`#rrggbb`); null when it isn't a colour. */
export function deriveAccent(colour: string): { light: AccentTokens; dark: AccentTokens } | null {
  const rgb = parseHex(colour);
  if (!rgb) return null;
  const hex = toHex(rgb);
  return { light: deriveTheme(hex, "light"), dark: deriveTheme(hex, "dark") };
}

/** The stylesheet that puts `t` in force while the root has data-accent="system", by the same
 *  selectors as styles.css's light, system-dark and dark blocks, one step more specific, and for a
 *  note shown in the other theme (Light or dark › Documents, .docsurf[data-doc-theme]). */
export function accentCss(t: { light: AccentTokens; dark: AccentTokens }): string {
  const vars = (x: AccentTokens) =>
    `--accent: ${x.accent}; --accent-ink: ${x.accentInk}; --accent-soft: ${x.accentSoft}; --on-accent: ${x.onAccent}; --ring: ${x.ring};`;
  const root = ':root[data-accent="system"]';
  return [
    `${root} { ${vars(t.light)} }`,
    `@media (prefers-color-scheme: dark) { ${root}:not([data-theme="light"]) { ${vars(t.dark)} } }`,
    `${root}[data-theme="dark"] { ${vars(t.dark)} }`,
    `${root} .docsurf[data-doc-theme="light"] { ${vars(t.light)} }`,
    `${root} .docsurf[data-doc-theme="dark"] { ${vars(t.dark)} }`,
  ].join("\n");
}
