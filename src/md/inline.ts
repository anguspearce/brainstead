// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A task's words as the row draws them: `code`, **bold**, *italic*, [[wikilinks]], images, and the
// links of src/md/refs.ts, as a tree. TaskText draws this tree, and the collectors (the MCP tools,
// the project and note cards) list its links, so what a card or a tool says a task links to is
// exactly what its row shows as a link.

import { NO_PATTERNS, type Ref, type RefPattern, refPatterns, scan, type Tok, wikiLabel } from "./refs";

export type Inline =
  | { t: "text"; text: string }
  | { t: "code"; text: string }
  | { t: "wiki"; label: string }
  | { t: "image"; alt: string }
  | { t: "em" | "strong"; children: Inline[] }
  | { t: "a"; kind: "ref" | "link" | "url"; href: string; text: string; children: Inline[] };

/** A private-use character the text doesn't hold, to stand for each piece that isn't plain text;
 *  null in the unlikely text that holds all of them. */
function spare(text: string): string | null {
  for (let c = 0xe000; c <= 0xf8ff; c++) {
    const ch = String.fromCharCode(c);
    if (!text.includes(ch)) return ch;
  }
  return null;
}

const EMPHASIS = /(\*\*[^*]+\*\*|\*[^*\s][^*]*\*)/g;

/** One scanned piece as a node; a link's label is read for its own code and bold. */
function node(t: Exclude<Tok, { k: "text" }>): Inline {
  switch (t.k) {
    case "code":
      return { t: "code", text: t.inner };
    case "wiki":
      return { t: "wiki", label: wikiLabel(t.inner) };
    case "image":
      return { t: "image", alt: t.alt };
    case "plink":
      return { t: "text", text: t.raw };
    case "link":
      return { t: "a", kind: "link", href: t.href, text: t.text, children: parse(t.text, NO_PATTERNS, "label") };
    default:
      return { t: "a", kind: t.k, href: t.href, text: t.text, children: [{ t: "text", text: t.text }] };
  }
}

function parse(text: string, patterns: readonly RefPattern[], mode: "full" | "label"): Inline[] {
  const toks = scan(text, patterns, mode);
  const HOLE = spare(text);
  // No stand-in to be had: the pieces without bold or italic around them.
  if (HOLE === null) return toks.map((t): Inline => (t.k === "text" ? { t: "text", text: t.raw } : node(t)));
  // The text with each non-text piece as one placeholder, so bold and italic can wrap pieces
  // (`**ORBIT-12**`) and are found as they always were, by their asterisks. The text itself is
  // kept whole, as the placeholder is a character it doesn't hold.
  const pieces: Exclude<Tok, { k: "text" }>[] = [];
  let virt = "";
  for (const t of toks) {
    if (t.k === "text") virt += t.raw;
    else {
      pieces.push(t);
      virt += HOLE;
    }
  }
  let next = 0;
  const build = (s: string): Inline[] => {
    const out: Inline[] = [];
    let run = "";
    const flush = () => {
      if (run) out.push({ t: "text", text: run });
      run = "";
    };
    for (const c of s) {
      if (c === HOLE) {
        flush();
        out.push(node(pieces[next++]));
      } else run += c;
    }
    flush();
    return out;
  };
  return virt.split(EMPHASIS).flatMap((part, i): Inline[] => {
    if (i % 2 === 0) return build(part);
    return part.startsWith("**")
      ? [{ t: "strong", children: build(part.slice(2, -2)) }]
      : [{ t: "em", children: build(part.slice(1, -1)) }];
  });
}

/** A task's words as a tree of what the row draws. */
export const parseInline = (text: string, patterns: readonly RefPattern[] = refPatterns()): Inline[] => parse(text, patterns, "full");

/** The words of a tree without its marks: a link's label as a card says it. */
export function plainOf(nodes: readonly Inline[]): string {
  return nodes
    .map((n) => {
      switch (n.t) {
        case "text":
        case "code":
          return n.text;
        case "wiki":
          return n.label;
        case "image":
          return n.alt;
        default:
          return plainOf(n.children);
      }
    })
    .join("");
}

/** The links a task's row draws, each once: a key, a `[label](url)` (by its words) and a bare URL
 *  (by its domain). */
export function taskRefs(text: string, patterns: readonly RefPattern[] = refPatterns()): Ref[] {
  const out: Ref[] = [];
  const walk = (nodes: readonly Inline[]) => {
    for (const n of nodes) {
      if (n.t === "a") {
        const key = n.kind === "link" ? plainOf(n.children).trim() || n.href : n.text;
        if (!out.some((r) => r.key === key && r.url === n.href)) out.push({ key, url: n.href, kind: n.kind });
      } else if (n.t === "em" || n.t === "strong") walk(n.children);
    }
  };
  walk(parseInline(text, patterns));
  return out;
}
