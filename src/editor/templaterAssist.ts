// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Help with Templater while editing a template: inside a `<% … %>` tag the editor completes the
// script as JavaScript, the way Templater runs it (src/notes/templater.ts compiles every tag of a
// template into one async function, so a name declared in an earlier `<%* %>` is in scope later).
// It offers the keywords (const, await…), the names the template has declared, `tp`, `tR` and
// `moment`, the JavaScript globals, and after `tp.` the modules and functions the engine runs, each
// with its arguments, a line of meaning and its docs page. A tp function goes in with its arguments
// as fields (Tab moves between them; `${2:0}` is CodeMirror's numbered field, so a number shows) and
// closes the tag with `%>` if it isn't already. tp.web, tp.obsidian and tp.app aren't offered: the
// engine refuses them.

import { Completion, CompletionContext, CompletionResult, snippetCompletion, startCompletion } from "@codemirror/autocomplete";
import { completionPath, javascriptLanguage, localCompletionSource, scopeCompletionSource } from "@codemirror/lang-javascript";
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState, Extension, Facet } from "@codemirror/state";
import { hoverTooltip, ViewPlugin, ViewUpdate } from "@codemirror/view";
import moment from "moment";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { settings } from "../store";

/** Whether the editor holds a template, so the help is on. */
const inTemplate = Facet.define<boolean, boolean>({ combine: (v) => v.some(Boolean) });

const D = "https://silentvoid13.github.io/Templater/internal-functions/internal-modules/";
const DOCS = {
  date: `${D}date-module.html`,
  file: `${D}file-module.html`,
  frontmatter: `${D}frontmatter-module.html`,
  config: `${D}config-module.html`,
  hooks: `${D}hooks-module.html`,
  system: `${D}system-module.html`,
  user: "https://silentvoid13.github.io/Templater/user-functions/script-user-functions.html",
};

export interface TpItem {
  /** What's typed to find it, and shown: `tp.date.now`. */
  label: string;
  /** The arguments, as shown: `(format, offset, reference, reference_format)`. */
  detail: string;
  info: string;
  docs: string;
  /** What's inserted, with `${name}` fields (CodeMirror's snippet syntax). */
  snippet: string;
}

const fn = (label: string, args: string, snippet: string, info: string, docs: string): TpItem => ({
  label,
  detail: args,
  info,
  docs,
  snippet,
});

