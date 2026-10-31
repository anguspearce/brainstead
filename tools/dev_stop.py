#!/usr/bin/env python3
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later
"""Finds the dev processes a Claude Code session or a checkout leaves running, and stops them.

    python3 tools/dev_stop.py                      # list the dev processes this Claude Code session started (a dry run)
    python3 tools/dev_stop.py --kill               # stop them, children first
    python3 tools/dev_stop.py --anyone             # every dev process of this checkout, whoever started it
    python3 tools/dev_stop.py --worktree <path>    # another checkout
    python3 tools/dev_stop.py --all                # also every worktree under .claude/worktrees/ whose session has ended
    python3 tools/dev_stop.py --all --include-live --kill   # every worktree, live sessions too
    python3 tools/dev_stop.py --store-stubs        # Windows: hung python3 Store aliases, anywhere on the machine

(`python` rather than `python3` on Windows.)

Dev processes: a program under the worktree's `src-tauri/target/` (the debug app, a repro copy, a test binary), Vite, the
Tauri CLI and `npm run dev` started for it, WebView2 or WebKit helpers under those, and cargo, cargo-clippy and rustc
building into it. The wrappers that started them (npm, npx, npm's script shell, the rustup and cargo-clippy proxies) go
with them. Never the installed app, a `--mcp` server or Claude.

Whose they are. A session's processes are the descendants of its own Claude Code process and nothing else: `CLAUDE_PID`
when it names a process above this script, otherwise the nearest Claude Code process above it (`claude`, or Node running
`@anthropic-ai/claude-code`) that isn't the desktop app. They count in the session's checkout and in every other checkout
of the same repository (its main folder and `.claude/worktrees/`). A developer's own `npm run tauri dev` or `make check`,
and another session's processes, don't descend from it, so they are never the session's; nor is a process that has left
the session's tree (its shell ended and it ran on). When no session is found (a terminal, another agent, a Claude Code
process not recognised), nothing in the checkout is the session's: nothing there is stopped, and the script says why.
`--anyone` lists every dev process of the checkout, whoever started it.

`--all` adds the worktrees under `.claude/worktrees/` whose folder is gone or whose session has ended, whoever started
their processes. A worktree is live while its folder exists and either a Claude transcript in `~/.claude/projects/`
changed in the last 12 hours or one of its processes still descends from a running Claude; `--all` leaves live worktrees
alone unless `--include-live` is given. The main checkout is never swept: only its own session, or `--anyone` run there,
stops what runs in it.

The `SessionEnd` hook in `.claude/settings.json` runs `tools/dev_stop.sh --session-end`: the ending session's dev
processes, stopped (not on `/clear`, which keeps the session going), never failing the hook.
"""

from __future__ import annotations

import argparse
import base64
import dataclasses
import json
import os
import pathlib
import re
import signal
import subprocess
import sys
import time

CHECKOUT = pathlib.Path(__file__).resolve().parent.parent
WINDOWS = sys.platform == "win32"
LIVE_HOURS = 12
# A Store stub younger than this may still be starting the real interpreter.
STUB_MIN_AGE = 120
# The longest the CIM fallback may take under `--session-end`, inside the SessionEnd hook's 30 s timeout
# (.claude/settings.json) with time left to stop what it finds.
HOOK_CIM_TIMEOUT = 20
CIM_TIMEOUT = 60
# The Claude desktop app, which runs sessions as `claude` processes below it and is never a session itself: the
# Microsoft Store and Squirrel installs on Windows, the app bundle on macOS. Matched on the program's path, lower case.
DESKTOP = re.compile(r"/windowsapps/claude_[^/]*/|/anthropicclaude/|/claude\.app/contents/macos/claude$")

RUST = {"cargo", "cargo-clippy", "rustc", "clippy-driver"}
SHELLS = {"cmd", "sh", "bash", "zsh", "dash"}
# What a dev process's own children may be; anything else under it is left alone.
CHILDREN = RUST | SHELLS | {"msedgewebview2", "node", "esbuild", "conhost", "link", "brainstead"}
VITE = re.compile(r"\bvite(?:/bin/vite\.js|\.js)?(?=[\s\"']|$)")
TAURI_CLI = re.compile(r"@tauri-apps/cli|/tauri\.js\b|\bnpm(?:-cli\.js[\"']?)? run tauri\b")
NPM_RUN_DEV = re.compile(r"\bnpm(?:-cli\.js[\"']?)? run dev\b")
NODE_WRAPPER = re.compile(r"npm-cli\.js|npx-cli\.js|^npm\s|^npx\s|/npm\s|/npx\s|@tauri-apps/cli|/tauri\.js\b")
# Characters that may come right before a path in a command line.
BOUNDARY = " \t\"'=,;("


