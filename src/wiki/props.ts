// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A wiki page's editable properties (aliases, tags, sources, description), read from and written
// into the page's text as it is in the editor. Only the property being changed is rewritten, in
// the style it had (an inline `[a, b]` list or a block `- a` list); the rest of the file stays as
// it was.

const eolOf = (t: string) => (t.includes("\r\n") ? "\r\n" : "\n");

/** The frontmatter's lines (between the `---` lines), or null when the page has none. */
function bounds(lines: string[]): { start: number; end: number } | null {
  if (lines[0]?.trimEnd() !== "---") return null;
  const end = lines.findIndex((l, i) => i > 0 && (l.trimEnd() === "---" || l.trimEnd() === "..."));
  return end > 0 ? { start: 1, end } : null;
}

/** The lines `key:` takes: its own, and the indented or `- ` lines under it. */
function extent(lines: string[], b: { start: number; end: number }, key: string): [number, number] | null {
  const at = lines.slice(b.start, b.end).findIndex((l) => l.startsWith(`${key}:`));
  if (at < 0) return null;
  const s = b.start + at;
  let e = s + 1;
  while (e < b.end && (/^\s+\S/.test(lines[e]) || /^-\s/.test(lines[e]))) e++;
  return [s, e];
}

const unquote = (v: string) => {
  const t = v.trim();
  if ((t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'"))) {
    try {
      return t.startsWith('"') ? (JSON.parse(t) as string) : t.slice(1, -1).replace(/''/g, "'");
    } catch {
      return t.slice(1, -1);
    }
  }
  return t;
};

/** Splits `a, "b, c", [[d]]` at its top-level commas. */
function splitInline(s: string): string[] {
  const out: string[] = [];
  let cur = "";
  let q: string | null = null;
  let depth = 0;
  for (const c of s) {
    if (q) {
      cur += c;
      if (c === q) q = null;
    } else if ((c === '"' || c === "'") && cur.trim() === "") {
      // A quote opens a value only at its start: the apostrophe in O'Brien is a letter.
      q = c;
      cur += c;
    } else if (c === "[") {
      depth++;
      cur += c;
    } else if (c === "]") {
      depth--;
      cur += c;
    } else if (c === "," && depth === 0) {
      out.push(cur);
      cur = "";
    } else cur += c;
  }
  if (cur.trim()) out.push(cur);
  return out.map(unquote).filter(Boolean);
}

/** A list property's values (a single value reads as a list of one). */
export function getList(text: string, key: string): string[] {
  const lines = text.split(/\r?\n/);
  const b = bounds(lines);
  const x = b && extent(lines, b, key);
  if (!x) return [];
  const rest = lines[x[0]].slice(key.length + 1).trim();
  if (rest.startsWith("[") && rest.endsWith("]")) return splitInline(rest.slice(1, -1));
  if (rest) return [unquote(rest)];
  return lines
    .slice(x[0] + 1, x[1])
    .map((l) => /^\s*-\s+(.*)$/.exec(l)?.[1])
    .filter((v): v is string => v !== undefined)
    .map(unquote)
    .filter(Boolean);
}

/** A one-line property's value, or "". */
export function getScalar(text: string, key: string): string {
  const lines = text.split(/\r?\n/);
  const b = bounds(lines);
  const x = b && extent(lines, b, key);
  return x ? unquote(lines[x[0]].slice(key.length + 1)) : "";
}

const WORDS = new Set(["y", "n", "yes", "no", "on", "off", "true", "false", "null", "~"]);
/** A value as YAML: plain when that reads back the same, else double-quoted. */
export const yamlValue = (v: string) =>
  /^[\p{L}\p{N}][\p{L}\p{N} _./()&'-]*$/u.test(v) && !WORDS.has(v.toLowerCase()) ? v : JSON.stringify(v);

function put(text: string, key: string, render: (block: boolean) => string[] | null): string {
  const eol = eolOf(text);
  const trailing = /\r?\n$/.test(text);
  const lines = text.replace(/\r?\n$/, "").split(/\r?\n/);
  const b = bounds(lines);
  if (!b) {
    const add = render(false);
    if (!add) return text;
    return ["---", ...add, "---", ...(text ? [""] : [])].join(eol) + (text ? eol + text.replace(/^\r?\n/, "") : eol);
  }
  const x = extent(lines, b, key);
  const wasBlock = !!x && lines[x[0]].slice(key.length + 1).trim() === "" && x[1] > x[0] + 1;
  const next = render(wasBlock);
  if (x) lines.splice(x[0], x[1] - x[0], ...(next ?? []));
  else if (next) lines.splice(b.end, 0, ...next);
  return lines.join(eol) + (trailing ? eol : "");
}

/** The text with the list property set (removed when empty), in the style it had. */
export function setList(text: string, key: string, values: string[]): string {
  return put(text, key, (block) => {
    if (!values.length) return null;
    if (block) return [`${key}:`, ...values.map((v) => `  - ${yamlValue(v)}`)];
    return [`${key}: [${values.map(yamlValue).join(", ")}]`];
  });
}

/** The text with a one-line property set (removed when empty). */
export function setScalar(text: string, key: string, value: string): string {
  const v = value.replace(/\s+/g, " ").trim();
  return put(text, key, () => (v ? [`${key}: ${yamlValue(v)}`] : null));
}