/** Every function and value the engine supports, in Templater's order. */
export const TP: TpItem[] = [
  fn(
    "tp.date.now",
    "(format, offset, reference, reference_format)",
    'tp.date.now("${YYYY-MM-DD}")',
    'Today\'s date in a format; offset adds days (a number) or a duration ("P1W"), from a reference date if given.',
    DOCS.date,
  ),
  fn("tp.date.tomorrow", "(format)", 'tp.date.tomorrow("${YYYY-MM-DD}")', "Tomorrow's date in a format.", DOCS.date),
  fn("tp.date.yesterday", "(format)", 'tp.date.yesterday("${YYYY-MM-DD}")', "Yesterday's date in a format.", DOCS.date),
  fn(
    "tp.date.weekday",
    "(format, weekday, reference, reference_format)",
    'tp.date.weekday("${YYYY-MM-DD}", ${2:0})',
    "A day of this week (0 is the first day, 7 next week's), or of a reference date's week.",
    DOCS.date,
  ),
  fn("tp.file.title", "", "tp.file.title", "The new note's title.", DOCS.file),
  fn("tp.file.content", "", "tp.file.content", "The note's content: empty, as a new note has none yet.", DOCS.file),
  fn("tp.file.tags", "", "tp.file.tags", "The note's tags: none yet in a new note.", DOCS.file),
  fn(
    "tp.file.creation_date",
    "(format)",
    'tp.file.creation_date("${YYYY-MM-DD HH:mm}")',
    "When the note was made: now, for a new note.",
    DOCS.file,
  ),
  fn(
    "tp.file.last_modified_date",
    "(format)",
    'tp.file.last_modified_date("${YYYY-MM-DD HH:mm}")',
    "When the note last changed: now, for a new note.",
    DOCS.file,
  ),
  fn("tp.file.cursor", "(order)", "tp.file.cursor(${1:1})", "Where the caret goes when the new note opens.", DOCS.file),
  fn(
    "tp.file.cursor_append",
    "(content)",
    'tp.file.cursor_append("${text}")',
    "Text put in at the first cursor once the template has run.",
    DOCS.file,
  ),
  fn(
    "tp.file.create_new",
    "(template, filename, open_new, folder)",
    'await tp.file.create_new("${text}", "${Untitled}")',
    "Makes another note, from a template (tp.file.find_tfile) or text. Needs await, in a <%* block.",
    DOCS.file,
  ),
  fn("tp.file.exists", "(filepath)", 'await tp.file.exists("${path.md}")', "Whether a file is in the vault. Needs await.", DOCS.file),
  fn("tp.file.find_tfile", "(filename)", 'tp.file.find_tfile("${name}")', "A file in the vault, by name or path.", DOCS.file),
  fn(
    "tp.file.folder",
    "(absolute)",
    "tp.file.folder(${false})",
    "The new note's folder: its name, or its whole path with true.",
    DOCS.file,
  ),
  fn(
    "tp.file.include",
    "(include_link)",
    'await tp.file.include("[[${Template}]]")',
    "Another file's text, its templates run; [[note#Heading]] takes one section.",
    DOCS.file,
  ),
  fn("tp.file.move", "(new_path)", 'await tp.file.move("${folder/Name}")', "Puts the new note in another folder, by name.", DOCS.file),
  fn("tp.file.path", "(relative)", "tp.file.path(${true})", "The new note's path: in the vault with true, else the whole path.", DOCS.file),
  fn("tp.file.rename", "(new_title)", 'await tp.file.rename("${title}")', "Names the new note.", DOCS.file),
  fn("tp.file.selection", "()", "tp.file.selection()", "The selected text: empty when making a note.", DOCS.file),
  fn("tp.frontmatter", "", "tp.frontmatter.${name}", "The note's properties: none yet in a new note.", DOCS.frontmatter),
  fn("tp.config.template_file", "", "tp.config.template_file", "The template being run.", DOCS.config),
  fn("tp.config.target_file", "", "tp.config.target_file", "The note being made.", DOCS.config),
  fn("tp.config.run_mode", "", "tp.config.run_mode", "How the template was started: 0, making a new note.", DOCS.config),
  fn("tp.config.active_file", "", "tp.config.active_file", "The note open when the template was started, if any.", DOCS.config),
  fn(
    "tp.hooks.on_all_templates_executed",
    "(callback)",
    "tp.hooks.on_all_templates_executed(async () => {\n  ${}\n})",
    "Runs once the template and those it includes have run.",
    DOCS.hooks,
  ),
  fn("tp.system.clipboard", "()", "await tp.system.clipboard()", "What's on the clipboard. Needs await.", DOCS.system),
  fn(
    "tp.system.prompt",
    "(prompt_text, default_value, throw_on_cancel, multiline)",
    'await tp.system.prompt("${Question}")',
    "Asks a question in the New note dialog and gives the answer. Needs await.",
    DOCS.system,
  ),
  fn(
    "tp.system.suggester",
    "(text_items, items, throw_on_cancel, placeholder, limit)",
    'await tp.system.suggester(["${A}", "B"], ["A", "B"])',
    "Offers a pick list and gives the item chosen. Needs await.",
    DOCS.system,
  ),
  fn(
    "tp.system.multi_suggester",
    "(text_items, items, throw_on_cancel, title, limit)",
    'await tp.system.multi_suggester(["${A}", "B"], ["A", "B"])',
    "Offers a list to pick several from, and gives those chosen. Needs await.",
    DOCS.system,
  ),
];