@dataclasses.dataclass(frozen=True)
class Proc:
    pid: int
    ppid: int
    exe: str = ""
    cmd: str = ""
    cwd: str = ""
    name: str = ""
    cpu: float | None = None  # seconds of CPU used
    started: float | None = None  # epoch seconds


@dataclasses.dataclass(frozen=True)
class Scope:
    current: str  # the checkout always covered, normalised (`norm`)
    roots: frozenset[str] = frozenset()  # the repository's other checkouts that exist, normalised
    worktrees_dir: str = ""  # `<repo>/.claude/worktrees`, normalised; finds folders that are gone
    active: frozenset[str] = frozenset()  # roots with a recent Claude transcript
    include_live: bool = False
    stubs: bool = False
    windows: bool = WINDOWS
    self_pid: int = 0
    now: float = 0.0
    # `--all`: other worktrees' dev processes too, whoever started them.
    sweep: bool = False
    # The session's Claude Code process: only its descendants are matched in `current` and `roots` (outside a sweep).
    # Empty: no session, so nothing there. None: every dev process of `current` (`--anyone`).
    owners: frozenset[int] | None = None


@dataclasses.dataclass(frozen=True)
class Match:
    pid: int
    name: str
    worktree: str
    why: str
    depth: int


def norm(text: str, windows: bool) -> str:
    """Forward slashes, one at a time; on Windows lower case and `/c/...` (MSYS) as `c:/...`."""
    t = re.sub(r"/{2,}", "/", text.replace("\\", "/"))
    if windows:
        t = re.sub(r"(^|[\s\"'=,;])/([a-z])/", r"\1\2:/", t.lower())
    return t


def base(p: Proc) -> str:
    name = p.name or os.path.basename(p.exe.replace("\\", "/")) or first_token(p.cmd).replace("\\", "/").rsplit("/", 1)[-1]
    name = name.lower()
    return name[:-4] if name.endswith(".exe") else name


def first_token(cmd: str) -> str:
    cmd = cmd.strip()
    if cmd.startswith('"'):
        return cmd[1:].split('"', 1)[0]
    return cmd.split(" ", 1)[0]


def within(path: str, root: str) -> bool:
    """`path` is `root` or under it, and not under a worktree nested inside it (the main checkout's `.claude/worktrees/`)."""
    if path == root:
        return True
    return path.startswith(root + "/") and not path[len(root) + 1 :].startswith(".claude/worktrees/")


def mentions(text: str, root: str) -> bool:
    needle = root + "/"
    i = text.find(needle)
    while i >= 0:
        if (i == 0 or text[i - 1] in BOUNDARY) and not text[i + len(needle) :].startswith(".claude/worktrees/"):
            return True
        i = text.find(needle, i + 1)
    return False


def exe_of(p: Proc, windows: bool) -> str:
    return norm(p.exe or first_token(p.cmd), windows)


def protected(p: Proc, roots: list[str], windows: bool) -> bool:
    """Claude, MCP servers and any Brainstead not built in a worktree: never matched, and walks stop there."""
    b = base(p)
    if b == "claude" or (b == "brainstead" and "--mcp" in p.cmd):
        return True
    if b == "brainstead":
        exe = exe_of(p, windows)
        return not any(within(exe, r + "/src-tauri/target") for r in roots)
    return False


class Table:
    """A process table with its parent links. A parent that started after its child is a reused pid: the real one is
    gone, so the child has no parent."""

    def __init__(self, procs: list[Proc]):
        self.by_pid = {p.pid: p for p in procs}
        self.children: dict[int, list[Proc]] = {}
        for p in procs:
            self.children.setdefault(p.ppid, []).append(p)

    def parent(self, p: Proc) -> Proc | None:
        q = self.by_pid.get(p.ppid)
        if q is None or q.pid == p.pid or (q.started and p.started and q.started > p.started):
            return None
        return q

    def ancestors(self, p: Proc) -> list[Proc]:
        out, seen = [], {p.pid}
        while (p := self.parent(p)) is not None and p.pid not in seen and len(out) < 64:
            out.append(p)
            seen.add(p.pid)
        return out

    def under(self, p: Proc, pids: set[int]) -> bool:
        """`p` is one of `pids` or descends from one."""
        return p.pid in pids or any(q.pid in pids for q in self.ancestors(p))


