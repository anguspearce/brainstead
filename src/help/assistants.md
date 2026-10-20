---
title: Assistants and Brainstead's tools
kind: screen
screens: []
order: 38
summary: What AI assistants can do in Brainstead through its tools, in Ask and in sessions outside the app, and what waits for you.
---
Brainstead gives AI assistants a set of tools that do what its screens do, as an MCP server (the standard way to add tools to an assistant). Ask's Claude Code, Codex and Copilot chats get them, and so does an assistant run outside the app once you add them (the command is in [Settings › AI assistants](app:settings/assistants)).

An assistant never edits a file itself. Its tools ask the app, which makes each change with the same checks the screens use. In short, an assistant can do whatever the screens do, and the rest of this page says what happens to each kind of change.

## What happens at once
These change things straight away, and each is undoable with `⌘Z`. A toast in the app says an assistant made it.

- Accept or skip the [Weekly review](app:weekly)'s prepared suggestions, and **Finish and save** the review.
- Accept or skip Find tasks and projects' suggestions. Each one accepted is a change in [Changes](app:review), with **Revert**.
- Save an [Ask](app:ask) chat to the vault.
- Add or remove bookmarks, save a search, add files to Sources, and restore from the Trash.
- Put areas and projects in order on [Projects](app:projects). The order is kept in Brainstead's settings, not in a note, so it isn't in Changes; moving it back undoes it.
- Knowledge health's safe fixes and its issue buttons (**Create**, **Not duplicates**, **Add them to index.md**, **Restore from Trash**, moving an unused image to the Trash), and Fix a name everywhere, with the same choices as its screen.

These happen at once with no `⌘Z`, and only when you ask:

- The Weekly review's **Start over**. Its progress and notes go; what it changed in the vault stays.
- Moving over ([Settings › General](app:settings/general)): ticking items, and **Retire them** once **The other app is stopped** is ticked. The other app's skills and scripts go to the Trash, where you can restore them.
- Going through the Weekly review, and marking a [contradiction](app:contradictions) resolved or ignored.
- Knowledge health's **Ignore**, **Ignore all** and **Show again**.
- Ask's chats (rename, pin, move to the Trash), Tasks' saved lists, and a note's own look.
- Settings, except the vault, **Read-only** and the folders left out, which are yours to change. This includes how long [Changes](app:review) keeps its history, **Rebuild index**, and your tools that start Claude Code sessions.
- Runs, with their progress and **Stop**: an ingest, the daily or weekly summary, the weekly review's preparation, the daily check, Find tasks and projects, a contradictions check, a meeting note from a transcript, **Write Current state**, bookmark triage, a drafted reply (never sent, and revised when you ask for a change), a doc check and a comparison of two documents. They use the models set in [Settings › AI assistants](app:settings/assistants). What a run changes in the vault is listed in Changes, and an assistant can undo a run when you ask.

## Tasks, the Inbox, projects and prose
These are made at once too. Each is listed in [Changes](app:review), where you can revert it, and `⌘Z` undoes it straight after.

- Tasks: tick or untick, change the words, dates, priority, contexts, effort, project, waiting for or follow-up, cancel, reorder, add and delete.
- [Quick capture](help:capture): a task to the Inbox or a thought to the Scratchpad.
- The Inbox: clarify an item, as the form does. When that changes two notes, both are made, or both held, together.
- Projects: create one, rename it, or set its status, area or **Done looks like**; rename an area on every project under it.
- Pages: edit any page, make a note (from a template too) or a wiki page, rename a file, or move it to the Trash.
- Images: add one to the vault's `images/` folder, as pasting it into a note does, for a note it writes to embed. It's never held, and **Revert** moves it to Brainstead's Trash.
- **Add the example notes**, Doc check's **Save as note** and **Start the register**, a saved Contradictions report, and Knowledge health's **Reshape pages**, **Link to…** and **Link to it**.

A quote that isn't in its source is flagged, once.

Some changes are always held for you to accept:

- a change to a template, including a new template, or renaming or trashing one;
- a change to a system note's header (the "This is a system note" callout);
- a change that adds code that runs when a note is shown.

An assistant in a session nobody is watching, such as a loop, says so. Its changes that fail a check are then held for you instead of made, and so are those of the runs it starts. See [Changes](help:review) for the checks.

An assistant can list changes, revert one when you ask, and reject a held one. It can accept a held change only while you're there, and only one held because a check failed. The others are yours to accept on the Changes screen. An assistant nobody is watching can't accept anything, start the Weekly review over or retire the other app's skills and scripts.

## What assistants can read
An assistant can read what the screens show:

- search, any page or section, backlinks, and the facts kept for each wiki page;
- tasks by list, the Inbox (with suggestions when asked), projects (flagged **Stuck** or **Quiet**) and [Today](app:today);
- the daily and weekly summaries, so it can answer "what did I do yesterday?";
- the Weekly review, Changes, runs, transcripts, the Trash and Activity;
- Sources and their status, Knowledge health's report, a source's provenance, and the contradictions found;
- the links around a page, as [Graph](app:graph) shows them;
- Ask's chats, your settings, the assistants found, and whether the app is running;
- this help.

Reading is kept apart from changing: each of these is a tool of its own, marked as only reading, so you can let an assistant use them without asking you first.

Two things stay with you on the [Trash](app:trash) screen: **Empty the Trash** and **Delete forever**. They can't be undone, so no assistant can do them.

## When Brainstead isn't open
Reading pages, search, facts, the summaries, the health report and the Page shape check work whether the app is open or not. Everything else needs the app: the assistant opens Brainstead and waits for it.

Updating Brainstead doesn't disconnect an assistant in a session outside the app on a Mac: its tools move to the new version by themselves, between calls, a few seconds after the update is in place. On Windows the update stops them, because Windows can't replace a program while it runs; reconnect them in the assistant afterwards (in Claude Code, `/mcp`).

An assistant can also bring Brainstead's window to the front when you ask to see something: a screen, a Settings pane, a note, a search, or [Graph](app:graph) around a page.

Some things stay with you: what you're typing and haven't saved, **Edit before accepting** in Changes, and the file menu's **Export PDF…**, copying and **Reveal in Finder** (**Show in Explorer** on Windows). While the vault is read-only ([Settings › Vault](app:settings/vault)) nothing can be changed.

## Limits
- An assistant can make up to 120 tool calls a minute.
- Long lists come a page at a time, each saying how many there are.
- Antigravity takes no outside tools in the mode Ask runs it, so it reads with its own tools only.
