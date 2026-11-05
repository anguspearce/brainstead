# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later
"""Tests for dev_stop.py's matching, over fake process tables (`python3 tools/check.py python`)."""

from __future__ import annotations

import contextlib
import io
import os
import pathlib
import subprocess
import sys
import tempfile
import time
import unittest
import unittest.mock

import dev_stop
from dev_stop import (
    Proc,
    Scope,
    find,
    norm,
    owners_of,
    parse_lsof,
    session_of,
)

REPO = r"C:\Work\orbit"
WT = REPO + r"\.claude\worktrees"
HERE = WT + r"\brave-lena-1a2b3c"
OTHER = WT + r"\calm-maya-4d5e6f"
GONE = WT + r"\old-acme-7a8b9c"
INSTALLED = r"C:\Users\maya\AppData\Local\Brainstead\Brainstead.exe"
STUB = r"C:\Users\maya\AppData\Local\Microsoft\WindowsApps\python3.exe"
NODE = r"C:\Program Files\nodejs\node.exe"
CARGO = r"C:\Users\maya\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe"
RUSTC = r"C:\Users\maya\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe"
NOW = 1_800_000_000.0

# A Claude session (2) with its shell (3), and the dev processes it started.
SESSION = [
    Proc(1, 0, r"C:\Program Files\WindowsApps\Claude_4.29.0.0_x64__abc\app\Claude.exe", name="claude.exe", started=NOW - 9000),
    Proc(2, 1, r"C:\Users\maya\claude-code\claude.exe", "claude.exe --output-format stream-json", name="claude.exe", started=NOW - 8000),
    Proc(3, 2, r"C:\Program Files\Git\usr\bin\bash.exe", "bash -c 'npm run dev &'", name="bash.exe", started=NOW - 7000),
]


def dev_tree(root: str, first: int, parent: int) -> list[Proc]:
    """`npm run dev` and `npm run tauri dev` for `root`, as Windows shows them: npm, its cmd shell, vite, esbuild,
    cargo through the rustup proxy, rustc, the debug app and its WebView2."""
    t = NOW - 6000
    p = first
    return [
        Proc(p, parent, NODE, f'"{NODE}" "C:\\Program Files\\nodejs/node_modules/npm/bin/npm-cli.js" run dev', name="node.exe", started=t),
        Proc(p + 1, p, r"C:\Windows\System32\cmd.exe", 'C:\\WINDOWS\\system32\\cmd.exe /d /s /c "vite"', name="cmd.exe", started=t + 1),
        Proc(p + 2, p + 1, NODE, f'"node"   "{root}\\node_modules\\.bin\\\\..\\vite\\bin\\vite.js"', name="node.exe", started=t + 2),
        Proc(p + 3, p + 2, root + r"\node_modules\@esbuild\win32-x64\esbuild.exe", "esbuild --service=0.25.0 --ping", started=t + 3),
        Proc(p + 4, parent, r"C:\Users\maya\.cargo\bin\cargo.exe", "cargo  run --no-default-features", name="cargo.exe", started=t + 4),
        Proc(p + 5, p + 4, CARGO, f'"{CARGO}" run --no-default-features', name="cargo.exe", started=t + 5),
        Proc(
            p + 6,
            p + 5,
            RUSTC,
            f'"{RUSTC}" --crate-name brainstead_lib src\\lib.rs --out-dir {root}\\src-tauri\\target\\debug\\deps',
            name="rustc.exe",
            started=t + 6,
        ),
        Proc(p + 7, p + 5, root + r"\src-tauri\target\debug\brainstead.exe", name="Brainstead.exe", started=t + 7),
        Proc(
            p + 8,
            p + 7,
            r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application\msedgewebview2.exe",
            "msedgewebview2.exe --embedded-browser-webview=1 --user-data-dir=C:\\Users\\maya\\AppData\\Local\\com.orbit\\EBWebView",
            name="msedgewebview2.exe",
            started=t + 8,
        ),
        Proc(
            p + 9,
            p + 8,
            r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application\msedgewebview2.exe",
            "msedgewebview2.exe --type=renderer",
            name="msedgewebview2.exe",
            started=t + 9,
        ),
    ]


