---
title: Wiki
kind: screen
screens: [wiki]
order: 11
page: Read a wiki page
summary: Browse and edit the wiki, the pages about people, products and ideas that are built from your sources.
---
The wiki is the `wiki/` folder of your vault. Each page is about one entity (a person, team, product or company) or concept (an idea, process or topic), and cites its sources. Go to it with `⌘9`. A page can also hold live `tasks` and `dataview` queries, such as a project's open tasks; see [Task and Dataview queries](help:queries).

## Find a page
The list shows each page's type, source count, health and tags, with aliases under the title as "also …". On the right it counts the pages shown and what they take on disk. Narrow it down with:

- The tag tree on the left: **All pages**, **Untagged**, or one tag.
- The type chips above the list, such as entity or concept. Click a chip again to show every type.
- **Search the wiki**, which searches the pages' text and names.

Click a column heading to sort by it.

## Read the health column
Each page's health says whether it needs you; hover it for what it means. Amber means you have something to decide:

- **Held**: a change to the page is held for you in [Changes](app:review).
- **Stale**: a source it cites changed after the page was written.
- **Check**: another check in [Wiki health](app:health) lists something on it to decide.

Grey text means nothing needs you:

- **Worth a look**: Wiki health suggests a look, such as a page others rely on that hasn't changed in months.
- **Safe fix**: Brainstead can fix it by itself, such as an `updated:` date that's out of date.
- **Healthy**: nothing to do.

When there are safe fixes, one line above the list says how many, such as "26 safe fixes". Click **Fix** on it to make them all, as **Fix safe issues** on Wiki health does; `⌘Z` undoes them. Click the count to open Wiki health at them and see which pages they're on.

## Make a new page
Click **New wiki page**, type a name, choose **Concept**, **Entity** or **Initiative** and press `↩`. The page is created in `wiki/concepts/`, `wiki/entities/` or `wiki/initiatives/` and opens for you to write. It is your own page, so it isn't listed in Changes.

## Track an initiative
An initiative is something you're tracking: a page in `wiki/initiatives/` with **Actions** and **Decisions** sections. Its `status:` is active, paused or done.

Each action is one list item: what's to be done, then whose it is, its status and, if it has one, a due date. Under it go the quotes it came from:

```markdown
- Send the revised pricing [owner:: me] [status:: open] [due:: 2026-11-25] ^a-3f9c
  - "Can you send the revised pricing by Tuesday?" — [[Mail. Pricing - 2026-11-17]]
```

`[owner:: me]` is yours, `[delegated:: Lena]` is one you handed to Lena (you're waiting for her), and `[owner:: Lena]` is Lena's own. A delegated action also gets `[since:: …]`, the day it was handed over, written for you. The status is open, done or dropped. Each decision is a dated item, newest first: `- 2026-11-12 — Annual billing ^d-91c2`, with its quotes under it the same way. Actions are plain list items, not checkboxes, so they don't show up in Tasks.

You can write and edit them by hand; [Wiki health](app:health) says when one has no owner, a status or date that doesn't read, or a quote that isn't in the note it cites. Assistants add and change them for you too, each change listed in [Changes](app:review).

To move an action to another initiative, hover over it on the initiative's page and choose **Move to…**: it moves whole, with its quotes, and a task made from it follows it. (`M` in the [Inbox](app:inbox) does the same.)

An action made a task in the Inbox links its task to it: the task ends with the initiative's name, which links the action. Tick the task, wherever you tick it, and the action is marked done; cancel it and the action is dropped; open it again and so is the action. Each of these is listed in [Changes](app:review). Tasks made before this link existed aren't followed.

## Read a wiki page
Entity, concept and initiative pages share one layout: the opening text, **Current state**, the page's topics, a **Timeline** of dated entries (newest first, each with a **Source** line), then **See also**. [Wiki health](app:health) lists pages laid out another way, and **Reshape pages** there fixes them.

Under the title are the page's aliases, its `updated:` date and its source count. A link to a source shows as a small number: click it to open the source at the passage cited. The **Sources** card lists them in the same order.

The **Facts** card lists facts that ingest checked against the sources: the latest value of each, with its "as of" date and what it replaced. Point at one to see its quote; click it to open the source. An older page may have no facts yet. Reverting the ingest in [Changes](app:review) takes its facts back too.

If another page disagrees with this one, a banner says so. **See it** opens [Contradictions](app:contradictions).

The top bar has:

- **Ask**: start a chat about the page.
- **Refresh from sources**: ingest the page's sources again.
- The graph button: open the [Graph](app:graph) centred on the page.

## Edit a page's details
In Edit and Source, **Page details** sits above the text:

- **Aliases** and **Tags**: type one and press `↩` or a comma to add it; click its cross to remove it.
- **Description**: one line saying what the page is.
- **Sources**: the files the page rests on. Type in **Add a source** to find one, then click it.

Each change is an ordinary edit to the page. While it is unsaved, a banner offers **Save** (`⌘S`) and **Discard**.

## Changes from the assistants
Changes from ingest, Ask and Wiki health apply at once and are listed in [Changes](app:review), where you can revert them. Some are held there for you instead: a scheduled run's change that fails a check, and any change to a template, to a system note's header, or that adds code that runs.

## Resize the panes
Drag the line between the tags and the list to resize them, and a column heading's left edge to resize that column (**Tags** included); **Title** takes the rest, and a double-click puts either back ([pane widths](help:settings-general)). In a window narrower than about 940 pixels the list leaves out **Tags** (the tags beside it still filter) and **Sources**; widen the window to see them.
