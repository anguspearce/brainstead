---
title: Activity
kind: screen
screens: [activity]
order: 16
summary: What has changed in the vault: the log of Brainstead's own changes, a heatmap of files changed by day, the notes at a glance, and Sync & ingest, what the sync brought in and what each ingest did.
---
Activity shows the vault's `log.md`, newest first, under a heatmap of files changed per day, with charts of your notes beside it. Open it from the foot of the sidebar.

## Read the heatmap
Each square is a day, with a column per week from Monday. Darker means more notes, wiki pages and sources last changed that day; hover for the count. Templates aren't counted. The chevrons page to earlier and later weeks.

Click a day to filter the log to it. A card lists the files that last changed that day; click one to open it. **Clear the day** shows the whole log again.

## The notes at a glance
- **Notes by type** and **Notes by tag** are pie charts of the five most common types or tags, plus one slice for the rest, with counts in the legend.
- Click a slice or its row (except "other tags"): a type opens [Notes](app:notes) filtered to it, a tag searches for it.
- **Most linked** lists the most linked-to notes and wiki pages; click one to open it.

An assistant can read these counts, and the wiki's from [Wiki health](app:health).

## Read the log
The log lists every change Brainstead makes to the vault: ingests, assistants' changes, Wiki health fixes, the daily and weekly summaries, name fixes and the daily check's catalogue update. Your own edits, captures and dropped files aren't logged.

Entries are grouped by day; click a day's band to show only that day. Click an entry's title to open its page, or hover for its description.

## Filter and search the log
- The chips filter by kind of change, such as **Ingested**, or **Summary** for a daily or weekly summary. **All** shows every kind.
- **Search activity** searches titles, descriptions and kinds; **Best match** or **Latest** orders the matches.
- While searching, `↑` and `↓` move through the matches, `↩` opens the entry's page (or shows its day) and `Esc` clears the box.

An assistant can filter and search the log the same way.

## Resize the panes
Drag the line beside the side column to make it wider or narrower; a double-click on it puts it back ([pane widths](help:settings-general)). The log's **When** (or **Time**) column resizes by its heading's right edge and **Action** by its left edge; **Item** takes the rest.

## Sync & ingest
**Sync & ingest**, beside **Log** at the top of Activity, shows what Brainstead does in the background, newest first, over the last two weeks:

- **Each sync pass** that brought something in from Teams, Outlook or Slack: the sources it wrote new and those it topped up (a chat day or email thread with more messages), by service. A pass that found nothing new isn't listed; the line at the top says when the sync last ran.
- **A pass that had a problem or couldn't run**, and why (signed out, read-only). When the same thing happens pass after pass it's one entry with a count and the time of the last one, not a line every few minutes.
- **Each ingest**: the wiki pages it changed, **nothing for the wiki**, **nothing in it, moved to the Trash** (a chat day with nothing in it, which you can put back from [Changes](app:review)), or why it failed. An ingest still going says which step it's on.

The line at the top sums it up, as the menu-bar window does: when the sync last ran, how many sources came in today, how many are being ingested and how many problems there were today. The chips keep one service (**Teams**, **Outlook**, **Slack**) or **Problems** only. Click a source or page under an entry to open it.

By default Brainstead doesn't notify you about each synced message or each ingest, as Teams, Outlook and Slack have already told you; this view is where to look instead. **Notifications** in [Settings › General](app:settings/general) changes that.

An assistant reads the same timeline with the `sync_activity` tool.
