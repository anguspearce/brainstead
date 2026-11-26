// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Templater as documented at https://silentvoid13.github.io/Templater/, plus the templates
// ported from the previous app's bff/src/routes/mutations-workflow.test.ts.

import { describe, expect, it } from "vitest";
import { composeFilename, parseName } from "./filename";
import { Ask, checkTemplate, Env, freeName, parseInclude, runTemplate, section, StopRun, tfile, tokens } from "./templater";

const now = new Date(2026, 9, 2, 9, 30); // Friday 2 October 2026, 09:30

/** An Env whose prompts answer from `answers` in turn (a string, chosen indexes, or null to cancel). */
function env(answers: (string | number[] | null)[] = [], over: Partial<Env> = {}) {
  const asked: Ask[] = [];
  const made: Record<string, string> = {};
  const files: Record<string, string> = {
    "index.md": "Entry point.",
    "Snippets.md": "# Snippets\n\n## Sign-off\nRegards,\nW\n\n## Other\nx\n\nA para ^p1\n\n- item one ^b2\n- item two",
    "Templates/Footer.md": "Made <% tp.date.now('D MMM') %>",
    ...(over.files ? {} : {}),
  };
  const e: Env = {
    root: "/vault",
    files: Object.keys(files),
    read: async (p) => {
      if (!(p in files)) throw new Error(`no ${p}`);
      return files[p];
    },
    create: async (p, c) => void (made[p] = c),
    clipboard: async () => "from the clipboard",
    ask: (async (q: Ask) => {
      asked.push(q);
      return answers.length ? answers.shift()! : null;
    }) as Env["ask"],
    templateFile: tfile("Templates/T.md"),
    now,
    ...over,
  };
  return { e, asked, made };
}

const run = async (src: string, answers: (string | number[] | null)[] = [], over: Partial<Env> = {}) => {
  const x = env(answers, over);
  return { ...(await runTemplate(src, x.e)), asked: x.asked, made: x.made };
};

describe("syntax", () => {
  it("interpolates, executes, comments and keeps tR", async () => {
    const r = await run('A<% 1 + 1 %>B<%* tR += "C"; %>D<%# nothing %>E<%+ "once" %>');
    expect(r.content).toBe("A2BCDEonce");
  });
  it("writes nothing for null and undefined, and awaits promises", async () => {
    expect((await run("[<% null %>][<% undefined %>][<% Promise.resolve(3) %>]")).content).toBe("[][][3]");
  });
  it("trims whitespace as the docs say", async () => {
    const t = (s: string) => tokens(s).map((x) => ("text" in x ? x.text : "#"));
    expect(t("a\n<%- 1 %>")).toEqual(["a", "#", ""]);
    expect(t("a \n\n <%_ 1 %>")).toEqual(["a", "#", ""]);
    expect(t("<% 1 -%>\nb")).toEqual(["", "#", "b"]);
    expect(t("<% 1 -%>\n\nb")).toEqual(["", "#", "\nb"]);
    expect(t("<% 1 _%> \n\n b")).toEqual(["", "#", "b"]);
    const src =
      '<%* if (tp.file.title == "MyFile" ) { -%>\nThis is my file!\n<%* } else { -%>\nThis isn\'t my file!\n<%* } -%>\nSome content ...';
    expect((await run(src)).content).toBe("This isn't my file!\nSome content ...");
  });
  it("flags a template that doesn't compile, without running it", () => {
    expect(checkTemplate("<%* const ( = %>")).toMatch(/./);
    expect(checkTemplate("<% tp.date.now() %>")).toBeNull();
  });
});

