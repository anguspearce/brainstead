---
title: Settings › Vault
kind: screen
screens: [settings/vault]
order: 32
summary: Which folder is your vault, whether Brainstead may change it, and which folders it skips.
---
The vault is the folder of markdown notes Brainstead reads. Its index is kept outside the folder.

## Choose the vault folder
Under **Vault folder**, click **Choose…** and pick the folder. Every `.md` and `.txt` file in it is indexed, except in system and excluded folders. **Show in Finder** (**Show in Explorer** on Windows) opens it.

The card below counts the indexed files (notes, wiki pages, sources, templates, open tasks and unresolved links) and says when the index was last updated. **Rebuild index** reads every file again and rebuilds the index; the files aren't changed. An assistant sees the same counts, or why the index failed.

## Make the vault editable
A folder you choose opens with **Read-only** on: Brainstead never changes anything in it. A new vault made on the first screen opens with it off. Turn **Read-only** off to tick tasks, set dates, capture, save chats and let the summaries write.

While it's on:

- captures from the browser extensions are refused;
- a saved Ask chat's new turns wait in the app data folder until it's off;
- a **Read-only** pill shows in the sidebar's status line and the menu-bar window; click it to come here.

Even with read-only off, Brainstead only saves a file if it hasn't changed since Brainstead read it. Every AI change is listed in [Changes](app:review), where you can revert it.

## Skip folders you don't want indexed
Under **Excluded folders**, type a folder name (not a path) and press `↩` or click **Add**. Any folder with that name, at any depth, is skipped. Click the cross on a name to index it again. System folders can't be excluded.

## Add the example notes
**Add the example notes** adds a new vault's starter notes: **Start here**, a project, a meeting note, two wiki pages and the example templates, each marked as safe to delete. Only missing ones are added; nothing existing changes. Read-only has to be off. An assistant can add them when you ask: each is listed in [Changes](app:review) with Revert, and the templates wait there for you to accept.

## Know the system folders
Brainstead treats these folders specially:

- `sources/`: imported files and captured emails and transcripts.
- `wiki/`: entities, concepts and source summaries.
- `Templates/`: note templates, in Templater syntax.
- `images/`: pasted and dropped images. Not indexed.
- `.trash/` and hidden folders: deleted items and other apps' settings. Not indexed.
