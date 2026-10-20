---
title: Meeting notes
kind: guide
screens: []
order: 9
summary: Turn a Teams transcript into a meeting or 1-1 note, check its details and let it be ingested into the wiki.
---
The AI writes a meeting or 1-1 note from a Teams transcript in your sources, using your meeting template.

## Bring in a transcript
Capture a meeting's transcript with the Teams extension (set it up in [Settings › Capture extensions](app:settings/capture)), or drop a transcript file onto [Sources](app:sources). It arrives in the [Inbox](app:inbox) too.

## Open Meeting notes
Click **Tools › Meeting notes** in the sidebar, or go to [Meeting notes](app:meeting) from `⌘K`. The list shows the transcripts still to do; select one to read it. From [Sources](app:sources), **Meeting note** on a transcript's row opens it here, or makes the note in one step when Brainstead is sure of everything.

## Check the note's details
Brainstead works out the **Type** (Meeting, 1-1, Workshop or Interview), the **Date** and, for a 1-1, who it was **With**. "check" means it isn't sure: change any of them in **The note** card on [Meeting notes](app:meeting), and click **Confirm** beside a date that's only the capture day. The file name, such as `1-1. Maya - 2026-10-01.md`, shows below. Your own name in [Settings › General](app:settings/general) helps it tell a 1-1 from a meeting.

## Make the note
Click **Make and ingest the note**. The AI fills in `Templates/<Type>.md`, then Brainstead ingests the note into the wiki and moves the transcript to the Trash. To do several, tick them and click **Make and ingest all N**. The note is listed in [Changes](app:review) with Revert.

## Choose what happens after
Turn ingesting the note, or moving the transcript to the Trash, off in [Settings › AI assistants](app:settings/assistants), under Meeting notes. With both off, Brainstead asks after each note. [Meeting notes](help:meeting) explains each state the card can show.
