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

- **Actions**: the actions on your [initiatives](help:wiki) that are yours, and those you delegated that are past their due date (or, with no due date, that you handed over more than a week ago), until you decide what each becomes. Each shows its initiative, whose it is, its due date and the quotes it came from. **Next action** (`N`) makes it a task (`Waiting for` (`W`) a `#waiting-for` task, first for a delegated one) with its due date filled in, and marks the action with where the task went, so it leaves the Inbox; **Someday / maybe** parks it the same way. The task links the action, so ticking the task later marks the action done. **Done** marks it done on its initiative, **Drop** marks it dropped, and **Move to…** (`M`) moves it to another initiative and leaves it in the Inbox. ⌘Z undoes each. Suggest doesn't ask about actions: ingest found them already.
- **Captures**: emails, Teams chats and transcripts from the browser capture extensions, and any other source in `sources/` that isn't ingested yet (a file dropped onto or imported into [Sources](app:sources), or put in the folder from outside Brainstead). They stay until you clarify them or they're ingested: once an ingest has read one, however it was started (here, from [Sources](app:sources), or on its own as it arrived), it leaves the Inbox and stays in Sources, even if no wiki page cites it. Its actions are on your [initiatives](help:wiki). This is the one place sources wait to be dealt with: [Sources](app:sources) lists every source with its status, and [Wiki health](app:health) only counts the ones here.
- **Scratchpad**: each note in the Scratchpad (a `## 2026-10-03 09:15` block). Clarifying it removes the block.
- **To Do list › Other**: tasks under `#### Other` on the To Do list, where Quick capture puts them. A task leaves once it has a project, a context, `#waiting-for`, `#someday-maybe` or `#followup`, or its box isn't empty (done, cancelled or in progress). Until then it stays off Next actions in [Tasks](app:tasks).

Under them, a **Bookmarks** band lists the bookmarks to triage: gone stale (two weeks untouched) or pointing at a note that's gone, as Today's To process counts them. Click one to open [Triage](help:triage) on it, where you keep it, ingest it into the wiki, make a task of it or archive it. They aren't items to clarify, so `J` and `K` and the count leave them out.

The sidebar shows how many items are waiting.

## Clarify an item
Click an item, or move with `J` and `K` (or `↑` and `↓`), to open it. The choices come in two lists, GTD's question: **It needs doing** (Draft a reply, Next action, To Do, New project, Waiting for, Done now) and **It doesn't need doing** (a meeting note, Ingest into the wiki, Someday / maybe, File as reference or Keep in Sources, Delete or Move to the Trash). **Ask about this source** sits under them, as it leaves the item in the Inbox. Choose with a button or its key:

