// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { getList, getScalar, setList, setScalar } from "./props";

const PAGE = `---
title: Orbit App
type: entity
aliases: [OA, The Orbit App]
tags: [project, hub]
sources:
  - "[[Roadmap Update 2026-09-18]]"
---
# Orbit App

Body.
`;

describe("wiki page properties", () => {
  it("reads inline, block and single values", () => {
    expect(getList(PAGE, "aliases")).toEqual(["OA", "The Orbit App"]);
    expect(getList(PAGE, "sources")).toEqual(["[[Roadmap Update 2026-09-18]]"]);
    expect(getList("---\ntags: hub\n---\n", "tags")).toEqual(["hub"]);
    expect(getScalar(PAGE, "type")).toBe("entity");
    expect(getList(PAGE, "missing")).toEqual([]);
    // An apostrophe inside a value isn't a quote: the two stay two, and survive the next edit.
    const ob = "---\naliases: [O'Brien, OB]\n---\n";
    expect(getList(ob, "aliases")).toEqual(["O'Brien", "OB"]);
    expect(getList(setList(ob, "aliases", ["O'Brien", "OB", "Obie"]), "aliases")).toEqual(["O'Brien", "OB", "Obie"]);
    expect(getList("---\naliases: ['a, b', c]\n---\n", "aliases")).toEqual(["a, b", "c"]);
  });

  it("writes only the property, in its own style", () => {
    const a = setList(PAGE, "aliases", ["OA", "Orbit"]);
    expect(a).toBe(PAGE.replace("aliases: [OA, The Orbit App]", "aliases: [OA, Orbit]"));
    const s = setList(PAGE, "sources", ["[[Roadmap Update 2026-09-18]]", "[[Email. Steerco minutes - 2026-09-28]]"]);
    expect(s).toContain('sources:\n  - "[[Roadmap Update 2026-09-18]]"\n  - "[[Email. Steerco minutes - 2026-09-28]]"\n---');
    expect(setList(PAGE, "tags", [])).toBe(PAGE.replace("tags: [project, hub]\n", ""));
    expect(setScalar(PAGE, "description", "The staff app")).toContain("description: The staff app\n---");
    expect(setScalar(PAGE, "type", "concept")).toContain("type: concept\naliases");
  });

  it("starts properties when there are none, and keeps CRLF", () => {
    expect(setList("# Title\n", "aliases", ["T"])).toBe("---\naliases: [T]\n---\n\n# Title\n");
    const crlf = PAGE.replace(/\n/g, "\r\n");
    expect(setList(crlf, "aliases", ["OA"])).toBe(crlf.replace("aliases: [OA, The Orbit App]", "aliases: [OA]"));
  });
});
