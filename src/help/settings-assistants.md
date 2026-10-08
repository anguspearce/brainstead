---
title: Settings › AI assistants
kind: screen
screens: [settings/assistants]
order: 33
summary: Which AI assistants Brainstead found, which model chats and jobs use, and the ingest switches.
---
Brainstead doesn't have a model of its own. It runs the AI assistants' command-line tools installed on this computer, signed in with your own account, with the vault as their working folder. They read the vault directly and change it only through Brainstead's tools.

## Choose the model for new chats
**New chats use** sets the model a new chat in [Ask](app:ask) starts with. Each chat keeps the model it started with. Background jobs use this model too, unless one has its own.

## Suggest a next message
With **Suggest a next message** on (on by default), after each answer in [Ask](app:ask) a model reads the last few exchanges and offers a follow-up in the message box; `→` types it in. It's one more call to the assistant per answer, using the model set for **Next-message suggestions in Ask** under Models by job.

## Changes
What assistants and runs change is made at once and listed in [Changes](app:review), where any of it can be reverted. **Keep the history of agent changes** sets how long: 90 days or 500 MB unless you change them, whichever comes first. An assistant can read or set it too, when you ask. If your vault is in a OneDrive folder, older versions of a page are also in OneDrive's version history.

## Ingest
The **Meeting notes** and **Ingest** groups hold the switches about ingesting.

With **Ingest new sources as they arrive** on, an email or chat captured from the browser, or a file dropped or imported into Sources, is ingested straight away, as [Sources](app:sources) would. A Teams transcript isn't: it waits for its meeting note.

Under **Meeting notes**, **Ingest a meeting note once it's made** and **Then move the transcript to the Trash** (both on) are what happens after a note is written from a transcript. With both off, Brainstead asks after each note.

With **Also refresh pages whose sources changed** on, the daily check in [Settings › Jobs & schedule](app:settings/jobs) ingests again each source that changed since the pages citing it were written. What it finds is made at once, and a change that fails a check is held for you in [Changes](app:review).

## Give a job its own model
**Models by job** lets each kind of background work use a different model: **Daily and weekly summaries**, **Weekly review preparation** (the summaries' model unless it has its own), **Find tasks and projects**, **Ingest**, **Meeting notes from transcripts**, **Clarify suggestions**, **Contradiction checks**, **Triage, Draft reply and Doc check**, and **Next-message suggestions in Ask**. Click a job's button and pick a model. **As new chats** puts it back to the model above. Weekly review preparation's button reads **As the summaries** when the summaries have a model of their own, and goes back to theirs.

## See which assistants are installed
**Found on this computer** lists Claude Code, Codex, Antigravity and Copilot, with each one's version, how many models it offers and where it is. If one is missing, **How to install** opens its install instructions in your browser. If one is installed but lists no models, open Terminal, run it and sign in. Click **Look again** after installing or signing in. An assistant can read this list, and look again, too.

## See which vault skills Brainstead now does itself
If the vault has Claude Code skills that Brainstead now does on its own screens, **Skills Brainstead does now** lists them and the screen each opens. Picking one in Ask's `/` list opens that screen. The vault's copies keep working in Claude Code until you retire them with **Retire them** under Moving over in [Settings › General](app:settings/general), which moves them to the Trash.

## Use Brainstead's tools from Terminal
**Brainstead's tools** shows, for each assistant found on this computer, the command that adds Brainstead's tools to it in a terminal (Terminal on a Mac, PowerShell or Windows Terminal on Windows), in every folder. On Windows the command points at the installed Brainstead.exe. **Copy** copies it; run it once for each assistant you use. One installed later shows here after **Look again**. In Ask, Claude Code, Codex and Copilot get the tools already; Antigravity reads with its own tools only. When an image is ingested, Claude Code and Codex see the picture; Copilot and Antigravity get only the text read from it. What each tool does and what waits for you is in [Assistants and Brainstead's tools](help:assistants).
