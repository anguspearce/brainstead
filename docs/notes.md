# Notes

[A note](#a-note) · [Read aloud](#read-aloud) · [Writing](#writing) · [Saving and drafts](#saving-and-drafts) · [Queries, callouts and diagrams](#queries-callouts-and-diagrams) · [Templates](#templates) · [Finding things](#finding-things) · [System notes](#system-notes) · [The Trash](#the-trash)

## A note

<a href="images/index.md#notes"><picture><source media="(prefers-color-scheme: dark)" srcset="images/note-dark.png"><img alt="A meeting note in View: properties, actions as tasks, and the side pane with Linked from, open tasks and the outline" src="images/note-light.png"></picture></a>

A note has three modes:

- **View** (⌘1) shows it rendered: properties, links, tables, tasks, callouts, diagrams and maths.
- **Edit** (⌘2) is the markdown, with the syntax hidden except on the lines you're on.
- **Source** (⌘3) is the plain markdown in a monospaced font, with no toolbar.

Beside the note are **Linked from** (every note that links here, with the line), its **open tasks** to tick, and its **outline**. On a 1-1 note, **Last time with** shows the previous 1-1 and the follow-ups still open with that person.

The top bar has **Ask** (a chat about this note), a bookmark, **Copy** (markdown, rich text, path or title), **Rename…** (⇧⌘R, rewriting every link to the note), **Graph** and **Focus mode** (⌘., Esc leaves it). The **Note** menu has most of these. Right-click a file for its menu, which also has a source's actions (**Make and ingest a meeting note**, **Draft a reply**, **Ingest into the wiki**, **Ask about this source**) where they fit, and **Compare with…**. **Export PDF…** (⌘P) writes the note in its look, images included, with page numbers. Each note can have its own look (Brainstead, Business, Editorial, Modern, Minimal, Report or Technical), colour, and light or dark.

## Read aloud

<a href="images/index.md#notes"><picture><source media="(prefers-color-scheme: dark)" srcset="images/read-aloud-dark.png"><img alt="A wiki page being read aloud: the word being said highlighted, and the player bar at the foot of the page" src="images/read-aloud-light.png"></picture></a>

Click the speaker on the top bar, or press ⇧⌘P, to hear the note or wiki page in macOS's voices. Read aloud is Mac only for now: it's hidden on Windows. Each word is highlighted as it's said. Space plays and pauses; Esc stops. Choose the voice and speed in Settings › Notes.

## Writing

<a href="images/index.md#notes"><picture><source media="(prefers-color-scheme: dark)" srcset="images/editor-dark.png"><img alt="The same note in Edit, with the formatting toolbar and the markdown shown as you'd read it" src="images/editor-light.png"></picture></a>

- The toolbar formats text, lists, tasks, quotes, code, tables, links and images, and inserts today's date or a query.
- `[[` links a note and `#` adds a tag. Both are suggested as you type part of a name: `[[` alone lists your newest notes, and `#calls` finds `#context/calls`. ⌘-click a link to open it.
- Paste or drop an image to save it in `images/` and embed it. In Edit, hover it to resize by a corner (written as `![[pic.png|400]]`) or remove it from the note; the file stays. Double-click an image to edit its markdown.
- Click an image in View, or its expand button in Edit, to see it fitted to the window. Click for actual size; Escape closes it.
- In a task, `due:`, `defer:`, `start:` and `created:` followed by a word (tomorrow, fri, +3d) become dates. A date picker opens when you type the colon.
- Spelling and grammar are checked as you type, in the English you choose.
- ⌘F finds, and in Edit and Source replaces, one match or all.

**New note** (⌘N) names the note from its type, title and date (`Meeting. Orbit App Steerco - 2026-09-30.md`), or makes it from a template.

## Saving and drafts

⌘S saves, and never reformats what you didn't change. Leaving a note with unsaved changes asks first.

- Unsaved text is kept as a draft a second after you stop typing. If the app quits, opening the note again offers it back.
- If another app changed the file meanwhile, changes on other lines are merged in. Changes on the same lines show a banner with **Reload**, **Keep mine** and **Compare** (which marks the lines that differ). Nothing is overwritten.

## Queries, callouts and diagrams

- **Tasks queries** (`tasks` blocks) and **Dataview** (`dataview` blocks: TABLE, LIST, TASK and CALENDAR) run in View, with tasks you can tick. In Edit, the editor suggests what comes next, and **Build query…** opens a builder with a live preview.
- **Callouts** (`> [!tip] Title`) are tinted boxes with an icon; `[!tip]-` or `[!tip]+` makes them fold.
- **Mermaid** diagrams, with pan, zoom and a full-screen button (Escape closes it).
- **KaTeX** maths (`$…$` and `$$…$$`); an amount like $5 isn't taken for maths.
- Highlighted code, and `==highlights==` in colours.

Plain queries work in every note. Scripts in queries (`dataviewjs` blocks, inline `$=` and Tasks' `filter by function`) run only in your own notes. They don't run in sources, wiki pages, templates, saved chats, the summary notes or notes an assistant wrote. Delete `created-by: assistant` from a note an assistant wrote to trust it.

## Templates

A template turns a routine into one click. A 1-1 template can ask who it's with, file the note under their name and date, and pull in every open follow-up tagged for them. A weekly plan can lay out the days ahead and what's due in each.

Inside a `<% %>` tag the editor suggests `tp` functions with their arguments, `moment` dates and your own names, each explained with a link to its docs. Hover over a name to see what it does.

<a href="images/index.md#notes"><picture><source media="(prefers-color-scheme: dark)" srcset="images/template-caret-dark.png"><img alt="A 1-1 template in Edit: inside a tag, the editor suggests tp.file's functions with their arguments, and explains the one chosen, with a link to its docs" src="images/template-caret-light.png"></picture></a>

Templates are the files in `Templates/`, written in Templater's syntax: prompts and pick lists, dates, file names, properties, includes and your own scripts in `Templates/scripts`. Its questions are asked in the New note dialog. **Test run** shows what it would make without writing anything.

A new vault comes with example templates: Meeting, Daily note (date offsets and an include), 1-1 (a pick list and a `#followup` task), Project, Weekly plan (a loop and a user script) and Reading note (properties and a move to a folder). **Settings › Vault › Add the example notes** adds any that are missing.

## Finding things

<a href="images/index.md#notes"><picture><source media="(prefers-color-scheme: dark)" srcset="images/search-dark.png"><img alt="Search: results with their best passage and the matches marked, the layer chips, the date range and saved searches" src="images/search-light.png"></picture></a>

- **⌘K** finds notes, wiki pages, sources, open tasks, screens and commands from any screen, with **Search everything**, **Ask** and **Capture … as a task**.
- **Search** (⌘⇧F) searches the text of every note, wiki page and source. Words match in any form. `+word`, `-word`, `"exact phrase"`, `tag:hiring`, `since:2026-09` and `before:2026-10-01` narrow it. Choose the layers, a date range and the order. **Save search** keeps one under Saved searches, where its x deletes it.
- **Notes**, **Wiki** and **Sources** list their files, sorted and filtered as you type. Notes can also be grouped by type.

## System notes

Some notes are Brainstead's own: the To Do list, the Scratchpad, bookmarks, saved searches, the canonical docs register, the daily and weekly summaries, `CLAUDE.md`, `index.md`, `log.md`, and any note that opens with a **This is a system note** callout. Other features rely on them:

- They can't be renamed or moved to the Trash, from the app or by an assistant.
- The header callout is locked in the editor. Edit freely below it.
- If the header goes missing or changes, Knowledge health flags it. Its safe fix puts back the last header Brainstead saw.
- An assistant's change to the header is always held for you in Changes, and only you can accept it.

Other apps can still change these files.

## The Trash

Moving a note to the Trash puts it in the vault's `.trash` folder, with Undo. Trashing a transcript first points its links at its meeting note, when it has one. A capture restored from the Trash goes back to the Inbox. The Trash screen lists and searches what's there, puts files back where they were, and deletes them for good. **Quick Look** (`Space`) previews a file on a Mac; Windows has no preview yet.
