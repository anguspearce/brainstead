// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Inline fields in View, as small labels: `[status:: open]` reads "status open", as Dataview shows
// them. The value may be a `[[link]]` (an initiative's `[delegated:: [[Lena]]]`), which stays a
// link: this runs before the wikilinks. Not in code or links. The note's text is unchanged.

import type { PhrasingContent, Root } from "mdast";
import { findAndReplace } from "mdast-util-find-and-replace";

/** `[key:: value]`: the same rule as src-tauri/core/src/initiatives.rs's FIELD. */
export const FIELD = /\[([A-Za-z][\w-]*)::[ \t]*((?:\[\[[^\]\n]*\]\]|[^[\]\n])*?)[ \t]*\]/g;

export function remarkInlineFields() {
  return (tree: Root) => {
    findAndReplace(
      tree,
      [
        [
          FIELD,
          (_m: string, key: string, value: string): PhrasingContent => ({
            type: "emphasis",
            data: { hName: "span", hProperties: { className: ["mdfield"], "data-field": key } },
            children: [
              {
                type: "emphasis",
                data: { hName: "span", hProperties: { className: ["mdfield-k"] } },
                children: [{ type: "text", value: key }],
              },
              { type: "text", value: value ? ` ${value}` : "" },
            ],
          }),
        ],
      ],
      { ignore: ["link", "linkReference", "inlineCode", "code"] },
    );
  };
}