describe("tp.date", () => {
  it("formats with moment, offsets in days or ISO durations, from a reference", async () => {
    const r = await run(
      [
        "<% tp.date.now() %>",
        '<% tp.date.now("dddd Do MMMM YYYY, HH:mm") %>',
        '<% tp.date.now("YYYY-MM-DD", 7) %>',
        '<% tp.date.now("YYYY-MM-DD", -1) %>',
        '<% tp.date.now("YYYY-MM-DD", "P-1M") %>',
        '<% tp.date.now("YYYY-MM-DD", 1, "2020-02-28", "YYYY-MM-DD") %>',
        '<% tp.date.now("[W]W gggg") %>',
        "<% tp.date.tomorrow() %> <% tp.date.yesterday('DD/MM') %>",
      ].join("\n"),
    );
    expect(r.content.split("\n")).toEqual([
      "2026-10-02",
      "Friday 2nd October 2026, 09:30",
      "2026-10-09",
      "2026-10-01",
      "2026-09-02",
      "2020-02-29",
      "W40 2026",
      "2026-10-03 01/10",
    ]);
  });
  it("finds weekdays, Monday first", async () => {
    const r = await run(
      '<% tp.date.weekday("YYYY-MM-DD", 0) %> <% tp.date.weekday("ddd D", 7) %> <% tp.date.weekday("YYYY-MM-DD", -7, "2026-10-14") %>',
    );
    expect(r.content).toBe("2026-09-28 Mon 5 2026-10-05");
  });
});

