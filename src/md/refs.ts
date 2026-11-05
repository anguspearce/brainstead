// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Links in plain text, found one way everywhere a line of text is drawn (task rows, Dataview
// values, the editor, a note's properties and the References cards): `[label](url)`, a bare
// http(s) URL, and reference keys. A reference key is a configured prefix followed by digits
// (`ORBIT-` and `ORBIT-12`), as Settings › Notes › Reference links sets them; each pattern's URL
// template turns a key into the page it opens. With no patterns set only the first two are found.
//
// One scan reads the text a character at a time, left to right, and knows the markdown that is not
// plain text: code spans, wikilinks, images and links of any kind. What is inside those is never a
// key, so every surface that uses it agrees on what is a link.

import { useMemo, useSyncExternalStore } from "react";
import { settings } from "../store";

/** One Reference links row: keys that start with `prefix` open `url`, where `{key}` is the whole
 *  key and `{num}` its digits. */
export interface RefPattern {
  prefix: string;
  url: string;
}

/** A piece of text: plain, or a link with where it goes. `ref` is a reference key, `link` a
 *  `[label](url)` and `url` a bare URL (its text is the domain). */
export type RefSeg = { text: string } | { text: string; href: string; kind: "ref" | "link" | "url" };

/** A link found in a value: its text (the key, the label, or the domain) and where it goes. */
export interface Ref {
  key: string;
  url: string;
  kind: "ref" | "link" | "url";
}

/** An address the app opens in the browser or mail, not a file in the vault. */
export const isExternalHref = (href: string) => /^(https?:|mailto:)/.test(href);