/** The tag the caret is in, if any: where its code starts (after `<%`, `*`, `-`, `_` and spaces). */
export function openTag(before: string): { codeStart: number } | null {
  const open = before.lastIndexOf("<%");
  if (open < 0 || before.lastIndexOf("%>") > open) return null;
  const m = /^<%[*_-]?\s*/.exec(before.slice(open));
  return { codeStart: open + (m ? m[0].length : 2) };
}

/** Whether the tag is closed after the caret, before another opens. */
export function closedAfter(after: string): boolean {
  const close = after.indexOf("%>");
  const open = after.indexOf("<%");
  return close >= 0 && (open < 0 || close < open);
}

function infoNode(item: TpItem): () => Node {
  return () => {
    const d = document.createElement("div");
    d.className = "cm-qinfo";
    if (item.label || item.detail) {
      const s = document.createElement("code");
      s.textContent = item.label + item.detail;
      d.append(s);
    }
    const p = document.createElement("p");
    p.textContent = item.info;
    d.append(p);
    if (!item.docs) return d;
    const a = document.createElement("a");
    a.href = item.docs;
    a.textContent = "Docs ↗";
    a.className = "qdocs";
    a.addEventListener("mousedown", (e) => e.preventDefault());
    a.addEventListener("click", (e) => {
      e.preventDefault();
      void openUrl(item.docs);
    });
    d.append(a);
    return d;
  };
}

/** The user scripts, as `tp.user.<name>()`, looked up again after a while (they're files). */
let scripts: { at: number; items: TpItem[] } | null = null;
async function userItems(): Promise<TpItem[]> {
  if (scripts && Date.now() - scripts.at < 30_000) return scripts.items;
  const folder = settings.get().templateScripts ?? "Templates/scripts";
  const list = (await api.userScripts(folder).catch(() => [])) ?? [];
  const items = list.map((s) => fn(`tp.user.${s.name}`, "()", `tp.user.${s.name}(\${})`, `Your script ${folder}/${s.name}.js.`, DOCS.user));
  scripts = { at: Date.now(), items };
  return items;
}

/** A tp function as the member being typed (`now` after `tp.date.`): its snippet without the
 *  path, and `%>` after it when the tag is open. */
function memberOption(item: TpItem, prefix: string, close: string): Completion {
  const tail = item.snippet.replace(/^await /, "").slice(prefix.length);
  return snippetCompletion(tail + close, {
    label: item.label.slice(prefix.length),
    detail: item.detail,
    info: infoNode(item),
    type: item.detail ? "function" : "property",
    boost: 2,
  });
}

/** The modules after `tp.`. */
const MODULES: [string, string, string][] = [
  ["date", "Dates, formatted with moment.js formats.", DOCS.date],
  ["file", "The note being made: its title, folder, cursor, includes and more.", DOCS.file],
  ["frontmatter", "The note's properties: none yet in a new note.", DOCS.frontmatter],
  ["config", "The template being run and the note it makes.", DOCS.config],
  ["hooks", "Code to run once every template has run.", DOCS.hooks],
  ["system", "Prompts, pick lists and the clipboard.", DOCS.system],
  ["user", "Your own scripts, in the user scripts folder (Settings › Notes).", DOCS.user],
];

const MDN = "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/";

