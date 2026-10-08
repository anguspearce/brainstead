// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Templater (https://silentvoid13.github.io/Templater/), run in the window. A template is
// compiled to an async function and run top to bottom; prompts are asked as the run reaches
// them, through `Env` (src/notes/TemplateRun.tsx draws them). Templates are the vault owner's
// own code (decided 2026-10-02), so a template runs as a Function with no sandbox beyond the
// `tp` it's given.
//
// Supported: the commands `<% %>`, `<%* %>`, `<%+ %>` (run once) and `<%# %>`, whitespace
// control (`<%_` `<%-` `-%>` `_%>`), `tR`, `moment`, and tp.date, tp.file, tp.frontmatter,
// tp.config, tp.hooks, tp.system and tp.user (scripts in a vault folder). Not available:
// tp.obsidian, `app` and tp.web (another app's internals, and the internet), which fail with a
// message saying so. Dates use moment with the en-GB locale, so weeks start on Monday.

import moment from "moment";
import "moment/locale/en-gb";

/** A file as Templater hands them round (its TFile): vault-relative. */
export interface TFile {
  path: string;
  /** The name without the extension. */
  basename: string;
  name: string;
  extension: string;
}

export function tfile(path: string): TFile {
  const name = path.split("/").pop() ?? path;
  const dot = name.lastIndexOf(".");
  return { path, name, basename: dot > 0 ? name.slice(0, dot) : name, extension: dot > 0 ? name.slice(dot + 1) : "" };
}

export interface PromptRequest {
  kind: "prompt";
  text: string;
  value: string;
  multiline: boolean;
  selectAll: boolean;
}
export interface SuggestRequest {
  kind: "suggest" | "multi";
  /** The placeholder (suggester) or title (multi_suggester). */
  text: string;
  labels: string[];
  limit?: number;
  /** Indexes chosen to start with. */
  chosen: number[];
}
export type Ask = PromptRequest | SuggestRequest;

/** What a run needs from the app. */
export interface Env {
  /** The vault's folder on disk, for tp.file.path(). */
  root: string;
  /** Every file in the vault, vault-relative: for tp.file.exists and find_tfile. */
  files: string[];
  read(path: string): Promise<string>;
  create(path: string, content: string): Promise<void>;
  clipboard(): Promise<string>;
  /** Asks; resolves to the answer (a string, or the chosen indexes), or null when cancelled. */
  ask(q: PromptRequest): Promise<string | null>;
  ask(q: SuggestRequest): Promise<number[] | null>;
  /** tp.user's scripts: file name (no `.js`) and source. */
  scripts?: { name: string; source: string }[];
  templateFile: TFile;
  activeFile?: TFile | null;
  now?: Date;
}

export interface RunResult {
  /** Where the note goes, vault-relative with `.md`: from tp.file.rename / move, else "Untitled.md". */
  path: string;
  /** Whether the template named the note (rename or move). */
  named: boolean;
  content: string;
  /** Where the caret goes: the lowest-numbered tp.file.cursor(), or null. */
  cursor: number | null;
  /** Every cursor, in order. */
  cursors: number[];
  /** tp.hooks.on_all_templates_executed callbacks, to run once the note is written. */
  hooks: (() => unknown)[];
  /** Notes made by tp.file.create_new to open afterwards. */
  open: string[];
}

/** Raised by an Env to stop a run (the window's Stop); not shown as an error. */
export class StopRun extends Error {
  constructor() {
    super("Stopped.");
    this.name = "StopRun";
  }
}

const unavailable = (what: string, why: string) => new Error(`${what} isn't available in Brainstead: ${why}.`);

/** Something that fails, naming itself, on any use. */
function refuse(what: string, why: string): unknown {
  const fail = () => {
    throw unavailable(what, why);
  };
  return new Proxy(fail, { get: fail, apply: fail, construct: fail });
}

const CUR_A = "\u{f0000}";
const CUR_B = "\u{f0001}";
const CURSOR_MARK = /\u{f0000}(-?\d*(?:\.\d+)?)\u{f0001}/gu;

type Token = { text: string } | { mode: "" | "*" | "+" | "#"; code: string; pre: "" | "-" | "_"; post: "" | "-" | "_" };

