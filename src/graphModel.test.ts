// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { Graph, GraphNode } from "./api";
import { kindLabel, kindOf, kindsIn, labelled, neighbours, pick, visible } from "./graphModel";

const node = (id: string, layer: GraphNode["layer"], type: string | null = null, depth = 1, degree = 1): GraphNode => ({
  id,
  title: id.replace(/^.*\//, "").replace(/\.md$/, ""),
  layer,
  type,
  degree,
  depth,
});

const G: Graph = {
  nodes: [
    node("wiki/entities/Orbit App.md", "wiki", "entity", 0, 5),
    node("wiki/concepts/Hub Platform.md", "wiki", "concept"),
    node("wiki/summaries/Update.md", "wiki", "summary"),
    node("sources/Update.md", "source"),
    node("Meeting. Orbit.md", "note"),
    node("ghost:roadmap", "ghost"),
  ],
  edges: [
    ["Meeting. Orbit.md", "wiki/entities/Orbit App.md"],
    ["ghost:roadmap", "wiki/entities/Orbit App.md"],
    ["wiki/concepts/Hub Platform.md", "wiki/entities/Orbit App.md"],
    ["sources/Update.md", "wiki/summaries/Update.md"],
  ],
  truncated: false,
};

describe("graph", () => {
  it("colours a node by its layer, and a wiki page by its type", () => {
    expect(G.nodes.map(kindOf)).toEqual(["entity", "concept", "summary", "source", "note", "ghost"]);
    expect(kindLabel(G.nodes[2])).toBe("Summary");
    expect(kindLabel(G.nodes[3])).toBe("Source");
    expect(kindOf({ layer: "wiki", type: null })).toBe("wiki");
  });

  it("hides kinds but never the centre, and drops their links", () => {
    const v = visible(G, ["entity", "ghost"], "wiki/entities/Orbit App.md");
    expect(v.nodes.map((n) => n.id)).not.toContain("ghost:roadmap");
    expect(v.nodes.map((n) => n.id)).toContain("wiki/entities/Orbit App.md");
    expect(v.edges).toHaveLength(3);
    expect(visible(G, [], null)).toBe(G);
    expect(kindsIn(G)).toEqual(["entity", "concept", "summary", "source", "note", "ghost"]);
  });

  it("knows each node's neighbours and which get a label (the centre and the biggest)", () => {
    const nb = neighbours(G);
    expect([...nb.get("wiki/entities/Orbit App.md")!].sort()).toEqual([
      "Meeting. Orbit.md",
      "ghost:roadmap",
      "wiki/concepts/Hub Platform.md",
    ]);
    const big: Graph = { ...G, nodes: G.nodes.map((n) => (n.id === "Meeting. Orbit.md" ? { ...n, degree: 4 } : n)) };
    expect([...labelled(big, "wiki/entities/Orbit App.md", 1)]).toEqual([
      "wiki/entities/Orbit App.md",
      "Meeting. Orbit.md",
      "ghost:roadmap",
    ]);
    expect(labelled(G, null).has("ghost:roadmap")).toBe(true);
  });

  it("picks pages by every word typed, shortest title first", () => {
    const files = [{ title: "Orbit App Steerco" }, { title: "Orbit App" }, { title: "Hub" }];
    expect(pick(files, "orbit app").map((f) => f.title)).toEqual(["Orbit App", "Orbit App Steerco"]);
    expect(pick(files, "  ")).toEqual([]);
  });
});
