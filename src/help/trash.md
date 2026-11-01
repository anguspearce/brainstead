---
title: Trash
kind: screen
screens: [trash]
order: 17
summary: Files moved to the vault's Trash, to put back or delete for good.
---
Trashed files go to the vault's `.trash` folder, not the system's Trash or Recycle Bin, and stay there until you empty it. Sync clients and other apps that use the folder see them too.

## Move a file to the Trash
Right-click a file in a list and choose **Move to the Trash**. For an open note, it's also on the ⋯ menu, the **Note** menu and `⌘K` (**Move this note to the Trash**). A message offers **Undo**. A Teams transcript's links are first pointed at its meeting note, when it has one, so no page is left linking to a missing file.

System notes can't be moved to the Trash or renamed: the To Do list, the Scratchpad, the bookmarks, the saved searches, the canonical documents register, the daily and weekly summaries' notes, and the vault's `CLAUDE.md`, `index.md` and `log.md`. Their menu items are greyed out. A note that opens with a "This is a system note" callout is one too, and Brainstead refuses to trash it. [A note](help:note) says how their header is locked.

Unsaved changes in a trashed note go into the Trash with it, and come back as its draft if you restore it.

## Find something in the Trash
Newest deletions come first. Each row shows the layer, name, the folder it **Was in**, when it was deleted and its size.
- **Search the Trash** filters by name or folder.
- **Show** in the top bar limits the list to notes, wiki pages, sources or templates, each with its count; in a narrow window the counts go, then it's a drop-down menu.
- Click the eye button, or select one item and press `Space`, to preview a file: Quick Look on a Mac, on Windows the file drawn in the window by its preview handler (Explorer's preview pane's), where it lies in the Trash; `Space` or `Esc` closes it again (after a click into the document, keys go to the document until you click the panel's bar). A type Windows can't preview opens in its own app instead, and so does a file downloaded from the internet, as Explorer's preview pane won't show it either.

## Put files back
**Restore** on a row puts the file back where it was. To restore several, tick them (the header box ticks all) and click **Restore** above the list.

If another file now has the old path, the row says so; **Restore as…** restores to a path you type.

An assistant's restores are listed in [Changes](app:review) with **Revert**; one into `Templates/`, or of a note with code that runs, waits there for you to accept.

## Delete for good
Tick files and click **Delete forever**, or click **Empty the Trash** in the top bar to delete everything (it shows the space freed, unless the window is narrow). Both ask first. No app can restore the files after this; it can't be undone.

## Resize the columns
Drag the left edge of **Type**, **Was in**, **Deleted** or **Size** in the header to resize that column; **Name** takes the rest; a cut-off name or folder shows whole when you rest on it. A double-click on the edge puts the column back ([pane widths](help:settings-general)).
