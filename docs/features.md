# Features

The user guide to Brainstead, a Mac and Windows app for tasks, notes and an [LLM wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) over one folder of plain markdown files: your vault. Each screen's help (`?`) goes deeper.

<a href="images/index.md#features"><picture><source media="(prefers-color-scheme: dark)" srcset="images/today-dark.png"><img alt="Today: overdue, due and deferred tasks, what you're waiting for, the capture box, and where you left off" src="images/today-light.png"></picture></a>

| Page | What's in it |
|---|---|
| [Tasks and GTD](tasks.md) | Today, Quick capture, the Inbox, Tasks, Projects, the weekly review, and how tasks are written |
| [Notes](notes.md) | View, Edit and Source, saving and drafts, queries and callouts, templates, search and ⌘K, the Trash |
| [The wiki and sources](wiki.md) | Sources and the browser extensions, ingest, Changes, wiki pages, Knowledge health, contradictions, meeting notes and the other tools |
| [Ask and assistants](ask.md) | Chats with Claude Code, Codex, Copilot and Antigravity, or Ollama's local models through Codex, what an assistant can do, Brainstead's tools in a terminal |
| [Day to day](day-to-day.md) | The menu bar (system tray on Windows), the daily and weekly summaries and the daily check, help, Settings, and what happens in the background |

## Requirements

- A Mac with Apple silicon (M1 or later) and macOS 14 or later, or a PC with Windows 10 or 11 (64-bit).
- For Ask and the AI features: Claude Code, Codex, GitHub Copilot's CLI or Antigravity, installed and signed in; or Ollama with a model that calls tools, and Codex to run it. Everything else works without them.
- For the capture extensions: Chrome, Edge, Brave, Vivaldi or Chromium.

## First run

The Welcome screen asks for two things (one on Windows, which has no Full Disk Access step):

- **Full Disk Access (Mac only).** A vault in a cloud-synced folder (OneDrive, iCloud Drive, Dropbox) needs it, or macOS asks for each protected place separately. Click **Open System Settings**, turn Brainstead on under Privacy & Security › Full Disk Access, and let macOS reopen it. You can skip this and do it later from Settings › Permissions.
- **Your vault**: the folder of markdown notes. Brainstead opens it **read-only**: nothing changes until you turn **Read-only** off in Settings › Vault.

No vault yet? **Create a new vault** asks for a name and a place, and adds a few example notes: a To Do list, a project, a meeting note, a template, wiki pages and sources. Each says it's an example you can delete, and **Start here** lists them. A new vault opens writable, as nothing in it is yours yet. Settings › Vault › **Add the example notes** puts back any that are missing.

Once the vault is open, a welcome offers **Getting started**: step-by-step guides in the help, beginning with your vault and finding your way around. **Not now** closes it until the next launch; tick **Don't ask me again**, or turn off **Offer Getting started when Brainstead opens** in Settings › General, to stop it. The guides are always in the help: press `?` and click **Getting started**.

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

On Windows, scrollbars stay out of sight until the pointer is over a list or page, or it scrolls; then a thin line shows where you are, and it widens under the pointer to drag. It goes again a second after scrolling stops. Where Windows shows scroll bars all the time (Settings › Accessibility › Visual effects › **Always show scrollbars**), Brainstead shows them all the time too. A Mac keeps its native scrollbars.

