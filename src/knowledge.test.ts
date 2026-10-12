// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { ChangeRow, LintReport } from "./api";
import { filedPage } from "./Ask";
import {
  byRun,
  changeMatches,
  nearName,
  noteFromTranscript,
  IGNORE_ALL,
  claimsByPage,
  decisions,
  fixWithAskPrompt,
  held,
  originLabel,
  pageName,
  runLabel,
  safeFixes,
  trendLabels,
  trendPoints,
  trendShown,
} from "./knowledge";

const row = (p: Partial<ChangeRow>): ChangeRow => ({
  id: "1",
  created: "2026-10-02T09:00:00",
  origin: { kind: "chat", chat: "Chat. Launch.md" },
  model: "claude:sonnet",
  page: "wiki/entities/Orbit App.md",
  kind: "edit",
  title: "Launch date",
  reason: "",
  status: "applied",
  decided: null,
  flags: [],
  warnings: [],
  group: "Chat. Launch.md",
  revertable: true,
  ...p,
});

const report: LintReport = {
  wikiPages: 4,
  sources: 3,
  ms: 10,
  checks: [
    {
      id: "missing-pages",
      title: "Missing pages",
      classic: true,
      items: [{ text: "[[Steerco Charter]]", name: "Steerco Charter", safe: false }],
    },
    {
      id: "missing-crossrefs",
      title: "Missing cross-links",
      classic: true,
      items: [{ text: "wiki/entities/Northwind.md: 'Orbit App'", page: "wiki/entities/Northwind.md", name: "Orbit App", safe: true }],
    },
    {
      id: "stale-pages",
      title: "Stale pages",
      classic: false,
      items: [{ text: "Orbit App", page: "wiki/entities/Orbit App.md", safe: false }],
    },
  ],
};

describe("changes", () => {
  it("lists the held ones and groups by run", () => {
    const rows = [
      row({
        id: "a",
        status: "held",
        group: "daily-check-1",
        origin: { kind: "ingest", label: "Ingest of Steerco", run: "daily-check-1" },
      }),
      row({ id: "b", group: "daily-check-1", origin: { kind: "lint", label: "Wiki catalogue", run: "daily-check-1" } }),
      row({ id: "c" }),
    ];
    expect(held(rows).map((r) => r.id)).toEqual(["a"]);
    const g = byRun(rows);
    expect(g.map((x) => [x.group, x.label, x.rows.length])).toEqual([
      ["daily-check-1", "Daily check", 2],
      ["Chat. Launch.md", "Ask", 1],
    ]);
  });

  it("says where a change came from", () => {
    expect(originLabel(row({}))).toBe("Ask");
    expect(originLabel(row({ origin: { kind: "lint" } }))).toBe("Knowledge health");
    expect(originLabel(row({ origin: { kind: "chat", label: "Terminal session" } }))).toBe("Terminal session");
    expect(originLabel(row({ origin: { kind: "review", label: "daily-summary 2026-10-02" } }))).toBe("Daily summary 2026-10-02");
    expect(originLabel(row({ origin: { kind: "review", label: "weekly-summary 2026-W40" } }))).toBe("Weekly summary 2026-W40");
    expect(runLabel(row({ origin: { kind: "contradiction", run: "contradictions-1" } }))).toBe("Contradiction check");
    expect(pageName("wiki/entities/Orbit App.md")).toBe("Orbit App");
  });
});