/** Templater's tags, with whitespace control applied to the text around them. */
export function tokens(src: string): Token[] {
  const re = /<%([_-]?)([*+#]?)([_-]?)([\s\S]*?)([_-]?)%>/g;
  const out: Token[] = [];
  let last = 0;
  for (let m; (m = re.exec(src));) {
    out.push({ text: src.slice(last, m.index) });
    const pre = (m[1] || m[3]) as "" | "-" | "_";
    out.push({ mode: m[2] as "" | "*" | "+" | "#", code: m[4], pre, post: m[5] as "" | "-" | "_" });
    last = m.index + m[0].length;
  }
  out.push({ text: src.slice(last) });
  // `<%_` trims all whitespace before the tag, `<%-` one newline; `_%>` and `-%>` the same after.
  for (let i = 0; i < out.length; i++) {
    const t = out[i];
    if ("text" in t) continue;
    const before = out[i - 1] as { text: string } | undefined;
    const after = out[i + 1] as { text: string } | undefined;
    if (before && "text" in before) {
      if (t.pre === "_") before.text = before.text.replace(/\s+$/, "");
      else if (t.pre === "-") before.text = before.text.replace(/\r?\n$/, "");
    }
    if (after && "text" in after) {
      if (t.post === "_") after.text = after.text.replace(/^\s+/, "");
      else if (t.post === "-") after.text = after.text.replace(/^\r?\n/, "");
    }
  }
  return out;
}

/** A template as the body of an async function of (tp, moment, app) that returns `tR`. */
export function compile(src: string): string {
  let body = "let tR = '';\n";
  for (const t of tokens(src)) {
    if ("text" in t) {
      if (t.text) body += `tR += ${JSON.stringify(t.text)};\n`;
    } else if (t.mode === "*") body += `${t.code}\n`;
    else if (t.mode !== "#" && t.code.trim()) body += `tR += __s(await (${t.code}\n));\n`;
  }
  return body + "return tR;";
}

const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor as new (...args: string[]) => (...a: unknown[]) => Promise<string>;

function build(src: string) {
  return new AsyncFunction("tp", "moment", "app", "__s", compile(src));
}

/** Why a template can't run, without running it (a syntax error), or null. */
export function checkTemplate(src: string): string | null {
  try {
    build(src);
    return null;
  } catch (e) {
    return e instanceof Error ? e.message : String(e);
  }
}

/** What an interpolation writes: nothing for null and undefined. */
const str = (v: unknown) => (v === null || v === undefined ? "" : String(v));

const m = (d?: moment.MomentInput, f?: string) => (f ? moment(d, f) : moment(d)).locale("en-gb");

/** `[[Note#Heading]]`, `[[Note#^block]]`, `Note`, as a link and its section. */
export function parseInclude(link: string): { target: string; heading?: string; block?: string } {
  const inner = link
    .trim()
    .replace(/^!?\[\[/, "")
    .replace(/\]\]$/, "")
    .split("|")[0];
  const hash = inner.indexOf("#");
  const target = (hash < 0 ? inner : inner.slice(0, hash)).trim();
  const frag = hash < 0 ? "" : inner.slice(hash + 1).trim();
  if (frag.startsWith("^")) return { target, block: frag.slice(1) };
  return frag ? { target, heading: frag } : { target };
}

/** The part of a note a link points at: a heading's section (to the next heading as high), a
 *  block (the paragraph or list item ending in `^id`), or all of it. */
export function section(text: string, heading?: string, block?: string): string {
  const lines = text.split("\n");
  if (heading) {
    const want = heading.toLowerCase();
    const i = lines.findIndex(
      (l) =>
        /^#{1,6}\s/.test(l) &&
        l
          .replace(/^#+\s+/, "")
          .trim()
          .toLowerCase() === want,
    );
    if (i < 0) return "";
    const level = /^#+/.exec(lines[i])![0].length;
    let j = i + 1;
    while (j < lines.length && !(/^#{1,6}\s/.test(lines[j]) && /^#+/.exec(lines[j])![0].length <= level)) j++;
    return lines.slice(i, j).join("\n").replace(/\n+$/, "");
  }
  if (block) {
    const i = lines.findIndex((l) => new RegExp(`\\s\\^${block.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*$`).test(l));
    if (i < 0) return "";
    let a = i;
    if (!/^\s*([-*+]|\d+[.)])\s/.test(lines[i])) while (a > 0 && lines[a - 1].trim()) a--;
    return lines
      .slice(a, i + 1)
      .join("\n")
      .replace(new RegExp(`\\s\\^${block}\\s*$`), "");
  }
  return text;
}

/** Runs a template to make a new note. Throws StopRun when the window stops it. */
export async function runTemplate(src: string, env: Env): Promise<RunResult> {
  const now = env.now ?? new Date();
  const target = { folder: "", title: "Untitled", named: false };
  const targetPath = () => `${target.folder ? `${target.folder}/` : ""}${target.title}.md`;
  const hooks: (() => unknown)[] = [];
  const open: string[] = [];
  const lower = new Map(env.files.map((f) => [f.toLowerCase(), f]));
  const created = new Set<string>();
  const exists = (p: string) => lower.has(p.toLowerCase()) || created.has(p.toLowerCase());

  const findTfile = (name: string): TFile | null => {
    const n = name.replace(/^\[\[|\]\]$/g, "").trim();
    const exact = lower.get(n.toLowerCase()) ?? lower.get(`${n}.md`.toLowerCase());
    if (exact) return tfile(exact);
    const base = (n.split("/").pop() ?? n).toLowerCase();
    const hit = env.files.find((f) => tfile(f).basename.toLowerCase() === base || tfile(f).name.toLowerCase() === base);
    return hit ? tfile(hit) : null;
  };

  const userScripts: Record<string, unknown> = {};
  for (const s of env.scripts ?? []) {
    const module = { exports: {} as unknown };
    const requireStub = (id: string) => {
      throw unavailable(`require("${id}")`, "user scripts can't load other modules");
    };
    try {
      new Function("module", "exports", "require", s.source)(module, module.exports, requireStub);
    } catch (e) {
      throw new Error(`The user script ${s.name}.js didn't load: ${e instanceof Error ? e.message : String(e)}`, { cause: e });
    }
    userScripts[s.name] = module.exports;
  }

  const app = refuse("app", "it's another app's internal interface");
  /** tp.file.cursor_append's text. */
  const appends: string[] = [];
  const ask = env.ask.bind(env) as (q: Ask) => Promise<string | number[] | null>;
  const cancelled = (throwOnCancel: boolean) => {
    if (throwOnCancel) throw new Error("Cancelled by the user.");
    return null;
  };

  // Runs a template's text with the same tp (for include and create_new from a template), at most
  // ten deep, as Templater stops a note that includes itself.
  let depth = 0;
  const runText = async (text: string): Promise<string> => {
    if (depth >= 10) throw new Error("Reached inclusion depth limit (max = 10)");
    depth++;
    try {
      return await build(text)(tp, moment, app, str);
    } finally {
      depth--;
    }
  };

  const tp: Record<string, unknown> = {
    date: {
      now(format = "YYYY-MM-DD", offset?: number | string, reference?: string, referenceFormat?: string) {
        const d = reference ? m(reference, referenceFormat) : m(now);
        if (typeof offset === "number") d.add(offset, "days");
        else if (typeof offset === "string" && offset) d.add(moment.duration(offset));
        return d.format(format);
      },
      tomorrow: (format = "YYYY-MM-DD") => m(now).add(1, "day").format(format),
      yesterday: (format = "YYYY-MM-DD") => m(now).subtract(1, "day").format(format),
      weekday(format = "YYYY-MM-DD", weekday = 0, reference?: string, referenceFormat?: string) {
        const d = reference ? m(reference, referenceFormat) : m(now);
        return d.weekday(weekday).format(format);
      },
    },
    file: {
      // A new note has nothing in it yet.
      content: "",
      tags: [] as string[],
      get title() {
        return target.title;
      },
      creation_date: (format = "YYYY-MM-DD HH:mm") => m(now).format(format),
      last_modified_date: (format = "YYYY-MM-DD HH:mm") => m(now).format(format),
      cursor: (order?: number) => `${CUR_A}${order ?? ""}${CUR_B}`,
      cursor_append(content: string) {
        appends.push(String(content ?? ""));
        return "";
      },
      async create_new(template: TFile | string, filename = "Untitled", openNew = false, folder?: { path: string } | string) {
        const dir = (typeof folder === "string" ? folder : (folder?.path ?? "")).replace(/^\/+|\/+$/g, "");
        const path = `${dir ? `${dir}/` : ""}${filename}.md`;
        if (exists(path)) throw new Error(`Couldn't make ${path}: there's already a note there.`);
        const text = typeof template === "string" ? template : await runText(await env.read(template.path));
        await env.create(path, text.replace(CURSOR_MARK, ""));
        created.add(path.toLowerCase());
        if (openNew) open.push(path);
        return tfile(path);
      },
      exists: async (path: string) => exists(String(path)),
      find_tfile: (name: string) => findTfile(String(name)),
      folder: (absolute = false) => (absolute ? target.folder : (target.folder.split(/[\\/]/).pop() ?? "")),
      async include(link: TFile | string) {
        const l = typeof link === "string" ? parseInclude(link) : { target: link.path };
        const f = findTfile(l.target);
        if (!f) throw new Error(`Couldn't include ${typeof link === "string" ? link : link.path}: it isn't in the vault.`);
        return runText(section(await env.read(f.path), "heading" in l ? l.heading : undefined, "block" in l ? l.block : undefined));
      },
      async move(newPath: string) {
        const p = String(newPath).replace(/^\/+/, "").replace(/\.md$/i, "");
        const i = p.lastIndexOf("/");
        target.folder = i < 0 ? "" : p.slice(0, i);
        target.title = i < 0 ? p : p.slice(i + 1);
        target.named = true;
      },
      path: (relative = false) => (relative ? targetPath() : `${env.root.replace(/[\\/]+$/, "")}/${targetPath()}`),
      async rename(newTitle: string) {
        target.title = String(newTitle);
        target.named = true;
      },
      selection: () => "",
    },
    // The new note's properties: none yet.
    frontmatter: {},
    config: {
      template_file: env.templateFile,
      get target_file() {
        return tfile(targetPath());
      },
      // Templater's RunMode.CreateNewFromTemplate.
      run_mode: 0,
      active_file: env.activeFile ?? undefined,
    },
    hooks: {
      on_all_templates_executed(cb: () => unknown) {
        hooks.push(cb);
      },
    },
    system: {
      clipboard: () => env.clipboard(),
      async prompt(text = "", value = "", throwOnCancel = false, multiline = false, selectAll = false) {
        const a = (await ask({ kind: "prompt", text: str(text), value: str(value), multiline: !!multiline, selectAll: !!selectAll })) as
          string | null;
        return a === null ? cancelled(throwOnCancel) : a;
      },
      async suggester(
        textItems: string[] | ((item: unknown) => string),
        items: unknown[],
        throwOnCancel = false,
        placeholder = "",
        limit?: number,
        defaultValue?: unknown,
      ) {
        const labels = typeof textItems === "function" ? items.map((x) => str(textItems(x))) : textItems.map(str);
        const d = defaultValue === undefined ? -1 : items.indexOf(defaultValue);
        const a = (await ask({ kind: "suggest", text: str(placeholder), labels, limit, chosen: d >= 0 ? [d] : [] })) as number[] | null;
        return a === null || !a.length ? cancelled(throwOnCancel) : items[a[0]];
      },
      async multi_suggester(
        textItems: string[] | ((item: unknown) => string),
        items: unknown[],
        throwOnCancel = false,
        title = "",
        limit?: number,
        defaultValues?: unknown[],
      ) {
        const labels = typeof textItems === "function" ? items.map((x) => str(textItems(x))) : textItems.map(str);
        const chosen = (defaultValues ?? []).map((v) => items.indexOf(v)).filter((i) => i >= 0);
        const a = (await ask({ kind: "multi", text: str(title), labels, limit, chosen })) as number[] | null;
        return a === null ? cancelled(throwOnCancel) : a.map((i) => items[i]);
      },
    },
    user: userScripts,
    obsidian: refuse("tp.obsidian", "it's another app's internal interface"),
    web: refuse("tp.web", "templates can't reach the internet here"),
    app: refuse("tp.app", "it's another app's internal interface"),
  };
  const raw = await runText(src);

  // The cursors: markers out, offsets kept; tp.file.cursor_append text goes in at the first.
  const marks: { order: number; at: number }[] = [];
  let content = "";
  let last = 0;
  for (const c of raw.matchAll(CURSOR_MARK)) {
    content += raw.slice(last, c.index);
    marks.push({ order: c[1] ? Number(c[1]) : 0, at: content.length });
    last = c.index + c[0].length;
  }
  content += raw.slice(last);
  marks.sort((a, b) => a.order - b.order || a.at - b.at);
  if (appends.length) {
    const add = appends.join("");
    const at = marks[0]?.at ?? content.length;
    content = content.slice(0, at) + add + content.slice(at);
    // The caret stays before what was appended; later cursors move along.
    for (const k of marks.slice(1)) if (k.at >= at) k.at += add.length;
  }
  return {
    path: targetPath(),
    named: target.named,
    content,
    cursor: marks[0]?.at ?? null,
    cursors: marks.map((k) => k.at),
    hooks,
    open,
  };
}

/** A free name for a note nobody named: "Untitled.md", then "Untitled 1.md"… */
export function freeName(path: string, taken: (p: string) => boolean): string {
  if (!taken(path)) return path;
  const base = path.replace(/\.md$/i, "");
  for (let i = 1; ; i++) if (!taken(`${base} ${i}.md`)) return `${base} ${i}.md`;
}