# The installed app, an MCP server it serves and a window the server opened: never matched.
INSTALLED_APP = [
    Proc(500, 1, INSTALLED, INSTALLED, name="Brainstead.exe", started=NOW - 50000),
    Proc(501, 2, INSTALLED, INSTALLED + " --mcp", name="Brainstead.exe", started=NOW - 5000),
    Proc(502, 501, INSTALLED, f'"{INSTALLED}"', name="Brainstead.exe", started=NOW - 4000),
    Proc(
        503, 500, r"C:\EdgeWebView\msedgewebview2.exe", "msedgewebview2.exe --type=renderer", name="msedgewebview2.exe", started=NOW - 40000
    ),
]


def stub(pid: int, ppid: int, **kw) -> Proc:
    fields = {"cmd": f'{STUB} -I -S -c "import sys"', "name": "python3.exe", "cpu": 0.0, "started": NOW - 3600} | kw
    return Proc(pid, ppid, **fields)


def scope(**kw) -> Scope:
    fields = {"current": norm(HERE, True), "windows": True, "self_pid": 999, "now": NOW} | kw
    return Scope(**fields)


def everything(**kw) -> Scope:
    return scope(roots=frozenset({norm(OTHER, True)}), worktrees_dir=norm(WT, True), sweep=True, **kw)


def session(**kw) -> Scope:
    """Session 2's view, as make_scope builds it: the repository's checkouts are roots, and only its processes count."""
    fields = {"roots": frozenset({norm(OTHER, True), norm(REPO, True)}), "worktrees_dir": norm(WT, True)} | kw
    return scope(owners=frozenset({2}), **fields)


def pids(matches) -> set[int]:
    return {m.pid for m in matches}