- `N` **Next action**: a form for the text, project, context, effort and due date. It needs a project or a context, or it would land back in the Inbox; until then **Save** stays off.
- `T` **To Do**: puts it on your To Do list as it is, a task at the top of **To Do** (with no rank, so it sorts first), with nothing to fill in (an action's due date comes with it). **Next action** is the one that asks for a project or context.
- `P` **New project**: opens New project with the item as its name.
- `W` **Waiting for**: the same form; the task gets `#waiting-for`.
- `D` **Done now (under 2 min)**: a task is ticked (a recurring one gets its next occurrence), a Scratchpad note removed, a capture taken out of the Inbox.
- `S` **Someday / maybe**: becomes a task tagged `#someday-maybe`.
- `F` **File as reference**: adds the text as a bullet at the end of a note, or makes a new note.
- `⌫` **Delete**.

After each choice the next item opens, and a toast offers Undo.

The keys ignore typing that wasn't meant for the Inbox: a key in the second and a half after Brainstead comes back to the front, and keys pressed in quick succession, as when a password goes to the wrong window. Press the key again on its own.

To do the same to several items, tick their boxes (the box in a section's header ticks or unticks every item in it). With more than one ticked, the pane lists them and offers what can be done to all at once: **Make and ingest N meeting notes** (all transcripts), **Ingest N into the wiki** and **Keep N in Sources** (all captures), **Put N on the To Do list**, **Mark N done**, **Move N to Someday / maybe**, and **Delete N** (captures go to the Trash; **Move N to the Trash** when every ticked item is a capture), with the same keys. Next action, Waiting for and New project need a form each, so they're one item at a time: click an item to clarify it alone. One that can't be done doesn't stop the rest: it stays ticked, and the message says how many were done and why one failed ("4 done, 1 failed: …"). `⌘Z` undoes one item at a time.

## Update an item with what's happened
**Update…** (`U`), under the choices, takes a line saying what has happened, such as "Lena has already replied to Maya" on a *Close the loop with Maya* action. An assistant applies it in the background, in an Ask chat of its own called **Update: …**, with Brainstead's own tools: it marks things done or dropped, changes a date, owner or wording, adds a follow-up (a waiting-for when someone else has the next step) and notes what happened on the project's or initiative's page. Change only what the update means is the rule it works to; when it can't tell, it changes nothing and says what it needs. While it works it's in the sidebar's Running line; when it ends a message says what it changed, with **Open** for its chat. Every change is in [Changes](help:review) with **Revert**, made at once as anything you start is.

## Where clarified items go
A next action or waiting-for with a project goes under the project note's `## Next actions` or `## Waiting for` heading, with a link to the project. Without a project it goes on the To Do list. The form shows the line before you save.

A task under Other keeps its line, rewritten with what you chose, and drops out of the Inbox.

## Clarify captured emails and chats
A capture offers only the choices that make sense for it. **Open the capture** shows the whole file.

- **A Teams transcript**: `M` **Make and ingest a meeting note** (highlighted; **Make a meeting note** when ingesting made notes is off), `I` **Ingest into the wiki** (the transcript itself, with no note), `A` **Ask about this source**, `K` **Keep in Sources** and `⌫` **Move to the Trash**. When Brainstead is sure of the note, it writes and ingests it and trashes the transcript in one step; otherwise [Meeting notes](app:meeting) opens to ask. Suggest doesn't ask the AI about a transcript. While its note is being written and ingested, a transcript's row says **Writing note…** or **Ingesting…** and can't be opened or ticked; it leaves the Inbox once the note is made and ingested (once it's made, with ingesting off). Pressing **M** again meanwhile starts nothing more. If the note or its ingest fails (or the note is held in Changes), the row says so, with the reason on hover, and can be chosen again. Once a held note is accepted in Changes, the transcript leaves the Inbox, whatever you answer the offer that follows. If the note is reverted in Changes and the transcript wasn't trashed, it's back in the Inbox, its row saying **Note reverted in Changes**, to be chosen again. **Move to the Trash** first points links to the transcript at its meeting note, if it has one.
- **An email thread or Teams chat**: `D` **Draft a reply** (highlighted; opens [Draft reply](app:reply)), `I` **Ingest into the wiki**, `A` **Ask about this source**, then Next action, To Do, New project, Waiting for and Someday / maybe, `K` **Keep in Sources** and `⌫` **Move to the Trash**.
- **Any other capture**: `I` **Ingest into the wiki** (highlighted), then the same, without Draft a reply or Waiting for.

The first choices on a capture are its source's own, the same as its row on [Sources](app:sources) offers.

**Ask about this source** starts a chat about it in [Ask](app:ask) and leaves it in the Inbox, to clarify once you know what it is. Ask and Draft reply show a **Still in the Inbox** strip with the choices that take it out (**Done, keep in Sources**, **Ingest and take out**, **Move to the Trash**, or for a transcript its meeting note), so there's no going back to the Inbox for them. In Ask it shows on the chat about the capture; a new chat (`⌘T`) or asking about something else drops it. If a choice fails, the strip stays.

**Ingest into the wiki** lists its changes in [Changes](app:review). **Keep in Sources** takes it out of the Inbox and leaves the file in [Sources](app:sources), so a capture has no Done now. **Move to the Trash** moves the file to the vault's Trash; restored from there (or with `⌘Z`), it's back in the Inbox.

As a task, a capture starts as `Follow up on [[the capture]]`.

## Let the AI suggest
**Suggest** on an open item asks the AI what it should become, with a project, context, effort, due date and why. **Suggest** in the top bar asks about the first 20 items at once; once you've asked, it's **Suggest again**. The chip beside it counts the items with a suggestion (**3 suggestions**, with a spinner while the AI works); click it to go to the next one, and each row says what it suggests. A Teams transcript is always suggested **a meeting note**, without asking the AI; a captured email or chat can also be suggested **ingest into the wiki** or, for a thread, **a reply**. The AI only suggests what the Inbox offers for that item: never Done now for a capture, and Waiting for only for a thread, Scratchpad note or task. The suggestion shows first, above the choices, with its project, context, effort and due date in grey and why under them. Nothing happens until you accept: press `↩` or **Accept** (a next action or waiting-for opens the form first). **Edit** changes the words first, for a task, waiting-for or project; **Skip** drops the suggestion and leaves the item to clarify yourself. While there's a suggestion, the item's usual first choice isn't highlighted. The model is set under Models by job in [Settings › AI assistants](app:settings/assistants).

An assistant can ask for the same suggestions and accept one by clarifying the item. Like the form, it makes a next action only with a project or a context, and only in an active project.

When this week's [Weekly review](app:weekly) is prepared, its Inbox suggestions show here too, until you click **Suggest** again.

## Resize the panes
Drag the line between the list and the open item to make the list wider or narrower; a double-click on it puts it back ([pane widths](help:settings-general)).
