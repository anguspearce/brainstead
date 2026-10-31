// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { accentCss, APP_ACCENT, contrast, deriveAccent, hueOf, luminance } from "./accentTints";

/** Hue distance in degrees, round the circle. */
const hueGap = (a: number, b: number) => Math.min(Math.abs(a - b), 360 - Math.abs(a - b));

// The Windows default blue, a yellow (Windows' Gold) and a dark purple.
const ACCENTS = { blue: "#0078d4", yellow: "#ffb900", purple: "#4b1a7a" };

describe("the accent tints from a system colour", () => {
  for (const [name, colour] of Object.entries(ACCENTS)) {
    for (const theme of ["light", "dark"] as const) {
      it(`keep WCAG AA and the hue for ${name} in ${theme}`, () => {
        const t = deriveAccent(colour)![theme];
        const app = APP_ACCENT[theme];
        expect(contrast(t.onAccent, t.accent)).toBeGreaterThanOrEqual(4.5);
        for (const bg of [...app.grounds, t.accentSoft]) expect(contrast(t.accentInk, bg)).toBeGreaterThanOrEqual(4.5);
        expect(contrast(app.ink, t.accentSoft)).toBeGreaterThanOrEqual(4.5);
        if (theme === "dark") for (const bg of app.grounds) expect(contrast(t.accent, bg)).toBeGreaterThanOrEqual(3);
        // The lightness moves, the hue doesn't.
        expect(hueGap(hueOf(t.accent), hueOf(colour))).toBeLessThan(3);
        expect(hueGap(hueOf(t.accentInk), hueOf(colour))).toBeLessThan(3);
        // Light text on a dark page, dark on a light one, as today.
        expect(luminance(t.accentInk) < luminance(t.accentSoft)).toBe(theme === "light");
      });
    }
  }

  it("uses the Windows default blue as it is, in both themes", () => {
    const t = deriveAccent(ACCENTS.blue)!;
    expect(t.light.accent).toBe("#0078d4");
    expect(t.dark.accent).toBe("#0078d4");
  });

  it("darkens a yellow that white text can't sit on, and lightens a dark purple in dark", () => {
    const y = deriveAccent(ACCENTS.yellow)!;
    expect(luminance(y.light.accent)).toBeLessThan(luminance(ACCENTS.yellow));
    const p = deriveAccent(ACCENTS.purple)!;
    expect(p.light.accent).toBe(ACCENTS.purple);
    expect(luminance(p.dark.accent)).toBeGreaterThan(luminance(ACCENTS.purple));
  });

  it("gives each tint today's contrast against the page where AA already holds", () => {
    const t = deriveAccent(ACCENTS.blue)!;
    for (const theme of ["light", "dark"] as const) {
      const app = APP_ACCENT[theme];
      const page = app.grounds[2];
      for (const k of ["accentSoft", "ring"] as const)
        expect(Math.abs(contrast(t[theme][k], page) - contrast(app[k], page))).toBeLessThan(0.1);
    }
  });

  it("refuses what isn't a colour", () => {
    expect(deriveAccent("blue")).toBeNull();
  });

  it('writes the tokens for data-accent="system" under each theme\'s selector', () => {
    const css = accentCss(deriveAccent(ACCENTS.blue)!);
    expect(css).toContain(':root[data-accent="system"] { --accent: #0078d4;');
    expect(css).toContain('@media (prefers-color-scheme: dark) { :root[data-accent="system"]:not([data-theme="light"])');
    expect(css).toContain(':root[data-accent="system"][data-theme="dark"]');
    expect(css).toContain(':root[data-accent="system"] .docsurf[data-doc-theme="dark"]');
  });

  it("keeps the app's tokens in step with styles.css", () => {
    const css = readFileSync("src/styles.css", "utf8");
    const block = (sel: string) => css.slice(css.indexOf(sel), css.indexOf("}", css.indexOf(sel)));
    const vars = { accent: "accent", accentInk: "accent-ink", accentSoft: "accent-soft", onAccent: "on-accent", ring: "ring", ink: "ink" };
    const grounds = ["ground", "side", "surface", "sunk"];
    for (const [theme, sel] of [
      ["light", ":root {"],
      ["dark", ':root[data-theme="dark"] {'],
    ] as const) {
      const b = block(sel);
      const v = (name: string) => new RegExp(`--${name}: (#[0-9a-f]+);`).exec(b)?.[1];
      for (const [k, name] of Object.entries(vars)) expect(v(name), `${theme} --${name}`).toBe(APP_ACCENT[theme][k as keyof typeof vars]);
      expect(grounds.map(v)).toEqual(APP_ACCENT[theme].grounds);
    }
  });
});
