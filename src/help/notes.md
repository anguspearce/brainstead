---
title: Notes
kind: screen
screens: [notes]
order: 7
summary: Every note in the vault outside the system folders, to search, sort, filter and open.
---
Notes lists every markdown note in the vault except the wiki, sources and templates. Click a row to open it. For live lists of tasks or notes inside a note, see [Task and Dataview queries](help:queries).

## Search the list
Type two or more characters in **Search notes** to search text, titles and tags, with the same syntax as [Search](app:search). Words match in any form, so `launch` finds `launching`.

- `↑` `↓` move between matches and `↩` opens one with the matches marked.
- `Esc` clears the box.
- The switch beside it orders by **Best match** or **Latest**.
- If the search fails, the error shows with **Retry**.

## Sort and filter
Click a column header (Type, Title, Date, File, Modified, Size) to sort; click again to reverse. The list starts newest-modified first. Date comes from the file name, or the `date` or `created` property.

The chips filter by type: **All**, the five commonest types with counts, and **Other** for the rest and untyped notes. A type picked elsewhere, such as from an Activity pie, gets its own chip. **Group by type** bands the notes by type, untyped last. The count shown is on the right.

## Act on a note from the list
Right-click a row for its menu:

- **Ask about this note**, **Ingest into the wiki**, **Bookmark** (or **Remove bookmark**).
- On a source, its actions where they fit: **Make and ingest a meeting note** (a transcript), **Draft a reply** (an email or chat thread) and **Ask about this source**; and **Compare with…**, which opens [Compare](app:compare) with this file as A.
- **Copy markdown**, **Copy path** (the file's full path on disk), **Copy title**.
- **Reveal in Finder** (**Show in Explorer** on Windows).
- **Rename…**, which can update every link to the note, and **Discard draft** when it has one.
- **Move to the Trash** (the vault's `.trash` folder), with Undo. See [Trash](app:trash).

## Make a new note
Click **New note**, or press `⌘N` anywhere. Choose **Blank note** and give it a type, title and date, or pick a template: it shows a summary of its questions, with **Show code** for its code. The filename is `Type. Title - YYYY-MM-DD.md`, and the dialog says whether it's free. Press `⌘↩` or click **Create**.

## Spot unsaved drafts
A note with an unsaved draft shows an amber **Draft** chip by its title.
