#!/usr/bin/env python3
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later
"""Puts the copyright and licence header at the top of every source file, or checks it's there.

    python3 tools/license_headers.py          # add it where it's missing
    python3 tools/license_headers.py --check  # list the files without it, and fail if any

The source files are the tracked ones with a known comment style, outside _sift/ (sift's own
runtime, which has its own licence) and src-tauri/starter-vault/ (example notes and scripts that
are copied into the user's vault and become theirs).
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LINES = [
    "Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.",
    "See LICENSE for the full text.",
    "SPDX-License-Identifier: GPL-3.0-or-later",
]
MARK = "SPDX-License-Identifier: GPL-3.0-or-later"
STYLES = {
    ".rs": ("// ", ""),
    ".ts": ("// ", ""),
    ".tsx": ("// ", ""),
    ".js": ("// ", ""),
    ".mjs": ("// ", ""),
    ".swift": ("// ", ""),
    ".py": ("# ", ""),
    ".css": ("/* ", " */"),
    ".html": ("<!-- ", " -->"),
}


def files():
    out = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=True).stdout
    for name in out.splitlines():
        p = ROOT / name
        if name.startswith(("_sift/", "src-tauri/starter-vault/")) or not p.is_file():
            continue
        if p.suffix in STYLES or p.name == "Makefile":
            yield p


def header(p):
    start, end = STYLES.get(p.suffix, ("# ", ""))
    return "".join(f"{start}{line}{end}\n" for line in LINES)


def add(p):
    text = p.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=True)
    # After a shebang or a doctype, which have to come first.
    at = 1 if lines and (lines[0].startswith("#!") or lines[0].lower().startswith("<!doctype")) else 0
    head = header(p)
    rest = "".join(lines[at:])
    sep = "" if rest.startswith("\n") or not rest else "\n"
    p.write_text("".join(lines[:at]) + head + sep + rest, encoding="utf-8", newline="\n")


def main():
    check = "--check" in sys.argv
    missing = [p for p in files() if MARK not in "".join(p.read_text(encoding="utf-8").splitlines(keepends=True)[:6])]
    if check:
        for p in missing:
            print(f"no licence header: {p.relative_to(ROOT)}")
        sys.exit(1 if missing else 0)
    for p in missing:
        add(p)
        print(f"added: {p.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