describe("tp.file", () => {
  it("renames and moves the new note, and its title follows", async () => {
    const r = await run(
      '<%* await tp.file.rename("Plan") %><% tp.file.title %>|<%* await tp.file.move("Projects/Q4/Kickoff") %><% tp.file.title %>|<% tp.file.folder() %>|<% tp.file.folder(true) %>|<% tp.file.path(true) %>|<% tp.file.path() %>',
    );
    expect(r.content).toBe("Plan|Kickoff|Q4|Projects/Q4|Projects/Q4/Kickoff.md|/vault/Projects/Q4/Kickoff.md");
    expect(r.path).toBe("Projects/Q4/Kickoff.md");
    expect(r.named).toBe(true);
  });
  it("is Untitled until named, with nothing in it yet", async () => {
    const r = await run(
      "<% tp.file.title %>[<% tp.file.content %>][<% tp.file.tags.length %>][<% tp.file.selection() %>]<% tp.file.creation_date() %>|<% tp.file.last_modified_date('YYYY') %>",
    );
    expect(r.content).toBe("Untitled[][0][]2026-10-02 09:30|2026");
    expect(r).toMatchObject({ path: "Untitled.md", named: false });
    expect(freeName("Untitled.md", (p) => p === "Untitled.md" || p === "Untitled 1.md")).toBe("Untitled 2.md");
  });
  it("places numbered cursors, the lowest first, and appends at it", async () => {
    const r = await run("a<% tp.file.cursor(2) %>b<% tp.file.cursor(1) %>c<% tp.file.cursor_append('!') %>");
    expect(r.content).toBe("ab!c");
    expect(r.cursor).toBe(2);
    expect(r.cursors).toEqual([2, 1]);
    expect((await run("x<% tp.file.cursor() %>y")).cursor).toBe(1);
  });
  it("checks and finds files", async () => {
    const r = await run(
      '<% await tp.file.exists("index.md") %> <% await tp.file.exists("nope.md") %> <% tp.file.find_tfile("Footer").path %> <% tp.file.find_tfile("missing") %>',
    );
    expect(r.content).toBe("true false Templates/Footer.md ");
  });
  it("includes notes, sections and blocks, running templates in them", async () => {
    const r = await run(
      '<% tp.file.include("[[index]]") %>|<% tp.file.include("[[Snippets#Sign-off]]") %>|<% tp.file.include("[[Snippets#^p1]]") %>|<% tp.file.include("[[Snippets#^b2]]") %>|<% tp.file.include(tp.file.find_tfile("Footer")) %>',
    );
    expect(r.content).toBe("Entry point.|## Sign-off\nRegards,\nW|A para|- item one|Made 2 Oct");
    await expect(run('<% tp.file.include("[[Nowhere]]") %>')).rejects.toThrow(/Nowhere.*isn't in the vault/);
    const loop = { files: ["Loop.md"], read: async () => '<% tp.file.include("[[Loop]]") %>' };
    await expect(run('<% tp.file.include("[[Loop]]") %>', [], loop)).rejects.toThrow(/inclusion depth limit \(max = 10\)/);
    expect(parseInclude("[[A/B#H|alias]]")).toEqual({ target: "A/B", heading: "H" });
    expect(section("# A\n1\n## B\n2\n# C", "a")).toBe("# A\n1\n## B\n2");
  });
  it("creates other notes, refusing ones that exist", async () => {
    const r = await run(
      '<%* await tp.file.create_new("hello", "Other", true, "Inbox") %><%* await tp.file.create_new(tp.file.find_tfile("Footer"), "F") %>done',
    );
    expect(r.made).toEqual({ "Inbox/Other.md": "hello", "F.md": "Made 2 Oct" });
    expect(r.open).toEqual(["Inbox/Other.md"]);
    await expect(run('<%* await tp.file.create_new("x", "index") %>')).rejects.toThrow(/already/);
  });
});

describe("tp.system", () => {
  it("asks as the run reaches each prompt, in order, with the docs' arguments", async () => {
    const r = await run(
      [
        '<%* const a = await tp.system.prompt("Name?", "Bob", false, true) -%>',
        '<%* const b = await tp.system.suggester(["Low", "High"], ["l", "h"], false, "Priority", 5, "h") -%>',
        '<%* const c = await tp.system.multi_suggester((x) => x.toUpperCase(), ["x", "y", "z"], false, "Pick", undefined, ["z"]) -%>',
        "<% a %> <% b %> <% c.join() %>",
      ].join("\n"),
      ["Ada", [0], [0, 2]],
    );
    expect(r.content).toBe("Ada l x,z");
    expect(r.asked).toEqual([
      { kind: "prompt", text: "Name?", value: "Bob", multiline: true, selectAll: false },
      { kind: "suggest", text: "Priority", labels: ["Low", "High"], limit: 5, chosen: [1] },
      { kind: "multi", text: "Pick", labels: ["X", "Y", "Z"], limit: undefined, chosen: [2] },
    ]);
  });
  it("returns null on cancel, or throws with throw_on_cancel", async () => {
    expect((await run('[<% await tp.system.prompt("A") %>][<% await tp.system.suggester(["a"], ["a"]) %>]', [null, null])).content).toBe(
      "[][]",
    );
    await expect(run('<% await tp.system.prompt("A", "", true) %>', [null])).rejects.toThrow(/Cancelled/);
  });
  it("stops when the window stops it", async () => {
    const x = env([], {
      ask: (async () => {
        throw new StopRun();
      }) as Env["ask"],
    });
    await expect(runTemplate('<% tp.system.prompt("A") %>', x.e)).rejects.toBeInstanceOf(StopRun);
  });
  it("reads the clipboard", async () => {
    expect((await run("<% tp.system.clipboard() %>")).content).toBe("from the clipboard");
  });
});

describe("the rest of tp", () => {
  it("has config, frontmatter and hooks", async () => {
    const r = await run(
      '<% tp.config.template_file.basename %> <% tp.config.run_mode %> <%* await tp.file.rename("N") %><% tp.config.target_file.path %> <% Object.keys(tp.frontmatter).length %><%* tp.hooks.on_all_templates_executed(() => 1) %>',
    );
    expect(r.content).toBe("T 0 N.md 0");
    expect(r.hooks).toHaveLength(1);
  });
  it("runs user scripts, a function or an object of them", async () => {
    const scripts = [
      { name: "greet", source: "module.exports = function (who) { return `Hello ${who}`; };" },
      { name: "util", source: "module.exports = { twice: (x) => x * 2, later: async () => 'async ok' };" },
    ];
    const r = await run("<% tp.user.greet('Ada') %> <% tp.user.util.twice(21) %> <% tp.user.util.later() %>", [], { scripts });
    expect(r.content).toBe("Hello Ada 42 async ok");
    await expect(run("x", [], { scripts: [{ name: "bad", source: "require('fs')" }] })).rejects.toThrow(/bad\.js.*require/);
  });
  it("gives moment to templates", async () => {
    expect((await run('<% moment("2026-01-31").add(1, "month").format("YYYY-MM-DD") %>')).content).toBe("2026-02-28");
  });
  it("says which calls aren't available", async () => {
    await expect(run("<% tp.web.daily_quote() %>")).rejects.toThrow(/tp\.web isn't available in Brainstead/);
    await expect(run("<% tp.obsidian.Notice %>")).rejects.toThrow(/tp\.obsidian isn't available/);
    await expect(run("<% app.vault.getName() %>")).rejects.toThrow(/app isn't available/);
  });
});

describe("the previous app's templates", () => {
  it("runs the 1-1 template", async () => {
    const src = [
      "<%*",
      'const name = await tp.system.prompt("Person");',
      'await tp.file.rename(`1-1. ${name} - ${tp.date.now("YYYY-MM-DD")}`);',
      "-%>",
      "# 1-1 with <% name %>",
      "",
      "## Notes",
      "",
      "<% tp.file.cursor() %>",
    ].join("\n");
    const r = await run(src, ["Bob"]);
    expect(r.path).toBe("1-1. Bob - 2026-10-02.md");
    expect(r.content).toBe("# 1-1 with Bob\n\n## Notes\n\n");
    expect(r.cursor).toBe(r.content.length);
  });
  it("runs the fixture vault's Meeting template", async () => {
    const src = [
      '<%* const prefix = await tp.system.prompt("Prefix?", "Meeting") -%>',
      '<%* const name = await tp.system.prompt("Name?", "") -%>',
      '<%* await tp.file.rename(prefix + ". " + name + " - " +tp.date.now("YYYY-MM-DD")) -%>',
      "",
      "## Notes",
      "<% tp.file.cursor(1) -%>",
      "",
    ].join("\n");
    const r = await run(src, ["Meeting", "Steerco"]);
    expect(r.path).toBe("Meeting. Steerco - 2026-10-02.md");
    expect(r.content).toBe("\n## Notes\n");
    expect(r.cursor).toBe(10);
  });
});

describe("filenames", () => {
  it("reads the grammar as filename.rs does", () => {
    expect(parseName("Meeting. Northwind Q4 prep - 2026-10-02.md")).toEqual({
      type: "Meeting",
      title: "Northwind Q4 prep",
      date: "2026-10-02",
    });
    expect(parseName("Victor/Me. Daily Reviews - 2026-10.md")).toEqual({ type: "Me", title: "Daily Reviews", date: "2026-10" });
    expect(parseName("1-1. Omar - Data sync and dashboards - 2024-09-30.md").title).toBe("Omar - Data sync and dashboards");
    expect(parseName("Untyped note.txt")).toEqual({ type: null, title: "Untyped note", date: null });
  });

  it("builds names as the previous app does", () => {
    expect(composeFilename({ type: "Workshop", title: "Role Framework kickoff", date: "2026-07-14" })).toBe(
      "Workshop. Role Framework kickoff - 2026-07-14.md",
    );
    expect(composeFilename({ type: null, title: " a/b:c  d " })).toBe("abc d.md");
    expect(composeFilename({ type: null, title: ".NET migration" })).toBe("NET migration.md");
    expect(composeFilename({ type: "Idea", title: ".NET migration" })).toBe("Idea. .NET migration.md");
    expect(composeFilename({ type: null, title: "..." })).toBe("");
    expect(composeFilename({ type: null, title: "Plan #2 [draft] ^x | v1" })).toBe("Plan 2 draft x v1.md");
    expect(composeFilename({ type: "X", title: "  " })).toBe("");
  });
});

describe("the editor's Templater help", () => {
  it("offers only what the engine has", async () => {
    // Every name the help offers (src/editor/templaterAssist.ts) is there when a template runs.
    const { TP } = await import("../editor/templaterAssist");
    // `in`, not typeof: tp.config.active_file is there but undefined with no note open.
    const src = TP.filter((i) => i.label !== "tp.frontmatter")
      .map((i) => {
        const parts = i.label.split(".");
        const key = parts.pop();
        return `<% "${key}" in ${parts.join(".")} ? "" : "missing ${i.label}" %>`;
      })
      .join("");
    expect((await run(src)).content).toBe("");
  });
});