describe("knowledge health", () => {
  it("counts what needs a decision, leaving out stale pages and safe fixes", () => {
    expect(decisions(report)).toBe(1);
    expect(decisions(null)).toBe(0);
    expect(safeFixes(report)).toEqual([
      { check: "missing-crossrefs", page: "wiki/entities/Northwind.md", name: "Orbit App", detail: undefined },
    ]);
  });

  it("asks Ask about the issues that need judgement only", () => {
    const p = fixWithAskPrompt(report);
    expect(p).toContain("edit_page");
    expect(p).toContain("[[Steerco Charter]]");
    expect(p).not.toContain("Northwind");
    expect(p).not.toContain("Stale pages");
  });

  it("draws the trend from the oldest day to the newest", () => {
    expect(trendPoints([["2026-10-01", { decisions: 4 }]], 100, 40)).toBe("");
    const pts = trendPoints(
      [
        ["2026-10-01", { decisions: 4 }],
        ["2026-10-02", { decisions: 0 }],
      ],
      100,
      40,
    ).split(" ");
    expect(pts).toEqual(["0.0,4.0", "100.0,36.0"]);
  });

  it("shows the trend only when a day needed a decision, with labelled ends", () => {
    const flat: [string, { decisions: number }][] = [
      ["2026-10-01", { decisions: 0 }],
      ["2026-10-02", { decisions: 0 }],
    ];
    expect(trendShown(flat)).toBe(false);
    const h: [string, { decisions: number }][] = [
      ["2026-10-01", { decisions: 4 }],
      ["2026-10-02", { decisions: 6 }],
      ["2026-10-03", { decisions: 1 }],
    ];
    expect(trendShown(h)).toBe(true);
    expect(trendLabels(h)).toEqual({
      from: "1 Oct: 4",
      to: "3 Oct: 1",
      tip: "Issues needing a decision each day, 1 Oct to 3 Oct. Highest: 6.",
    });
  });

  it("groups a page's claims with no citation into one row", () => {
    const c = (page: string, line: string) => ({ text: `${page}: ${line}`, page, detail: line, safe: false });
    const g = claimsByPage([
      c("wiki/Orbit App.md", "Launch is on 14 November"),
      c("wiki/Maya.md", "Owner of the rota"),
      c("wiki/Orbit App.md", "Budget is agreed"),
    ]);
    expect(g.map((g) => [g.page, g.items.map((i) => i.detail)])).toEqual([
      ["wiki/Orbit App.md", ["Launch is on 14 November", "Budget is agreed"]],
      ["wiki/Maya.md", ["Owner of the rota"]],
    ]);
  });

  it("files an answer as a page with properties and its current state", () => {
    expect(filedPage("Launch readiness", "concept", "  Ready by November.\n")).toBe(
      "---\ntype: concept\nname: Launch readiness\ndescription: \nsources: []\n---\n\n## Current state\n\nReady by November.\n",
    );
  });
});

describe("changeMatches", () => {
  it("needs every word, in the title, page, origin or model, any case", () => {
    const c = row({ to: "wiki/entities/Orbit Launch.md" });
    expect(changeMatches(c, "")).toBe(true);
    expect(changeMatches(c, "launch orbit")).toBe(true);
    expect(changeMatches(c, "ASK sonnet")).toBe(true);
    expect(changeMatches(c, "orbit maya")).toBe(false);
  });
});

describe("Ignore all", () => {
  it("is on the same checks as knowledge.rs offers it on", () => {
    const rs = readFileSync("src-tauri/src/knowledge.rs", "utf8").match(/IGNORE_ALL: \[&str; \d+\] = \[([^\]]*)\]/)![1];
    expect([...rs.matchAll(/"([^"]+)"/g)].map((m) => m[1])).toEqual([...IGNORE_ALL]);
  });
});

describe("nearName", () => {
  const paths = [
    "Meeting. Orbit Steerco meeting - 2026-10-02.md",
    "wiki/entities/Maya.md",
    "1-1. Lena - 2026-10-01.md",
    "1-1. Lena - 2026-10-02.md",
  ];
  it("finds the note a missing link is a slip away from", () => {
    expect(nearName("Meeting. Orbit Steerco meting - 2026-10-02", paths)).toBe("Meeting. Orbit Steerco meeting - 2026-10-02.md");
    expect(nearName("Mya", paths)).toBeNull(); // too short to guess
    expect(nearName("Meeting. Roadmap review - 2026-10-02", paths)).toBeNull();
    // Two as close: no guess.
    expect(nearName("1-1. Lena - 2026-10-0", paths)).toBeNull();
  });
});

describe("noteFromTranscript", () => {
  const paths = [
    "Meeting. Orbit Funding Budget information - 2026-07-30.md",
    "Meeting. Orbit Funding Portfolio - 2026-07-31.md",
    "1-1. Lena - 2026-07-30.md",
  ];
  it("finds the meeting note a trashed transcript became, its parts as one meeting", () => {
    expect(noteFromTranscript("Teams. Transcript. Orbit Funding Budget information - Part 1 - 2026-07-30", paths)).toBe(paths[0]);
    expect(noteFromTranscript("Teams. Transcript. Orbit Funding Budget information - Part 2 - 2026-07-30", paths)).toBe(paths[0]);
    expect(noteFromTranscript("Teams. Transcript. Orbit Funding Portfolio - 2026-07-31", paths)).toBe(paths[1]);
    expect(noteFromTranscript("Teams. Transcript. Orbit Funding Portfolio - 2026-08-01", paths)).toBeNull();
    expect(noteFromTranscript("Meeting. Orbit Funding Portfolio - 2026-07-31", paths)).toBeNull();
  });
});
