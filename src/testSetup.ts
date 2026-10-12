// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Before every test file (vite.config.ts): `waitFor` and `findBy…` wait up to 5 s rather than 1, so
// a machine busy compiling the Rust side (make check runs both at once) doesn't fail a test that
// passes when it's idle. A test that passes still returns as soon as it can. And jsdom has no
// layout, so the ranges CodeMirror measures (to draw the selection, to scroll the caret into view)
// give empty boxes rather than throwing.

import { configure } from "@testing-library/react";

configure({ asyncUtilTimeout: 5000 });

if (typeof Range !== "undefined") {
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
  Range.prototype.getBoundingClientRect = () => new DOMRect();
}
