# Ask and assistants

Ask chats with the AI assistants already on your computer, Claude Code, Codex, Copilot and Antigravity, signed in with your own accounts, inside your vault.

[Ask](#ask) · [What an assistant can do](#what-an-assistant-can-do) · [Sessions outside the app](#sessions-outside-the-app) · [Questions about Brainstead](#questions-about-brainstead)

## Ask

<a href="images/index.md#ask-and-assistants"><picture><source media="(prefers-color-scheme: dark)" srcset="images/ask-dark.png"><img alt="Ask: a chat in a tab, the assistant's answer with links to notes, and the actions under it" src="images/ask-light.png"></picture></a>

Most screens have an **Ask** button beside `?`. It starts a chat about what you're looking at (the selected task, project, Inbox item, change or page, or else the screen itself; its tooltip says which), with the question begun for you to finish.

Ask (⌥⌘7) keeps each chat in a tab (⌘T for a new one). A chat stays in Brainstead's app data until you click **Save**. That puts it in the vault as `Chat. <title>.md` and keeps the note up to date from then on. Background runs' chats stay out of the vault the same way.

**History** finds, renames, pins, saves or moves old chats to the Trash. Closed chats that aren't saved or pinned are deleted once 20 newer ones are in History.

Pick the assistant and model before the first message. New chats start with the model set in **Settings › AI assistants**, where each background job can have its own model too.

- An empty chat offers example questions and Brainstead's skills to click, or says **No assistant found** and what to install.
- Keys: ↩ sends, ⇧↩ adds a line, ↑ and ↓ recall earlier messages, Esc (or **Stop**) stops an answer.
- Messages sent while it answers queue up; a cross takes one out.
- A dot on a tab marks an answer you haven't seen; the sidebar's Ask badge counts them. An answer that arrives while the window is closed (a scheduled summary, say) stays unseen until the window is back, even on the tab it was left on. With Claude Code, a meter shows how full the chat's context is.
- `/` lists workflows. Brainstead's `/wiki` answers from the vault with citations, with any assistant. The vault's own Claude Code skills are listed too.
- `[[` links a note and `#` a tag.
- After each answer the box offers a next message in grey; → types it in. Turn it off with **Suggest a next message** in Settings › AI assistants; **Models by job** picks its model.
- Under an answer: **Copy**, **File this answer** (as a new wiki page), **Add tasks**, **Update the wiki**, **Save as note** and **Ask another model**.

## What an assistant can do

The assistant never edits a file itself. Claude Code, Codex and Copilot get Brainstead's tools, which ask the app to do what its screens do, with the same checks. So ⌘Z, `log.md` and the rest work just as if you'd done it yourself. Antigravity takes no outside tools in the mode Ask runs it, so it only reads, with its own tools.

In short, an assistant can do whatever the screens do. The help tool, and the in-app help, have the details of each.

**Made at once, listed in Changes with Revert.** An assistant can:

- change tasks: tick, rename, dates, priority, contexts, effort, project, waiting for, follow-up, cancel, reorder, add and delete;
- capture to the Inbox or the Scratchpad, and clarify Inbox items;
- create projects, change their status, area and **Done looks like**, rename an area on all its projects, and put areas and projects in order;
- edit any page, make notes (from your templates too) and wiki pages, rename, and move notes to or from the Trash;
- add an image to `images/` (from a file or its bytes) for a note it writes to embed, as pasting one does; Revert moves it to Brainstead's Trash;
- add the example notes, save a Doc check or start its register, save a comparison, save a Contradictions report, and use Knowledge health's **Reshape pages** and **Link to…**.

A quote not found in its source is flagged, once.

**Held for you to accept:** a change to a template (including a new template, or renaming or trashing one), however the assistant spells the path (Brainstead goes by the file's own name, so on Windows `templates\meeting.md` or the folder's short name `TEMPLA~1` is still `Templates/Meeting.md`), a change to a [system note's header](notes.md#system-notes), and one that adds code that runs. In a session nobody is watching, a change that fails a check is held too, and so is one from a run it starts that fails a check.

An assistant can list, reject or revert changes when you ask. It accepts a held change only in a session you're in, and only one held because a check failed. The rest are yours to accept on the Changes screen.

**At once, undoable with ⌘Z** (the app says an assistant made the change):

- accept or skip the Weekly review's prepared suggestions, and **Finish and save**;
- accept or skip Find tasks and projects' suggestions (each made as a change in Changes);
- save an Ask chat to the vault;
- bookmarks, saved searches, adding files to Sources, restoring from the Trash;
- Knowledge health's safe fixes and its issue buttons, and [Fix name](wiki.md#meeting-notes-and-the-other-tools).

**At once, with no ⌘Z, only when you ask:**

- the Weekly review's **Start over** (what it changed in the vault stays) and Moving over's ticks and **Retire them** (the previous app's skills and scripts go to the Trash). Neither works in a session nobody is watching;
- going through the Weekly review, marking contradictions resolved or ignored, Knowledge health's **Ignore**, **Ignore all** and **Show again**;
- Ask's chats (rename, pin, move to the Trash), Tasks' saved lists, a note's own look, and settings (all but the vault, Read-only and the folders left out);
- how long Changes keeps its history, **Rebuild index**, and your tools that start Claude Code sessions;
- runs, with their progress and **Stop**: an ingest, the daily or weekly summary, the weekly review's preparation, the daily check, Find tasks and projects, a contradictions check, a meeting note from a transcript, **Write Current state**, bookmark triage, a drafted reply (revised when you ask for a change), a doc check and a comparison of two documents. What a run changes is listed in Changes, with Revert.

**What it can read:** search, any page or section, backlinks, facts, tasks, the Inbox, projects, the summaries, the Weekly review, Changes, runs, transcripts, the Trash, Activity, Sources, Knowledge health's report, provenance, contradictions, the graph, Ask's chats, settings, Glance's counts, whether the app is running, and the in-app help. Each read is its own tool, marked read-only, so you can let an assistant read without asking you.

Long lists come a page at a time and say how many there are. Nothing changes while the vault is read-only.

## Sessions outside the app

<a href="images/index.md#ask-and-assistants"><picture><source media="(prefers-color-scheme: dark)" srcset="images/settings-assistants-dark.png"><img alt="Settings › AI assistants: the assistants found, the model for new chats and for each job, and the command that adds Brainstead's tools to Claude Code" src="images/settings-assistants-light.png"></picture></a>

The same tools work from an assistant in a terminal, in every folder. **Settings › AI assistants** shows each assistant's command to copy. Run it once:

```sh
claude mcp add -s user brainstead -- "/Applications/Brainstead.app/Contents/MacOS/Brainstead" --mcp   # Claude Code
codex mcp add brainstead -- "/Applications/Brainstead.app/Contents/MacOS/Brainstead" --mcp            # Codex
copilot mcp add brainstead -- "/Applications/Brainstead.app/Contents/MacOS/Brainstead" --mcp          # GitHub Copilot
agy mcp add brainstead -- "/Applications/Brainstead.app/Contents/MacOS/Brainstead" --mcp              # Antigravity
```

On Windows the command points at the installed exe instead, for example `claude mcp add -s user brainstead -- "%LOCALAPPDATA%\Brainstead\Brainstead.exe" --mcp`. Settings › AI assistants shows the exact path for each assistant.

Searching and reading the vault, facts, the summaries, the health report, the Page shape check and help work with Brainstead closed. Everything else needs the app, and opens it. On a Mac an update doesn't disconnect a session: its tools move to the new version by themselves, between calls. On Windows the update stops them, so reconnect them in the assistant (in Claude Code, `/mcp`). An assistant can also bring the window to the front on a screen, a Settings pane, a note, a search or an image.

Some things stay with you:

- what you're typing and haven't saved;
- Changes' **Edit before accepting**;
- the file menu's export, copy and **Reveal in Finder** (**Show in Explorer** on Windows) commands;
- the Trash's **Empty the Trash** and **Delete forever**. An assistant can move things to the Trash and restore them, but never delete them for good.

## Questions about Brainstead

Ask how to do something in Brainstead ("how do I change the capture shortcut?") and the assistant answers from the app's own help, naming where things are, rather than from your vault. The help is the same as the `?` drawer on every screen.
