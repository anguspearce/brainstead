---
title: A note
kind: screen
screens: [doc]
order: 8
summary: One note open, to read in View or write in Edit and Source, with its links, tasks and outline beside it.
---
A note opens in View; switch to Edit or Source to change it. Brainstead saves only when you ask, and never over a change made in another app.

## Switch between View, Edit and Source
The switch in the top bar has three modes:

- **View** (`⌥⌘1`) shows the note rendered: properties, links, tables, task queries, callouts, diagrams and maths. Diagrams have zoom, fit and full-screen buttons. Click an image to see it fitted to the window, click again for actual size, and press Escape to close. Maths in `$…$` or `$$…$$` is drawn as formulas; amounts like $5 stay as written.
- **Edit** (`⌥⌘2`) shows the markdown hidden except on the line you're editing: headings sized, links as labels, checkboxes and images drawn.
- **Source** (`⌥⌘3`) is plain markdown in a monospaced font, with no toolbar.

A note made from a template opens in Edit, with the caret where the template put it. Sources only open in View. To edit, turn off **Read-only** in [Settings › Vault](app:settings/vault).

## Write in Edit
The toolbar formats text: style and headings, bold, italic, strikethrough, highlight (`⇧⌘H`, colour from the chevron), inline code, lists, task list, quote, code block, divider, link, link to a note, image, table, today's date, and **Insert a query**.

Type `[[` to link a note and `#` for a tag; both suggest from the vault as you type (`[[` alone lists your newest notes, and `#calls` finds `#context/calls`). `⌘`-click a link to open it. Paste or drop an image and it's saved in the vault's `images/` folder and embedded.

Images stay drawn in Edit, including `<img>` tags from other apps. Hover one for its controls:

- Drag a corner to resize it. The width is written into the note (`![[pic.png|400]]`) and used by View, Export PDF and other markdown editors.
- The trash button takes it out of the note (`⌘Z` puts it back; the file stays in `images/`).
- The expand button shows it full size.
- Double-click it to edit its markdown.

Spelling and grammar are checked in Edit and Source: click an underlined word for suggestions, **Learn** or **Ignore**. Turn checking off in [Settings › Notes](app:settings/notes).

## Save, drafts and outside changes
Press `⌘S`, or click **Save** or the **Unsaved · ⌘S** pill. Unsaved text is kept as a draft a second after you stop typing, so nothing is lost if the app quits; reopening the note offers **Restore** or **Discard**.

Leaving a note with unsaved changes asks first: **Stay**, **Keep as draft**, **Discard changes** or **Save**.

If another app changes other lines, the change is merged in and saving keeps both. A change to the same lines shows a banner:

- **Reload**, which asks before dropping your unsaved changes.
- **Keep mine**, which keeps your text as a draft, leaving the file alone.
- **Compare**, which marks the lines that differ on each side.

Nothing is overwritten.

## System notes
System notes, such as the To Do list and the Scratchpad, are ones Brainstead relies on; [Trash](help:trash) lists them. Their header (the "This is a system note" callout) shows a lock and can't be edited here; the rest edits as usual. If another app changes it, [Knowledge health](app:health) offers to put it back.

## Find and replace
Press `⌘F` to find in the note. `↩` goes to the next match, `⇧↩` to the previous one, and `Esc` closes the bar.

In Edit and Source you can also replace: type in **Replace with**, then click **Replace** (or `↩`) or **Replace all**. `⌘Z` undoes it. View only finds.

## Tick tasks and run queries
Tick a task's box in View or Edit. In View with nothing unsaved, it's saved at once (`⌘Z` undoes it); otherwise it joins your unsaved edits. Hover a date icon for its meaning. In Edit, typing `due:`, `defer:`, `start:` or `created:` in a task opens a date picker.

`tasks` and `dataview` code blocks are live lists of tasks or notes. **Insert a query** builds one; see [Task and Dataview queries](help:queries).

## Read callouts and other blocks
When a note's first heading repeats its title, View shows the title once.

In View, a quote starting with `[!type]`, such as `> [!tip] Before you start`, is drawn as a tinted box with an icon and title. `[!type]-` starts folded and `[!type]+` starts open but foldable; click the title to fold it. Tasks inside work as usual.

## Change how the document looks
The row under the note's details has two buttons:

- The sun or moon shows all documents light or dark.
- **Look:** sets this note's theme and colour. **Use the defaults** drops it, and **Defaults…** opens [Settings › Notes](app:settings/notes). The look is kept in Brainstead's settings, not in the note.

Notes, wiki pages, sources and templates share the top bar: View, Edit and Source, Bookmark and ⋯, then Ask and help. ⋯ (or right-clicking the page) has **Graph around this page**, **Read aloud**, **Text size…** and **Focus mode** first, then copying, Export PDF, Reveal in Finder, Rename and Move to the Trash. One that doesn't apply is greyed, and its tooltip says why. While a note is being read aloud, its speaker shows beside ⋯ to pause and carry on.

Some pages add buttons before these: a wiki page has **Refresh from sources**; a captured email or Teams thread has **Draft a reply**; another source has **Doc check**; a template has **Test run** and **Templater docs ↗**.

Text size sets document text from 12 to 24 px for every note. `⌘=` and `⌘-` change it and `⌘0` resets it to 15 px. It's separate from the whole window's [Interface size](help:settings-general) (`⇧⌘=` `⇧⌘-` `⇧⌘0`), which scales this text too.

## Rename, copy, export and more
Rename edits the type, title and date, and shows the new filename and every link it will change; click **Rename & update links**. The note stays in its folder, and a message offers **Undo** (or press `⌘Z`).

**Export PDF…** (`⌘P`) saves a light PDF in the note's look, images included, with the title and page numbers in the footer. **Copy rich text** pastes formatted into email or a document. **Move to the Trash** is on the ⋯ menu and the Note menu, with Undo.

## Focus mode
Press `⌘.` (or **Focus mode** on ⋯, or **Focus Mode** on the Note menu) to give the page the whole window: the sidebar, side pane and details go, the text is centred a size larger, and the top bar keeps only the title, Read aloud, Text size and **Exit focus**. All three modes work. `Esc`, `⌘.` again, or another screen leaves it.

## Read aloud
Choose **Read aloud** on ⋯, or press `⇧⌘P`, to hear the note in View, with each word highlighted. To start from a paragraph, `⌥`-click it or right-click and choose **Read aloud from here**. The player bar at the foot has the controls, speed and voice; reading stops when you leave the note. Set the default voice and speed in [Settings › Notes](app:settings/notes).

- `⇧⌘P` or `F8`: start reading, or play and pause
- `Space`: play or pause
- `F7` `F9`: previous, next paragraph
- `Esc`: stop
- Click a paragraph while reading to read from there; media keys and headphone buttons work too.

## Use the side pane
Beside the note:

- **Linked from**: the notes and pages that link here, with the linking line.
- **Agent changes**: what an assistant or a run changed, each with **Revert**; click one to see it in [Changes](help:review).
- **Open tasks in this note**, to tick from the side.
- **Outline**: the note's headings; click one to jump to it.
- On a 1-1 note named like `1-1. Maya - 2026-10-02.md`, **Last time with Maya**: the previous 1-1 and open follow-ups with them (tasks tagged `#followup/maya`).

The chevron on **Linked from** folds the pane away; **Linked from** in the details row brings it back.
