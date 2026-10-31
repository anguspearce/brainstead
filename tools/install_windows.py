# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Windows' `make install-app`: builds the NSIS installer at the current version (no bump), quits a
running Brainstead, installs it for this user and opens it.

    python tools/install_windows.py

The installer runs silent (/S), so there is no SmartScreen prompt and no reinstall page; installing
over the same version replaces the files and keeps the data folder (%APPDATA%\\Brainstead).

Quitting asks first, as make install-app does on a Mac: `Brainstead.exe --quit` hands the request to
the running copy, which asks about unsaved edits and quits. One still running after QUIT_WAIT
seconds is stopped. Only the installed app's own window process is asked or stopped, never a
`--mcp` server an assistant started from the same program, nor a dev build elsewhere. The
installer's own check (Windows' Restart Manager) still stops anything running the installed program
file, MCP servers included, because Windows can't replace a program file in use, and its hook then
moves the old program aside (src-tauri/windows/hooks.nsh), so a server started again meanwhile can't
hold up the install. A silent NSIS install that can't write a file skips it and still succeeds, so
the installed program is compared with the one built (same_program); if the installer fails or the
program wasn't replaced, the processes still running it are listed.
"""

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONF = ROOT / "src-tauri" / "tauri.conf.json"
BUNDLE = ROOT / "src-tauri" / "target" / "release" / "bundle" / "nsis"
# The program the installer packs (MAINBINARYSRCPATH in the generated installer.nsi).
BUILT = ROOT / "src-tauri" / "target" / "release" / "Brainstead.exe"
QUIT_WAIT = 10
# Before packing the program, Tauri's bundler rewrites the last three letters of this marker in it
# to the bundle type (UNK to NSS; tauri-utils, platform.rs, __TAURI_BUNDLE_TYPE), so the installed
# program differs from BUILT there and only there.
BUNDLE_TYPE = re.compile(rb"(?<=__TAURI_BUNDLE_TYPE_VAR_)[A-Z]{3}")


def same_program(installed, built):
    """Whether `installed` is `built` as the installer packed it: the same bytes but the bundle type."""
    if installed.stat().st_size != built.stat().st_size:
        return False
    installed, built = installed.read_bytes(), built.read_bytes()
    return BUNDLE_TYPE.sub(b"___", installed) == BUNDLE_TYPE.sub(b"___", built)


def running(exe):
    """Every process running `exe`, as {ProcessId, CommandLine}."""
    query = (
        "Get-CimInstance Win32_Process -Filter \"Name='Brainstead.exe'\" | "
        "Select-Object ProcessId, ExecutablePath, CommandLine | ConvertTo-Json -Compress"
    )
    out = subprocess.run(["powershell", "-NoProfile", "-Command", query], capture_output=True, text=True).stdout.strip()
    if not out:
        return []
    rows = json.loads(out)
    rows = rows if isinstance(rows, list) else [rows]
    want = os.path.normcase(str(exe))
    return [r for r in rows if os.path.normcase(r.get("ExecutablePath") or "") == want]


def app_pids(exe):
    """The installed app's own processes: those running `exe`, less its `--mcp` servers."""
    return [r["ProcessId"] for r in running(exe) if "--mcp" not in (r.get("CommandLine") or "").split()]


def quit_app(exe):
    """Asks the installed app to quit (`--quit`), and stops it if it's still running after QUIT_WAIT seconds."""
    if not app_pids(exe):
        return
    subprocess.run([str(exe), "--quit"], cwd=exe.parent)
    for _ in range(QUIT_WAIT * 2):
        if not app_pids(exe):
            return
        time.sleep(0.5)
    print(f"Brainstead didn't quit within {QUIT_WAIT} s (unsaved edits?), so it is stopped", flush=True)
    for pid in app_pids(exe):
        subprocess.run(["taskkill", "/PID", str(pid), "/F"], capture_output=True)


def main():
    if sys.platform != "win32":
        sys.exit("install_windows: Windows only. On a Mac, run make install-app.")
    version = json.loads(CONF.read_text(encoding="utf-8"))["version"]
    build = time.strftime("%Y%m%d.%H%M%S")
    print(f"version {version}, build {build}", flush=True)
    # As the Makefile's release build: the build number for Settings › About, and the builder's
    # home folder kept out of the binary (Rust bakes absolute paths into panic metadata). Passed
    # encoded (flags apart by \x1f) because RUSTFLAGS splits on spaces, which a home folder can have.
    flags = [f"--remap-path-prefix={Path.home()}=/build", "--remap-path-scope=object"]
    env = {**os.environ, "BRAINSTEAD_BUILD": build, "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(flags)}
    if subprocess.run("npm run tauri build", shell=True, cwd=ROOT, env=env).returncode != 0:
        sys.exit("install_windows: the build failed")

    setup = BUNDLE / f"Brainstead_{version}_x64-setup.exe"
    if not setup.is_file():
        sys.exit(f"install_windows: no installer at {setup}")
    exe = Path(os.environ["LOCALAPPDATA"]) / "Brainstead" / "Brainstead.exe"
    if exe.is_file():
        quit_app(exe)
    failed = subprocess.run([str(setup), "/S"]).returncode != 0
    if failed or not exe.is_file() or not same_program(exe, BUILT):
        for r in running(exe):
            print(f"still running {exe.name}: {r['ProcessId']} {r.get('CommandLine') or ''}", flush=True)
        if failed:
            sys.exit("install_windows: the installer failed")
        if not exe.is_file():
            sys.exit(f"install_windows: installed, but {exe} isn't there")
        sys.exit(f"install_windows: the installer finished, but {exe} is still the old program")
    print(f"installed {exe}")
    try:
        subprocess.Popen([str(exe)], cwd=exe.parent, creationflags=subprocess.DETACHED_PROCESS)
    except OSError as e:
        # An antivirus or application control policy refusing the new, unsigned exe.
        sys.exit(f"install_windows: installed, but Windows refused to start it: {e}")


if __name__ == "__main__":
    main()