/** What each keyword does, and its MDN page under Reference/. */
const KEYWORDS: Record<string, [string, string]> = {
  await: ["Waits for a promise and gives its value: tp.system.prompt, tp.file.include and the rest need it.", "Operators/await"],
  async: ["Makes a function that can await, and returns a promise.", "Statements/async_function"],
  const: ["Declares a name that can't be given another value. Later tags can use it.", "Statements/const"],
  let: ["Declares a name whose value can change. Later tags can use it.", "Statements/let"],
  var: ["Declares a name the old way; prefer let or const.", "Statements/var"],
  function: ["Declares a function.", "Statements/function"],
  return: ["Ends a function, giving back a value.", "Statements/return"],
  if: ["Runs code only when a condition is true.", "Statements/if...else"],
  else: ["The code to run when the if's condition is false.", "Statements/if...else"],
  for: ["Repeats code: for (const x of list) goes through a list.", "Statements/for"],
  of: ["In for (const x of list): each item of a list in turn.", "Statements/for...of"],
  in: ['Whether an object has a property ("a" in obj), or for (const k in obj) over its keys.', "Operators/in"],
  while: ["Repeats code while a condition is true.", "Statements/while"],
  do: ["Runs code once, then repeats it while a condition is true.", "Statements/do...while"],
  switch: ["Picks code to run by comparing a value with each case.", "Statements/switch"],
  case: ["One choice in a switch.", "Statements/switch"],
  default: ["The switch's choice when no case matches.", "Statements/switch"],
  break: ["Leaves a loop or a switch.", "Statements/break"],
  continue: ["Skips to a loop's next turn.", "Statements/continue"],
  try: ["Runs code and catches an error it throws.", "Statements/try...catch"],
  catch: ["The code to run when the try's code throws an error.", "Statements/try...catch"],
  finally: ["Code that runs after try and catch, either way.", "Statements/try...catch"],
  throw: ['Stops with an error: throw new Error("why").', "Statements/throw"],
  new: ["Makes an object from a class: new Date().", "Operators/new"],
  typeof: ['The kind of a value, as text: "string", "number"…', "Operators/typeof"],
  instanceof: ["Whether a value was made by a class.", "Operators/instanceof"],
  delete: ["Takes a property off an object.", "Operators/delete"],
  void: ["Runs an expression and gives undefined.", "Operators/void"],
  true: ["Yes.", "Global_Objects/Boolean"],
  false: ["No.", "Global_Objects/Boolean"],
  null: ["No value, on purpose.", "Operators/null"],
  undefined: ["No value: what a name holds before it's given one.", "Global_Objects/undefined"],
  this: ["The object a function was called on.", "Operators/this"],
  class: ["Declares a class.", "Statements/class"],
};

/** What the JavaScript globals are, for their completions and hovers. */
const GLOBAL_INFO: Record<string, string> = {
  Math: "Maths: Math.round, Math.floor, Math.random, Math.max…",
  JSON: "Turns values into JSON text and back: JSON.stringify, JSON.parse.",
  Date: "JavaScript's own dates; moment is easier for formatting.",
  Array: "Lists: Array.from, Array.isArray.",
  Object: "Objects: Object.keys, Object.entries, Object.assign.",
  String: "Text: String(value) makes text of anything.",
  Number: 'Numbers: Number("3") makes a number of text.',
  Boolean: "true or false: Boolean(value).",
  Promise: "A value that comes later; await one to get it.",
  RegExp: "Patterns for finding text.",
  Error: 'An error to throw: throw new Error("why").',
  console: "Logs to the developer console: console.log.",
  parseInt: 'A whole number from text: parseInt("42").',
  parseFloat: 'A number from text: parseFloat("4.2").',
  isNaN: "Whether a value isn't a number.",
  encodeURIComponent: "Text made safe for a URL.",
  decodeURIComponent: "URL text turned back.",
};

const info = (text: string, docs?: string, head = ""): (() => Node) =>
  infoNode({ label: head, detail: "", info: text, docs: docs ?? "", snippet: "" });

/** The docs and a line for a member of a global (Math.floor, moment.duration). */
function memberInfo(path: readonly string[], name: string): (() => Node) | undefined {
  if (path[0] === "moment") return info(`moment.js: ${path.join(".")}.${name}.`, "https://momentjs.com/docs/", `${path.join(".")}.${name}`);
  if (path.length === 1 && path[0] in GLOBAL_INFO)
    return info(`${path[0]}.${name}, from JavaScript's ${path[0]}.`, `${MDN}Global_Objects/${path[0]}/${name}`, `${path[0]}.${name}`);
  return undefined;
}

