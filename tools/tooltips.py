#!/usr/bin/env python3
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later
"""Checks that every button in the app has a tooltip saying what it does (decided 2026-10-03).

    python3 tools/tooltips.py   # list the buttons without one, and fail if any

A <button> passes when its opening tag has a `title=`, or when it sits straight inside a
<span title=…> (a disabled button gets no pointer events in WebKit, so its tooltip goes on a
wrapper). Tests are skipped.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def open_tag(s: str, start: int) -> str:
    """The JSX opening tag from `start`, braces counted so `=>` inside them doesn't end it."""
    depth = 0
    i = start + 1
    while i < len(s):
        c = s[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        elif c == ">" and depth == 0:
            return s[start : i + 1]
        i += 1
    return s[start:]


def wrapped(s: str, start: int) -> bool:
    """The button is the first thing inside a <span …title=…>."""
    before = s[:start].rstrip()
    if not before.endswith(">"):
        return False
    lt = before.rfind("<span")
    return lt >= 0 and "title=" in open_tag(before, lt) and len(open_tag(before, lt)) == len(before) - lt


def main() -> int:
    missing = []
    for f in sorted((ROOT / "src").rglob("*.tsx")):
        if ".test." in f.name:
            continue
        s = f.read_text(encoding="utf-8")
        for m in re.finditer(r"<button\b", s):
            if "title=" not in open_tag(s, m.start()) and not wrapped(s, m.start()):
                missing.append(f"{f.relative_to(ROOT)}:{s.count(chr(10), 0, m.start()) + 1}")
    for x in missing:
        print(f"{x}: a button without a tooltip (title=)")
    if missing:
        print(f"{len(missing)} buttons without a tooltip", file=sys.stderr)
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
