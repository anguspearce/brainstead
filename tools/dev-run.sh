#!/bin/sh
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

# Cargo's runner on macOS (src-tauri/.cargo/config.toml): runs what cargo built. The app's own debug
# binary is signed first with the certificate signing.local names, so the Keychain knows a dev
# build again after each rebuild and stops asking for the sign-ins it keeps (it also runs as the
# extensions' helper under `make dev HOST=1`). Anything else (tests), or no signing.local, runs as is.
bin="$1"
case "$(basename "$bin")" in
Brainstead | brainstead)
  # DEV_SIGNING_IDENTITY first: an Apple Development certificate has a Team ID, and the Keychain
  # knows a build by its team. A self-signed one has none, so the Keychain goes by the exact build
  # and asks again after each rebuild, once for every sign-in it keeps.
  local_file="$(dirname "$0")/../signing.local"
  id=$(sed -n 's/^DEV_SIGNING_IDENTITY *= *//p' "$local_file" 2>/dev/null)
  [ -n "$id" ] || id=$(sed -n 's/^APPLE_SIGNING_IDENTITY *= *//p' "$local_file" 2>/dev/null)
  if [ -n "$id" ]; then
    codesign --force --sign "$id" "$bin" 2>/dev/null || echo "dev-run: couldn't sign with \"$id\"; the Keychain may ask again" >&2
  fi
  ;;
esac
exec "$@"