def is_claude_code(p: Proc, windows: bool) -> bool:
    """A Claude Code process: the `claude` program (not the desktop app) or Node running its npm package."""
    if base(p) == "claude":
        return not DESKTOP.search(norm(p.exe or first_token(p.cmd), False).lower())
    return base(p) == "node" and "@anthropic-ai/claude-code" in norm(p.cmd, windows)


def session_of(procs: list[Proc], self_pid: int, hint: int | None, windows: bool = WINDOWS) -> Proc | None:
    """The Claude Code session this process runs in: `hint` (`CLAUDE_PID`) when it is above it, otherwise the nearest
    Claude Code process above it. None outside a session (a terminal, the desktop app's own terminal)."""
    t = Table(procs)
    me = t.by_pid.get(self_pid)
    if me is None:
        return None
    above = t.ancestors(me)
    return next((q for q in above if q.pid == hint), None) or next((q for q in above if is_claude_code(q, windows)), None)


def anchor(p: Proc, roots: list[str], windows: bool) -> tuple[str, str] | None:
    """The worktree a process plainly belongs to (its program, command line or folder), and why."""
    b = base(p)
    exe = exe_of(p, windows)
    cmd = norm(p.cmd, windows)
    cwd = norm(p.cwd, windows).rstrip("/") if p.cwd else ""
    for r in roots:
        if within(exe, r + "/src-tauri/target"):
            if within(exe, r + "/src-tauri/target/repro"):
                return r, "repro copy under src-tauri/target/repro"
            return r, ("app" if b == "brainstead" else "program") + " under src-tauri/target"
    owner = next((r for r in roots if mentions(cmd, r) or (cwd and within(cwd, r))), None)
    if owner is None:
        return None
    if b == "node" or b in ("npm", "npx"):
        if VITE.search(cmd):
            return owner, "vite dev server"
        if TAURI_CLI.search(cmd):
            return owner, "Tauri CLI"
        if NPM_RUN_DEV.search(cmd):
            return owner, "npm run dev"
    if b == "esbuild":
        return owner, "esbuild (vite)"
    if b in RUST:
        return owner, f"{b} building in the worktree"
    if b == "msedgewebview2" and "--user-data-dir" in cmd:
        return owner, "WebView2 with its data in the worktree"
    if b.startswith("com.apple.webkit"):
        return owner, "WebKit helper for the worktree"
    return None


