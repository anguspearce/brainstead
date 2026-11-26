---
title: Activity
kind: screen
screens: [activity]
order: 16
summary: What has changed in the vault: a heatmap of files changed by day, the files of the day you pick, and Sync & ingest, what the sync brought in and what each ingest did.
---
Activity has two views, picked at the top: **Files by day**, a heatmap of the files changed per day with the files of one day under it, and **Sync & ingest**. Open it from **Vault** at the foot of the sidebar.

The vault's `log.md`, the log of Brainstead's own changes, is in [Changes › All](app:review) now, beside every change with **Revert**, so there's one history of what Brainstead and the assistants did. The pie charts and **Most linked** are on [Wiki health](app:health).

## Read the heatmap
Each square is a day, with a column per week from Monday. Darker means more notes, wiki pages and sources last changed that day; hover for the count. Templates aren't counted. The chevrons page to earlier and later weeks.

Under the heatmap is the latest day anything changed, with the files that last changed that day; click one to open it. Click a day to show its files instead, and **Clear the day** to go back to the latest day.

**Changes that day** opens [Changes › All](app:review) on that day: what assistants and Brainstead's runs changed then, and that day's lines in `log.md`. The heatmap counts every file that changed, your own edits too, so it can show more than Changes does.

An assistant reads the heatmap's days, and a day's files, with the `activity` tool.

## Sync & ingest
**Sync & ingest**, beside **Files by day** at the top of Activity, shows what Brainstead does in the background, newest first, over the last two weeks:

- **Each sync pass** that brought something in from Teams, Outlook or Slack: the sources it wrote new and those it topped up (a chat day or email thread with more messages), by service. A pass that found nothing new isn't listed; the line at the top says when the sync last ran.
- **A pass that had a problem or couldn't run**, and why (signed out, read-only). When the same thing happens pass after pass it's one entry with a count and the time of the last one, not a line every few minutes.
- **Each ingest**: the wiki pages it changed, **nothing for the wiki**, **nothing in it, moved to the Trash** (a chat day with nothing in it, which you can put back from [Changes](app:review)), or why it failed. An ingest still going says which step it's on.

The line at the top sums it up, as the menu-bar window does: when the sync last ran, how many sources came in today, how many are being ingested and how many problems there were today. Only the problems are in amber. The chips keep one service (**Teams**, **Outlook**, **Slack**) or **Problems** only. Click a source or page under an entry to open it.

By default Brainstead doesn't notify you about each synced message or each ingest, as Teams, Outlook and Slack have already told you; this view is where to look instead. **Notifications** in [Settings › General](app:settings/general) changes that.

An assistant reads the same timeline with the `sync_activity` tool.