/** The names a template has besides the JavaScript ones: tp, tR and moment. */
const TEMPLATE_NAMES: Completion[] = [
  {
    label: "tp",
    type: "namespace",
    detail: "Templater",
    info: info("Templater's functions: tp.date, tp.file, tp.system and the rest.", D + "../overview.html"),
    boost: 3,
  },
  {
    label: "tR",
    type: "variable",
    detail: "string",
    info: info(
      'What the template has written so far. In a <%* %> block, tR += "text" writes more.',
      "https://silentvoid13.github.io/Templater/commands/execution-command.html",
    ),
    boost: 3,
  },
  {
    label: "moment",
    type: "function",
    detail: "(date, format)",
    info: info('moment.js, for dates: moment().add(1, "week").format("YYYY-MM-DD").', "https://momentjs.com/docs/"),
    boost: 2,
  },
];

/** What the JavaScript globals complete from (members too: Math.floor, moment.duration…). */
const GLOBALS: Record<string, unknown> = {
  moment,
  Math,
  JSON,
  Date,
  Array,
  Object,
  String,
  Number,
  Boolean,
  Promise,
  RegExp,
  Error,
  console,
  parseInt,
  parseFloat,
  isNaN,
  encodeURIComponent,
  decodeURIComponent,
};
const globalSource = scopeCompletionSource(GLOBALS);

/** The script so far, as Templater compiles it: the code of every tag before the caret, in order
 *  (comment tags left out), then the tag the caret is in, up to the caret. */