const WEB = /^https?:\/\/\S+$/;
// A key has no word character (a letter, digit or `_`) directly before it, nor a `-` (a longer
// name), a `#` (a tag) or a `/` (a nested tag or a path). After it come no word character, and no
// `-` that is followed by one (`ORBIT-12-b`). Prefixes are case-sensitive.
const WORD = /[\p{L}\p{N}_]/u;
const BEFORE = /[\p{L}\p{N}_#/-]/u;

/** No patterns: one array, so the scanner built for it is built once. */
export const NO_PATTERNS: RefPattern[] = [];
const validCache = new WeakMap<object, RefPattern[]>();

type Row = { prefix?: unknown; url?: unknown };
const rowOwn = (r: Row | undefined): string | null => {
  if (!String(r?.prefix ?? "").trim()) return "Needs a prefix, such as ORBIT-.";
  if (!WEB.test(String(r?.url ?? "").trim())) return "The address must start with http:// or https:// and have no spaces.";
  return null;
};

/** What is wrong with row `i` of a list of patterns, in words, or null when it works: it needs a
 *  prefix, an address that opens on the web (never a script or a file), and a prefix no earlier
 *  working row has (after trimming). The Settings rows, the settings tool and `validPatterns` all
 *  use this one rule. */
export function rowProblem(list: readonly Row[], i: number): string | null {
  const own = rowOwn(list[i]);
  if (own) return own;
  const prefix = String(list[i].prefix).trim();
  if (list.slice(0, i).some((r) => !rowOwn(r) && String(r.prefix).trim() === prefix)) return "An earlier row has this prefix already.";
  return null;
}

/** The patterns that work, whitespace trimmed. The same array comes back for the same list, so
 *  what is built from it (the scanner's expression) is built once. */
export function validPatterns(list: readonly RefPattern[] | undefined): RefPattern[] {
  if (!list?.length) return NO_PATTERNS;
  const hit = validCache.get(list);
  if (hit) return hit;
  const out = list
    .map((p, i) => (rowProblem(list, i) ? null : { prefix: p.prefix.trim(), url: p.url.trim() }))
    .filter((p): p is RefPattern => !!p);
  const made = out.length ? out : NO_PATTERNS;
  validCache.set(list, made);
  return made;
}

/** The patterns now set, for code that isn't a component (the MCP tools, the editor). */
export const refPatterns = (): RefPattern[] => validPatterns(settings.get().referenceLinks);

/** The patterns now set, redrawing the component when they change. */
export function useRefPatterns(): RefPattern[] {
  // Only this setting's own changes redraw, not every setting's.
  const list = useSyncExternalStore(settings.subscribe, () => settings.get().referenceLinks);
  return useMemo(() => validPatterns(list), [list]);
}

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Where a key opens, by the pattern with the longest prefix that starts it and leaves only digits;
 *  null when no pattern fits. */
export function refUrl(key: string, patterns: readonly RefPattern[] = refPatterns()): string | null {
  let best: RefPattern | null = null;
  for (const p of patterns) {
    if (!key.startsWith(p.prefix) || !/^\d+$/.test(key.slice(p.prefix.length))) continue;
    if (!best || p.prefix.length > best.prefix.length) best = p;
  }
  if (!best) return null;
  return best.url.split("{key}").join(encodeURIComponent(key)).split("{num}").join(key.slice(best.prefix.length));
}

/** The link a template makes for a sample key, for Settings to show (no valid row: null). */
export const sampleRef = (p: RefPattern): { key: string; url: string } | null => {
  const [v] = validPatterns([p]);
  return v ? { key: `${v.prefix}12`, url: refUrl(`${v.prefix}12`, [v]) ?? "" } : null;
};

/** A wikilink's label as a row shows it: its alias, else the note's name. */
export function wikiLabel(inner: string): string {
  const [target, alias] = inner.split("|");
  return alias ?? target.split("#")[0].split("/").pop() ?? target;
}

/** A link's words without their marks: code without its backticks, bold and italic without their
 *  asterisks, an image as its alt text, a wikilink as its label. */
export function plainLabel(label: string): string {
  return scan(label, NO_PATTERNS, "label")
    .map((t) => {
      if (t.k === "code") return t.inner;
      if (t.k === "image") return t.alt;
      if (t.k === "wiki") return wikiLabel(t.inner);
      return t.raw.replace(/\*\*([^*]+)\*\*|\*([^*\s][^*]*)\*/g, "$1$2");
    })
    .join("");
}

/** A URL's domain without `www.`, for a bare URL's text. */
export const hostOf = (url: string): string => /^https?:\/\/(?:www\.)?([^/?#\s]+)/.exec(url)?.[1] ?? url;

/** A bare URL without the punctuation that ended the sentence, and without a closing bracket that
 *  has no opening one inside the URL. */
function trimUrl(u: string): string {
  let open = 0;
  let close = 0;
  for (const c of u) {
    if (c === "(") open++;
    else if (c === ")") close++;
  }
  for (;;) {
    const c = u[u.length - 1];
    if (!c) return u;
    if (/[.,;:!?'"*_~\]}>]/.test(c)) u = u.slice(0, -1);
    else if (c === ")" && close > open) {
      u = u.slice(0, -1);
      close--;
    } else return u;
  }
}

/** What the scan finds, in order, each with the text it came from (`raw`), so they join back to
 *  the text. `plink` is a markdown link that isn't drawn as one here: to a note or a heading, or by
 *  reference. */
export type Tok =
  | { k: "text"; raw: string }
  | { k: "code"; raw: string; inner: string }
  | { k: "wiki"; raw: string; inner: string }
  | { k: "image"; raw: string; alt: string }
  | { k: "plink"; raw: string }
  | { k: "link" | "url" | "ref"; raw: string; text: string; href: string };

/** The expression that finds where something may start: 1 a run of backticks, 2 `[[` or `![[`,
 *  3 `[` or `![`, 4 a bare URL, 5 a key. One per list of patterns, kept. */
const scanners = new WeakMap<readonly RefPattern[], RegExp>();
function scanner(patterns: readonly RefPattern[]): RegExp {
  let re = scanners.get(patterns);
  if (!re) {
    const alts = patterns
      .map((p) => p.prefix)
      .sort((a, b) => b.length - a.length)
      .map(escapeRe);
    const key = alts.length ? `|((?:${alts.join("|")})\\d+)` : "";
    re = new RegExp(`(\`+)|(!?\\[\\[)|(!?\\[)|(https?:\\/\\/[^\\s<>\\[\\]|]+)${key}`, "gu");
    scanners.set(patterns, re);
  }
  return re;
}

const MAX_LABEL = 1000;
const MAX_TARGET = 2000;
const MAX_TITLE = 200;

/** Where a link's `(…)` ends, or null: a destination (in `<…>`, or without spaces and with at most
 *  one level of balanced brackets inside), an optional title in quotes, and the closing bracket.
 *  `p` is just after the opening one. Every step is bounded, so a text of `[a](` repeated stays
 *  linear. */
function targetEnd(text: string, p: number): { end: number; dest: string } | null {
  const n = text.length;
  const blank = (c: string) => c === " " || c === "\t";
  while (p < n && blank(text[p])) p++;
  let dest: string;
  const stop = Math.min(n, p + MAX_TARGET);
  if (text[p] === "<") {
    let q = p + 1;
    while (q < stop && text[q] !== ">" && text[q] !== "<" && text[q] !== "\n") q++;
    if (text[q] !== ">") return null;
    dest = text.slice(p + 1, q);
    p = q + 1;
  } else {
    let depth = 0;
    let q = p;
    for (; q < stop; q++) {
      const c = text[q];
      if (c === "\\") q++;
      else if (c === "(") {
        if (++depth > 1) return null;
      } else if (c === ")") {
        if (depth === 0) break;
        depth--;
      } else if (c === " " || c === "\t" || c === "\n") break;
    }
    if (q >= stop && q < n) return null;
    dest = text.slice(p, q);
    p = q;
  }
  if (!dest) return null;
  while (p < n && blank(text[p])) p++;
  const open = text[p];
  if (open === '"' || open === "'" || open === "(") {
    const close = open === "(" ? ")" : open;
    const tstop = Math.min(n, p + MAX_TITLE);
    let q = p + 1;
    while (q < tstop && text[q] !== close && text[q] !== "\n") q++;
    if (text[q] !== close) return null;
    p = q + 1;
    while (p < n && blank(text[p])) p++;
  }
  return text[p] === ")" ? { end: p + 1, dest } : null;
}

/** The end of a link's label: the index of its `]`, or -1 (a `[`, a new line or too long first). */
function labelEnd(text: string, p: number): number {
  const stop = Math.min(text.length, p + MAX_LABEL);
  for (let q = p; q < stop; q++) {
    const c = text[q];
    if (c === "]") return q > p ? q : -1;
    if (c === "[" || c === "\n") return -1;
  }
  return -1;
}

/** What `scan` is for: `full` finds links, URLs and keys; `label` only the code, wikilinks and
 *  images, for the words of a link, which hold no link of their own. */
export type ScanMode = "full" | "label";

/** The text as a list of pieces: plain text, code, wikilinks, images, links (of any target), bare
 *  URLs and reference keys. Linear in the text. */
export function scan(text: string, patterns: readonly RefPattern[] = refPatterns(), mode: ScanMode = "full"): Tok[] {
  const out: Tok[] = [];
  const re = scanner(patterns);
  re.lastIndex = 0;
  let at = 0;
  // Backtick runs with no closing run of their length after them.
  const noClose = new Set<number>();
  const plain = (to: number) => {
    if (to > at) out.push({ k: "text", raw: text.slice(at, to) });
  };
  const take = (tok: Tok, to: number) => {
    out.push(tok);
    at = to;
    re.lastIndex = to;
  };
  const closeOf = (from: number, len: number): number => {
    if (noClose.has(len)) return -1;
    for (let j = text.indexOf("`", from); j >= 0; j = text.indexOf("`", from)) {
      let r = j;
      while (text[r] === "`") r++;
      if (r - j === len) return j;
      from = r;
    }
    noClose.add(len);
    return -1;
  };
  for (let m = re.exec(text); m; m = re.exec(text)) {
    const i = m.index;
    if (m[1]) {
      const len = m[1].length;
      const j = closeOf(i + len, len);
      if (j < 0) continue;
      plain(i);
      let inner = text.slice(i + len, j);
      // One space at each end goes, as CommonMark has it, when both are there and it isn't all spaces.
      if (inner.length > 1 && inner[0] === " " && inner[inner.length - 1] === " " && inner.trim()) inner = inner.slice(1, -1);
      take({ k: "code", raw: text.slice(i, j + len), inner }, j + len);
    } else if (m[2]) {
      const start = i + m[2].length;
      // The target runs to the first `]`, which has to be a `]]`; a `[` or a new line first, or more
      // than 500 characters, is no wikilink. Bounded, so a run of `[[x ` stays linear.
      let j = -1;
      for (let q = start, stop = Math.min(text.length, start + 501); q < stop; q++) {
        const c = text[q];
        if (c === "]") {
          if (text[q + 1] === "]" && q > start) j = q;
          break;
        }
        if (c === "[" || c === "\n") break;
      }
      if (j < 0) {
        // Not a wikilink: the `[` inside may still start a link.
        re.lastIndex = start - 1;
        continue;
      }
      plain(i);
      take({ k: "wiki", raw: text.slice(i, j + 2), inner: text.slice(start, j) }, j + 2);
    } else if (m[3]) {
      if (mode === "label") continue;
      const from = i + m[3].length;
      const e = labelEnd(text, from);
      let end = -1;
      let dest = "";
      if (e >= 0 && text[e + 1] === "(") {
        const t = targetEnd(text, e + 2);
        if (t) {
          end = t.end;
          dest = t.dest;
        }
      } else if (e >= 0 && text[e + 1] === "[") {
        // A reference link, `[label][ref]`: never a key's place, whatever it points at.
        const stop = Math.min(text.length, e + 102);
        let q = e + 2;
        while (q < stop && text[q] !== "]" && text[q] !== "[" && text[q] !== "\n") q++;
        if (text[q] === "]") end = q + 1;
      }
      if (end < 0) continue;
      plain(i);
      const raw = text.slice(i, end);
      const label = text.slice(from, e);
      if (m[3][0] === "!") take({ k: "image", raw, alt: label }, end);
      else if (dest && isExternalHref(dest)) take({ k: "link", raw, text: label, href: dest }, end);
      else take({ k: "plink", raw }, end);
    } else if (m[4]) {
      if (mode === "label") continue;
      const url = trimUrl(m[4]);
      // A scheme with nothing after it isn't an address.
      if (!/^https?:\/\/[^/?#\s]/.test(url)) continue;
      plain(i);
      take({ k: "url", raw: url, text: hostOf(url), href: url }, i + url.length);
    } else if (m[5]) {
      if (mode === "label") continue;
      const before = text[i - 1];
      const after = text[i + m[5].length];
      const tail = text[i + m[5].length + 1];
      if ((before && BEFORE.test(before)) || (after && (WORD.test(after) || (after === "-" && tail && WORD.test(tail))))) continue;
      const href = refUrl(m[5], patterns);
      if (!href) continue;
      plain(i);
      take({ k: "ref", raw: m[5], text: m[5], href }, i + m[5].length);
    }
  }
  plain(text.length);
  return out;
}

/** What `tokenize` looks for. */
export interface TokenizeOptions {
  patterns?: readonly RefPattern[];
  /** `[label](url)` and bare URLs (on unless false); off, only reference keys are found. */
  links?: boolean;
}

/** Text cut into plain pieces and links. A key is the prefix and its digits, not part of a longer
 *  name (see `BEFORE` and `WORD`), and never inside code, a wikilink, an image or any link. */
export function tokenize(text: string, opts: TokenizeOptions = {}): RefSeg[] {
  const links = opts.links !== false;
  const out: RefSeg[] = [];
  for (const t of scan(text, opts.patterns ?? refPatterns())) {
    if (t.k === "ref" || (links && (t.k === "link" || t.k === "url")))
      out.push({ text: t.k === "link" ? plainLabel(t.text) || t.href : t.text, href: t.href, kind: t.k });
    else if (t.raw) {
      const last = out[out.length - 1];
      if (last && !("href" in last)) last.text += t.raw;
      else out.push({ text: t.raw });
    }
  }
  return out.length ? out : [{ text }];
}

/** The keys and links in a value: text, a list of values, or a front matter value of either kind.
 *  Each once, in the order found. A bare URL's text is its domain. */
export function collectRefs(value: unknown, opts: TokenizeOptions = {}): Ref[] {
  const out: Ref[] = [];
  const add = (v: unknown) => {
    if (typeof v === "string") {
      for (const s of tokenize(v, opts))
        if ("href" in s && !out.some((r) => r.key === s.text && r.url === s.href)) out.push({ key: s.text, url: s.href, kind: s.kind });
    } else if (Array.isArray(v)) v.forEach(add);
  };
  add(value);
  return out;
}