def find(procs: list[Proc], scope: Scope) -> tuple[list[Match], dict[str, int]]:
    """The processes to stop, and per live worktree left alone how many of its processes were."""
    w = scope.windows
    tbl = Table(procs)
    by_pid, children, parent = tbl.by_pid, tbl.children, tbl.parent

    roots = {scope.current} | set(scope.roots)
    if scope.worktrees_dir:
        pattern = re.compile(re.escape(scope.worktrees_dir) + r"/([^/\s\"';,=]+)")
        for p in procs:
            for text in (norm(p.cmd, w), exe_of(p, w)):
                roots.update(f"{scope.worktrees_dir}/{name}" for name in pattern.findall(text))
    ordered = sorted(roots, key=len, reverse=True)

    mine = {scope.self_pid}
    q = by_pid.get(scope.self_pid)
    while q is not None and (q := parent(q)) is not None and q.pid not in mine:
        mine.add(q.pid)

    def off_limits(p: Proc) -> bool:
        return p.pid in mine or protected(p, ordered, w)

    owner_pids = {pid for pid in scope.owners or () if pid in by_pid}

    def owned(pid: int) -> bool:
        return scope.owners is not None and tbl.under(by_pid[pid], owner_pids)

    found: dict[int, tuple[str, str]] = {}
    for p in procs:
        if not off_limits(p) and (hit := anchor(p, ordered, w)):
            found[p.pid] = hit

    # Children: the app's WebView2, vite's esbuild, cargo's rustc and the test binaries.
    queue = list(found)
    while queue:
        pid = queue.pop()
        root = found[pid][0]
        for c in children.get(pid, []):
            if c.pid in found or off_limits(c) or parent(c) is not by_pid[pid]:
                continue
            if base(c) in CHILDREN or base(c).startswith("com.apple.webkit"):
                found[c.pid] = (root, f"child of {base(by_pid[pid])} {pid}")
                queue.append(c.pid)

    # Wrappers that started them: npm and npx, npm's script shell, the cargo and cargo-clippy proxies, the Tauri CLI.
    def wraps(q: Proc) -> bool:
        b = base(q)
        if b in ("cargo", "cargo-clippy"):
            return True
        if b in ("node", "npm", "npx"):
            return bool(NODE_WRAPPER.search(norm(q.cmd, w)))
        if b in SHELLS:
            up = parent(q)
            return up is not None and base(up) in ("node", "npm", "npx") and bool(NODE_WRAPPER.search(norm(up.cmd, w)))
        return False

    for pid in list(found):
        p = by_pid[pid]
        while (up := parent(p)) is not None and up.pid not in found and not off_limits(up) and wraps(up):
            found[up.pid] = (found[pid][0], f"started {base(p)} {p.pid}")
            p = up

    def claude_above(p: Proc) -> bool:
        seen = set()
        while (p := parent(p)) is not None and p.pid not in seen:
            if base(p) == "claude":
                return True
            seen.add(p.pid)
        return False

    status: dict[str, str] = {}
    for root in roots:
        pids = [pid for pid, (r, _) in found.items() if r == root]
        if root == scope.current:
            status[root] = ""
        elif root not in scope.roots:
            status[root] = " (gone)"
        elif root in scope.active or any(claude_above(by_pid[pid]) for pid in pids):
            status[root] = " (live)"
        else:
            status[root] = " (ended)"
    left: dict[str, int] = {}
    keep: dict[int, tuple[str, str]] = {}
    for pid, (root, why) in found.items():
        name = root.rsplit("/", 1)[-1]
        if owned(pid):
            keep[pid] = (name, why)
        elif root == scope.current:
            if scope.owners is None:
                keep[pid] = (name, why)
        elif not scope.sweep or not root.startswith(scope.worktrees_dir + "/"):
            continue  # another checkout's, and not this session's
        elif status[root] == " (live)" and not scope.include_live:
            left[name] = left.get(name, 0) + 1
        else:
            keep[pid] = (name + status[root], why)

    if scope.stubs:
        for p in procs:
            if p.pid not in keep and not off_limits(p) and hung_stub(p, parent(p), children.get(p.pid, []), scope.now, w):
                keep[p.pid] = ("-", "hung python3 Store stub (no CPU, parent gone)")

    def depth(pid: int) -> int:
        d, p = 0, by_pid[pid]
        while (p := parent(p)) is not None and p.pid in keep and d < 64:
            d += 1
        return d

    matches = [Match(pid, base(by_pid[pid]), label, why, depth(pid)) for pid, (label, why) in keep.items()]
    # Children first, so a wrapper doesn't restart what it started, and a kill never reaches a reused pid.
    matches.sort(key=lambda m: (m.worktree, -m.depth, m.pid))
    return matches, left


def hung_stub(p: Proc, parent: Proc | None, kids: list[Proc], now: float, windows: bool) -> bool:
    """A Store alias `python3.exe` that never started Python: no CPU, no children, its parent gone, and not new."""
    return (
        base(p) == "python3"
        and any("/windowsapps/" in norm(path, windows) for path in (p.exe, first_token(p.cmd)))
        and p.cpu == 0
        and parent is None
        and not kids
        and p.started is not None
        and now - p.started >= STUB_MIN_AGE
    )


# Reading the process table.

PS_SCRIPT = r"""
[Console]::OutputEncoding = [Text.Encoding]::UTF8
@(Get-CimInstance Win32_Process | ForEach-Object {
  $start = $null
  if ($_.CreationDate) { $start = ([DateTimeOffset]$_.CreationDate).ToUnixTimeSeconds() }
  [pscustomobject]@{ pid = [int]$_.ProcessId; ppid = [int]$_.ParentProcessId; name = $_.Name; exe = $_.ExecutablePath;
    cmd = $_.CommandLine; cpu = ([double]$_.KernelModeTime + [double]$_.UserModeTime) / 1e7; started = $start }
}) | ConvertTo-Json -Compress
"""


