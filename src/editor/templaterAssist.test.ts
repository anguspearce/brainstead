// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => (cmd === "user_scripts" ? [{ name: "greet", source: "" }] : null)),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));

import { closedAfter, openTag, scriptBefore, templaterAssist, templaterCompletions } from "./templaterAssist";

// jsdom has no layout; CodeMirror measures text after the view is made.
Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect = () => new DOMRect();

const complete = (doc: string, template = true, after = "") =>
  templaterCompletions(
    new CompletionContext(
      EditorState.create({ doc: doc + after, selection: { anchor: doc.length }, extensions: template ? [templaterAssist] : [] }),
      doc.length,
      false,
    ),
  ) as Promise<CompletionResult | null>;

/** Applies a completion to `doc` (caret at its end, followed by `after`) and gives the text. */
function apply(doc: string, after: string, r: CompletionResult, label: string): string {
  const view = new EditorView({
    state: EditorState.create({ doc: doc + after, selection: { anchor: doc.length }, extensions: [templaterAssist] }),
  });
  const c = r.options.find((o) => o.label === label) as Completion;
  (c.apply as (v: EditorView, c: Completion, from: number, to: number) => void)(view, c, r.from, doc.length);
  return view.state.doc.toString();
}

const labels = async (doc: string) => (await complete(doc))?.options.map((o) => o.label) ?? [];

describe("Templater help in a template", () => {
  it("knows when the caret is in a tag, and the script so far", () => {
    expect(openTag("Hi <% tp.")?.codeStart).toBe(6);
    expect(openTag("<%* const x = ")?.codeStart).toBe(4);
    expect(openTag("<% a %> after")).toBeNull();
    expect(closedAfter(" %> and <% x")).toBe(true);
    expect(closedAfter(" more <% x %>")).toBe(false);
    expect(scriptBefore("<%* const a = 1 %> text <%# note %> <% a")).toBe(" const a = 1 \n;\na");
    expect(scriptBefore("no tag here")).toBeNull();
  });

  it("offers keywords, tp, tR, moment and globals when a tag opens", async () => {
    const l = await labels("Date: <%* ");
    for (const w of ["tp", "tR", "moment", "const", "await", "let", "if", "Math"]) expect(l).toContain(w);
    expect(await labels("<%* aw")).toContain("await");
  });

  it("offers the names declared in earlier tags", async () => {
    const doc = '<%* const who = await tp.system.prompt("Who?") %>\nHello <% wh';
    expect(await labels(doc)).toContain("who");
  });

  it("offers them in a long template too, past what the first parse covers", async () => {
    const filler = `<%* const pad = [${"1, ".repeat(2000)}] %>\n`;
    const doc = `<%* const who = await tp.system.prompt("Who?") %>\n${filler}Hello <% wh`;
    expect(await labels(doc)).toContain("who");
  });

  it("offers tp's modules, then their functions, and the user scripts", async () => {
    expect(await labels("<% tp.")).toEqual(expect.arrayContaining(["date", "file", "system", "user"]));
    expect(await labels("<% tp.date.")).toEqual(expect.arrayContaining(["now", "tomorrow", "weekday"]));
    expect(await labels("<% tp.user.")).toContain("greet");
    const l = await labels("<% tp.");
    expect(l.some((x) => /^(web|obsidian|app)$/.test(x))).toBe(false);
  });

  it("completes members of the globals", async () => {
    expect(await labels("<% Math.fl")).toContain("floor");
  });

  it("offers nothing outside a tag, in a string, in a comment tag, or outside a template", async () => {
    expect(await complete("Outside <% x %> tp.")).toBeNull();
    expect(await complete('<% "tp.')).toBeNull();
    expect(await complete("<%# tp.")).toBeNull();
    expect(await complete("<% tp.", false)).toBeNull();
  });

  it("inserts a function with its arguments and closes the tag when it's open", async () => {
    expect(apply("<% tp.date.n", "", (await complete("<% tp.date.n"))!, "now")).toBe('<% tp.date.now("YYYY-MM-DD") %>');
    expect(apply("<% tp.date.n", " %>", (await complete("<% tp.date.n", true, " %>"))!, "now")).toBe('<% tp.date.now("YYYY-MM-DD") %>');
  });
});

describe("Templater code colours", () => {
  it("marks the script's tokens, not whole lines", async () => {
    const { codeTokens } = await import("../notes/templaterSyntax");
    const code = ' const who = await tp.system.prompt("Who?"); // ask ';
    const at = (cls: string) =>
      codeTokens(code)
        .filter(([, , c]) => c.includes(cls))
        .map(([a, b]) => code.slice(a, b));
    expect(at("tok-keyword")).toEqual(expect.arrayContaining(["const", "await"]));
    expect(at("tok-string")).toEqual(['"Who?"']);
    expect(at("tok-comment")).toEqual(["// ask "]);
    expect(at("tok-propertyName")).toEqual(expect.arrayContaining(["system", "prompt"]));
  });
});

describe("Templater tags in View", () => {
  it("draws each tag as coloured code, leaving markdown and code blocks alone", async () => {
    const { templaterHtml } = await import("../notes/templaterSyntax");
    const html = templaterHtml('<%* const a = "x" -%>\n**bold** <% tp.file.title %>\n```js\n<% kept %>\n```\n');
    expect(html).toContain('<span class="cm-tp-tag">&lt;%&#42;</span>');
    expect(html).toMatch(/<span class="tok-keyword" data-tp="\d+">const<\/span>/);
    expect(html).toContain("**bold**");
    expect(html).toContain("```js\n<% kept %>\n```");
  });

  it("marks each token with where it starts, so hovering one explains it as in the editor", async () => {
    const { templaterHtml } = await import("../notes/templaterSyntax");
    const { explainInText } = await import("./templaterAssist");
    const md = "# Day\n\n```\nskip\n```\nToday <% tp.date.now() %>";
    const html = templaterHtml(md);
    const now = /<span class="[^"]*" data-tp="(\d+)">now<\/span>/.exec(html);
    expect(now).toBeTruthy();
    expect(md.slice(Number(now![1]), Number(now![1]) + 3)).toBe("now");
    const tip = await explainInText(md, Number(now![1]) + 1);
    expect(tip?.textContent).toMatch(/date/i);
    expect(await explainInText(md, md.indexOf("Today") + 1)).toBeNull();
  });
});

describe("Explaining a name in a tag", () => {
  const at = async (doc: string, word: string) => {
    const { explainAt } = await import("./templaterAssist");
    const st = EditorState.create({ doc, extensions: [templaterAssist] });
    const e = await explainAt(st, doc.indexOf(word) + 1);
    if (!e || typeof e.option.info !== "function") return null;
    const n = (await e.option.info(e.option)) as HTMLElement;
    return n.textContent;
  };

  it("explains keywords, tp functions, globals and the template's own names", async () => {
    const doc = '<%* const who = await tp.system.prompt("Who?"); const n = Math.floor(2) %>Hi <% who %>';
    expect(await at(doc, "await")).toMatch(/Waits for a promise/);
    expect(await at(doc, "prompt")).toMatch(/Asks a question/);
    expect(await at(doc, "floor")).toMatch(/Math\.floor/);
    expect(await at(doc, "who %>")).toMatch(/declares/);
    expect(await at(doc, "Hi")).toBeNull();
  });
});
