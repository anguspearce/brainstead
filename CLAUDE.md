<!-- sift:begin (generated — edits inside this block are overwritten on upgrade; add local notes below the end marker) -->
# sift

This repo uses sift, in `_sift/`. Read `_sift/conventions.md` once per session and follow it — the privacy rule in it is the part that matters.
Before reading unfamiliar code: `python3 _sift/bin/sift.py map <path>` — it gives you sizes and symbol ranges to read instead of whole files.
Before fixing a bug: `python3 _sift/bin/sift.py bug find "<error>"`. After: `sift bug add …`.
After a decision worth remembering: `sift decide "<title>" --context … --decision … --consequences …` (one command, written complete); `sift decisions` reads them back.
There are no subsystem pages here and none are to be written; `_sift/conventions.md` says why.
<!-- sift:end -->

# Brainstead

macOS and Windows desktop app (Tauri 2 + React 19) for GTD, notes and an LLM-maintained wiki over a plain-markdown vault. It replaces the previous app (a web app over the same vault) and follows the conventions of the sibling apps.

The build plan (stages 1 to 10) and the agent-changes plan (D-20261005-03) are complete. They're archived, untracked, in `archive/build-instructions.md` and `archive/agent-changes.md`: read them for the history of a decision, not as instructions. Decisions are in `_sift/decisions.jsonl` (`sift decisions`); what the app does is in `README.md`, `docs/` and the in-app help (`src/help/`); developer notes are in `docs/development.md`.

New work:

- The MCP parity plan (`archive/mcp-parity.md`, local) is complete; its known gaps are fixed when their features next change (D-20261007-16).

- Agree a short plan with the user before building anything bigger than a fix: concrete items and a "Done when" line.
- If anything is unclear, contradictory or missing a decision, ask the user rather than guessing, and record the answer with `sift decide`, so the next session doesn't have to ask again.

Rules:

- The user's vault is the only source of truth and is also edited in other markdown editors and the previous app. Never write to it in tests; use `tests/fixtures/vault/` copied to a temp dir. The app opens an existing vault read-only until the user turns Read-only off; a vault it creates itself opens writable (D-20261004-18, D-20261004-22).
- Never reformat markdown the user didn't change. Writes are atomic and refuse to save if the file changed since it was read.
- No model writes to the vault directly: every change from an assistant or a run goes through the app (`src-tauri/src/changes.rs`), which records it in Changes with Revert (decided 2026-10-05, D-20261005-03). Changes the user started (Ask, a terminal session, a run started by hand) apply at once, flagged when a check fails; scheduled ones (the daily check, scheduled summaries, a terminal call with `unattended: true`) apply when they pass every check and are held otherwise. Renames and moves to the Trash apply at once, unless they touch `Templates/` (a template renamed or trashed, or link rewrites in one), which are held (D-20261005-11). A change to `Templates/`, one that adds runnable code (dataviewjs, inline `$=`, Tasks `by function`, Templater `<% %>`), or one that changes a system note's header callout (D-20261005-08) is always held, and only the user accepts those, in the app (D-20261005-09, D-20261005-14).
- No Microsoft Graph. Outlook and Teams reach the app only through the Chrome extensions.
- Keep OS-specific code inside the `platform` module so Windows and Linux stay possible.
- Update `docs/` and `README.md` in the same change as the behaviour they describe, and record decisions with `sift decide`.
- Update the screen's help (`src/help/*.md`) in the same change too: the `?` drawer shows it and Ask answers questions about the app from it.
- Every new feature, and every change to an existing one, is supported by the MCP server in the same change (`src-tauri/mcp/src/lib.rs`, `src/mcpActions.ts`): a tool to read it and, where the screen can change something, a tool to do that, with the same names the UI uses. Assistants should be able to do whatever the screens do. A tool calls the screen's own functions (its checks, ordering, search, follow-ups) rather than reimplementing them, so the two can't drift; where Rust and TypeScript both need a rule, share it or keep the two copies side by side with a test.
- Use screenshot scenes (`BRAINSTEAD_SCENE`, `tools/screenshots/scenes.json`) to check UI rather than clicking through it.
- UI mocks: https://claude.ai/artifact/Trh3uwqjL2Z8juRfWokASH (read with the Artifact tool). The design tokens (the `:root` variables in `src/styles.css`, from the archived plan's §3) are authoritative.
- The repo is public. Fixtures, docs, comments and UI text use invented names only (Orbit App, Maya, Lena, Acme…); real names, the user's employer, private projects and anything from the real vault never go in the repo.
- `make check` must pass before committing.
