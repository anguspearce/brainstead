// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The keys and links a note's text draws, found in the same parse View uses (remark with GFM, maths
// and wikilinks), so code of every kind, HTML comments, images, links to notes and the words of
// other links are never taken for keys. A bare URL and a `[label](url)` are the links GFM makes of
// them; a key is a configured prefix and digits in a text node (src/md/refs.ts). The text of a task
// line is left out: the task rows are listed on their own.

import type { Nodes, Root } from "mdast";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import remarkParse from "remark-parse";
import { unified } from "unified";
import { remarkDisplayMath, remarkPandocMath } from "./math";
import { splitFrontmatter } from "./plugins";
import { hostOf, isExternalHref, type Ref, type RefPattern, tokenize } from "./refs";
import { remarkWikilinks } from "./wikilinks";

const processor = unified()
  .use(remarkParse)
  .use(remarkGfm, { singleTilde: false })
  .use(remarkMath, { singleDollarTextMath: false })
  .use(remarkPandocMath)
  .use(remarkDisplayMath)
  .use(remarkWikilinks);

/** Nodes whose content is never drawn as text a key could be in: code, raw HTML (comments among
 *  it), maths, images and the parts of a link written by reference. */
const SKIP = new Set([
  "code",
  "inlineCode",
  "html",
  "math",
  "inlineMath",
  "image",
  "imageReference",
  "definition",
  "linkReference",
  "footnoteReference",
  "footnoteDefinition",
]);

const textOf = (n: Nodes): string => ("value" in n ? String(n.value) : "children" in n ? n.children.map(textOf).join("") : "");

/** The links and keys in a note's body (its text after the properties), each once. */
export function bodyRefs(md: string, patterns: readonly RefPattern[]): Ref[] {
  const out: Ref[] = [];
  const add = (r: Ref) => {
    if (!out.some((x) => x.key === r.key && x.url === r.url)) out.push(r);
  };
  const walk = (n: Nodes) => {
    if (SKIP.has(n.type)) return;
    if (n.type === "link") {
      // A wikilink is a link to a note; a link to a note or a heading isn't one to the web.
      if (!isExternalHref(n.url)) return;
      const label = textOf(n).trim();
      // GFM turns a bare address into a link whose words are the address itself.
      const bare = /^(https?:\/\/|www\.)/.test(label);
      add(bare ? { key: hostOf(n.url), url: n.url, kind: "url" } : { key: label || n.url, url: n.url, kind: "link" });
      return;
    }
    if (n.type === "text") {
      for (const s of tokenize(n.value, { patterns, links: false })) if ("href" in s) add({ key: s.text, url: s.href, kind: s.kind });
      return;
    }
    if (!("children" in n)) return;
    // A task's own line is the task's: its words are counted with the tasks.
    const task = n.type === "listItem" && typeof n.checked === "boolean";
    n.children.forEach((c, i) => {
      if (!(task && i === 0 && c.type === "paragraph")) walk(c);
    });
  };
  const { body } = splitFrontmatter(md);
  walk(processor.runSync(processor.parse(body)) as Root);
  return out;
}

/** Property names whose values are labels, never references. */
const LABELS = /^(tags?|aliases|alias)$/i;

/** The text of a note's properties, one value each (an item of a list is a value), without the
 *  names and without `tags` and `aliases`. */
export function propertyValues(md: string): string[] {
  const m = /^\uFEFF?---[ \t]*\r?\n([\s\S]*?)\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/.exec(md);
  if (!m) return [];
  const out: string[] = [];
  let skip = false;
  for (const line of m[1].split(/\r?\n/)) {
    const key = /^([^\s#:][^:]*):\s*(.*)$/.exec(line);
    if (key) {
      skip = LABELS.test(key[1].trim());
      if (!skip && key[2].trim()) out.push(key[2].trim());
    } else if (!skip && line.trim() && !line.trim().startsWith("#")) out.push(line.trim().replace(/^-\s+/, ""));
  }
  return out;
}
