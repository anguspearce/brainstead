// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { markdown } from "@codemirror/lang-markdown";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));

import { FUNCTIONS } from "../md/dataview/eval";
import { buildPages } from "../md/dataview/pages";
import { makeDv } from "../md/dataview/js";
import { parseQuery as parseDql } from "../md/dataview/parse";
import { GROUP_KEYS, parseQuery as parseTasks, SORT_KEYS } from "../tasksq/query";
import { closeFenceOnEnter, dataviewjsOptions, dataviewOptions, fenceAt, queryDiagnostics, tasksOptions } from "./queryAssist";
import { dvLiteral, dvText, emptyDvForm, emptyTasksForm, parseDvForm, parseTasksForm, tasksText } from "./queryBuild";
import { DV_API, DV_FUNCTION_INFO, setVaultVocabForTest, tasksInstructions } from "./queryVocab";

setVaultVocabForTest({
  tags: ["#project", "#work/admin"],
  folders: ["Clients", "wiki/entities"],
  notes: ["Me. To Do List"],
  fields: ["status", "owner"],
});

const labels = (r: { items: { label: string }[] } | null) => r?.items.map((i) => i.label) ?? [];
const state = (doc: string, at = doc.length) => EditorState.create({ doc, selection: { anchor: at }, extensions: [markdown()] });

describe("query blocks", () => {
  it("knows when the caret is inside one, and which", () => {
    const doc = "intro\n```tasks\nnot done\n```\n```js\nx\n```\n```dataview\nLIST";
    expect(fenceAt(state(doc), doc.indexOf("not done"))?.lang).toBe("tasks");
    expect(fenceAt(state(doc), 2)).toBeNull();
    expect(fenceAt(state(doc), doc.indexOf("x\n"))).toBeNull();
    expect(fenceAt(state(doc), doc.length)?.lang).toBe("dataview");
    // On the fence line itself: not inside.
    expect(fenceAt(state(doc), doc.indexOf("```tasks") + 3)).toBeNull();
  });
});

describe("help while typing a tasks query", () => {
  it("offers instructions at the start of a line", async () => {
    expect(labels(await tasksOptions("", true))).toContain("not done");
    expect(labels(await tasksOptions("", false))).toEqual([]);
    expect(labels(await tasksOptions("so", false))).toContain("sort by");
  });
  it("offers what follows a date, a sort, a group and the layout words", async () => {
    expect(labels(await tasksOptions("due ", false))).toEqual(expect.arrayContaining(["before", "on or after", "today"]));
    expect(labels(await tasksOptions("due before ", false))).toEqual(expect.arrayContaining(["today", "next week"]));
    expect(labels(await tasksOptions("sort by ", false))).toEqual(SORT_KEYS);
    expect(labels(await tasksOptions("sort by due ", false))).toEqual(["reverse"]);
    expect(labels(await tasksOptions("group by ", false))).toEqual(GROUP_KEYS);
    expect(labels(await tasksOptions("hide ", false))).toContain("backlink");
    expect(labels(await tasksOptions("priority is ", false))).toEqual(expect.arrayContaining(["above", "high"]));
    expect(labels(await tasksOptions("status.type is ", false))).toContain("IN_PROGRESS");
  });
  it("offers the vault's tags and folders", async () => {
    expect(labels(await tasksOptions("tags include ", false))).toEqual(["#project", "#work/admin"]);
    expect(labels(await tasksOptions("path includes Cl", false))).toEqual(["Clients", "wiki/entities"]);
  });
  it("offers boolean operators after a bracketed filter, and filters inside brackets", async () => {
    expect(labels(await tasksOptions("(due before today) ", false))).toContain("AND");
    expect(labels(await tasksOptions("(due before today) AND (no", false))).toContain("not done");
  });
  it("replaces from where the typed word starts", async () => {
    const r = await tasksOptions("  sort by du", false);
    expect(r?.start).toBe(10);
  });
  it("offers only instructions the engine reads", () => {
    const sample = (apply: string) =>
      /function $/.test(apply)
        ? `${apply}${apply.startsWith("group") ? "task.status.name" : apply.startsWith("sort") ? "task.due" : "task.description.length > 1"}`
        : /(before|after|on|in) $/.test(apply)
          ? `${apply}today`
          : /^priority is( \w+)? $/.test(apply)
            ? `${apply}high`
            : /^status\.type is( not)? $/.test(apply)
              ? `${apply}TODO`
              : /regex matches \/$/.test(apply)
                ? `${apply}x/`
                : /(include|includes) $/.test(apply)
                  ? `${apply}x`
                  : /^sort by $|^group by $/.test(apply)
                    ? `${apply}due`
                    : /^limit( groups)? $/.test(apply)
                      ? `${apply}5`
                      : /^(hide|show) $/.test(apply)
                        ? `${apply}tags`
                        : apply.trim();
    for (const i of tasksInstructions()) {
      if ((i.apply ?? i.label).includes("(")) continue;
      const line = sample(i.apply ?? i.label);
      expect(parseTasks(line, { path: "", scripts: true }).error, line).toBeNull();
    }
    for (const k of SORT_KEYS) expect(parseTasks(`sort by ${k}`, { path: "", scripts: true }).error, k).toBeNull();
    for (const k of GROUP_KEYS) expect(parseTasks(`group by ${k}`, { path: "", scripts: true }).error, k).toBeNull();
  });
});

