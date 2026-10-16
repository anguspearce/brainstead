# Development

Tauri 2: a React 19 and TypeScript frontend (Vite), a Rust backend.

| Folder | Holds |
|---|---|
| `src-tauri/core/` (`brainstead-core`) | Vault parsing, SQLite index and search, tasks, safe writes, agent changes, ingest, reviews, Knowledge health. No Tauri |
| `src-tauri/mcp/` (`brainstead-mcp`) | The MCP server |
| `src-tauri/src/` | The app: commands, watcher, jobs, menu bar, MCP actions bridge; OS-specific code in `platform/` |
| `src-tauri/evals/` | Workflows run against real models |
| `src/` | The frontend |

The finished build plan is untracked in `archive/`. Decisions: `_sift/decisions.jsonl`, read with `python3 _sift/bin/sift.py decisions`.

## Commands

The commands below are for macOS. Windows has no `make`: use `python tools/check.py` for the check (`python tools/check.py rust` runs one group), `npm run tauri dev` to run with hot reload, `npm run tauri build` to build the NSIS installer in `src-tauri/target/release/bundle/nsis/` (`Brainstead_<version>_x64-setup.exe`) and `python tools/install_windows.py` to build it, quit Brainstead, install it silently for your user and open it (`make install-app`'s counterpart). The `app`, `install-app`, `dmg`, `icons`, `sign-check` and `screenshots` targets are macOS only.

On Windows you need rustup (stable-msvc), Visual Studio Build Tools with the "Desktop development with C++" workload, Node.js 22 or later, Python 3 (the git hooks call `python3`, so it must be on your PATH under that name) and ruff. Run `npm ci` once. CI runs the full check on macOS and Windows and compiles on Linux.

| Command | What it does |
|---|---|
| `make dev` | Run with hot reload (Vite on localhost:1440) |
| `make help` | List the targets |
| `make check` | In parallel: Rust tests and Clippy (core, MCP, app), TypeScript check, ESLint, Vitest, formatting and repo checks (`tools/check.py`; `python3 tools/check.py rust` runs one group). Builds into `src-tauri/target/check`, so `make dev` doesn't block it. ~15 s warm, minutes cold |
| `make test` | Same as `make check` |
| `make lint` | Checks only: rustfmt, Clippy (warnings are errors), Prettier, ESLint, stylelint, ruff, licence headers, button tooltips |
| `make fmt` | Reformat everything; add missing licence headers |
| `make evals` | Workflow evals with real models on a copy of the fixture vault (`EVALS="help --model …"` picks); writes `src-tauri/target/evals/report.md`. Not in `check` |
| `make app` | Bump the version and build the .app, signed with `signing.local`'s identity if present |
| `make install-app` | Build at the current version (no bump) and replace the copy in /Applications, quitting Brainstead first (not its MCP servers, which pick up the new build) |
| `make dmg` | Pack the app into `src-tauri/target/release/bundle/dmg/Brainstead.dmg`, with the updater's files beside it (`tools/update_files.py`) |
| `make release NOTES="…"` | Publish a release: bump, build, DMG, commit, push, GitHub release, install (`tools/release.py`) |
| `make screenshots` | Retake `docs/images/` |
| `make icons` | Redraw the icon artwork and regenerate the icon set |
| `make sign-check` | Show the installed app's signer; fail unless it's `signing.local`'s identity |

- `make check` (`python tools/check.py` on Windows) must pass before every commit.
- Quit the installed Brainstead before `make dev` if they share an app data folder, or both run the scheduled jobs. `BRAINSTEAD_DATA` separates them. On Windows a second launch otherwise hands over to the copy already running (one lock for every build), so a dev copy needs `BRAINSTEAD_DATA` to run beside the installed app.
- Tests copy `tests/fixtures/vault/` to a temp folder; never a real vault.
- Capture extensions: Vitest tests sit beside their pure functions (`extensions/**/*.test.js`), run by `make check`. Bump `manifest.json`'s version on any change; users reload via Settings › Capture extensions, which shows each extension's version in its folder, and `chrome://extensions`. A dev build copies the extensions from the repo's `extensions/`, not its resources, so an edit reaches Chrome without a Rust rebuild (`src-tauri/src/nativehost.rs`, `shipped`).
- Evals default to cheap models (Haiku under test, Sonnet judging); ingest scenarios need and name Sonnet.

Releasing: `make release NOTES="What's new"` (`tools/release.py`). It refuses to start without notes or with uncommitted changes, then bumps and builds the app, makes the DMG and the updater's files (the notes go in the update offer), commits the bump as `Brainstead <version>`, pushes, creates the GitHub release with `Brainstead.dmg`, `Brainstead.app.tar.gz` and `latest.json`, and installs the app, opening it again if it was running. `INSTALL=0` skips the install, `VERSION=1.2.0` sets the version, `DRY_RUN=1` prints the steps. After a failure, run it again: the unreleased version is kept. Keep the name `Brainstead.dmg`: the README links to it.

Updates: the app checks `releases/latest/download/latest.json` (`tauri.conf.json`, `plugins.updater`) 20 seconds after launch and then daily, in release builds only (`src-tauri/src/updates.rs`). `make dmg` runs `tools/update_files.py`, which packs the app as `Brainstead.app.tar.gz`, signs it with the private key in `~/.tauri/waynetd777-updater.key` (no password; `TAURI_SIGNING_PRIVATE_KEY_PATH` overrides) and writes `latest.json`; `--notes "…"` adds what's new to the prompt. Back the key up: installed copies accept only packages signed with it, so losing it means no installed copy can update again. The offer, the download's progress and the updater's messages are shown in the main window's own dialog (`src/Update.tsx`, events `update-offer`, `update-progress`, `update-message`; it answers with `update_install`, `update_later` or `update_skip`); the system's dialogs stand in only when there's no main window. Scenes `update-offer`, `update-downloading` and `update-message` show it. Install and Restart goes through ⌘Q's road (the window asks about unsaved edits and saves settings, then `app_restart`), and waits while something is running (`busy` in `lib.rs`). `tools/dmg/make_dmg.py` renders the background from `tools/dmg/background.html` (WebKit) and has Finder lay out the icons, so the first run asks to let the terminal control Finder.

## Rules

`CLAUDE.md` has the full list. In short:

- The vault is the only source of truth and other apps edit it. Never reformat markdown the user didn't change. Writes are atomic and refuse if the file changed since read.
- No model writes to the vault directly: changes go through the app (`src-tauri/src/changes.rs`) and are recorded in Changes. User-started ones apply at once. Held: a scheduled change that fails a check, anything touching `Templates/` (renames and trashes too), runnable code, a system note's header. The rules go by the page's path as the file system spells it (`paths::vault_rel`, in the MCP server's page lookup and in `submit_many`): on Windows one file is also `templates\meeting.md` or `TEMPLA~1/Meeting.md`, and a name Win32 would read as another (`Templates./x.md`, `x.md::$DATA`) is refused.
- OS-specific code stays in `src-tauri/src/platform/`, for Windows and Linux.
- Same change as the behaviour: `docs/`, `README.md`, the screen's help (`src/help/*.md`) and MCP support.
- Invented names only (Orbit App, Maya, Lena, Acme…) in fixtures, docs, comments and UI text.

## Writing the docs

These pages are the user guide; the in-app help goes deeper per screen. Say what a feature does, how to use it, the keys. Short sentences, plain British English, one `##` per topic, invented names in examples.

## Where things live

In `~/Library/Application Support/Brainstead/` on a Mac or `%APPDATA%\Brainstead\` on Windows, or `BRAINSTEAD_DATA`. The files are the same on both.

| What | Where |
|---|---|
| Settings, including the order of areas and projects on Projects | `settings.json` |
| Index (SQLite, FTS5), rebuilt when its schema changes | `index.db` |
| Drafts, undo, extracted PDF and Office text | `drafts/`, `undo.json`, `text/` |
| Agent changes; texts before and after, compressed. Kept 90 days or 500 MB (Settings › AI assistants) | `changes/<id>.json`, `changes/text/` |
| The old review queue, moved in once, kept one release | `proposals-backup/` |
| Runs: ingest, daily and weekly summaries, daily check, contradictions, Write Current state | `runs/`, `summaries/`, `daily-check.json`, `contradictions/`, `current-state.json` |
| Knowledge health: not-duplicates, daily counts | `health.json` |
| Doc check and Compare results, kept while neither document changes | `doc-checks/` |
| Weekly review in progress | `weekly.json` |
| Weekly review's prepared suggestions (by week) and runs | `weekly-prep/` |
| Unsaved chats; saved chats' turns while read-only (by vault name); Antigravity's working folder | `chats/`, `chats-pending/`, `ask/agy/` |
| Capture extensions' copy for Show in Finder (Show in Explorer on Windows) | `extensions/` |
| Latest browser captures (watched) | `captures.json` |
| Captures waiting in the Inbox | `captures-inbox.json` |
| Captures already clarified | `inbox-captures.json` |
| Fix name's corrections | `substitutions.json` |
| Find tasks and projects: suggestions, accepted or skipped, last run | `find/state.json` |
| When each bookmark was last kept in triage | `bookmarks-kept.json` |
| Your tools that start Claude Code sessions, excluded from summaries (Settings › Jobs & schedule) | `summaries/automated.json` |
| MCP actions waiting for the app | `bridge/` |
| Logs | `logs/` |
| When updates were last checked, and a skipped version | `updates.json` |

## The MCP server

The app binary with `--mcp` is the MCP server (Ask adds `--data <folder>`, `--chat <id>`, `--model <id>`): newline-delimited JSON-RPC over stdio, protocol 2024-11-05 to 2025-06-18. `brainstead-mcp` also builds standalone. For Claude Code (Settings › AI assistants has Codex, Copilot and Antigravity's commands):

```sh
claude mcp add -s user brainstead -- "/Applications/Brainstead.app/Contents/MacOS/Brainstead" --mcp
```

On Windows the command is the installed exe, for example `claude mcp add -s user brainstead -- "%LOCALAPPDATA%\Brainstead\Brainstead.exe" --mcp`. Settings › AI assistants shows the exact path.

- It reads the vault from disk and the index read-only, and writes nothing. A change (`edit_page`, `create_note`, a task, a rename…) is checked, then sent to the app as `change.submit`, which applies or holds it.
- Other actions (ticking a task, clarifying the Inbox, starting a run, reverting) are request files in `bridge/`. The app hands them to the window, which runs the screens' own functions (`src/mcpActions.ts`), so undo, `log.md`, read-only and Templater behave as on screen.
- It opens the app if it isn't running.
- It picks up a new build (`mcp/src/reload.rs`, D-20261011-02). Every 3 seconds, idle or between requests, it compares the program file (device, inode, size, mtime) with the one it started from; once a new one is the same on two checks running (and a bundle has its `Info.plist`), and every request read has been answered, it `execv`s it with the same arguments and `BRAINSTEAD_MCP_RESUME` (the session id, `unattended`, what `initialize` agreed). It reads stdin unbuffered with `poll`, so the new process gets every unread byte. The new one sends `notifications/tools/list_changed` (initialize says `listChanged: true`) instead of waiting for an `initialize`. If the exec fails it logs to stderr and carries on, and doesn't try that file again. So `make install-app` and the updater leave the `--mcp` processes (and Chrome's capture host) running; the updater restarts only the app. Unix only: on Windows the installer (and so the updater) stops every process running the program file, `--mcp` servers included, through Restart Manager, since a program file in use can't be replaced.
- Tools have titles and the four MCP hints. Reads are separate tools marked read-only (`list_changes` beside `changes`, `list_trash` beside `restore_from_trash`…), so clients can run them unasked. A read tool sends its change tool's app action, limited to the read actions in `acting`.
- Unknown tool: JSON-RPC error. Bad arguments or an impossible action: tool error (`isError`) saying why, so the model can retry. 120 calls a minute.
- Listing tools have an `outputSchema` and return rows as `structuredContent` too: the action returns `listed(...)`'s `{ text, structured }` and `act` passes both on. `search`, `list_projects`, `list_changes`, `activity` and `graph` give one line per row unless `detail`.
- Tests check the instructions stay under 2,000 characters (Claude Code keeps no more) and every parameter has a description.
- A new screen action needs an action in `src/mcpActions.ts`, a tool in `src-tauri/mcp/src/lib.rs` and its name in `ask::MCP_TOOLS` (`src-tauri/core/src/ask.rs`). A test in `src-tauri/mcp` keeps the tools and that list in step.

## Read aloud

macOS only for now (hidden on Windows). Uses macOS's AVSpeechSynthesizer, not WebKit's `speechSynthesis`, which hides downloaded Premium and Enhanced voices and pauses unreliably.

- `src-tauri/src/platform/speech.rs`: the synthesiser, on the main thread, delegate kept alive (it holds it weakly).
- `src-tauri/src/speech.rs`: `tts_*` commands, synchronous so a stop and the next speak arrive in order.
- `src/speech/player.ts`: each utterance's id is its generation; older ones are ignored. Holds a paused utterance and remembers the next block if a pause lands at a block's end. Says title and date first and "That concludes …" last, outside the word positions so the highlight skips them.
- `src/speech/blocks.ts`: spoken text and the highlight's character map in one walk of the rendered note (UTF-16 offsets both ends).
- `platform/now_playing.rs`: media keys via Now Playing, cleared on stop. `caffeinate` keeps the screen awake while playing.
- `src/speech/*.test.*`: extraction and playback races, with a fake synthesiser.

## Signing and permissions

The Windows installer is unsigned, so SmartScreen warns on first run (More info, then Run anyway). Windows has no Full Disk Access, so the rest of this section is macOS only.

Ad-hoc builds lose Full Disk Access on every rebuild. To fix:

1. Create a self-signed code-signing certificate named `Brainstead Dev` (see `signing.local.example`). One per app.
2. Copy `signing.local.example` to `signing.local` (untracked) and put the certificate's name in it.
3. Give the installed app Full Disk Access once.

Open at login works only in the installed app, not `make dev`.

## Ask's turns

An Ask turn runs on a thread in `src-tauri/src/ask.rs` (`ask_send`), streams `ask-event` and ends with one `ask-done`. The thread catches a panic, so every turn ends, and logs each ending (`ask: chat … answered`) to `logs/`. Rust also keeps each chat's latest turn (`Turns`): if the window misses the `ask-done` (a chat once stayed on Writing… after the CLI had finished), `recover` in `src/askState.ts` takes the answer from `ask_outcome`, on Stop and every 5 seconds while a chat answers, and logs that it did. A turn ends once: a late `ask-done` is ignored.

## Help

The `?` drawer's topics are `src/help/*.md`: one per screen and Settings pane, plus getting-started guides and the keyboard list. The window imports them and Rust embeds them (`core/build.rs`), so Ask answers from the same text via the MCP `help` tool. `src/help/help.test.ts` checks every screen has a topic, links resolve to real screens, and there are no emoji or real names.

A new vault's example notes are in `src-tauri/starter-vault/`, embedded the same way (`core/src/starter.rs`). `{{date}}`, `{{date+N}}`, `{{date-N}}` and `{{now}}` in a name or text become dates when written, so example tasks aren't overdue. Tests check each example says it's one and every link resolves.

## Screenshots

macOS only. `make screenshots` retakes every scene in `tools/screenshots/scenes.json`, in both themes. Needs Pillow.

- Demo data is in `.demo/` (gitignored): the fixture vault plus a few files. The app's home is the demo's, so paths show as `~/Notes` and nothing outside `.demo/` is touched.
- `--fresh` remakes the demo; `--docs` takes only docs scenes; `-j N` runs N apps at once (default 4, each with its own demo app data); `--release` uses `make app`'s build, for release-only behaviour (its Content Security Policy).
- A scene's `caret` puts the editor caret at `|` in the text; `hover` rests the mouse there. For checking editor popups.
- `BRAINSTEAD_SCENE` is screenshot mode: the window is invisible and unfocused, so nothing flashes, yet WebKit still runs animation frames and CodeMirror popups. Text in images isn't read (kept text is reused). The webview snapshot is saved to the file in `BRAINSTEAD_SNAPSHOT`.
- The script redraws the window buttons and rounded corners, and keeps a shot only when it matches the saved image or a second capture a few seconds later; otherwise it retakes it.
- `"docs": true` scenes go to `docs/images/` (committed); others to `tools/screenshots/out/` (not). `"width"` keeps a scene's size (the menu-bar window is 720, its natural 2×).
- Docs show each screenshot in the reader's theme via `<picture>`, linked to `docs/images/index.md`, which lists them all. A new docs screenshot needs `"docs": true` and a line there.

## The product page

`site/` is the product page at https://brainstead.davies.co.za/: plain HTML, CSS and JS with no build step, light only. `.github/workflows/pages.yml` publishes it to GitHub Pages when `site/` changes on main.

- Preview it with `python3 -m http.server -d site 8765` and open http://localhost:8765/.
- Its screenshots are WebP copies of the light ones in `docs/images/`: after `make screenshots`, refresh them with `cwebp -q 80 docs/images/<name>-light.png -o site/img/<name>-light.webp`.
- `img/og.png` is the 1200×630 card shown when the link is shared. The canonical URL, the Open Graph tags, the structured data and `sitemap.xml` all name the address above, so change them together if it moves.
- Like the app, it never names the other markdown editors, and uses invented names only.

CI (`.github/workflows/ci.yml`) runs `make check` on macOS and compiles on Windows and Linux, for pushes that change the app. A push that only touches `docs/`, `site/`, `_sift/`, `archive/` or the top-level markdown is skipped, so run `make check` before committing. Start it by hand from the Actions tab when you want it anyway.
