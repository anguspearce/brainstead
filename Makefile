# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

# Brainstead — build, sign and install the app.
#
# This Makefile is for macOS. On Windows run `python tools/check.py` for the checks and
# `npm run tauri build` for the installer (`python tools/install_windows.py` builds and installs it).

APP      := src-tauri/target/release/bundle/macos/Brainstead.app

# The signing certificate is named in signing.local, which is untracked: the name of a keychain
# identity is local to the machine that holds it. Copy signing.local.example and put your own
# self-signed certificate's name in it. Without one the build is signed ad hoc, and macOS forgets
# Brainstead's permissions (Full Disk Access, Keychain items) after every rebuild. Brainstead has
# its own certificate, "Brainstead Dev": never a sibling app's (the archived build plan, §2).
-include signing.local
SIGN_ID  := $(APPLE_SIGNING_IDENTITY)
# "-" is an ad-hoc signature: an empty identity makes the bundler fail instead.
export APPLE_SIGNING_IDENTITY := $(if $(SIGN_ID),$(SIGN_ID),-)

.PHONY: check test lint fmt app install-app dmg release dev icons sign-check screenshots evals help

## cargo test and Clippy, the TypeScript type-check and ESLint, Vitest, and the formatting and repo checks: side by side (tools/check.py), cargo in its own target folder so make dev doesn't hold it up.
check:
	@python3 tools/check.py

## Formatting (rustfmt, Prettier, ruff), Clippy, ESLint, stylelint, ruff, the licence headers and every button's tooltip, all checked, nothing changed.
lint:
	cd src-tauri && cargo fmt --all --check && CARGO_TARGET_DIR=target/check cargo clippy --workspace --all-targets -- -D warnings
	npx prettier --check .
	npx eslint .
	npx stylelint "src/**/*.css"
	ruff check && ruff format --check
	python3 tools/license_headers.py --check
	python3 tools/tooltips.py

## Reformat everything, and add the licence header to any source file without one.
fmt:
	cd src-tauri && cargo fmt --all
	npx prettier --write .
	npx stylelint "src/**/*.css" --fix
	ruff check --fix && ruff format
	python3 tools/license_headers.py

## The workflow evals (ingest, meeting notes, contradictions, wiki questions, help questions, preparing the weekly review, fix name) with real models, on a copy of the fixture vault: target/evals/report.md. Not part of check.
evals:
	cd src-tauri && cargo run -q -p brainstead-evals -- $(EVALS)

## Same as check.
test: check

