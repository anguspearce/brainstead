# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Windows' `make install-app`: builds the NSIS installer at the current version (no bump), quits a
running Brainstead, installs it for this user and opens it.

    python tools/install_windows.py

The installer runs silent (/S), so there is no SmartScreen prompt and no reinstall page; installing
over the same version replaces the files and keeps the data folder (%APPDATA%\\Brainstead).
"""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONF = ROOT / "src-tauri" / "tauri.conf.json"
BUNDLE = ROOT / "src-tauri" / "target" / "release" / "bundle" / "nsis"


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
    # The installer would stop it too; quitting first keeps the files free to replace.
    subprocess.run(["taskkill", "/IM", "Brainstead.exe", "/F"], capture_output=True)
    if subprocess.run([str(setup), "/S"]).returncode != 0:
        sys.exit("install_windows: the installer failed")

    exe = Path(os.environ["LOCALAPPDATA"]) / "Brainstead" / "Brainstead.exe"
    if not exe.is_file():
        sys.exit(f"install_windows: installed, but {exe} isn't there")
    print(f"installed {exe}")
    try:
        subprocess.Popen([str(exe)], cwd=exe.parent, creationflags=subprocess.DETACHED_PROCESS)
    except OSError as e:
        # An antivirus or application control policy refusing the new, unsigned exe.
        sys.exit(f"install_windows: installed, but Windows refused to start it: {e}")


if __name__ == "__main__":
    main()
