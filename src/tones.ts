// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The colour of a type or a tag, the same wherever it shows: a type pill's dot, a pie's slice and
// the Graph's nodes. Six tones (--pie-1 to --pie-6, chosen apart from the layers' colours, --l-*).
// The wiki's own types have fixed ones. The vault's other types (and its tags) are given theirs
// together, commonest first, each the tone its letters pick unless a commoner name has it, so the
// six commonest never share one; past six, names share.

export const TONES = ["p1", "p2", "p3", "p4", "p5", "p6"] as const;
export type Tone = (typeof TONES)[number];

/** The wiki's page types, as the Graph draws them. */
export const WIKI_TYPE_TONE: Record<string, Tone> = { entity: "p5", concept: "p3", summary: "p2", "source-summary": "p2" };

/** A name's tone: a wiki type's fixed one, else one from its letters (case ignored). */
export function toneOf(name: string, wiki = false): Tone {
  const n = name.trim().toLowerCase();
  if (wiki && WIKI_TYPE_TONE[n]) return WIKI_TYPE_TONE[n];
  let h = 5381;
  for (const c of n) h = ((h * 33) ^ c.codePointAt(0)!) >>> 0;
  return TONES[h % TONES.length];
}

/** A tone's CSS custom property, and its colour. */
export const toneVar = (t: Tone) => `--pie-${t.slice(1)}`;
export const toneColour = (t: Tone) => `var(${toneVar(t)})`;

/** Tones for a whole list of names with their counts (a vault's note types, wiki types or tags):
 *  the wiki's fixed ones first, then commonest first, each its letters' tone or the next one free.
 *  Keyed by the name in lower case. */
export function assignTones(counts: { name: string; n: number }[], wiki = false): Map<string, Tone> {
  const m = new Map<string, Tone>();
  const used = new Set<Tone>();
  const named = counts.filter((c) => c.name.trim()).sort((a, b) => b.n - a.n || a.name.localeCompare(b.name));
  if (wiki)
    for (const c of named) {
      const t = WIKI_TYPE_TONE[c.name.trim().toLowerCase()];
      if (t) {
        m.set(c.name.trim().toLowerCase(), t);
        used.add(t);
      }
    }
  for (const c of named) {
    const k = c.name.trim().toLowerCase();
    if (m.has(k)) continue;
    let i = TONES.indexOf(toneOf(k));
    if (used.size < TONES.length) while (used.has(TONES[i])) i = (i + 1) % TONES.length;
    m.set(k, TONES[i]);
    used.add(TONES[i]);
  }
  return m;
}
