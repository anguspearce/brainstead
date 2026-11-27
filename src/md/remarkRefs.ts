// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Reference keys (`ORBIT-12`, from Settings › Notes › Reference links) in View, as links that open in
// the browser. The editor marks the same keys (src/editor/live.ts); both find them with
// src/md/refs.ts. Not in code or inside a link.

import type { PhrasingContent, Root } from "mdast";
import { findAndReplace } from "mdast-util-find-and-replace";
import { tokenize, type RefPattern } from "./refs";

export function remarkRefs(patterns: RefPattern[]) {
  return (tree: Root) => {
    if (!patterns.length) return;
    findAndReplace(
      tree,
      [
        [
          // The whole text node, cut by the tokenizer, so a key's edges are judged by one rule everywhere.
          /^[\s\S]+$/,
          (text: string): PhrasingContent[] | false => {
            const segs = tokenize(text, { patterns, links: false });
            if (!segs.some((s) => "href" in s)) return false;
            return segs.map((s): PhrasingContent =>
              "href" in s
                ? {
                    type: "link",
                    url: s.href,
                    children: [{ type: "text", value: s.text }],
                    data: { hProperties: { "data-ref": s.text, className: ["mdref"] } },
                  }
                : { type: "text", value: s.text },
            );
          },
        ],
      ],
      { ignore: ["link", "linkReference", "inlineCode", "code"] },
    );
  };
}
