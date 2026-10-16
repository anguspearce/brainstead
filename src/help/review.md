---
title: Changes
kind: screen
screens: [review]
order: 13
summary: See what AI assistants and Brainstead's runs changed in your vault, revert any of it, and decide the few changes held for you.
---
Every change assistants and Brainstead's runs make to the vault is listed here with **Revert**. You only need to step in when a change is held for you. Open Changes with `⌥⌘5`; the sidebar badge and Today's **Changes held for you** card count held changes only.

## What's made at once, and what's held
A change you started is made at once: something you asked for in Ask or a terminal session, or a run you started by hand (Ingest, **Run now**, **Draft the note**, a Knowledge health fix, **File this answer**, a bookmark made into a wiki page, a link from the weekly review). If a check fails, it's still made, but flagged.

A change from a scheduled run is made at once if it passes every check, and held for you otherwise. Scheduled runs are the daily check (with the ingests and contradiction check it starts) and the daily and weekly summaries when they run on their own. A terminal session nobody is watching, such as a loop, is treated the same way.

The checks:
- A quote isn't found in its source, or the change rests on text read from an image.
- It rewrites or removes text in one of your own notes (anything outside `wiki/`, `sources/`, `index.md`, `log.md` and the summaries). Adding to them is fine.
- The page is open in Brainstead with unsaved edits.
- The page is part of an open contradiction.
- The page's properties wouldn't read any more.
- The vault was read-only when a scheduled change came. (A change you start while it's read-only isn't made at all.)

Three kinds of change are always held, whoever started them:
- a change to a template in `Templates/`, however the assistant spells its path (on Windows, in any case, with `\` or by the folder's short name)
- a change to a system note's header (the "This is a system note" callout)
- a change that adds code that runs when a note is shown (a `dataviewjs` block, inline `$=`, a Tasks `by function` line or Templater's `<% %>`)

Renames and moves to the Trash are made at once, unless they touch a template (the note is one, the rename puts it in `Templates/`, or links it updates are in one): then they're held. An assistant's restore from the Trash is made at once too, unless it puts the file in `Templates/` or the note has code that runs. Revert renames the note back, restores it, or moves a restored one back to the Trash. An image an assistant adds to `images/`, for a note to embed, is made at once and shown here; Revert moves it to Brainstead's Trash.

Assistants' task, Inbox and project edits are listed and held by the same rules; your own edits on those screens aren't listed.

To have the summaries always held for you, turn on **Hold the summaries for me** in [Settings › Jobs & schedule](app:settings/jobs).

## Read the feed
The feed groups changes by run, newest first: "Daily check, 5 Oct: 6 changes to 4 pages", an ingest, an Ask chat. Each row shows the title, page, source, model and age, with **Flagged**, **Reverted** or a kind such as **New page**. `J` and `K` move between changes.

**Search changes** above the list narrows held changes and the feed to those with every word you type in their title, page, source or model. The box shows which match you're on out of how many (`3/12`); `↓` and `↑` step through them, and `Esc` or ✕ clears it.

On the right is the change as a diff under its section, with any flags above it. **What it rests on** lists the quotes behind it, each marked **Found in the source**, **Not found in the source**, or **Not checked** (a file Brainstead can't read). Click a quote's source to open it, or **Open the page**.

## Revert a change
**Revert** undoes a change on the page as it is now, keeping later edits. It works on any change in the history, not just the last. If the change's lines have since been edited, or appear in more than one place, it says so and shows the page as it was before, to copy from. A reverted change is marked **Reverted**; `⌘Z` straight after undoes the revert.

A run with several changes has **Revert all**, after asking: it reverts them newest first, and lists any page edited since (those edits are kept). With a search on, it reverts only the run's changes the search shows. Reverting a meeting note puts its transcript, if it wasn't trashed, back in the [Inbox](app:inbox).

A page's side pane also lists the agent changes to it, under **Agent changes**, each with **Revert**.

## Decide held changes
Held changes are at the top, under **Held for you**, by run, each with why it was held. On a held change:
- **Accept** (`A`) makes it on the page as it is now.
- **Reject** (`R`) turns it down and leaves the page as it is.
- **Edit before accepting** opens the page with the change as an unsaved draft; click **Restore**, edit, then save. If the page already has unsaved edits, save or discard them first. Revert still works after you save.

**Accept all** and **Reject all** (after asking) do every held change in a run, or with a search on only those the search shows; `⌘↩` accepts the run you're on. A meeting note accepted this way gets its follow-up offer; one that couldn't be made stays held. The keys do nothing while a dialog is open or you're typing.

A held change keeps what the assistant asked for (a section's new text, a find and replace, a task to add), not a copy of the page, so it still applies after other edits. If what it changes is gone, it says so: reject it or edit the page yourself.

A held change that rewrites a whole page won't overwrite edits made since it was held: Accept says so; use **Edit before accepting**, or reject it.

An assistant can accept a held change only while you're there, and only one held because a check failed; the rest are yours to decide.

While the vault is read-only, a note says so, with a button to [Settings › Vault](app:settings/vault), and nothing can be accepted or reverted.

## How long history is kept
Changes keeps 90 days or 500 MB of history, whichever comes first; held changes stay until you decide them. Set both in [Settings › AI assistants](app:settings/assistants). If your vault is in a OneDrive folder, older versions of a page are also in OneDrive's version history.
