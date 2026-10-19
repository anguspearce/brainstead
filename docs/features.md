# Features

The user guide to Brainstead, a Mac app for tasks, notes and a wiki over one folder of plain markdown files: your vault. Each screen's help (`?`) goes deeper.

<a href="images/index.md#features"><picture><source media="(prefers-color-scheme: dark)" srcset="images/today-dark.png"><img alt="Today: overdue, due and deferred tasks, what you're waiting for, the capture box, and where you left off" src="images/today-light.png"></picture></a>

| Page | What's in it |
|---|---|
| [Tasks and GTD](tasks.md) | Today, Quick capture, the Inbox, Tasks, Projects, the weekly review, and how tasks are written |
| [Notes](notes.md) | View, Edit and Source, saving and drafts, queries and callouts, templates, search and ⌘K, the Trash |
| [The wiki and sources](wiki.md) | Sources and the browser extensions, ingest, Changes, wiki pages, Knowledge health, contradictions, meeting notes and the other tools |
| [Ask and assistants](ask.md) | Chats with Claude Code, Codex, Copilot and Antigravity, what an assistant can do, Brainstead's tools in Terminal |
| [Day to day](day-to-day.md) | The menu bar, the daily and weekly summaries and the daily check, help, Settings, and what happens in the background |

## Requirements

- A Mac with Apple silicon (M1 or later) and macOS 14 or later.
- For Ask and the AI features: Claude Code, Codex, GitHub Copilot's CLI or Antigravity, installed and signed in. Everything else works without them.
- For the capture extensions: Chrome, Edge, Brave, Vivaldi or Chromium.

## First run

The Welcome screen asks for two things:

- **Full Disk Access.** A vault in a cloud-synced folder (OneDrive, iCloud Drive, Dropbox) needs it, or macOS asks for each protected place separately. Click **Open System Settings**, turn Brainstead on under Privacy & Security › Full Disk Access, and let macOS reopen it. You can skip this and do it later from Settings › Permissions.
- **Your vault**: the folder of markdown notes. Brainstead opens it **read-only**: nothing changes until you turn **Read-only** off in Settings › Vault.

No vault yet? **Create a new vault** asks for a name and a place, and adds a few example notes: a To Do list, a project, a meeting note, a template, wiki pages and sources. Each says it's an example you can delete, and **Start here** lists them. A new vault opens writable, as nothing in it is yours yet. Settings › Vault › **Add the example notes** puts back any that are missing.

Notifications are asked for the first time Brainstead has something to tell you.

Moving over from another notes app? When its files are in the vault, the **Moving over from another notes app** checklist at the top of Settings › General lists what's left to do:

- make the vault writable;
- run the daily and weekly summaries here;
- get captures arriving from the browser extensions;
- list your own automated sessions;
- switch off the other app's reviews, extensions and app;
- retire its skills and scripts: they move to the Trash, and the vault's `CLAUDE.md` tells agents to use Brainstead's tools, so their changes show in Changes.

## The window

<a href="images/index.md#features"><picture><source media="(prefers-color-scheme: dark)" srcset="images/palette-dark.png"><img alt="⌘K over Today: notes, wiki pages and tasks matching what's typed, and commands" src="images/palette-light.png"></picture></a>

The sidebar, top to bottom:

- **Today**, **Inbox**, **Tasks**, **Projects**, **Weekly review** and **Ask**;
- the library: **Search**, **Notes**, **Wiki**, **Sources**, **Templates**, **Graph**, then **Tools**, a menu of **Draft reply**, **Meeting notes** and **Compare** (a spinner beside it means one is at work);
- your **Bookmarks**;
- at the foot, **Changes**, **Knowledge health**, **Activity**, **Trash**, **Settings**, and the index's line (a dot only while it's indexing, or if it failed).

Counts that need you, such as overdue tasks, the Inbox and changes held for you, are in bold.

| Keys | What they do |
|---|---|
| ⌘K | Search or jump to anything, and run commands |
| ⌘⇧F | The Search page |
| ⌃⌥Space | Quick capture, from any app |
| ⌘N | New note |
| ⇧⌘P | Read the open note aloud, then play or pause |
| ⌘. | Focus mode for the open note, wiki page or source; Esc leaves it |
| ⌥⌘1 ⌥⌘2 ⌥⌘3 | View, Edit or Source for the open note |
| ⌘1 … ⌘9, ⌘0 | Today, Inbox, Tasks, Projects, Weekly review, Ask, Search, Notes, Wiki, Sources |
| ⌘[ ⌘] | Back, forward |
| ⌘Z | Undo the last change |
| ⌘, | Settings |
| ? | Help for this screen |

Every screen remembers its list, filter and selection across restarts. Back and forward go through where you've been. Closing the window leaves Brainstead running in the [menu bar](day-to-day.md#the-menu-bar). ⌘Q quits, asking first about unsaved edits.

## Licence and credits

Brainstead is free software under the GNU General Public License, version 3 or later.

- You may use it, share it and change it.
- Anything you pass on, changed or not, must stay free under the same licence, with its source.
- It comes with no warranty.
- The licence is in `LICENSE` in the source, and at [gnu.org](https://www.gnu.org/licenses/gpl-3.0.html).

Brainstead is built with Tauri and React, and uses CodeMirror (editor), unified with remark and rehype (markdown), KaTeX (maths), Mermaid (diagrams), pdf.js (PDFs), d3-force (graph), rrule (recurring tasks), moment, Luxon and chrono-node (dates), SQLite with FTS5 through rusqlite (the index) and `similar` (diffs). These are under the MIT, Apache 2.0, BSD or ISC licences. The type is Geist and Geist Mono, under the SIL Open Font License.
