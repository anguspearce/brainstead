---
title: Meeting note from a transcript
kind: screen
screens: [meeting]
order: 21
summary: Turn a Teams meeting transcript in your sources into a meeting or 1-1 note.
---
The AI writes a meeting or 1-1 note from a Teams transcript in `sources/`, using your meeting template. Open this screen from **Meeting notes** in the sidebar or on [Sources](app:sources), or from ⌘K.

**Make and ingest a meeting note** (**Make a meeting note** when ingesting a made note is off in [Settings › AI assistants](app:settings/assistants)) does it all in one step when Brainstead is sure of the note's type, name and date and no note of that name exists yet: it writes the note, ingests it and moves the transcript to the Trash. Otherwise it opens this screen at the question. Start it from:

- **Meeting note** on the transcript's row in [Sources](app:sources);
- **Make and ingest a meeting note** on the transcript's right-click menu;
- `M` on a transcript capture in the [Inbox](app:inbox).

## Pick a transcript
The list shows the transcripts still to do: those no wiki page cites and no note links. **Show** › **All** shows the rest too, marked "ingested" or "linked from a note". "note exists" means a note of that name is already in the vault, perhaps still empty. Select a transcript to read it in the middle.

Transcripts come from the Teams extension, or a file dropped onto Sources.

## Check the note's details
Brainstead works out the note's **Type** (Meeting, 1-1, Workshop or Interview), its **Date** and, for a 1-1, who it was **With**, from the transcript's file name. "check" in the list means it isn't sure. Your own name, set in [Settings › General](app:settings/general), helps it tell a 1-1 from a meeting.

The date is the meeting's day when the Teams extension found it. When it only has the day the transcript was captured, the list says "check the date" and the note waits: click **Confirm** beside the date, or type the right one.

For a 1-1, a name your earlier 1-1 notes already use for that person is taken as it is. When the transcript's name is none of them and those notes use more than one ("Theo" and "Theo Ward"), they're offered as buttons: click the one for this note, or keep the transcript's.

Change any of them in **The note** card. The file name it will get, such as `1-1. Maya - 2026-10-01.md`, shows below.

## Make the note
Click **Make and ingest the note** (**Make the note** when ingesting a made note is off). The AI fills in the note from `Templates/<Type>.md`, with names spelt as your vault spells them. When the date is only the day the transcript was captured, the button reads **Use <date> and make and ingest the note**: click it if that was the meeting's day, or type the right date first. A missing name or date says what's needed on the button.

To do several at once:

1. Tick them (**Tick all** ticks every one the list shows) and click **Make and ingest all N**.
2. A dialog lists their dates and marks those that are only the capture day. Fix any that's wrong.
3. Click **Make and ingest all N** there. That confirms every date, so there's no need to confirm each one.

## What the card says
The card always says where a transcript is and leads with the step that's left, as its highlighted button:

- **Writing note…** or **Ingesting…**: it carries on by itself, and the transcript leaves the list once it's done.
- **Held in Changes**: the note isn't in the vault yet. **Open Changes** to accept it.
- **Making the note failed** (with why): **Try again**, or **Move the transcript to the Trash**.
- **The note was made, but its ingest failed**: **Ingest the note again**, **Move the transcript to the Trash** or **Open the note**.
- **The note exists already**: **Ingest the existing note** (no new note), **Move the transcript to the Trash** or **Open the note**. If it's ingested already, only the transcript is left: **Move the transcript to the Trash**. Change the name or date for a new note instead.
- **The transcript was ingested itself, or a note links to it** (under **All**): **Move the transcript to the Trash**, or make a note anyway.
- **Read-only is on**: **Open Settings › Vault** to turn it off.

## After the note
The note is made in the vault, listed in [Changes](app:review) with Revert. Brainstead then ingests it into the wiki and, once that's done, moves the transcript to the Trash, first pointing any links to the transcript (and the facts the wiki took from it) at the note, so nothing is left linking to the Trash. A transcript whose note or ingest failed stays where it is.

Turn either step off in [Settings › AI assistants](app:settings/assistants), under Meeting notes. With both off, Brainstead asks after each note: tick what you want and click **Go**, or **Neither**.

A note held in Changes isn't in the vault yet, so the offer comes when you (or an assistant) accept it there. A note is held when a scheduled run's note failed a check, or when it adds code that runs, such as a `dataviewjs` block.

An assistant can list the transcripts as **Show** does and draft a note of any of the four types when you ask. Like **Make the note**, it needs a name and a date (the transcript's topic and today's date don't count), and won't draft a note that exists already or is being drafted; for a note that exists already it can do **Ingest the existing note** instead. A note drafted by an assistant nobody is watching is ingested the same way, with changes that fail a check held for you.