export function scriptBefore(before: string): string | null {
  const tag = openTag(before);
  if (!tag) return null;
  let js = "";
  for (const m of before.slice(0, before.lastIndexOf("<%")).matchAll(/<%[_-]?([*+#]?)[_-]?([\s\S]*?)[_-]?%>/g)) {
    if (m[1] !== "#") js += `${m[2]}\n;\n`;
  }
  return js + before.slice(tag.codeStart);
}

/** What to offer at the caret in a template: the script's completions, inside a `<% … %>` tag. */
export async function templaterCompletions(ctx: CompletionContext): Promise<CompletionResult | null> {
  if (!ctx.state.facet(inTemplate)) return null;
  const before = ctx.state.sliceDoc(0, ctx.pos);
  const js = scriptBefore(before);
  if (js === null || /^<%#/.test(before.slice(before.lastIndexOf("<%")))) return null;
  const state = EditorState.create({ doc: js, extensions: [javascriptLanguage] });
  // A generous budget: a short one ran out on a busy machine and left the names declared earlier out.
  ensureSyntaxTree(state, js.length, 2000);
  // Just after `<%` it's as if asked for: the list opens before anything is typed.
  const fresh = /<%[*_-]?\s*$/.test(before);
  const jctx = new CompletionContext(state, js.length, ctx.explicit || fresh);
  const where = completionPath(jctx);
  // In a string or a comment there's nothing to offer.
  if (!where) return null;
  const { path, name } = where;
  if (!name && !path.length && !ctx.explicit && !fresh) return null;
  const from = ctx.pos - name.length;
  const close = closedAfter(ctx.state.sliceDoc(ctx.pos, Math.min(ctx.state.doc.length, ctx.pos + 2000))) ? "" : " %>";
  let options: Completion[];
  if (path[0] === "tp") {
    const items = [...TP, ...(await userItems())];
    if (path.length === 1) {
      options = MODULES.map(([m, text, docs]) => ({ label: m, type: "namespace", info: info(text, docs), boost: 2 }));
    } else {
      const prefix = `${path.join(".")}.`;
      options = items
        .filter((i) => i.label.startsWith(prefix) && !i.label.slice(prefix.length).includes("."))
        .map((i) => memberOption(i, prefix, close));
    }
  } else if (path.length) {
    const r = await globalSource(jctx);
    options = (r?.options ?? []).map((o) => ({ ...o, info: o.info ?? memberInfo(path, o.label) }));
  } else {
    const locals = localCompletionSource(jctx)?.options ?? [];
    const globals = (await globalSource(jctx))?.options ?? [];
    options = [
      ...TEMPLATE_NAMES,
      ...Object.entries(KEYWORDS).map(([k, [text, docs]]): Completion => ({ label: k, type: "keyword", info: info(text, MDN + docs, k) })),
      ...locals.map((o) => ({
        ...o,
        boost: 4,
        info: info("A name this template declares in an earlier tag or above.", undefined, o.label),
      })),
      ...globals
        .filter((g) => g.label !== "moment")
        .map((g) => ({
          ...g,
          info: g.label in GLOBAL_INFO ? info(GLOBAL_INFO[g.label], `${MDN}Global_Objects/${g.label}`, g.label) : undefined,
        })),
    ];
  }
  if (ctx.aborted || !options.length) return null;
  // The whole word around the caret (put mid-word) comes first, so it's the one chosen.
  const w = ctx.state.wordAt(ctx.pos);
  const whole = w && w.to > ctx.pos ? ctx.state.sliceDoc(w.from, w.to) : null;
  if (whole) options = options.map((o) => (o.label === whole ? { ...o, boost: 99 } : o));
  return { from, options, validFor: /^[\w$]*$/ };
}

/** The name at `pos` in a tag, and what it is (as its completion explains it), or null. */
export async function explainAt(state: EditorState, pos: number): Promise<{ from: number; to: number; option: Completion } | null> {
  const w = state.wordAt(pos);
  if (!w) return null;
  const r = await templaterCompletions(new CompletionContext(state, w.to, true));
  const word = state.sliceDoc(w.from, w.to);
  const option = r?.options.find((o) => o.label === word);
  return option?.info ? { from: w.from, to: w.to, option } : null;
}

/** What the name at `pos` in a template's text is, as the editor's hover explains it, for View
 *  (src/md/Markdown.tsx); null when there's nothing to say. */
export async function explainInText(text: string, pos: number): Promise<Node | null> {
  const state = EditorState.create({ doc: text, extensions: [inTemplate.of(true)] });
  const e = await explainAt(state, pos);
  if (!e || typeof e.option.info !== "function") return null;
  const node = await e.option.info(e.option);
  return node ? ("dom" in node ? node.dom : node) : null;
}

/** Hovering a name in a tag explains it, as its completion does. */
const hover = hoverTooltip(async (view, pos) => {
  if (!view.state.facet(inTemplate)) return null;
  const e = await explainAt(view.state, pos);
  if (!e || typeof e.option.info !== "function") return null;
  const node = await e.option.info(e.option);
  if (!node) return null;
  return {
    pos: e.from,
    end: e.to,
    above: true,
    create: () => {
      const dom = document.createElement("div");
      dom.className = "cm-tpinfo";
      dom.append("dom" in node ? node.dom : node);
      return { dom };
    },
  };
});

/** Putting the caret on a name in a tag (by clicking or the arrow keys) opens the list there, with
 *  that name chosen and explained. */
const caret = ViewPlugin.fromClass(
  class {
    update(u: ViewUpdate) {
      if (u.docChanged || !u.selectionSet || !u.transactions.some((t) => t.isUserEvent("select"))) return;
      const sel = u.state.selection.main;
      if (!sel.empty || !u.state.wordAt(sel.head)) return;
      if (!openTag(u.state.sliceDoc(0, sel.head))) return;
      const view = u.view;
      void explainAt(u.state, sel.head).then((e) => {
        if (e && view.state.selection.main.head === sel.head) startCompletion(view);
      });
    }
  },
);

/** Turns the help on, for a template's editor (with the Templater colouring): completion, hover
 *  and the caret's explanations. */
export const templaterAssist: Extension = [inTemplate.of(true), hover, caret];
