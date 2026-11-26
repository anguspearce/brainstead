---
title: Triage bookmarks
kind: screen
screens: [triage]
order: 22
summary: Go through your bookmarked notes and decide what to do with each one.
---
Triage bookmarks lists every bookmark in `Me. Bookmarks.md`, oldest untouched first, so you can clear the ones that have sat too long. Open it from the **Bookmarks** band at the foot of the [Inbox](app:inbox) (a bookmark there opens Triage on it), **Triage** in Today's To process card, ⌘K's **Triage bookmarks**, or Ask's `/` picker. The sidebar lights **Inbox** while it's open.

## Read the list
Each bookmark shows how many days its note has gone untouched. "stale" means it has sat a long time; **Link missing** means its note is no longer in the vault. Click a bookmark to see its choices.

## Get a suggestion
Click **Suggest** in the top bar (then **Suggest again**) and the AI suggests a choice for each note, with a short reason. The chip beside it counts the bookmarks with a suggestion; click it to open the next one. For **Ingest into the wiki** it names the page and whether it's a concept or an entity; for **Make a task** it words the task. Nothing changes until you choose. On the open bookmark the suggestion has **Accept** (`↩`), **Edit**, to change the page's name or the task's words first, and **Skip**, to drop it and choose yourself.

## Decide
Press a key, or click the choice:

- `K` **Keep**: leave the bookmark. It counts as looked at, so it isn't stale or counted on Today for another two weeks.
- `I` **Ingest into the wiki**: make a new wiki page from the note, listed in [Changes](app:review) with Revert.
- `T` **Make a task**: add a task to the To Do list.
- `A` **Archive**: drop the bookmark; the note stays where it is.
- `⌫` **Remove**: drop the bookmark. The button shows on a bookmark whose note is missing, but the key works on any.
- `↩` **Accept**: do what was suggested. On a bookmark whose note is missing, `↩` removes it.

Every choice but Keep takes the line out of `Me. Bookmarks.md`. Press `⌘Z` to undo. **Open** opens the note. The keys do nothing while a dialog is open or you're typing, and a held key counts once.

An assistant can ask for suggestions too, and changes nothing until you decide. Then it does what the button does (its Make a task captures the task to the Inbox). It can remove a bookmark whose note is gone, and bookmarks only a note that exists.