describe("help while typing a dataview query", () => {
  it("offers the query types first, then commands on later lines", async () => {
    expect(labels(await dataviewOptions("", true))).toContain("TABLE");
    expect(labels(await dataviewOptions("TA", false))).toContain("TABLE WITHOUT ID");
    expect(labels(await dataviewOptions("LIST\nWH", false))).toContain("WHERE");
  });
  it("offers tags, folders and notes after FROM", async () => {
    expect(labels(await dataviewOptions("LIST\nFROM #pro", false))).toContain("#project");
    expect(labels(await dataviewOptions('LIST FROM "', false))).toContain('"Clients"');
    expect(labels(await dataviewOptions("LIST FROM [[Me", false))).toContain("[[Me. To Do List]]");
  });
  it("offers fields, functions and literals in expressions", async () => {
    const r = labels(await dataviewOptions("TABLE file.n", false));
    expect(r).toEqual(expect.arrayContaining(["file.name", "owner", "contains", "date(today)"]));
    expect(labels(await dataviewOptions("TASK\nWHERE ", true))).toContain("completed");
    expect(labels(await dataviewOptions("TABLE x\nSORT file.mtime ", false))).toEqual(expect.arrayContaining(["ASC", "DESC"]));
  });
  it("has a signature for every function the engine has, and no others", () => {
    expect(Object.keys(DV_FUNCTION_INFO).sort()).toEqual(Object.keys(FUNCTIONS).sort());
  });
  it("offers the dv API, and only members the dv object has", () => {
    expect(labels(dataviewjsOptions("const p = dv.pa"))).toContain("pages");
    expect(labels(dataviewjsOptions("dv.pages().wh"))).toContain("where");
    const dv = makeDv({
      set: buildPages([]),
      selfPath: null,
      container: document.createElement("div"),
      render: { value: () => {}, result: () => {}, tasks: () => {}, error: () => {} },
    }) as unknown as Record<string, unknown>;
    for (const [name] of DV_API) expect(dv[name], name).toBeDefined();
  });
});

describe("```tasks then Enter", () => {
  it("closes the block and puts the caret inside", () => {
    const v = new EditorView({
      state: EditorState.create({ doc: "Intro\n```tasks", selection: EditorSelection.cursor(14), extensions: [markdown()] }),
    });
    expect(closeFenceOnEnter(v)).toBe(true);
    expect(v.state.doc.toString()).toBe("Intro\n```tasks\n\n```");
    expect(v.state.selection.main.head).toBe(15);
  });
  it("leaves a block that's already closed, and other lines, alone", () => {
    const v = new EditorView({ state: EditorState.create({ doc: "```dataview\nLIST\n```", selection: EditorSelection.cursor(11) }) });
    expect(closeFenceOnEnter(v)).toBe(false);
    const w = new EditorView({ state: EditorState.create({ doc: "```js", selection: EditorSelection.cursor(5) }) });
    expect(closeFenceOnEnter(w)).toBe(false);
  });
});

describe("diagnostics", () => {
  it("underlines the tasks line the engine can't read", () => {
    const doc = "```tasks\nnot done\nsort by banana\n```";
    const d = queryDiagnostics(state(doc));
    expect(d).toHaveLength(1);
    expect(doc.slice(d[0].from, d[0].to)).toBe("sort by banana");
  });
  it("points at where a dataview query goes wrong", () => {
    const doc = "```dataview\nTABLE\nWHERE (x\n```";
    const d = queryDiagnostics(state(doc));
    expect(d).toHaveLength(1);
    expect(d[0].from).toBeGreaterThan(doc.indexOf("WHERE"));
    expect(queryDiagnostics(state("```dataview\nLIST FROM #a\n```"))).toEqual([]);
  });
});

