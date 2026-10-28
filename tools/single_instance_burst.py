# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Checks that a burst of launches leaves one Brainstead running (Windows).

    python tools/single_instance_burst.py --exe src-tauri/target/release/brainstead.exe [--count 8] [--wait 30] [--quit]

Starts the app `--count` times as fast as the shell can (a few hundred milliseconds for eight), waits
`--wait` seconds for the copies to start or hand over, then counts the Brainstead.exe processes that
are the app (not `--mcp` servers, not the capture extensions' host) and reads the day's log: one
"started" since the burst began, a "second launch handed over" line from each of the other copies,
and no "capture shortcut: … already registered" from a doubled start. Passes when exactly one copy
is left and every hand-over arrived. `--quit` quits that copy afterwards (`Brainstead --quit`, as
the install script does; a debug build needs `npm run dev` up to answer it). `--exe` is a build with
the fix (`npm run tauri build`, or the debug build): the default is the installed app, which has the
bug until it's reinstalled. `BRAINSTEAD_DATA` is honoured for the log folder, though a copy started
with it set doesn't take the lock at all.
"""

import argparse
import datetime
import os
import subprocess
import sys
import time
from pathlib import Path

PS = ["powershell", "-NoProfile", "-Command"]


def app_processes() -> list[tuple[int, str]]:
    """(pid, command line) of every Brainstead.exe that is the app: not an MCP server, not the capture
    extensions' native-messaging host (Chrome starts one per message, with its origin as the argument)."""
    out = subprocess.run(
        PS + ['Get-CimInstance Win32_Process -Filter "Name=\'Brainstead.exe\'" | ForEach-Object { "$($_.ProcessId)`t$($_.CommandLine)" }'],
        capture_output=True,
        text=True,
        check=False,
    ).stdout
    found = []
    for line in out.splitlines():
        pid, _, cmd = line.partition("\t")
        if pid.strip().isdigit() and "--mcp" not in cmd and "chrome-extension://" not in cmd:
            found.append((int(pid), cmd.strip()))
    return found


def log_lines_since(data: Path, start: datetime.datetime) -> list[str]:
    path = data / "logs" / f"brainstead-{start:%Y-%m-%d}.log"
    if not path.exists():
        return []
    lines = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            at = datetime.datetime.strptime(line[:8], "%H:%M:%S").replace(year=start.year, month=start.month, day=start.day)
        except ValueError:
            continue
        if at >= start.replace(microsecond=0):
            lines.append(line)
    return lines


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", default=str(Path(os.environ["LOCALAPPDATA"]) / "Brainstead" / "Brainstead.exe"))
    ap.add_argument("--count", type=int, default=8)
    ap.add_argument("--wait", type=float, default=30)
    ap.add_argument("--quit", action="store_true", help="quit the surviving copy at the end")
    a = ap.parse_args()

    exe = str(Path(a.exe).resolve())
    data = Path(os.environ.get("BRAINSTEAD_DATA") or Path(os.environ["APPDATA"]) / "Brainstead")
    before = app_processes()
    if before:
        print(f"already running before the burst: {len(before)} copy(ies) {[p for p, _ in before]}; quit it first")
        return 2

    start = datetime.datetime.now()
    t0 = time.perf_counter()
    for _ in range(a.count):
        subprocess.Popen([exe], close_fds=True)
    print(f"launched {a.count} copies of {exe} in {time.perf_counter() - t0:.2f} s")

    time.sleep(a.wait)
    live = app_processes()
    lines = log_lines_since(data, start)
    started = [ln for ln in lines if ln.endswith(" started")]
    handed = [ln for ln in lines if "second launch handed over" in ln]
    hotkey = [ln for ln in lines if "capture shortcut:" in ln]
    print(f"after {a.wait:g} s: {len(live)} live copy(ies) {[p for p, _ in live]}")
    print(
        f'log since {start:%H:%M:%S}: {len(started)} "started" line(s), {len(handed)} hand-over(s) of {a.count - 1}, '
        f"{len(hotkey)} capture shortcut line(s)"
    )
    for ln in started + handed + hotkey:
        print("  " + ln)

    ok = len(live) == 1 and len(started) == 1 and len(handed) == a.count - 1 and not hotkey
    print("PASS" if ok else "FAIL")
    if a.quit:
        subprocess.run([exe, "--quit"], check=False)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
