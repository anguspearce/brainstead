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
The list shows each page's type, source count, health and tags, with aliases under the title as "also …". Narrow it down with:

- The tag tree on the left: **All pages**, **Untagged**, or one tag.
- The type chips above the list, such as entity or concept. Click a chip again to show every type.
- **Search the wiki**, which searches the pages' text and names.

Click a column heading to sort by it.

## Read the health column
Each page has a health chip; hover one for what it means:

- **Held**: a change to the page is held for you in [Changes](app:review).
- **Stale**: a source changed since the page was written, or Knowledge health calls it stale.
- **Check**: another check in [Knowledge health](app:health) lists it.
- **Healthy**: nothing to do.

## Make a new page
Click **New wiki page**, type a name, choose **Concept** or **Entity** and press `↩`. The page is created in `wiki/concepts/` or `wiki/entities/` and opens for you to write. It is your own page, so it isn't listed in Changes.

## Read a wiki page
Entity and concept pages share one layout: the opening text, **Current state**, the page's topics, a **Timeline** of dated entries (newest first, each with a **Source** line), then **See also**. [Knowledge health](app:health) lists pages laid out another way, and **Reshape pages** there fixes them.

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
Changes from ingest, Ask and Knowledge health apply at once and are listed in [Changes](app:review), where you can revert them. Some are held there for you instead: a scheduled run's change that fails a check, and any change to a template, to a system note's header, or that adds code that runs.
