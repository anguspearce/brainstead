// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { appLink, GUIDES, helpUrl, KEYS, openSection, keyRows, parseTopic, searchHelp, TOPICS, topicFor } from ".";
import type { Screen, SettingsPane } from "../nav";

const SCREENS: Screen[] = [
  "today",
  "inbox",
  "tasks",
  "projects",
  "review",
  "ask",
  "notes",
  "wiki",
  "sources",
  "templates",
  "graph",
  "health",
  "activity",
  "trash",
  "settings",
  "doc",
  "search",
  "weekly",
  "contradictions",
  "meeting",
  "triage",
  "reply",
  "doccheck",
  "compare",
];
const PANES: SettingsPane[] = ["general", "notes", "vault", "assistants", "jobs", "capture", "permissions", "about"];

describe("help topics", () => {
  it("parses a file", () => {
    const t = parseTopic(
      "x",
      "---\ntitle: Inbox\nkind: screen\nscreens: [inbox, settings/capture]\norder: 2\nsummary: S.\n---\nIntro.\n\n## Keys\nJ.\n",
    );
    expect(t).toMatchObject({
      id: "x",
      title: "Inbox",
      kind: "screen",
      screens: ["inbox", "settings/capture"],
      order: 2,
      summary: "S.",
      intro: "Intro.",
    });
    expect(t.sections).toEqual([{ title: "Keys", body: "J." }]);
  });

  it("has a topic for every screen and every settings pane", () => {
    for (const s of SCREENS) expect(topicFor({ screen: s })?.screens, s).toContain(s);
    for (const pane of PANES) expect(topicFor({ screen: "settings", pane })?.screens, pane).toContain(`settings/${pane}`);
    // An open document's help follows what it is.
    expect(topicFor({ screen: "doc", path: "Templates/Meeting.md" })?.id).toBe("templates");
    expect(topicFor({ screen: "doc", path: "wiki/entities/Orbit App.md" })?.id).toBe("wiki");
    expect(topicFor({ screen: "doc", path: "sources/Roadmap Update 2026-09-18.md" })?.id).toBe("sources");
    expect(topicFor({ screen: "doc", path: "Meeting. Orbit App Steerco - 2026-09-30.md" })?.id).toBe("note");
    // …and opens on the section about that kind of page; from the list, on the first.
    const wiki = TOPICS.find((t) => t.id === "wiki")!;
    expect(openSection(wiki, { screen: "doc" })).toBe("Read a wiki page");
    expect(openSection(wiki, { screen: "wiki" })).toBe(wiki.sections[0].title);
    for (const t of TOPICS.filter((x) => x.page))
      expect(
        t.sections.map((s) => s.title),
        t.id,
      ).toContain(t.page);
  });

  it("has getting-started guides that link into every screen", () => {
    expect(GUIDES[0].id).toBe("guide-start");
    for (const g of GUIDES) {
      expect(g.sections.length, g.id).toBeGreaterThanOrEqual(3);
      for (const s of g.sections) expect(s.body, `${g.id} › ${s.title}`).toMatch(/\]\(app:/);
    }
    const reached = new Set(
      GUIDES.flatMap((g) =>
        [
          ...g.sections
            .map((s) => s.body)
            .join("\n")
            .matchAll(/\]\(app:([a-z]+)/g),
        ].map((m) => m[1]),
      ),
    );
    // A note opens from a list; every other screen has a guide that takes you there.
    for (const s of SCREENS.filter((x) => x !== "doc")) expect(reached, s).toContain(s);
  });

  it("keeps to the house rules", () => {
    for (const t of TOPICS) {
      const all = [t.title, t.summary, t.intro, ...t.sections.flatMap((s) => [s.title, s.body])].join("\n");
      expect(t.summary, t.id).not.toBe("");
      expect(t.sections.length, t.id).toBeGreaterThan(0);
      expect(all, t.id).not.toMatch(new RegExp(["obsidian", "previous app", "way" + "nes", "microsoft graph"].join("|"), "i"));
      // Emoji only inside code, where they show the file format.
      const prose = all.replace(/`[^`]*`/g, "");
      expect(prose, t.id).not.toMatch(/\p{Emoji_Presentation}/u);
      for (const s of t.screens) expect([...SCREENS, ...PANES.map((p) => `settings/${p}`)], `${t.id}: ${s}`).toContain(s);
      for (const m of all.matchAll(/\]\(help:([^)]*)\)/g))
        expect(
          TOPICS.map((x) => x.id),
          `${t.id}: help:${m[1]}`,
        ).toContain(m[1]);
      for (const m of all.matchAll(/\]\((app:[^)]*)\)/g)) {
        const to = appLink(m[1]);
        expect(to, `${t.id}: ${m[1]}`).not.toBeNull();
        expect(SCREENS, `${t.id}: ${m[1]}`).toContain(to!.screen);
        if (to!.pane) expect(PANES, `${t.id}: ${m[1]}`).toContain(to!.pane);
      }
    }
  });

  it("reads app links", () => {
    expect(appLink("app:inbox")).toEqual({ screen: "inbox" });
    expect(appLink("app:settings/vault")).toEqual({ screen: "settings", pane: "vault" });
    expect(appLink("https://example.com")).toBeNull();
  });

  it("keeps app and help links through the markdown renderer", () => {
    expect(helpUrl("app:settings/vault")).toBe("app:settings/vault");
    expect(helpUrl("help:capture")).toBe("help:capture");
    expect(helpUrl("https://example.com")).toBe("https://example.com");
    expect(helpUrl("javascript:alert(1)")).toBe("");
  });

  it("finds sections by every word typed", () => {
    const hits = searchHelp("capture shortcut");
    expect(hits.length).toBeGreaterThan(0);
    expect(hits.every((h) => /captur/i.test(h.topic.title + h.topic.summary + h.section.title + h.section.body))).toBe(true);
    expect(searchHelp("zzqx")).toEqual([]);
    expect(searchHelp("  ")).toEqual([]);
  });

  it("gives the shortcut list to the sheet", () => {
    expect(keyRows("- `⌘[` `⌘]` — Back, forward\nnot a row")).toEqual([["⌘[  ⌘]", "Back, forward"]]);
    expect(KEYS.map(([t]) => t)).toEqual([
      "Anywhere",
      "A note",
      "⌘K",
      "Quick capture",
      "Menu bar window",
      "Lists",
      "Task lists",
      "Inbox",
      "Bookmarks triage",
      "Changes",
      "Draft reply",
      "Ask",
    ]);
    expect(KEYS[0][1]).toContainEqual(["?", "Help for this screen"]);
  });
});
