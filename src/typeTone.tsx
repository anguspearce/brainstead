// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The vault's types and tags with their colours (src/tones.ts), from its counts at a glance, and the
// type pill and dot that show them. One fetch a change of the vault, shared by every pill on screen
// and the pies, so a type is the same colour in each.

import { useEffect, useState } from "react";
import { api, Glance } from "./api";
import { useVaultVersion } from "./state";
import { assignTones, Tone, toneColour, toneOf } from "./tones";

let cached: { version: number; glance: Promise<Glance> } | null = null;

/** The vault at a glance, fetched once per change of the vault. */
export function useGlance(): Glance | null {
  const v = useVaultVersion();
  const [g, setG] = useState<Glance | null>(null);
  useEffect(() => {
    let live = true;
    if (!cached || cached.version !== v) {
      const entry = { version: v, glance: api.glance() };
      cached = entry;
      // A failed fetch isn't kept, so the next draw asks again.
      entry.glance.catch(() => cached === entry && (cached = null));
    }
    void cached.glance.then((x) => live && setG(x)).catch(() => {});
    return () => {
      live = false;
    };
  }, [v]);
  return g;
}

export interface Tones {
  noteType: Map<string, Tone>;
  wikiType: Map<string, Tone>;
  /** Notes' and wiki pages' tags together: a tag is one colour on both pies. */
  tag: Map<string, Tone>;
}

const byGlance = new WeakMap<Glance, Tones>();

/** Each type's and tag's tone in this vault. */
export function tonesOf(g: Glance): Tones {
  let t = byGlance.get(g);
  if (!t) {
    const tags = new Map<string, number>();
    for (const c of [...g.noteTags, ...g.wikiTags]) tags.set(c.name, (tags.get(c.name) ?? 0) + c.n);
    t = {
      noteType: assignTones(g.noteTypes),
      wikiType: assignTones(g.wikiTypes, true),
      tag: assignTones([...tags].map(([name, n]) => ({ name, n }))),
    };
    byGlance.set(g, t);
  }
  return t;
}

/** A type's tone: the vault's, or from its letters until the counts are in. */
export function typeTone(tones: Tones | null, type: string, wiki?: boolean): Tone {
  return (wiki ? tones?.wikiType : tones?.noteType)?.get(type.trim().toLowerCase()) ?? toneOf(type, wiki);
}

/** A type's dot in its colour, as on the pies: in a type pill, or before a type filter's name.
 *  `wiki`: a wiki page's type. `colour` instead: a colour of its own (the Graph's kinds). */
export function TypeDot({ type, wiki, colour }: { type?: string; wiki?: boolean; colour?: string }) {
  const g = useGlance();
  const c = colour ?? toneColour(typeTone(g && tonesOf(g), type ?? "", wiki));
  return <i className="tdot" style={{ background: c }} />;
}

/** A note's or wiki page's type as a pill, with its dot. */
export function TypePill({ type, wiki }: { type: string; wiki?: boolean }) {
  return (
    <span className="chip">
      <TypeDot type={type} wiki={wiki} />
      {type}
    </span>
  );
}
