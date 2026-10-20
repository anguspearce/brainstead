// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { KIND_COLOUR } from "./graphModel";
import { assignTones, TONES, toneOf, toneVar } from "./tones";

describe("a type's or tag's colour", () => {
  it("is the same for a name wherever it shows, whatever its case", () => {
    expect(toneOf("Meeting")).toBe(toneOf("meeting"));
    expect(toneOf("platform")).toBe(toneOf(" platform "));
    expect(TONES).toContain(toneOf("1-1"));
  });

  it("is fixed for the wiki's own types, and the Graph draws them so", () => {
    expect(toneOf("entity", true)).toBe("p5");
    expect(toneOf("concept", true)).toBe("p3");
    expect(KIND_COLOUR.entity).toBe(toneVar(toneOf("entity", true)));
    expect(KIND_COLOUR.summary).toBe(toneVar(toneOf("summary", true)));
    // Notes and sources are their layer's colour, as their squares.
    expect([KIND_COLOUR.note, KIND_COLOUR.source, KIND_COLOUR.wiki]).toEqual(["--l-note", "--l-source", "--l-wiki"]);
  });

  it("gives the six commonest names of a list six different tones, and keeps the wiki's fixed ones", () => {
    const names = ["Me", "Idea", "Spec", "Project", "1-1", "Meeting", "Plan"].map((name, i) => ({ name, n: 10 - i }));
    const m = assignTones(names);
    expect(new Set(names.slice(0, 6).map((c) => m.get(c.name.toLowerCase()))).size).toBe(6);
    // The same list gives the same tones.
    expect(assignTones([...names].reverse())).toEqual(m);
    const w = assignTones(
      [
        { name: "concept", n: 1 },
        { name: "decision", n: 9 },
        { name: "entity", n: 5 },
      ],
      true,
    );
    expect([w.get("entity"), w.get("concept")]).toEqual(["p5", "p3"]);
    expect(["p5", "p3"]).not.toContain(w.get("decision"));
  });
});