class ThisWorktree(unittest.TestCase):
    def test_every_dev_process_and_its_wrappers(self):
        procs = SESSION + dev_tree(HERE, 10, 3) + INSTALLED_APP
        matches, left = find(procs, scope())
        self.assertEqual(pids(matches), set(range(10, 20)))
        self.assertEqual(left, {})
        why = {m.pid: m.why for m in matches}
        self.assertEqual(why[12], "vite dev server")
        self.assertEqual(why[17], "app under src-tauri/target")
        self.assertEqual(why[16], "rustc building in the worktree")
        self.assertEqual(why[10], "started cmd 11")
        self.assertEqual(why[19], "child of msedgewebview2 18")

    def test_children_before_parents(self):
        matches, _ = find(SESSION + dev_tree(HERE, 10, 3), scope())
        order = [m.pid for m in matches]
        for child, parent in [(19, 18), (18, 17), (17, 15), (16, 15), (15, 14), (13, 12), (12, 11), (11, 10)]:
            self.assertLess(order.index(child), order.index(parent))

    def test_never_the_session_its_shell_the_installed_app_or_mcp_servers(self):
        matches, _ = find(SESSION + dev_tree(HERE, 10, 3) + INSTALLED_APP, everything(include_live=True))
        self.assertFalse(pids(matches) & {1, 2, 3, 500, 501, 502, 503})

    def test_a_dev_build_serving_mcp_is_left(self):
        app = HERE + r"\src-tauri\target\debug\brainstead.exe"
        matches, _ = find(SESSION + [Proc(20, 2, app, f"{app} --mcp", name="brainstead.exe")], scope())
        self.assertEqual(matches, [])

    def test_repro_copies(self):
        copies = [Proc(30 + i, 3, HERE + rf"\src-tauri\target\repro\{i}\Brainstead.exe", name="Brainstead.exe") for i in range(3)]
        matches, _ = find(SESSION + copies, scope())
        self.assertEqual(pids(matches), {30, 31, 32})
        self.assertTrue(all(m.why == "repro copy under src-tauri/target/repro" for m in matches))

    def test_npx_vite_and_cargo_clippy(self):
        clippy = r"C:\Users\maya\.rustup\toolchains\stable\bin\clippy-driver.exe"
        procs = SESSION + [
            Proc(40, 3, NODE, f'"{NODE}" "C:\\Program Files\\nodejs/node_modules/npm/bin/npx-cli.js" vite --strictPort', name="node.exe"),
            Proc(41, 40, r"C:\Windows\System32\cmd.exe", "cmd.exe /d /s /c vite --strictPort", name="cmd.exe"),
            Proc(42, 41, NODE, f'"node" "{HERE}\\node_modules\\.bin\\..\\vite\\bin\\vite.js" --strictPort', name="node.exe"),
            Proc(43, 3, r"C:\Users\maya\.cargo\bin\cargo-clippy.exe", "cargo-clippy clippy -- -D warnings", name="cargo-clippy.exe"),
            Proc(44, 43, CARGO, f'"{CARGO}" check --workspace', name="cargo.exe"),
            Proc(45, 44, clippy, f'"{clippy}" {RUSTC} --crate-name core -L dependency={HERE}\\src-tauri\\target\\check\\debug\\deps'),
        ]
        matches, _ = find(procs, scope())
        self.assertEqual(pids(matches), {40, 41, 42, 43, 44, 45})

    def test_webview_data_under_the_worktree_without_its_app(self):
        procs = [Proc(50, 49, r"C:\EdgeWebView\msedgewebview2.exe", f'msedgewebview2.exe --user-data-dir="{HERE}\\.scene\\EBWebView"')]
        self.assertEqual(pids(find(procs, scope())[0]), {50})

    def test_unrelated_children_of_the_app_are_left(self):
        app = HERE + r"\src-tauri\target\debug\brainstead.exe"
        procs = [Proc(60, 3, app, name="brainstead.exe"), Proc(61, 60, r"C:\Windows\notepad.exe", name="notepad.exe")]
        self.assertEqual(pids(find(procs, scope())[0]), {60})

    def test_paths_in_any_case_and_slash(self):
        procs = [
            Proc(70, 3, NODE, 'node "c:/work/orbit/.claude/worktrees/brave-lena-1a2b3c/node_modules/vite/bin/vite.js"', name="node.exe"),
            Proc(71, 3, NODE, "node /c/Work/Orbit/.claude/worktrees/brave-lena-1a2b3c/node_modules/.bin/vite", name="node.exe"),
        ]
        self.assertEqual(pids(find(procs, scope())[0]), {70, 71})

    def test_not_vitest_eslint_or_another_project(self):
        procs = [
            Proc(80, 3, NODE, f'node "{HERE}\\node_modules\\vitest\\vitest.mjs" run', name="node.exe"),
            Proc(81, 3, NODE, f'node "{HERE}\\node_modules\\eslint\\bin\\eslint.js" .', name="node.exe"),
            Proc(82, 3, NODE, r'node "C:\Work\elsewhere\node_modules\vite\bin\vite.js"', name="node.exe"),
            Proc(83, 3, RUSTC, r"rustc --out-dir C:\Work\elsewhere\target\debug\deps", name="rustc.exe"),
        ]
        self.assertEqual(find(procs, scope())[0], [])

    def test_own_process_and_its_parents_are_left(self):
        procs = SESSION + [
            Proc(90, 2, r"C:\Windows\System32\cmd.exe", "cmd.exe /c npm-cli.js", name="cmd.exe"),
            Proc(999, 90, NODE, f'node "{HERE}\\node_modules\\vite\\bin\\vite.js"', name="node.exe"),
        ]
        self.assertEqual(find(procs, scope())[0], [])

    def test_main_checkout_does_not_claim_its_worktrees(self):
        matches, _ = find(SESSION + dev_tree(HERE, 10, 3), scope(current=norm(REPO, True)))
        self.assertEqual(matches, [])

    def test_cargo_by_folder(self):
        # read_posix fills the folder from /proc on Linux and lsof on macOS, read_win32 from the PEB (Folders below).
        root = "/Users/maya/orbit/.claude/worktrees/brave-lena-1a2b3c"
        procs = [
            Proc(100, 1, "/Users/maya/.cargo/bin/cargo", "cargo test", cwd=root + "/src-tauri", name="cargo"),
            Proc(101, 100, root + "/src-tauri/target/debug/deps/brainstead_core-1f2e", "brainstead_core-1f2e", name="brainstead_core-1f2e"),
            Proc(102, 1, root + "/src-tauri/target/debug/brainstead", root + "/src-tauri/target/debug/brainstead", name="brainstead"),
            Proc(
                103, 1, "/Applications/Brainstead.app/Contents/MacOS/brainstead", "/Applications/Brainstead.app/Contents/MacOS/brainstead"
            ),
            Proc(104, 1, "/usr/bin/zsh", "-zsh", cwd=root, name="zsh"),
        ]
        matches, _ = find(procs, Scope(current=root, windows=False, now=NOW))
        self.assertEqual(pids(matches), {100, 101, 102})


