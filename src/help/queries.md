---
title: Task and Dataview queries
kind: screen
screens: []
order: 8
summary: Put a live list of tasks or notes in any note, with a tasks or dataview code block, and build one without writing it.
---
A query is a code block that lists tasks or notes from across the vault, kept up to date while the note is in View. It's plain markdown, so other apps that run Tasks and Dataview queries read it too.

## Put a query in a note
In Edit or Source, write a code block named after its kind:

````
```tasks
not done
due before next week
sort by due
```
````

Wiki pages and templates can hold queries too. View shows the results; ticking a task there saves it in its own note.

Or use the builder: **Insert a query** on the toolbar, or **Build query…** on a block's first line. Fill in the form and watch the preview. **Insert** adds the block at the caret; **Update** rewrites the one you opened. Lines the form can't show, such as filters joined with AND or OR, are kept as written. In a template, a tag can hold a Templater tag, such as `#followup/<% name %>`.

As you type in a block, the editor suggests what comes next, explains each option and underlines a line it can't read.

## Task queries
A `tasks` block lists tasks, one instruction a line:

- **Filters**: `not done`, `done`, `due before tomorrow`, `scheduled on or after 2026-10-01`, `happens this week`, `tags include #followup`, `tags do not include #someday-maybe`, `path includes Projects`, `heading includes Actions`, `description includes Lena`, `priority is above medium`, `is recurring`. Dates take words (`today`, `next week`, `this month`) as well as dates.
- **Combine them** with brackets and AND, OR or NOT: `(due before today) OR (priority is high)`.
- **Order and group**: `sort by due`, `sort by priority reverse`, `group by filename`, `group by tags`. Brainstead adds `sort by rank`, the order you drag tasks into on the task lists.
- **How much and how**: `limit 10`, `short mode`, `hide backlink`, `hide due date`, and `explain` to show how the query was read.

## Dataview queries
A `dataview` block lists notes or tasks:

```
TABLE file.mtime AS "Changed", status
FROM #project AND "Projects"
WHERE status != "done"
SORT file.mtime DESC
LIMIT 20
```

- **What to show**: `LIST`, `TABLE` with columns (`AS "Name"` names one, `WITHOUT ID` drops the file column), `TASK` for tasks you can tick, or `CALENDAR file.day`.
- **FROM**: a `#tag`, a `"folder"`, `[[a note]]` (the notes linking to it) or `outgoing([[a note]])`, joined with AND or OR, and `-` to leave one out.
- **Then**: `WHERE` to filter (`!completed`, `contains(tags, "#followup")`, `file.mtime > date(today) - dur(7 days)`), `SORT`, `GROUP BY`, `FLATTEN` and `LIMIT`.
- **Fields**: a note's properties, inline fields written `key:: value`, and the file's own (`file.name`, `file.mtime`, `file.tags`, `file.day`…).

Inline queries go in the text: `` `= this.file.name` `` writes a value in place.

## Scripts in queries
`dataviewjs` blocks, inline `` `$= …` `` and Tasks' `filter by function` run JavaScript, only in your own notes. Not in sources, wiki pages, templates, saved chats, summary notes, or notes an assistant wrote (`created-by: assistant`; delete it to trust the note). There a short notice stands in for the script; plain queries still work.

An assistant's change that adds a script or changes one, or any change to `Templates/`, is always held for you in [Changes](help:review).