describe("the builder", () => {
  it("makes a tasks query the engine reads, and reads it back", () => {
    const f = {
      ...emptyTasksForm(),
      dates: [{ field: "due" as const, op: "before" as const, value: "next week" }],
      tagsInclude: ["work"],
      tagsExclude: ["#someday-maybe"],
      folders: ["Clients"],
      priority: { op: "is above" as const, value: "medium" },
      recurring: "no" as const,
      sort: [{ key: "due", reverse: false }],
      group: [{ key: "filename", reverse: true }],
      limit: 20,
      shortMode: true,
      hide: ["backlink"],
    };
    const text = tasksText(f);
    expect(text).toBe(
      "not done\ndue before next week\ntags include #work\ntags do not include #someday-maybe\npath includes Clients\npriority is above medium\nis not recurring\nsort by due\ngroup by filename reverse\nlimit 20\nshort mode\nhide backlink",
    );
    expect(parseTasks(text, { path: "", scripts: true }).error).toBeNull();
    expect(tasksText(parseTasksForm(text))).toBe(text);
  });
  it("keeps lines the tasks form can't show", () => {
    const f = parseTasksForm("not done\n(due before today) OR (has scheduled date)\nsort by due");
    expect(f.extra).toEqual(["(due before today) OR (has scheduled date)"]);
    expect(tasksText(f)).toContain("(due before today) OR (has scheduled date)");
  });
  it("reads yes-or-no fields in a dataview WHERE", () => {
    const line =
      'WHERE !completed AND !contains(tags, "#followup") AND !contains(tags, "#waiting-for") AND !contains(tags, "#someday-maybe")';
    const f = parseDvForm(`TASK\n${line}`);
    expect(f.extra).toEqual([]);
    expect(f.where[0]).toEqual({ field: "completed", op: "is false", value: "" });
    expect(f.where[1]).toEqual({ field: "tags", op: "does not contain", value: "#followup" });
    expect(dvText(f)).toContain(line);
    expect(parseDvForm("TASK\nWHERE fullyCompleted").where[0].op).toBe("is true");
  });
  it("reads a single filter in brackets", () => {
    const f = parseTasksForm("not done\n(tags include #followup/Lena)\n( path includes Clients )");
    expect(f.tagsInclude).toEqual(["#followup/Lena"]);
    expect(f.folders).toEqual(["Clients"]);
    expect(f.extra).toEqual([]);
    expect(parseTasksForm("(tags include #a) OR (tags include #b)").extra).toHaveLength(1);
    // A template's query: a Templater tag in the tag, brackets in it or not.
    const tp = parseTasksForm('not done\n(tags include #followup/<% name %>)\ntags do not include #y<% tp.date.now("YYYY") %>');
    expect(tp.tagsInclude).toEqual(["#followup/<% name %>"]);
    expect(tp.tagsExclude).toEqual(['#y<% tp.date.now("YYYY") %>']);
    expect(tp.extra).toEqual([]);
    expect(parseTasksForm("tags include #a #b").extra).toHaveLength(1);
    expect(parseTasksForm("(NOT done)").extra).toEqual(["(NOT done)"]);
  });
  it("makes dataview queries the engine reads, and reads them back", () => {
    const f = {
      ...emptyDvForm("TABLE"),
      fields: [
        { expr: "file.mtime", alias: "Changed" },
        { expr: "owner", alias: "" },
      ],
      from: [
        { kind: "tag" as const, value: "project", not: false },
        { kind: "folder" as const, value: "Archive", not: true },
      ],
      where: [
        { field: "status", op: "!=" as const, value: "done" },
        { field: "file.tags", op: "contains" as const, value: "#work" },
      ],
      sort: [{ field: "file.mtime", dir: "DESC" as const }],
      limit: 10,
    };
    const text = dvText(f);
    expect(text).toBe(
      'TABLE file.mtime AS "Changed", owner\nFROM #project and -"Archive"\nWHERE status != "done" AND contains(file.tags, "#work")\nSORT file.mtime DESC\nLIMIT 10',
    );
    expect(() => parseDql(text)).not.toThrow();
    expect(dvText(parseDvForm(text))).toBe(text);
    for (const t of ["LIST", "TASK", "CALENDAR"] as const) expect(() => parseDql(dvText(emptyDvForm(t)))).not.toThrow();
  });
  it("writes values as Dataview reads them", () => {
    expect(dvLiteral("3")).toBe("3");
    expect(dvLiteral("date(today)")).toBe("date(today)");
    expect(dvLiteral("[[Note]]")).toBe("[[Note]]");
    expect(dvLiteral("open")).toBe('"open"');
  });
});
