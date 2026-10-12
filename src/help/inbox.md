---
title: Inbox
kind: screen
screens: [inbox]
order: 2
summary: Clarify what you've captured: decide what each item is, and Brainstead writes it where it belongs.
---
The Inbox is GTD's clarify step: everything captured but not yet decided. For each item you choose what it becomes. Nothing changes until you choose, and ⌘Z undoes each choice.

## What lands in the Inbox
There's a band per source:

- **Captures**: emails, Teams chats and transcripts from the browser capture extensions, and any other source in `sources/` that isn't ingested yet (a file dropped onto or imported into [Sources](app:sources), or put in the folder from outside Brainstead). They stay until you clarify them or they're ingested.
- **Scratchpad**: each thought in the Scratchpad (a `## 2026-10-03 09:15` block). Clarifying it removes the block.
- **To Do list › Other**: tasks under `#### Other` on the To Do list, where Quick capture puts them. A task leaves once it has a project, a context, `#waiting-for`, `#someday-maybe` or `#followup`, or its box isn't empty (done, cancelled or in progress). Until then it stays off Next actions in [Tasks](app:tasks).

The sidebar shows how many items are waiting.

## Clarify an item
Click an item, or move with `J` and `K` (or `↑` and `↓`), to open it. Then choose with a button or its key:

- `N` **Next action**: a form for the text, project, context, effort and due date. It needs a project or a context, or it would land back in the Inbox; until then **Save** stays off.
- `P` **New project**: opens New project with the item as its name.
- `W` **Waiting for**: the same form; the task gets `#waiting-for`.
- `2` **Done now (under 2 min)**: a task is ticked (a recurring one gets its next occurrence), a thought removed, a capture taken out of the Inbox.
- `S` **Someday / maybe**: becomes a task tagged `#someday-maybe`.
- `R` **File as reference**: adds the text as a bullet at the end of a note, or makes a new note.
- `⌫` **Delete**.

After each choice the next item opens, and a toast offers Undo.

To do the same to several items, tick their boxes (the box in a section's header ticks or unticks every item in it). With more than one ticked, the pane lists them and offers what can be done to all at once: **Make and ingest N meeting notes** (all transcripts), **Ingest N into the wiki** and **Keep N in Sources** (all captures), **Mark N done**, **Move N to Someday / maybe**, and **Delete N** (captures go to the Trash), with the same keys. Next action, Waiting for and New project need a form each, so they're one item at a time: click an item to clarify it alone. `⌘Z` undoes one item at a time.

## Where clarified items go
A next action or waiting-for with a project goes under the project note's `## Next actions` or `## Waiting for` heading, with a link to the project. Without a project it goes on the To Do list. The form shows the line before you save.

A task under Other keeps its line, rewritten with what you chose, and drops out of the Inbox.

## Clarify captured emails and chats
A capture offers only the choices that make sense for it. **Open the capture** shows the whole file.

- **A Teams transcript**: `M` **Make and ingest a meeting note** (highlighted; **Make a meeting note** when ingesting made notes is off), `I` **Ingest into the wiki** (the transcript itself, with no note), `A` **Ask about this source**, `R` **Keep in Sources** and `⌫` **Move to the Trash**. When Brainstead is sure of the note, it writes and ingests it and trashes the transcript in one step; otherwise [Meeting note from a transcript](app:meeting) opens to ask. Suggest doesn't ask the AI about a transcript. While its note is being written and ingested, a transcript's row says **Writing note…** or **Ingesting…** and can't be opened or ticked; it leaves the Inbox once that's done. If the note or its ingest fails (or the note is held in Changes), the row says so, with the reason on hover, and can be chosen again.
- **An email thread or Teams chat**: `D` **Draft a reply** (highlighted; opens [Draft reply](app:reply)), `I` **Ingest into the wiki**, `A` **Ask about this source**, then Next action, New project, Waiting for and Someday / maybe, `R` **Keep in Sources** and `⌫` **Move to the Trash**.
- **Any other capture**: `I` **Ingest into the wiki** (highlighted), then the same, without Draft a reply or Waiting for.

The first choices on a capture are its source's own, the same as its row on [Sources](app:sources) offers.

**Ask about this source** starts a chat about it in [Ask](app:ask) and leaves it in the Inbox, to clarify once you know what it is. Ask and Draft reply show a **Still in the Inbox** strip with the choices that take it out (**Done, keep in Sources**, **Ingest and take out**, **Move to the Trash**, or for a transcript its meeting note), so there's no going back to the Inbox for them.

**Ingest into the wiki** lists its changes in [Changes](app:review). **Keep in Sources** takes it out of the Inbox and leaves the file in [Sources](app:sources), so a capture has no Done now. **Move to the Trash** moves the file to the vault's Trash; restored from there (or with `⌘Z`), it's back in the Inbox.

As a task, a capture starts as `Follow up on [[the capture]]`.

## Let the AI suggest
**Suggest** on an open item asks the AI what it should become, with a project, context, effort, due date and why. **Suggest for all** asks about the first 20 items at once. A Teams transcript is always suggested **a meeting note**, without asking the AI; a captured email or chat can also be suggested **ingest into the wiki** or, for a thread, **a reply**. Nothing happens until you accept: press `↩` or **Accept suggestion** (a next action or waiting-for opens the form first). The model is set under Models by job in [Settings › AI assistants](app:settings/assistants).

An assistant can ask for the same suggestions and accept one by clarifying the item. Like the form, it makes a next action only with a project or a context, and only in an active project.

When this week's [Weekly review](app:weekly) is prepared, its Inbox suggestions show here too, until you click **Suggest** again.