class AllWorktrees(unittest.TestCase):
    def test_another_worktree_is_left_without_all(self):
        self.assertEqual(find(SESSION + dev_tree(OTHER, 10, 3), scope())[0], [])

    def test_live_by_transcript(self):
        procs = dev_tree(OTHER, 10, 400)  # its shell is gone, but its transcript is recent
        matches, left = find(procs, everything(active=frozenset({norm(OTHER, True)})))
        self.assertEqual(matches, [])
        self.assertEqual(left, {"calm-maya-4d5e6f": 10})

    def test_live_by_running_session(self):
        matches, left = find(SESSION + dev_tree(OTHER, 10, 3), everything())
        self.assertEqual(matches, [])
        self.assertEqual(left, {"calm-maya-4d5e6f": 10})

    def test_live_by_a_claude_code_session_run_by_node(self):
        node = [
            Proc(2, 1, NODE, f'"{NODE}" "C:\\Users\\maya\\AppData\\Roaming\\npm\\node_modules\\@anthropic-ai\\claude-code\\cli.js"'),
            SESSION[2],
        ]
        matches, left = find(node + dev_tree(OTHER, 10, 3), everything())
        self.assertEqual(matches, [])
        self.assertEqual(left, {"calm-maya-4d5e6f": 10})
        # Never matched itself, even with the dev tree below it.
        self.assertFalse(pids(find(node + dev_tree(OTHER, 10, 3), everything(include_live=True))[0]) & {2, 3})

    def test_include_live(self):
        matches, left = find(SESSION + dev_tree(OTHER, 10, 3), everything(include_live=True))
        self.assertEqual(pids(matches), set(range(10, 20)))
        self.assertEqual({m.worktree for m in matches}, {"calm-maya-4d5e6f (live)"})
        self.assertEqual(left, {})

    def test_ended_session(self):
        shell = Proc(3, 400, r"C:\Program Files\Git\usr\bin\bash.exe", "bash", name="bash.exe", started=NOW - 7000)
        matches, _ = find([shell] + dev_tree(OTHER, 10, 3), everything())
        self.assertEqual(pids(matches), set(range(10, 20)))
        self.assertEqual({m.worktree for m in matches}, {"calm-maya-4d5e6f (ended)"})

    def test_reused_parent_pid_is_not_a_running_session(self):
        # pid 2 now belongs to a Claude that started after the dev tree did.
        late = [Proc(2, 1, r"C:\claude.exe", name="claude.exe", started=NOW - 10), SESSION[2]]
        matches, _ = find(late + dev_tree(OTHER, 10, 3), everything())
        self.assertEqual(pids(matches), set(range(10, 20)))

    def test_gone_worktree(self):
        matches, _ = find(SESSION + dev_tree(GONE, 10, 3), everything())
        self.assertEqual(pids(matches), set(range(10, 20)))
        self.assertEqual({m.worktree for m in matches}, {"old-acme-7a8b9c (gone)"})
        self.assertEqual(find(SESSION + dev_tree(GONE, 10, 3), scope())[0], [])

    def test_this_worktree_even_while_live(self):
        matches, _ = find(SESSION + dev_tree(HERE, 10, 3), everything(active=frozenset({norm(HERE, True)})))
        self.assertEqual(pids(matches), set(range(10, 20)))


