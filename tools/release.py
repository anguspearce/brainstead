# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Publishes a release in one go: bumps the version (tools/bump_version.py, once the current one
has a release), builds the app and the DMG with the update files, commits the bump as
"<Name> <version>", pushes, creates the GitHub release with the DMG and the update files, then
installs the app in /Applications and opens it again if it was running.

    make release NOTES="What's new"                 # shown in the app's update offer and on the release
    make release NOTES="…" VERSION=1.1.0            # a minor or major step
    make release NOTES="…" INSTALL=0                # without installing it here
    make release NOTES="…" DRY_RUN=1                # print the steps, change nothing

Refuses to start without NOTES, or while the working tree has changes other than a bump left by an
earlier try, so what's released is what's committed. After a failure, run it again: a version with
no release yet is kept, so it carries on with the same one.
"""

import json
import os
import shlex
import subprocess
import sys
from pathlib import Path

from bump_version import released

ROOT = Path(__file__).resolve().parent.parent
NAME = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())["productName"]
OUT = "src-tauri/target/release/bundle/dmg"
INSTALLED = f"/Applications/{NAME}.app"
# What tools/bump_version.py changes, and all the release commits.
BUMPED = ["src-tauri/tauri.conf.json", "package.json", "package-lock.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"]
DRY_RUN = os.environ.get("DRY_RUN", "") not in ("", "0")


def run(*cmd):
    print("+", shlex.join(cmd), flush=True)
    if not DRY_RUN:
        subprocess.run(cmd, cwd=ROOT, check=True)


def output(*cmd):
    return subprocess.run(cmd, cwd=ROOT, check=True, capture_output=True, text=True).stdout


def version():
    old = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())["version"]
    if not DRY_RUN:
        return old
    major, minor, patch = old.split(".")  # what make app's bump would make it
    return os.environ.get("VERSION") or (f"{major}.{minor}.{int(patch) + 1}" if released(old) else old)


def running():
    """Whether the installed app is open: its binary with no arguments, not the command line."""
    lines = output("ps", "-axo", "args=").splitlines()
    return f"{INSTALLED}/Contents/MacOS/{NAME}" in (line.strip() for line in lines)


def main():
    notes = os.environ.get("NOTES", "").strip()
    if not notes:
        sys.exit('release: say what\'s new: make release NOTES="…" (the update offer and the release show it)')
    changed = [line[3:] for line in output("git", "status", "--porcelain").splitlines() if line[3:] not in BUMPED]
    if changed:
        sys.exit("release: commit or stash these first, so what's released is what's committed:\n  " + "\n  ".join(changed))
    run("make", "--no-print-directory", "app")
    run("make", "--no-print-directory", "dmg")  # which gives NOTES to tools/update_files.py
    v = version()
    title = f"{NAME} {v}"
    if DRY_RUN or output("git", "status", "--porcelain", "--", *BUMPED):
        run("git", "commit", "-q", "-m", title, "--", *BUMPED)
    run("git", "push", "-q", "origin", "HEAD")
    head = "HEAD" if DRY_RUN else output("git", "rev-parse", "HEAD").strip()
    dash = NAME.replace(" ", "-")
    files = [f"{OUT}/{dash}.dmg", f"{OUT}/{dash}.app.tar.gz", f"{OUT}/latest.json"]
    run("gh", "release", "create", f"v{v}", *files, "--target", head, "--title", title, "--notes", notes)
    if os.environ.get("INSTALL", "1") != "0":
        was_running = running()
        run("make", "--no-print-directory", "-o", "app", "install-app")
        if was_running:
            run("open", INSTALLED)
    print(f"released {title}")


if __name__ == "__main__":
    main()
