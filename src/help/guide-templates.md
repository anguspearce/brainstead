---
title: Templates and note looks
kind: guide
screens: []
order: 8
summary: Make notes from templates, write and test your own, add live queries and choose how notes look.
---
## Make a note from a template
Press `⌘N`, or click **New note** on [Notes](app:notes). Pick a template on the left, then click **Create** or press `⌘↩`. Answer its questions as they come and press `↩`; **Stop** ends the run and makes nothing.

## Write your own template
Open [Templates](app:templates) and click **New template**, type a name and click **Create**. It opens in Edit as `Templates/<name>.md`. `<% tp.date.now() %>` writes today's date, and `<%* … %>` runs code such as a question. The editor suggests completions inside a tag; `⌃Space` opens the list. A new vault's examples (**Meeting**, **1-1**, **Project** and more) are good to start from; [Templates](help:templates) explains the tags.

## Test it before you use it
On an open template in [Templates](app:templates), click **Test run**. It asks the questions and shows the note it would make, its name and where the caret would go, without writing anything. A template that doesn't compile is marked **Doesn't run**.

## Add a live query
In Edit on a note or template, click **Insert a query** on the toolbar, fill in the form and watch the preview, then click **Insert**. The block lists tasks or notes from across the vault, such as tasks tagged `#followup` that aren't done, and keeps up to date in View. Start from [Notes](app:notes); [Task and Dataview queries](help:queries) lists what a query can say.

## Choose how notes look
On an open note, **Look:** sets its theme and colour, and the sun or moon shows documents light or dark. For every note, pick a **Document look** in [Settings › Notes](app:settings/notes): Brainstead, Business, Editorial, Modern, Minimal, Report or Technical. **Export PDF…** (`⌘P`) uses the note's look. See [Note](help:note) for more.