class Stubs(unittest.TestCase):
    def test_hung_stub(self):
        manager = r"C:\Program Files\WindowsApps\PythonSoftwareFoundation.PythonManager_26.4.240.0_x64__abc\python3.exe"
        matches, _ = find([stub(600, 4242), stub(608, 4242, exe=manager)], scope(stubs=True))
        self.assertEqual([(m.pid, m.worktree) for m in matches], [(600, "-"), (608, "-")])

    def test_only_with_store_stubs(self):
        # They belong to no worktree, so --all, which never reaches outside the repository, leaves them.
        self.assertEqual(find([stub(600, 4242)], scope())[0], [])
        self.assertEqual(find([stub(600, 4242)], everything(include_live=True))[0], [])

    def test_not_a_stub(self):
        procs = SESSION + [
            stub(601, 2),  # its parent is running
            stub(602, 4242, cpu=0.02),  # it ran
            stub(603, 4242, started=NOW - 30),  # may still be starting
            stub(604, 4242),  # it started the real interpreter
            Proc(605, 604, r"C:\Python\python.exe", "python -m http.server", name="python.exe", started=NOW - 3000),
            stub(606, 4242, cmd=r"C:\Python\python3.exe -I -S -c pass"),  # a real install
            stub(607, 4242, name="pythonw.exe", cmd=r"C:\Users\maya\AppData\Local\Microsoft\WindowsApps\pythonw.exe"),
        ]
        self.assertEqual(find(procs, scope(stubs=True))[0], [])


