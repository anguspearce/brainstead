#!/usr/bin/env python3
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later
"""Runs the previous app's infer_meeting.py over the file names in cases.json and freezes its answers.

One answer differs on purpose (D-20261012-03): a 1-1 whose name is one the existing notes use
already ("Theo, Maya" with notes for Theo and Theo Ward) is made without asking, so after freezing,
set that case's confident back to true.

    python3 tests/fixtures/meeting/freeze.py <path to the previous app's scripts/>

The script's owner is set to the cases' owner, and its vault is a temp folder holding the cases'
notes. The output is expected.json, which src-tauri/core/src/meeting.rs compares against; run
Prettier on it afterwards. Invented names only.
"""

import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main() -> int:
    sys.path.insert(0, str(Path(sys.argv[1]).expanduser()))
    import infer_meeting as m

    cases = json.loads((HERE / "cases.json").read_text(encoding="utf-8"))
    m.OWNER = cases["owner"]
    with tempfile.TemporaryDirectory() as t:
        m.VAULT = Path(t)
        for n in cases["notes"]:
            (m.VAULT / n).write_text("x\n", encoding="utf-8")
        out = []
        for f in cases["files"]:
            r = m.infer(f)
            out.append({k: r.get(k) for k in ("type", "name", "date", "confident", "suggested_filename")})
    (HERE / "expected.json").write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