Every pane can be dragged wider or narrower by the line between it and the next: the sidebar, each screen's left list and right-hand pane, and the columns of every table (Notes, Wiki, Sources, Templates, Trash, Activity's log, Doc check's register) by their headings' edges (a column after the table's flexible one by its left edge), one flexible column in each taking the rest. A double-click on the line puts the pane back, **Reset pane widths** in Settings › General puts every one back, **Lock pane widths** and **Lock column widths** there stop either type being resized, and the widths are kept per computer ([day-to-day](day-to-day.md#settings)).

| Keys | What they do |
|---|---|
| ⌘K | Search or jump to anything, and run commands |
| ⌘⇧F | The Search page |
| ⌃⌥Space (Ctrl+Shift+Space on Windows) | Quick capture, from any app |
| ⌘N | New note |
| ⇧⌘P | Read the open note aloud, then play or pause |
| ⌘. | Focus mode for the open note, wiki page or source; Esc leaves it |
| ⌥⌘1 ⌥⌘2 ⌥⌘3 (Ctrl+Shift+1 … 3 on Windows) | View, Edit or Source for the open note |
| ⌘1 … ⌘9, ⌘0 (Ctrl+1 … 0 on Windows) | Today, Inbox, Tasks, Projects, Weekly review, Ask, Search, Notes, Wiki, Sources |
| ⌘[ ⌘] | Back, forward |
| ⌘Z | Undo the last change |
| ⇧⌘= ⇧⌘- ⇧⌘0 (Ctrl+Shift+= … on Windows) | Interface size: everything bigger, smaller, or back to 100% |
| ← → ⇧← ⇧→ ↩ | On a pane's divider (Tab reaches it): move it 8 px, 40 px, or put the pane back |
| Home End | On a pane's divider: the pane at its least or greatest width |
| ⌘, | Settings |
| ? | Help for this screen |

On Windows, ⌘ is Ctrl, ⌥ is Alt and ⇧ is Shift, so ⌘K is Ctrl+K. The window has no title bar, as on a Mac: its minimise, maximise and close buttons are at the top right, where Windows puts them, and the empty part of any screen's top bar drags the window (double-click it to maximise or restore). Hovering over maximise offers Windows 11's snap layouts, when they're on in Windows Settings › System › Multitasking. Where a Mac offers Quick Look (a Word, PowerPoint or Excel source, one Brainstead can't show, the Trash), Windows shows **Preview**: the file drawn in the window by the same preview handler Explorer's preview pane uses (Office's for its own files, Windows' for text, RTF, fonts and mail), framed by the file's name, **Open in its app** and Close. Each handler runs in its own process, as Explorer runs it (Office's in Word, Excel or PowerPoint, the rest in Windows' preview host), never inside Brainstead. A type Windows has no preview handler for, or one whose handler can only run inside the app, opens in its own app instead. So does a file Windows marks as downloaded from the internet (its mark of the web), which Explorer's preview pane has refused to show since Windows' October 2025 update; the panel says why.

On Windows, Settings › General › Appearance › **Accent colour** can be **System**: buttons, links, the selected row, the focus ring and checked boxes then take the Windows accent colour, as Windows Settings and Terminal do, and follow it within a few seconds when it changes. A colour that text on it would be hard to read is made lighter or darker, keeping its hue.

Every screen remembers its list, filter and selection across restarts. Back and forward go through where you've been. Closing the window leaves Brainstead running in the [menu bar](day-to-day.md#the-menu-bar) (on Windows, if **Show in the menu bar** is on; otherwise closing the window quits). ⌘Q quits (on Windows, which has no menu bar, the tray's **Quit Brainstead**), asking first about unsaved edits.

## Licence and credits

Brainstead is free software under the GNU General Public License, version 3 or later.

- You may use it, share it and change it.
- Anything you pass on, changed or not, must stay free under the same licence, with its source.
- It comes with no warranty.
- The licence is in `LICENSE` in the source, and at [gnu.org](https://www.gnu.org/licenses/gpl-3.0.html).

Brainstead is built with Tauri and React, and uses CodeMirror (editor), unified with remark and rehype (markdown), KaTeX (maths), Mermaid (diagrams), pdf.js (PDFs), d3-force (graph), rrule (recurring tasks), moment, Luxon and chrono-node (dates), SQLite with FTS5 through rusqlite (the index) and `similar` (diffs). These are under the MIT, Apache 2.0, BSD or ISC licences. The type is Geist and Geist Mono, under the SIL Open Font License.
