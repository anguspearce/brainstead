#!/usr/bin/env python3
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later
"""`make check`: the Rust tests and Clippy, the TypeScript type-check, ESLint, Vitest, and the
formatting and repo checks, the groups side by side (they don't depend on each other). Each
group's output is printed whole when it finishes, with how long it took; the first failure in a
group stops that group, and any failure fails the check.

    python3 tools/check.py           # every group
    python3 tools/check.py rust tsc  # only these

Cargo builds into src-tauri/target/check, not the dev build's folder, so a running `make dev`
doesn't hold the check up on cargo's lock (or rebuild what the check just built).
"""

import os
import pathlib
import subprocess
import sys
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
TAURI = ROOT / "src-tauri"
# The running interpreter, quoted for the shell: `python3` is not on every Windows PATH, and a path
# under "Program Files" needs the quotes.
PY = f'"{sys.executable}"'
CARGO_ENV = {"CARGO_TARGET_DIR": str(TAURI / "target" / "check")}

# (directory, command, extra environment) per step, in order within the group.
GROUPS = {
    "rust": [
        (TAURI, "cargo fmt --all --check", {}),
        (TAURI, "cargo test -p brainstead-core", CARGO_ENV),
        (TAURI, "cargo test -p brainstead-mcp", CARGO_ENV),
        # The app program's own tests too: on Windows only it gets the manifest the menu code needs.
        (TAURI, "cargo test --lib --bins -p brainstead", CARGO_ENV),
        (TAURI, "cargo clippy --workspace --all-targets -- -D warnings", CARGO_ENV),
    ],
    "tsc": [
        (ROOT, "npx tsc --noEmit -p tsconfig.json", {}),
    ],
    # The caches (in node_modules/.cache) skip files unchanged since the last run.
    "eslint": [
        (ROOT, "npx eslint --cache --cache-location node_modules/.cache/eslint/ .", {}),
    ],
    "vitest": [
        (ROOT, "npx vitest run", {}),
    ],
    "format": [
        (ROOT, "npx prettier --check --cache .", {}),
        (ROOT, 'npx stylelint "src/**/*.css"', {}),
        (ROOT, "ruff check", {}),
        (ROOT, "ruff format --check", {}),
        (ROOT, f"{PY} tools/license_headers.py --check", {}),
        (ROOT, f"{PY} tools/tooltips.py", {}),
    ],
}

lock = threading.Lock()
failed: list[str] = []


def run(name: str) -> None:
    start = time.monotonic()
    out: list[str] = []
    ok = True
    for cwd, cmd, env in GROUPS[name]:
        out.append(f"$ {cmd}")
        p = subprocess.run(
            cmd,
            shell=True,
            cwd=cwd,
            env={**os.environ, "PYTHONUTF8": "1", **env},
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        out.append((p.stdout + p.stderr).rstrip())
        if p.returncode != 0:
            ok = False
            break
    took = time.monotonic() - start
    with lock:
        print(f"\n━━ {name}: {'ok' if ok else 'FAILED'} in {took:.0f} s", flush=True)
        if not ok:
            failed.append(name)
        # A passing group says only what ran; a failing one shows everything it printed.
        print("\n".join(out if not ok else [line for line in out if line.startswith("$ ")]), flush=True)


def main() -> int:
    # The box-drawing characters in the group headers are not encodable in a legacy Windows console
    # code page (cp1252), so stdout is switched to UTF-8 rather than failing on print.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    names = sys.argv[1:] or list(GROUPS)
    unknown = [n for n in names if n not in GROUPS]
    if unknown:
        print(f"Unknown group: {', '.join(unknown)}. The groups: {', '.join(GROUPS)}.")
        return 2
    start = time.monotonic()
    threads = [threading.Thread(target=run, args=(n,)) for n in names]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    took = time.monotonic() - start
    if failed:
        print(f"\nmake check failed ({', '.join(failed)}) in {took:.0f} s")
        return 1
    print(f"\nAll checks passed in {took:.0f} s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
