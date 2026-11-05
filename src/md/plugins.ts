// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The steps around react-markdown: what's taken out of the text first, and the rehype plugins
// that give headings and blocks ids to link to.

import GithubSlugger from "github-slugger";
import type { Element, Root, Text } from "hast";
import { toString } from "hast-util-to-string";
import { defaultSchema } from "rehype-sanitize";
import { visit } from "unist-util-visit";

/** The body after frontmatter (`---` on the first line, closed by `---` or `...`), as the index splits it. */
export function splitFrontmatter(src: string): { body: string; hasFrontmatter: boolean } {
  const s = src.replace(/^\uFEFF/, "");
  const m = /^---[ \t]*\r?\n/.exec(s);
  if (!m) return { body: s, hasFrontmatter: false };
  const rest = s.slice(m[0].length);
  const close = /^(---|\.\.\.)[ \t]*(\r?\n|$)/m.exec(rest);
  if (!close) return { body: s, hasFrontmatter: false };
  return { body: rest.slice(close.index + close[0].length), hasFrontmatter: true };
}

/** The previous app's `stripTaskRankTokens`: `^rank-N` at the end of a task line, outside code. Spaces
 *  and tabs only, where it had `\s`, which also ate the blank line after the task. */
export function stripRanks(md: string): string {
  return md
    .split(/(```[\s\S]*?```|`[^`\n]*`)/)
    .map((part, i) =>
      i % 2 ? part : part.replace(/^((?:[ \t]{0,3}>[ \t]?)*[ \t]*[-*+][ \t]+\[[ xX]\][ \t]+.+?)[ \t]+\^rank-\d+[ \t]*$/gm, "$1"),
    )
    .join("");
}

export const headingSlug = (text: string) => new GithubSlugger().slug(text);

/** Ids on headings (their slug) and on paragraphs and list items ending in a block id
 *  (`^q3`, taken out of the text), so `[[Note#Heading]]` and `[[Note#^q3]]` can scroll to them. */
export function rehypeAnchors() {
  return (tree: Root) => {
    const slugger = new GithubSlugger();
    visit(tree, "element", (el: Element) => {
      if (/^h[1-6]$/.test(el.tagName)) {
        el.properties = { ...el.properties, id: slugger.slug(toString(el)) };
        return;
      }
      if (el.tagName !== "p" && el.tagName !== "li") return;
      const last = el.children[el.children.length - 1];
      if (last?.type !== "text") return;
      const m = /\s\^([A-Za-z0-9-]+)\s*$/.exec((last as Text).value);
      if (!m) return;
      (last as Text).value = (last as Text).value.slice(0, m.index);
      el.properties = { ...el.properties, id: `^${m[1]}` };
    });
  };
}

/** What raw HTML in a note may keep: GitHub's rules, plus classes (math, code languages), the
 *  wikilink data and ids, without GitHub's "user-content-" prefix on ids. */
export const sanitizeSchema = {
  ...defaultSchema,
  clobberPrefix: "",
  tagNames: [...(defaultSchema.tagNames ?? []), "mark"],
  attributes: {
    ...defaultSchema.attributes,
    "*": [...(defaultSchema.attributes?.["*"] ?? []), "className", "id"],
    // dataRef: a reference key drawn as a link (src/md/remarkRefs.ts).
    a: [...(defaultSchema.attributes?.a ?? []), "dataWiki", "dataRef"],
    img: [...(defaultSchema.attributes?.img ?? []), "dataWiki", "width", "height"],
    // dataMath: `$$…$$` within a line (src/md/math.ts).
    code: [...(defaultSchema.attributes?.code ?? []), "className", "dataMath"],
    // dataTp: a template tag's token and where it starts (src/notes/templaterSyntax.ts).
    span: [...(defaultSchema.attributes?.span ?? []), "dataTdate", "dataTag", "dataFemoji", "dataTp"],
    li: [...(defaultSchema.attributes?.li ?? []), "dataStatus"],
    blockquote: [...(defaultSchema.attributes?.blockquote ?? []), "dataCallout", "dataFold"],
    p: [...(defaultSchema.attributes?.p ?? []), "dataCalloutTitle"],
  },
  protocols: {
    ...defaultSchema.protocols,
    href: [...(defaultSchema.protocols?.href ?? []), "wiki"],
    src: [...(defaultSchema.protocols?.src ?? []), "wiki"],
  },
};