def read_windows(cim_timeout: float) -> list[Proc]:
    """The Win32 calls take well under a second; the CIM query, the fallback, can take half a minute on a busy machine,
    so it is given `cim_timeout` seconds (none at all with 0) and raises `subprocess.TimeoutExpired` after that."""
    try:
        return read_win32()
    except (OSError, AttributeError, ValueError):
        if not cim_timeout:
            raise
        return read_cim(cim_timeout)


def read_win32() -> list[Proc]:
    import ctypes
    from ctypes import wintypes

    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    ntdll = ctypes.WinDLL("ntdll")

    class Entry(ctypes.Structure):  # PROCESSENTRY32W
        _fields_ = [
            ("dwSize", wintypes.DWORD),
            ("cntUsage", wintypes.DWORD),
            ("th32ProcessID", wintypes.DWORD),
            ("th32DefaultHeapID", ctypes.c_size_t),
            ("th32ModuleID", wintypes.DWORD),
            ("cntThreads", wintypes.DWORD),
            ("th32ParentProcessID", wintypes.DWORD),
            ("pcPriClassBase", ctypes.c_long),
            ("dwFlags", wintypes.DWORD),
            ("szExeFile", ctypes.c_wchar * 260),
        ]

    class UnicodeString(ctypes.Structure):
        _fields_ = [("Length", ctypes.c_ushort), ("MaximumLength", ctypes.c_ushort), ("Buffer", ctypes.c_void_p)]

    k32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    k32.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    k32.Process32FirstW.argtypes = k32.Process32NextW.argtypes = [wintypes.HANDLE, ctypes.POINTER(Entry)]
    k32.OpenProcess.restype = wintypes.HANDLE
    k32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    k32.CloseHandle.argtypes = [wintypes.HANDLE]
    k32.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
    k32.GetProcessTimes.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
    k32.ReadProcessMemory.argtypes = [wintypes.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p]
    k32.IsWow64Process.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.BOOL)]
    ntdll.NtQueryInformationProcess.restype = ctypes.c_long
    ulong_p = ctypes.POINTER(wintypes.ULONG)
    ntdll.NtQueryInformationProcess.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.ULONG, ulong_p]
    command_line = 60  # ProcessCommandLineInformation, Windows 8.1 and later
    query_limited = 0x1000  # PROCESS_QUERY_LIMITED_INFORMATION

    snap = k32.CreateToolhelp32Snapshot(2, 0)  # TH32CS_SNAPPROCESS
    if not snap or snap == wintypes.HANDLE(-1).value:
        raise OSError(ctypes.get_last_error(), "CreateToolhelp32Snapshot failed")
    rows = []
    entry = Entry(dwSize=ctypes.sizeof(Entry))
    ok = k32.Process32FirstW(snap, ctypes.byref(entry))
    while ok:
        rows.append((entry.th32ProcessID, entry.th32ParentProcessID, entry.szExeFile))
        ok = k32.Process32NextW(snap, ctypes.byref(entry))
    k32.CloseHandle(snap)

    def ticks(ft: wintypes.FILETIME) -> int:
        return ft.dwHighDateTime << 32 | ft.dwLowDateTime

    def folder(pid: int) -> str:
        """The process's working folder, from its PEB (a 64-bit process, read from 64-bit Python): ProcessParameters
        at 0x20 in the PEB, CurrentDirectory's path at 0x38 in those. Empty when it can't be read."""
        if ctypes.sizeof(ctypes.c_void_p) != 8:
            return ""
        h = k32.OpenProcess(0x0410, False, pid)  # PROCESS_QUERY_INFORMATION | PROCESS_VM_READ
        if not h:
            return ""
        try:
            wow = wintypes.BOOL()
            if not k32.IsWow64Process(h, ctypes.byref(wow)) or wow.value:
                return ""
            basic = (ctypes.c_void_p * 6)()  # PROCESS_BASIC_INFORMATION: the PEB's address is the second field
            if ntdll.NtQueryInformationProcess(h, 0, basic, ctypes.sizeof(basic), None) != 0 or not basic[1]:
                return ""
            params, path = ctypes.c_void_p(), UnicodeString()
            if not k32.ReadProcessMemory(h, basic[1] + 0x20, ctypes.byref(params), 8, None) or not params.value:
                return ""
            if not k32.ReadProcessMemory(h, params.value + 0x38, ctypes.byref(path), ctypes.sizeof(path), None):
                return ""
            buf = ctypes.create_unicode_buffer(path.Length // 2 + 1)
            if not path.Buffer or not k32.ReadProcessMemory(h, path.Buffer, buf, path.Length, None):
                return ""
            return buf.value
        finally:
            k32.CloseHandle(h)

    procs = []
    for pid, ppid, name in rows:
        exe = cmd = ""
        cpu = started = None
        h = k32.OpenProcess(query_limited, False, pid) if pid else None
        if h:
            buf = ctypes.create_unicode_buffer(32768)
            size = wintypes.DWORD(len(buf))
            if k32.QueryFullProcessImageNameW(h, 0, buf, ctypes.byref(size)):
                exe = buf.value
            times = [wintypes.FILETIME() for _ in range(4)]
            if k32.GetProcessTimes(h, *(ctypes.byref(t) for t in times)):
                started = (ticks(times[0]) - 116444736000000000) / 1e7  # from 1601 to 1970, in 100 ns
                cpu = (ticks(times[2]) + ticks(times[3])) / 1e7
            need = wintypes.ULONG(0)
            ntdll.NtQueryInformationProcess(h, command_line, None, 0, ctypes.byref(need))
            if need.value:
                raw = ctypes.create_string_buffer(need.value)
                if ntdll.NtQueryInformationProcess(h, command_line, raw, need, ctypes.byref(need)) == 0:
                    us = UnicodeString.from_buffer(raw)
                    if us.Buffer:
                        cmd = ctypes.wstring_at(us.Buffer, us.Length // 2)
            k32.CloseHandle(h)
        # Only cargo and the like are matched by their folder, and only they are read (a read costs a few calls).
        cwd = folder(pid) if h and name.lower().removesuffix(".exe") in RUST else ""
        procs.append(Proc(pid, ppid, exe, cmd, cwd, name, cpu, started))
    return procs


def read_cim(timeout: float) -> list[Proc]:
    script = base64.b64encode(PS_SCRIPT.encode("utf-16-le")).decode()
    out = subprocess.run(
        ["powershell", "-NoProfile", "-NonInteractive", "-EncodedCommand", script],
        capture_output=True,
        check=True,
        timeout=timeout,
    ).stdout.decode("utf-8", "replace")
    rows = json.loads(out.lstrip("\ufeff") or "[]")
    return [
        Proc(r["pid"], r["ppid"], r.get("exe") or "", r.get("cmd") or "", "", r.get("name") or "", r.get("cpu"), r.get("started"))
        for r in (rows if isinstance(rows, list) else [rows])
    ]


def seconds(text: str) -> float:
    """`ps` times: `[[dd-]hh:]mm:ss[.cc]`."""
    days, _, rest = text.strip().rpartition("-")
    total = 0.0
    for part in rest.split(":"):
        total = total * 60 + float(part)
    return total + (int(days) * 86400 if days else 0)


def read_posix() -> list[Proc]:
    now = time.time()
    ps = ["ps", "-A", "-ww", "-o"]
    rows = subprocess.run([*ps, "pid=,ppid=,etime=,time=,args="], capture_output=True, text=True, check=True).stdout
    comm = subprocess.run([*ps, "pid=,comm="], capture_output=True, text=True, check=True).stdout
    names = dict(line.strip().split(None, 1) for line in comm.splitlines() if len(line.split(None, 1)) == 2)
    procs = []
    for line in rows.splitlines():
        parts = line.split(None, 4)
        if len(parts) < 4:
            continue
        pid, ppid = int(parts[0]), int(parts[1])
        exe = cwd = ""
        if os.path.isdir(f"/proc/{pid}"):  # Linux
            try:
                exe, cwd = os.readlink(f"/proc/{pid}/exe"), os.readlink(f"/proc/{pid}/cwd")
            except OSError:
                pass
        comm_path = names.get(parts[0], "")
        if not exe and comm_path.startswith("/"):  # macOS gives the program's path
            exe = comm_path
        name = os.path.basename(exe or comm_path)
        procs.append(Proc(pid, ppid, exe, parts[4] if len(parts) > 4 else "", cwd, name, seconds(parts[3]), now - seconds(parts[2])))
    # macOS has no /proc: cargo and the like, the processes matched by their folder, get theirs from lsof.
    folders = lsof_folders([p.pid for p in procs if not p.cwd and base(p) in RUST])
    return [dataclasses.replace(p, cwd=folders[p.pid]) if p.pid in folders else p for p in procs]


def lsof_folders(pids: list[int]) -> dict[int, str]:
    if not pids:
        return {}
    try:
        cmd = ["lsof", "-a", "-d", "cwd", "-Fn", "-p", ",".join(map(str, pids))]
        return parse_lsof(subprocess.run(cmd, capture_output=True, text=True, timeout=5).stdout)
    except (OSError, subprocess.SubprocessError):
        return {}


def parse_lsof(out: str) -> dict[int, str]:
    """`lsof -Fn` output: a `p<pid>` line, then an `f` line and an `n<path>` line per file."""
    folders: dict[int, str] = {}
    pid = None
    for line in out.splitlines():
        if line.startswith("p") and line[1:].isdigit():
            pid = int(line[1:])
        elif line.startswith("n") and pid is not None:
            folders[pid] = line[1:]
    return folders


def read_procs(cim_timeout: float = CIM_TIMEOUT) -> list[Proc]:
    return read_windows(cim_timeout) if WINDOWS else read_posix()


def repo_root(checkout: pathlib.Path) -> pathlib.Path:
    parts = checkout.parts
    for i in range(len(parts) - 2):
        if parts[i] == ".claude" and parts[i + 1] == "worktrees":
            return pathlib.Path(*parts[:i])
    return checkout


def transcript_dir(root: pathlib.Path) -> pathlib.Path:
    config = pathlib.Path(os.environ.get("CLAUDE_CONFIG_DIR") or pathlib.Path.home() / ".claude")
    return config / "projects" / re.sub(r"[^A-Za-z0-9]", "-", str(root))


def recently_used(root: pathlib.Path, now: float) -> bool:
    try:
        return any(now - f.stat().st_mtime < LIVE_HOURS * 3600 for f in transcript_dir(root).glob("*.jsonl"))
    except OSError:
        return False


def make_scope(
    checkout: pathlib.Path,
    everything: bool,
    include_live: bool,
    stubs: bool = False,
    owners: frozenset[int] | None = None,
) -> Scope:
    """With `everything` (`--all`) or a session (`owners`), the repository's other checkouts are roots too: a sweep
    takes its ended worktrees' processes, a session its own wherever they run."""
    now = time.time()
    roots: dict[str, pathlib.Path] = {}
    repo = repo_root(checkout)
    wt_dir = repo / ".claude" / "worktrees"
    if everything or owners is not None:
        if wt_dir.is_dir():
            roots = {norm(str(d), WINDOWS): d for d in wt_dir.iterdir() if d.is_dir()}
        if owners is not None:
            roots[norm(str(repo), WINDOWS)] = repo
    current = norm(str(checkout), WINDOWS)
    roots.pop(current, None)
    return Scope(
        current=current,
        roots=frozenset(roots),
        worktrees_dir=norm(str(wt_dir), WINDOWS) if everything or owners is not None else "",
        active=frozenset(r for r, d in roots.items() if everything and recently_used(d, now)),
        include_live=include_live,
        stubs=stubs and WINDOWS,
        self_pid=os.getpid(),
        now=now,
        sweep=everything,
        owners=owners,
    )


def claude_pid_hint() -> int | None:
    value = os.environ.get("CLAUDE_PID", "")
    return int(value) if value.isdigit() else None


def owners_of(claude: Proc | None, anyone: bool) -> frozenset[int] | None:
    """Whose dev processes in the checkout are stopped: everyone's with `--anyone` (None), otherwise the descendants of
    the session's Claude Code process, and nobody's when no session runs this script."""
    if anyone:
        return None
    return frozenset({claude.pid}) if claude else frozenset()


def table(matches: list[Match]) -> str:
    rows = [("PID", "NAME", "WORKTREE", "WHY")] + [(str(m.pid), m.name, m.worktree, m.why) for m in matches]
    widths = [max(len(r[i]) for r in rows) for i in range(3)]
    return "\n".join(f"{r[0]:>{widths[0]}}  {r[1]:<{widths[1]}}  {r[2]:<{widths[2]}}  {r[3]}" for r in rows)


def stop(matches: list[Match]) -> list[tuple[Match, str]]:
    """Stops each process, children first; returns what failed and why."""
    failed = []
    for m in matches:
        try:
            os.kill(m.pid, signal.SIGTERM)  # TerminateProcess on Windows
        except ProcessLookupError:
            pass
        except OSError as e:
            if WINDOWS:
                p = subprocess.run(["taskkill", "/F", "/PID", str(m.pid)], capture_output=True, text=True)
                if p.returncode not in (0, 128):  # 128: already gone
                    failed.append((m, (p.stdout + p.stderr).strip() or str(e)))
            else:
                failed.append((m, str(e)))
    if not WINDOWS:
        deadline = time.monotonic() + 5
        alive = [m for m in matches if running(m.pid)]
        while alive and time.monotonic() < deadline:
            time.sleep(0.2)
            alive = [m for m in alive if running(m.pid)]
        for m in alive:
            try:
                os.kill(m.pid, signal.SIGKILL)
            except OSError as e:
                failed.append((m, str(e)))
    return failed


def running(pid: int) -> bool:
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except OSError:
        return True


def main(argv: list[str] | None = None) -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("--worktree", type=pathlib.Path, default=CHECKOUT, help="the checkout to clean up (default: this one)")
    ap.add_argument("--anyone", action="store_true", help="every dev process of the checkout, not only this session's")
    ap.add_argument("--all", action="store_true", help="also every worktree whose session has ended, and gone folders")
    ap.add_argument("--include-live", action="store_true", help="with --all: live worktrees too")
    ap.add_argument("--store-stubs", action="store_true", help="Windows: hung python3 Store aliases anywhere on the machine")
    ap.add_argument("--kill", action="store_true", help="stop them (default: only list them)")
    ap.add_argument("--session-end", action="store_true", help="for the SessionEnd hook: this session's, stopped; never fails")
    args = ap.parse_args(argv)
    if args.include_live and not args.all:
        ap.error("--include-live needs --all")
    if not args.session_end:
        return run(args)
    # The SessionEnd hook: whatever goes wrong is reported, and the hook still succeeds.
    try:
        return run(args)
    except Exception as e:
        print(f"dev_stop: {type(e).__name__}: {e}", file=sys.stderr)
        return 0


def run(args: argparse.Namespace) -> int:
    hook = args.session_end
    if hook:
        try:
            if json.loads(sys.stdin.read() or "{}").get("reason") == "clear":
                return 0  # /clear keeps the session going, and what it started
        except (ValueError, AttributeError, OSError):
            pass
        args.kill = True
        args.anyone = False
    worktree = args.worktree.resolve()
    try:
        procs = read_procs(HOOK_CIM_TIMEOUT if hook else CIM_TIMEOUT)
        claude = session_of(procs, os.getpid(), claude_pid_hint())
        owners = owners_of(claude, args.anyone)
        scope = make_scope(worktree, args.all, args.include_live, args.store_stubs, owners)
        matches, left = find(procs, scope)
    except (OSError, subprocess.SubprocessError, ValueError) as e:
        print(f"dev_stop: couldn't read the processes: {e}", file=sys.stderr)
        return 0 if hook else 1
    if owners is None:
        print(f"Every dev process of {worktree}" + (", and of ended worktrees:" if args.all else ":"))
    elif claude:
        print(f"The dev processes Claude Code session {claude.pid} started" + (", and ended worktrees':" if args.all else ":"))
    else:
        print(
            f"No Claude Code process is above this script (CLAUDE_PID or a claude process), so nothing in {worktree} is "
            "a session's to stop; --anyone stops every dev process there."
        )
    print(table(matches) if matches else "None found.")
    if left:
        print("Left alone (live): " + ", ".join(f"{k} ({n})" for k, n in sorted(left.items())))
    failed = stop(matches) if args.kill and matches else []
    if args.kill and matches:
        print(f"\nStopped {len(matches) - len(failed)} of {len(matches)}.")
        for m, why in failed:
            print(f"  {m.pid} {m.name}: {why}")
    return 0 if hook or not failed else 1


if __name__ == "__main__":
    sys.exit(main())
