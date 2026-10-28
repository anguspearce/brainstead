# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Checks that a burst of launches leaves one Brainstead running (Windows).

    python tools/single_instance_burst.py [--exe PATH] [--count 8] [--wait 15] [--quit]

Starts the app `--count` times as fast as the shell can (a few hundred milliseconds for eight), waits
`--wait` seconds for the copies to start or hand over, then counts the Brainstead.exe processes that
aren't `--mcp` servers and reads the day's log for the lines a doubled start writes: more than one
"started" since the burst began, or "capture shortcut: … already registered". Passes when exactly
one copy is left and the log shows one start. `--quit` quits that copy afterwards (`Brainstead
--quit`, as the install script does). The default exe is the installed app; `BRAINSTEAD_DATA` is
honoured for the log folder, though a copy started with it set doesn't take the lock at all.
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
    """(pid, command line) of every Brainstead.exe that isn't an MCP server."""
    out = subprocess.run(
        PS + ['Get-CimInstance Win32_Process -Filter "Name=\'Brainstead.exe\'" | ForEach-Object { "$($_.ProcessId)`t$($_.CommandLine)" }'],
        capture_output=True,
        text=True,
        check=False,
    ).stdout
    found = []
    for line in out.splitlines():
        pid, _, cmd = line.partition("\t")
        if pid.strip().isdigit() and "--mcp" not in cmd:
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
    ap.add_argument("--wait", type=float, default=15)
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
    hotkey = [ln for ln in lines if "capture shortcut:" in ln]
    print(f"after {a.wait:g} s: {len(live)} live copy(ies) {[p for p, _ in live]}")
    print(f'log since {start:%H:%M:%S}: {len(started)} "started" line(s), {len(hotkey)} capture shortcut line(s)')
    for ln in started + hotkey:
        print("  " + ln)

    ok = len(live) == 1 and len(started) == 1 and not hotkey
    print("PASS" if ok else "FAIL")
    if a.quit:
        subprocess.run([exe, "--quit"], check=False)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
