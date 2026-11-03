---
title: Templates
kind: screen
screens: [templates]
order: 9
page: Write a template
summary: The note templates in the vault's Templates folder, to write, test and make notes from.
---
Templates lists the files in the vault's `Templates/` folder, with their count and what they take on disk on the right. New note offers each one and runs its Templater tags when it makes the note. **Use <% and <%* tags** below explains the tags.

## Make a note from a template
Press `⌘N`, or click **New note** on [Notes](app:notes). Pick a template on the left (`↑` `↓` choose). It shows a summary of its questions and lists and whether it names the note, with **Show code** for the code. Click **Create** or press `⌘↩`.

Questions appear in the dialog as the run reaches them; answer and press `↩`.

- **Cancel** skips a question and the template carries on.
- **Stop** ends the run and makes nothing.

A template that doesn't name its note makes `Untitled.md`. The note opens in Edit with the caret at `tp.file.cursor()`, then the finishing steps (`tp.hooks.on_all_templates_executed`) run. An assistant using a template answers the same questions and runs the same finishing steps.

## Write a template
Click **New template**, type a name and click **Create**. It's made as `Templates/<name>.md`, headed with the note's title, and opens in Edit. A template can hold [query blocks](help:queries) too, which every note made from it runs.

Since a template can run code, anything an assistant does to one (making, changing, renaming or trashing it) is held for you to accept in [Changes](help:review).

Open a template from the list to change it. In Edit and Source the `<% %>` tags stand out, the script inside is coloured as code, and a banner says when it doesn't compile.

Inside a tag the editor suggests completions as you type (keywords, your names, `tp` and its functions, `tR`, `moment`), each with its arguments and a **Docs ↗** link. `⌃Space` opens the list anywhere in a tag.

Hover a name in a tag, or put the caret on it, to see what it is: a keyword such as `await`, a `tp` function, a JavaScript name such as `Math.floor`, or one your template declares. View explains names on hover too; move onto the box to click **Docs ↗**.

## Test a template
On an open template, click **Test run**. It asks the template's questions and shows the note it would make, its name, where the caret would go and any other notes it would create. Nothing is written to the vault.

An assistant can test-run a template the same way first. Other notes a template makes are listed in [Changes](app:review) beside the note. Each note's folder and date are checked as for any new note: not in `sources/`, `wiki/` or `Templates/`, a folder that exists, and a date as YYYY-MM-DD or YYYY-MM.

A template that doesn't compile is marked **Doesn't run**; hover the mark for the error. New note shows the same mark and won't run it.

## Use <% and <%* tags
Templates use Templater's tags, and all the tags in a template are one script:

- `<% … %>` writes the value of what's inside into the note: `<% tp.date.now() %>` puts in today's date.
- `<%* … %>` runs code (declaring names, asking questions, deciding things) and writes only what it adds with `tR += "…"`.
- `<%# … %>` is a comment, left out of the note.
- A `-` or `_` inside the marks (`<%-`, `-%>`) trims the line break, or all the spaces, beside the tag.

A name declared in a `<%*` tag can be used in any tag after it:

```
<%* const who = await tp.system.prompt("Who is it with?") -%>
# Meeting with <% who %>
```

## Know what templates can do
Inside the tags: `tR`, `moment`, and `tp.date`, `tp.file`, `tp.frontmatter`, `tp.config`, `tp.hooks` and `tp.system` (prompts, pick lists and the clipboard). `tp.file.include` runs another template, or one heading or block of it.

`tp.user` runs your own `.js` scripts from `Templates/scripts`, or another folder set in [Settings › Notes](app:settings/notes). Not available: `tp.web` (no internet) and calls into another app's internals; a template using them stops with an error naming the call.

## Start from the examples
A new vault comes with example templates: **Meeting**, **Daily note**, **1-1**, **Project**, **Weekly plan** and **Reading note**, plus a snippet in `Templates/Snippets/` and a script in `Templates/scripts/`. Each one's first line says which features it shows. To add any that are missing, click **Add the example notes** in [Settings › Vault](app:settings/vault).
