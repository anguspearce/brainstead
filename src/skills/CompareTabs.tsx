// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or
// later. See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The sidebar's Compare: Doc check and any two documents, as two tabs at the top of either screen.

import { nav } from "../nav";
import { Seg } from "../ui";

export function CompareTabs({ on }: { on: "doccheck" | "compare" }) {
  return (
    <Seg<"doccheck" | "compare">
      label="Compare"
      value={on}
      onChange={(s) => s !== on && nav.replace({ screen: s })}
      options={[
        [
          "doccheck",
          "Doc check",
          "A document against the version in force (the canonical docs register), or against your earlier feedback",
        ],
        [
          "compare",
          "Any two documents",
          "Two documents side by side, neither taking precedence: their differences, or what changed between versions",
        ],
      ]}
    />
  );
}
