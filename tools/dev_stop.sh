#!/bin/sh
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The dev_stop hook in .claude/settings.json, which passes its arguments on to tools/dev_stop.py: `--session-end` stops
# the dev processes the Claude Code session started when it ends. Runs the first python3 or python on PATH that isn't a
# Windows Store alias (a WindowsApps folder): an alias takes seconds to start and can be left hung when a hook times out.
# Never runs /usr/bin/python3 on a Mac without the Command Line Tools, which would open their install dialog. Exits 0
# when no interpreter is found, so it can never hold a session up.
DIR=$(cd "$(dirname "$0")" 2>/dev/null && pwd) || exit 0
NOCLT=
if [ "$(uname 2>/dev/null)" = "Darwin" ] && ! xcode-select -p >/dev/null 2>&1; then NOCLT=1; fi
OLDIFS=$IFS
IFS=:
for d in $PATH; do
  case "$d" in *[Ww]indows[Aa]pps*) continue ;; esac
  for f in "$d/python3" "$d/python3.exe" "$d/python" "$d/python.exe"; do
    [ -f "$f" ] && [ -x "$f" ] || continue
    [ -n "$NOCLT" ] && [ "$f" = /usr/bin/python3 ] && continue
    IFS=$OLDIFS
    exec "$f" "$DIR/dev_stop.py" "$@"
  done
done
IFS=$OLDIFS
# Only Store aliases on PATH: one of them is better than leaving everything running.
[ -n "$NOCLT" ] && exit 0
PY=$(command -v python3 2>/dev/null || command -v python 2>/dev/null) || exit 0
exec "$PY" "$DIR/dev_stop.py" "$@"