# Release builds strip the builder's home directory out of the binary (Rust bakes absolute
# paths into panic metadata). Debug builds skip this so `make dev` keeps its incremental cache.
RELEASE_RUSTFLAGS := --remap-path-prefix=$(HOME)=/build --remap-path-scope=object
# macOS 27's linker (ld-27037) sometimes writes a library whose string table dyld refuses
# ("mis-aligned LINKEDIT string pool"), so rustc can't load a proc macro it has just built and
# stops with "can't find crate" (E0463). Release builds link with Rust's own lld instead; lld
# can't read the macOS 27 SDK's .tbd files, so it links against the 26.x SDK when that's there.
LLD_DIR := $(shell rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/gcc-ld
OLD_SDK := $(wildcard /Library/Developer/CommandLineTools/SDKs/MacOSX26*.sdk)
ifneq ($(OLD_SDK),)
RELEASE_RUSTFLAGS += -Clink-arg=-fuse-ld=lld -Clink-arg=-B$(LLD_DIR)
RELEASE_ENV := SDKROOT=$(lastword $(OLD_SDK))
endif

# Each release build bumps the version (tools/bump_version.py: 0.1.0 → 0.1.1) and gets its own
# build number, the same on the app (CFBundleVersion) and the binary (Settings › About shows both).
BUILD := $(shell date +%Y%m%d.%H%M%S)

## Bump the version (or set it: make app VERSION=1.1.0) and build the .app, signed with the identity in signing.local when there is one.
app:
	@echo "version $$(python3 tools/bump_version.py $(VERSION)), build $(BUILD)"
	BRAINSTEAD_BUILD=$(BUILD) $(RELEASE_ENV) RUSTFLAGS="$(RELEASE_RUSTFLAGS)" npm run tauri build -- --config '{"bundle":{"macOS":{"bundleVersion":"$(BUILD)"}}}'
	@if [ -n "$(SIGN_ID)" ]; then \
	  codesign -dv --verbose=2 "$(APP)" 2>&1 | grep -E "^Authority=$(SIGN_ID)" >/dev/null \
	    && echo "signed with $(SIGN_ID)" \
	    || { echo "WARNING: app is not signed with $(SIGN_ID)"; exit 1; }; \
	else echo "note: no signing.local, so the app is signed ad hoc"; fi

# The app's own processes: not the MCP servers (--mcp) or Chrome's capture host that run the same
# program, which keep running and pick up the new build themselves.
APP_PIDS = ps -axo pid=,args= | awk '$$2 ~ /\/Brainstead$$/ && !/ --mcp|chrome-extension:/ { print $$1 }'

## Build and replace /Applications/Brainstead.app, keeping the version: only a release build (make app) bumps it. Quits the app first, as ⌘Q does, but not the MCP servers Claude sessions are using: they start the new build when it's in place.
install-app: VERSION = $(shell python3 -c "import json; print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
install-app: app
	@if [ -n "$$($(APP_PIDS))" ]; then \
	  osascript -e 'quit app id "com.wayned.brainstead"' >/dev/null 2>&1 || true; \
	  for _ in 1 2 3 4 5 6 7 8 9 10; do [ -n "$$($(APP_PIDS))" ] || break; sleep 0.5; done; \
	  pids=$$($(APP_PIDS)); [ -z "$$pids" ] || kill $$pids; \
	fi
	@rm -rf "/Applications/Brainstead.app"
	@ditto "$(APP)" "/Applications/Brainstead.app"
	@echo "installed /Applications/Brainstead.app"

## Pack the built app into the release DMG (src-tauri/target/release/bundle/dmg/), laid out like other Mac installers, and write the updater's files beside it (tools/update_files.py): Brainstead.app.tar.gz, its signature and latest.json, with NOTES="…" as what's new.
dmg:
	@python3 tools/dmg/make_dmg.py
	@python3 tools/update_files.py --notes "$$NOTES"

## Publish a release: make release NOTES="What's new" bumps the version, builds the app and the DMG, commits the bump, pushes, creates the GitHub release and installs the app (not with INSTALL=0). DRY_RUN=1 prints the steps (tools/release.py).
release:
	@python3 tools/release.py

## Redraw design/icon.png, then regenerate the Tauri icon set.
icons:
	@python3 tools/make_icons.py
	@npx tauri icon design/icon.png >/dev/null
	@rm -rf src-tauri/icons/android src-tauri/icons/ios
	@echo "regenerated src-tauri/icons"

## Retake docs/images/*-light.png and *-dark.png from tools/screenshots/scenes.json.
screenshots:
	@python3 tools/screenshots.py

## Show who signed /Applications/Brainstead.app, and fail unless it's the identity in signing.local.
sign-check:
	@codesign -dv --verbose=2 "/Applications/Brainstead.app" 2>&1 | grep -E "^(Identifier|Authority|Signature|TeamIdentifier)"
	@if [ -n "$(SIGN_ID)" ]; then \
	  codesign -dv --verbose=2 "/Applications/Brainstead.app" 2>&1 | grep -qE "^Authority=$(SIGN_ID)$$" \
	    && echo "signed with $(SIGN_ID)" || { echo "not signed with $(SIGN_ID)"; exit 1; }; \
	else echo "no signing.local: copy signing.local.example and create the Brainstead Dev certificate"; exit 1; fi

## Run the app in development, with hot reload (Vite on localhost:1440).
dev:
	npm run tauri dev

## List these targets.
help:
	@awk '/^## /{d=substr($$0,4);next} /^[a-z-]+:/{if(d){sub(/:.*/,"",$$1);printf "  make %-12s %s\n",$$1,d};d=""}' $(MAKEFILE_LIST)
