---
title: Search
kind: screen
screens: [search]
order: 10
summary: Search the whole vault, filter by layer and date, and keep searches you run often.
---
Search looks through the text, titles, headings and tags of every note, wiki page and source. Open it from **Search** in the sidebar, with `⌥⌘6` or `⌘⇧F`, or from `⌘K` with **Search everything**.

## Write a search
Type at least two characters. Words match in any form, so `launch` finds `launching`, and files with more of your words rank higher. Each result shows its best passage, matches marked, and how many passages match.

- `+word` must be there; `-word`, `NOT word` or `-"exact phrase"` mustn't. Both apply to the whole file: `+budget +launch` finds a note with each in a different section.
- `"orbit app"` finds the words together.
- `pilot OR trial` finds either word.
- `budget AND forecast` needs both words.
- `tag:hiring` finds files with that tag or its sub-tags; `-tag:archived` leaves them out.
- `since:2026-09` and `before:2026-10-01` limit by the file's date (or last change if it has none), by day or month.

Clicking a `#tag` in a note searches for it here.

## Narrow and order the results
The **Note**, **Wiki** and **Source** chips choose which layers are searched; at least one stays on. The date menu (**Any time** by default) limits results to the **Past week**, **Past month**, **Past 3 months** or **Past year**, unless your search has its own `since:`.

**Best match** shows the strongest results first; **Latest** and **Oldest** order by date. The line above the results gives the number of files and the time taken; only the best 200 are shown. Snippets are plain text. If the search fails, the error shows with **Retry**.

`↑` `↓` move through the results and `↩` or a click opens one, scrolled to the first match with every match marked. `Esc` clears the box. Right-click a result for the same menu as on [Notes](app:notes).

## Keep a search
Click **Save search**, name it and click **Save**. It's kept, with its layers, in `Me. Smart Lists.md` in the vault and listed under **Saved searches** on the left; click one to run it. The x beside one deletes it (`⌘Z` puts it back). To change one, edit `Me. Smart Lists.md`. **Save search** is greyed out for a search that's already saved.

## Jump anywhere with ⌘K
`⌘K` opens a box over any screen. Type to find:

- **Files**: the best matching notes, wiki pages and sources, with this page's syntax. `Tab` switches between **All**, **Notes**, **Wiki** and **Sources**; narrowed to one, only files and Search everything show.
- **Tasks**, on **All** only: open tasks containing every word typed; choosing one opens its note at the task's heading.
- Screens to go to, and commands such as **New note**, **Rebuild index** or **Light appearance**. On an open note there's also **Rename this note**, **Insert a query…**, **Ask about this note** and **Move this note to the Trash**.
- **Search everything** (`⌘↩`), **Ask** with what you typed, and **Capture … as a task**, which adds it to the To Do list.

`↑` `↓` move, and before you type `←` `→` move across the Go to columns. `↩` runs the selected item and `Esc` closes.