# A developer's terminal and a second Claude session (5) in the same checkout.
TERMINAL = [
    Proc(300, 0, r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe", name="WindowsTerminal.exe"),
    Proc(301, 300, r"C:\Program Files\PowerShell\7\pwsh.exe", "pwsh", name="pwsh.exe", started=NOW - 20000),
]
SECOND = [
    Proc(5, 1, r"C:\Users\maya\claude-code\claude.exe", "claude.exe", name="claude.exe", started=NOW - 8000),
    Proc(6, 5, r"C:\Program Files\Git\usr\bin\bash.exe", "bash -c 'npm run tauri dev'", name="bash.exe", started=NOW - 7000),
]


def make_check(root: str, first: int, parent: int) -> list[Proc]:
    """`make check` (tools/check.py) from a terminal: Python, its shell, cargo test and the rustc it runs."""
    t = NOW - 5000
    out = f"{root}\\src-tauri\\target\\check\\debug\\deps"
    cargo = f'"{CARGO}" test -p brainstead-core'
    return [
        Proc(first, parent, r"C:\Python\python.exe", "python tools/check.py", name="python.exe", started=t),
        Proc(first + 1, first, r"C:\Windows\System32\cmd.exe", "cmd /c cargo test", name="cmd.exe", started=t + 1),
        Proc(first + 2, first + 1, CARGO, cargo, cwd=root + r"\src-tauri", name="cargo.exe", started=t + 2),
        Proc(first + 3, first + 2, RUSTC, f'"{RUSTC}" --crate-name core --out-dir {out}', name="rustc.exe", started=t + 3),
    ]


class Session(unittest.TestCase):
    def test_only_what_the_session_started(self):
        mine = dev_tree(HERE, 10, 3)
        theirs = dev_tree(HERE, 30, 301) + make_check(HERE, 40, 301) + dev_tree(HERE, 50, 6)
        procs = SESSION + TERMINAL + SECOND + mine + theirs + INSTALLED_APP
        matches, left = find(procs, session())
        self.assertEqual(pids(matches), set(range(10, 20)))
        self.assertEqual(left, {})
        # --anyone: every dev process of the checkout, the check's cargo and rustc too.
        self.assertLessEqual(set(range(30, 40)) | {42, 43} | set(range(50, 60)), pids(find(procs, scope())[0]))

    def test_a_developers_dev_run_and_check_are_never_the_sessions(self):
        procs = SESSION + TERMINAL + dev_tree(HERE, 30, 301) + make_check(HERE, 40, 301)
        self.assertEqual(find(procs, session()), ([], {}))

    def test_no_session_matches_nothing(self):
        procs = SESSION + dev_tree(HERE, 10, 3)
        self.assertEqual(find(procs, scope(owners=frozenset()))[0], [])

    def test_what_outlived_its_shell_is_no_longer_the_sessions(self):
        # The shell (3) ended and npm and cargo ran on: nothing ties them to the session's Claude Code process.
        self.assertEqual(find(SESSION[:2] + dev_tree(HERE, 10, 404), session())[0], [])

    def test_its_processes_in_the_repositorys_other_checkouts(self):
        procs = SESSION + TERMINAL + dev_tree(OTHER, 10, 3) + dev_tree(REPO, 30, 3) + dev_tree(OTHER, 50, 301)
        matches, _ = find(procs, session())
        self.assertEqual(pids(matches), set(range(10, 20)) | set(range(30, 40)))
        self.assertEqual({m.worktree for m in matches}, {"calm-maya-4d5e6f", "orbit"})

    def test_all_still_sweeps_ended_worktrees_but_never_the_main_checkout(self):
        shell = Proc(7, 400, r"C:\Program Files\Git\usr\bin\bash.exe", "bash", name="bash.exe", started=NOW - 7000)
        procs = SESSION + TERMINAL + [shell] + dev_tree(OTHER, 10, 7) + dev_tree(REPO, 30, 7) + dev_tree(HERE, 50, 301)
        self.assertEqual(pids(find(procs, session(sweep=True))[0]), set(range(10, 20)))


class Owners(unittest.TestCase):
    SHELL = Proc(8, 2, r"C:\Program Files\Git\usr\bin\bash.exe", "bash", name="bash.exe", started=NOW - 100)
    ME = Proc(9, 8, r"C:\Python\python.exe", "python tools/dev_stop.py", name="python.exe", started=NOW - 99)

    def test_the_session_is_the_claude_code_process_above(self):
        procs = SESSION + [self.SHELL, self.ME]
        self.assertEqual(session_of(procs, 9, None, True).pid, 2)
        self.assertEqual(session_of(procs, 9, 2, True).pid, 2)
        # A CLAUDE_PID that isn't above this process is ignored.
        self.assertEqual(session_of(procs + SECOND, 9, 5, True).pid, 2)
        # The desktop app's own terminal: no session.
        terminal = Proc(8, 1, r"C:\Windows\System32\cmd.exe", name="cmd.exe", started=NOW - 100)
        self.assertIsNone(session_of(SESSION + [terminal, self.ME], 9, None, True))
        mac = [
            Proc(1, 0, "/Applications/Claude.app/Contents/MacOS/Claude", name="Claude", started=NOW - 9000),
            Proc(2, 1, "/usr/local/bin/node", "node /usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js", started=NOW - 8000),
            Proc(8, 2, "/bin/zsh", "zsh -c ...", name="zsh", started=NOW - 100),
            Proc(9, 8, "/usr/bin/python3", "python3 tools/dev_stop.py", name="python3", started=NOW - 99),
        ]
        self.assertEqual(session_of(mac, 9, None, False).pid, 2)

    def test_whose_processes_count(self):
        self.assertEqual(owners_of(SESSION[1], False), frozenset({2}))
        self.assertEqual(owners_of(None, False), frozenset())
        self.assertIsNone(owners_of(SESSION[1], True))
        self.assertIsNone(owners_of(None, True))


class Main(unittest.TestCase):
    """main() over a fake process table for a checkout in a temp folder: the session's run, a developer's own
    `npm run tauri dev` and `make check` from a terminal, and this script (9)."""

    def setUp(self):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        repo = pathlib.Path(d.name).resolve() / "orbit"
        (repo / ".claude" / "worktrees").mkdir(parents=True)
        self.repo = str(repo)
        root = self.repo
        tauri_dev = [
            Proc(60, 301, NODE, f'"{NODE}" "C:\\Program Files\\nodejs/node_modules/npm/bin/npm-cli.js" run tauri dev', name="node.exe"),
            Proc(61, 60, NODE, f'node "{root}\\node_modules\\@tauri-apps\\cli\\tauri.js" dev', name="node.exe"),
        ]
        self.foreign = tauri_dev + dev_tree(root, 30, 61) + make_check(root, 40, 301)
        self.mine = dev_tree(root, 10, 3)

    def run_main(self, procs, *argv, read_error=None, stop_fails=False):
        """main() with `argv`: the process table is `procs`, or reading it raises `read_error`; `stop_fails` makes
        stopping each process fail. Returns the exit code, what was stopped, stdout and stderr."""
        stopped = []

        def read_procs(timeout):
            if read_error:
                raise read_error
            return procs

        def stop(matches):
            stopped.extend(m.pid for m in matches)
            return [(m, "Access is denied.") for m in matches] if stop_fails else []

        out, err = io.StringIO(), io.StringIO()
        with contextlib.ExitStack() as stack:
            stack.enter_context(unittest.mock.patch.object(dev_stop, "read_procs", read_procs))
            stack.enter_context(unittest.mock.patch.object(dev_stop, "stop", stop))
            stack.enter_context(unittest.mock.patch("os.getpid", return_value=9))
            stack.enter_context(unittest.mock.patch.dict(os.environ, {"CLAUDE_PID": ""}))
            stack.enter_context(contextlib.redirect_stdout(out))
            stack.enter_context(contextlib.redirect_stderr(err))
            code = dev_stop.main(["--worktree", self.repo, *argv])
        return code, set(stopped), out.getvalue(), err.getvalue()

    def test_a_session_stops_only_its_own_never_a_developers_tauri_dev(self):
        me = [Proc(8, 2, "bash.exe", "bash", name="bash.exe"), Proc(9, 8, "python.exe", "python tools/dev_stop.py", name="python.exe")]
        procs = SESSION + me + TERMINAL + self.mine + self.foreign
        code, stopped, _, _ = self.run_main(procs, "--kill")
        self.assertEqual((code, stopped), (0, set(range(10, 20))))

    def test_no_session_stops_nothing_and_says_why(self):
        me = [Proc(9, 301, "python.exe", "python tools/dev_stop.py", name="python.exe")]
        procs = TERMINAL + me + self.foreign
        code, stopped, out, _ = self.run_main(procs, "--kill")
        self.assertEqual((code, stopped), (0, set()))
        self.assertIn("No Claude Code process is above this script", out)
        # --anyone is the explicit way to stop every dev process of the checkout.
        code, stopped, _, _ = self.run_main(procs, "--anyone", "--kill")
        self.assertLessEqual(set(range(30, 40)) | {42, 43, 60, 61}, stopped)

    def test_a_process_table_that_cant_be_read_fails_the_run(self):
        for error in (OSError("CreateToolhelp32Snapshot failed"), subprocess.TimeoutExpired("powershell", 60)):
            code, stopped, _, err = self.run_main([], "--kill", read_error=error)
            self.assertEqual((code, stopped), (1, set()), error)
            self.assertIn("dev_stop: couldn't read the processes", err)

    def test_a_process_that_wont_stop_fails_the_run_and_is_named(self):
        me = [Proc(8, 2, "bash.exe", "bash", name="bash.exe"), Proc(9, 8, "python.exe", "python tools/dev_stop.py", name="python.exe")]
        procs = SESSION + me + self.mine
        code, stopped, out, _ = self.run_main(procs, "--kill", stop_fails=True)
        self.assertEqual((code, stopped), (1, set(range(10, 20))))
        self.assertIn("Stopped 0 of 10.", out)
        self.assertIn("Access is denied.", out)
        # Listing alone stops nothing, so nothing can fail.
        self.assertEqual(self.run_main(procs, stop_fails=True)[:2], (0, set()))


class Folders(unittest.TestCase):
    def test_lsof_output(self):
        out = "p100\nfcwd\nn/Users/maya/orbit/src-tauri\np101\nfcwd\nn/tmp\n"
        self.assertEqual(parse_lsof(out), {100: "/Users/maya/orbit/src-tauri", 101: "/tmp"})

    @unittest.skipUnless(sys.platform == "win32", "Windows reads the folder from the PEB")
    def test_windows_reads_a_cargo_folder(self):
        # Only cargo and the like have their folder read: here Python counts as one, started in a folder of its own.
        name = pathlib.Path(sys.executable).stem.lower()
        with tempfile.TemporaryDirectory() as d, unittest.mock.patch.object(dev_stop, "RUST", dev_stop.RUST | {name}):
            p = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"], cwd=d)
            try:
                time.sleep(0.5)
                got = next(q for q in dev_stop.read_win32() if q.pid == p.pid)
                self.assertTrue(os.path.samefile(got.cwd, d), got.cwd)
            finally:
                p.kill()
                p.wait()


if __name__ == "__main__":
    unittest.main()
