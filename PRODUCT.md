# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

One design language on macOS and Windows, drawn in a web view inside a desktop app (Tauri). Native touches only where the system demands them: the window's own buttons, the menu bar (system tray on Windows), system dialogs, the accent colour.

## Users

Built first for its author's own daily use: someone who runs their work and personal life by Getting Things Done, keeps everything in one folder of plain markdown, sits in a lot of meetings, email and chat, and wants an AI-kept wiki of what those say without handing the files to a service. Published so that others who work the same way can use it too. It is opened many times a day, for minutes at a time (capture, clarify, tick, look something up), and for longer sittings once a day and once a week (the Inbox, the weekly review, reading the wiki).

## Product Purpose

A home for everything on your mind: tasks, notes and an LLM wiki that writes itself, over a folder of plain markdown (the vault). Tasks are `- [ ]` lines in notes, gathered into GTD lists. AI assistants already installed on the computer write and maintain wiki pages from the user's sources (emails, chats, transcripts, PDFs, Office files, images).

Success is trust: the user lets the app and its assistants work on their real vault, every day, and nothing is ever lost, reformatted or changed without a record.

## Positioning

Two things together, neither of which a neighbouring app can claim with the other:

- **One home over plain files.** GTD, notes and a wiki in one app over one folder of markdown that any other markdown editor can still open. No database of record, no lock-in.
- **AI that is safe to let loose on it.** No assistant writes to the vault directly. Every change goes through the app, is checked (quotes are verified against their sources), recorded in Changes and revertible; risky ones are held for the user to accept.

## Operating Context

- The vault is the only source of truth and is also edited in other markdown editors and synced by cloud folders (OneDrive, iCloud Drive, Dropbox). The app opens an existing vault read-only until the user turns that off.
- The GTD rituals: quick capture from any app, clarifying the Inbox, Today, Tasks, Projects, a guided weekly review.
- Sources arrive by import, by browser extensions for Outlook, Teams and Slack, and by a background sync of the user's own chats, transcripts and mail.
- Assistants are the command-line tools the user already has (Claude Code, Codex, GitHub Copilot's CLI, Antigravity, or local Ollama models through Codex). They work in Ask, in scheduled runs (daily and weekly summaries, a daily check, the weekly review's preparation) and from a terminal through the app's MCP server, which can do whatever the screens do.
- It keeps running from the menu bar or system tray with its window closed.

## Capabilities and Constraints

- Mac (Apple silicon, macOS 14 or later) and Windows 10 or 11.
- Writes are atomic, refuse to save over a file changed since it was read, and never reformat markdown the user didn't change.
- Markdown as other tools write it keeps working: wikilinks, tags, callouts, maths, diagrams, Tasks-format task fields and queries, Dataview, Templater.
- A change to a template, one that adds or changes code that runs, or one that changes a system note's header is always held for the user.
- No Microsoft Graph: Outlook and Teams reach the app only through the browser extensions.
- Every feature a screen has, the MCP server has too, under the same names.
- Terminology is GTD's and the app's own: vault, Inbox, clarify, next action, waiting for, someday / maybe, project, initiative, source, ingest, wiki page, Changes, held, Revert.
- Free software under the GPL, version 3 or later. The repository is public: examples, fixtures and UI text use invented names only.

## Brand Commitments

- The name is Brainstead, with its blue house-and-brain mark.
- Voice: plain British English, short sentences, says what a thing does and what happens next. Controls name their action. No marketing tone inside the app.
- Line icons drawn in one weight; no emoji in the interface (a task's emoji fields are drawn as icons).
- Visible text never names another note-taking product; it says "other markdown editors".
- Every button has a tooltip.
- The design tokens in `src/styles.css` (`:root`) are authoritative; the UI mocks are referenced from `CLAUDE.md`.

## Evidence on Hand

- The user guide in `docs/`, with screenshots of every main screen in light and dark (`docs/images/`), retaken from scenes (`tools/screenshots/scenes.json`).
- The in-app help, one page a screen plus step-by-step guides (`src/help/`).
- The product website and its four tutorials (`site/`).
- A fixture vault with invented people and projects (`tests/fixtures/vault/`).
- There are no testimonials, customer names, usage numbers or press; none may be invented.

## Product Principles

1. The files are the user's. Never lose, reformat or silently change what's in the vault.
2. Nothing an assistant does is hidden: it is checked, recorded and revertible, and anything risky waits for the user.
3. One home: a task, a note and a wiki page are the same kind of thing in the same place, reached the same way.
4. Fast for the minute-long visit, calm for the long sitting: the keyboard reaches everything, and the screen shows what needs you and little else.
5. Whatever a screen can do, an assistant can do through the same functions, so the two never drift.

## Accessibility & Inclusion

Light and dark themes, an interface size from 80% to 150%, a reading size for notes, reduced motion honoured, everything reachable by keyboard, and read-aloud for notes. No formal standard has been set; text contrast is kept at 4.5:1 or better.
