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
import { parseTask } from "../tasksq/fields";
import { remarkWikilinks } from "./wikilinks";

const processor = unified()
  .use(remarkParse)
  .use(remarkGfm, { singleTilde: false })
  .use(remarkMath, { singleDollarTextMath: false })
  .use(remarkPandocMath)
  .use(remarkDisplayMath)
  .use(remarkWikilinks);

/** Nodes whose content is never drawn as text a key could be in: code, raw HTML (comments among
 *  it), maths, images and link definitions. */
const SKIP = new Set(["code", "inlineCode", "html", "math", "inlineMath", "image", "imageReference", "definition", "footnoteReference"]);

/** A node's words as a link draws them, an image as its alt text (as a task row draws it). */
const textOf = (n: Nodes): string =>
  n.type === "image" || n.type === "imageReference"
    ? (n.alt ?? "")
    : "value" in n
      ? String(n.value)
      : "children" in n
        ? n.children.map(textOf).join("")
        : "";

/** The links and keys in a note's body (its text after the properties), each once. */
export function bodyRefs(md: string, patterns: readonly RefPattern[]): Ref[] {
  const out: Ref[] = [];
  const add = (r: Ref) => {
    if (!out.some((x) => x.key === r.key && x.url === r.url)) out.push(r);
  };
  // Where a link written by reference (`[Spec][s]`) goes: the first definition of its label, as
  // View resolves it.
  const defs = new Map<string, string>();
  const collect = (n: Nodes) => {
    if (n.type === "definition" && !defs.has(n.identifier)) defs.set(n.identifier, n.url);
    else if ("children" in n) n.children.forEach(collect);
  };
  // `taskLine` is the line of the task whose words are being read (1-based; 0 for none).
  const walk = (n: Nodes, taskLine = 0) => {
    if (SKIP.has(n.type)) return;
    const onTaskLine = !!taskLine && n.position?.start.line === taskLine;
    if (n.type === "linkReference") {
      // The parse makes one only for a label that has a definition.
      const url = defs.get(n.identifier);
      if (url === undefined || onTaskLine || !isExternalHref(url)) return;
      const href = url.replace(/ /g, "%20");
      add({ key: textOf(n).trim() || href, url: href, kind: "link" });
      return;
    }
    if (n.type === "link") {
      if (onTaskLine) return;
      // A wikilink is a link to a note; a link to a note or a heading isn't one to the web.
      if (!isExternalHref(n.url)) return;
      // An address in `<…>` may hold spaces; it is listed encoded, as `tokenize` gives it for a task.
      const url = n.url.replace(/ /g, "%20");
      const label = textOf(n).trim();
      // GFM turns a bare address into a link whose words are the address itself (`www.` ones open
      // with http://); a written link whose words are another address keeps its words.
      const bare = label === n.url || (label.startsWith("www.") && n.url === `http://${label}`);
      add(bare ? { key: hostOf(url), url, kind: "url" } : { key: label || url, url, kind: "link" });
      return;
    }
    if (n.type === "text") {
      let value = n.value;
      if (onTaskLine) {
        // Only what follows the task's own line: its continuation lines are the note's text.
        const nl = value.indexOf("\n");
        if (nl < 0) return;
        value = value.slice(nl + 1);
      }
      for (const s of tokenize(value, { patterns, links: false })) if ("href" in s) add({ key: s.text, url: s.href, kind: s.kind });
      return;
    }
    if (!("children" in n)) return;
    // A task's own line is the task's (the words the index holds for it, which `taskRefs` reads),
    // whatever its status (`[-]` and `[/]` too, which GFM reads as plain items), by the app's own
    // reading of a task line.
    const line = n.position?.start.line ?? 0;
    const task = n.type === "listItem" && !!line && parseTask(lines[line - 1] ?? "") !== null;
    n.children.forEach((c, i) => walk(c, task ? (i === 0 && c.type === "paragraph" ? line : 0) : taskLine));
  };
  const { body } = splitFrontmatter(md);
  const lines = body.split(/\r?\n/);
  const tree = processor.runSync(processor.parse(body)) as Root;
  collect(tree);
  walk(tree);
  return out;
}

/** Property names whose values are labels, never references: the property chips and the cards
 *  both read them so. */
export const LABELS = /^(tags?|aliases|alias)$/i;

/** The text a note's properties hold, from the properties as the index parsed them: each text value
 *  and each text item of a list, trimmed, except those of `tags` and `aliases`. The same rule as
 *  `property_text` in src-tauri/core/src/projects.rs, which gives a project's; both are tested
 *  against tests/fixtures/property-text.json. */
export function propertyText(fm: Record<string, unknown> | null | undefined): string[] {
  const out: string[] = [];
  for (const [k, v] of Object.entries(fm ?? {})) {
    if (LABELS.test(k)) continue;
    if (typeof v === "string") {
      if (v.trim()) out.push(v.trim());
    } else if (Array.isArray(v)) for (const item of v) if (typeof item === "string" && item.trim()) out.push(item.trim());
  }
  return out;
}
