---
title: Settings › Jobs & schedule
kind: screen
screens: [settings/jobs]
order: 34
summary: The weekly review's day and time and its preparation, the scheduled daily and weekly summaries, the daily check, and recent runs.
---
The page has two parts. **Your weekly review** is something you do. **Summaries Brainstead writes** are done for you: an assistant writes up each day and each week. The weekly summary shows beside your weekly review's steps.

## Set when you do the weekly review
**Weekly review** is when you do the guided [Weekly review](app:weekly): Friday 16:00 by default. It isn't a job: [Today](app:today) reminds you from then until the end of the next day.

## Prepare the weekly review ahead of time
**Prepare the weekly review** reads the week and saves suggestions for each step. It runs 4 hours before the review time, only while **Brainstead runs the daily and weekly summaries** is on, and only if the week has changed since it was last prepared. Its switch turns the schedule off. Starting the review prepares it too if needed.

The row shows the last and next run, **Open** for the last run's chat, and **Run now** (or **Stop**). Its model and effort are **Weekly review preparation** under Models by job in [Settings › AI assistants](app:settings/assistants). Its runs aren't in Recent runs.

## When jobs run
Jobs run while Brainstead is running, even with its window closed. A run missed while the app was closed or the computer asleep catches up when Brainstead next runs. Times use this computer's clock, in 24-hour HH:MM.

## Choose a job's model and effort
Each job's assistant, model and effort are under **Models by job** in [Settings › AI assistants](app:settings/assistants): the summaries, the weekly review's preparation, Find tasks and projects, ingests (the daily check's too), meeting notes, Inbox suggestions (**Clarify suggestions**), and **Contradiction checks**, which also covers the daily check's check and Write Current state. A job's **Effort** is its own: **Assistant's setting** runs it at the assistant's, and a level set for a job never becomes the one a new chat starts with. The weekly review's preparation borrows the summaries' model, not their effort.

## Let Brainstead write the daily and weekly summaries
Only one app should write them, so Brainstead runs them only when **Brainstead runs the daily and weekly summaries** is on. If another app also runs them, switch them off there. Until it's on, the summary rows are greyed. Then switch on:

- **Daily summary**: every day at 06:30 by default.
- **Weekly summary**: Friday at 16:00 by default. This is the AI-written block for the week, not the [Weekly review](app:weekly) you go through yourself.

Brainstead gathers the notes, tasks and Claude Code sessions, the model writes the block, and Brainstead puts it at the top of the month's summaries note. While the vault is read-only, a scheduled summary is held in [Changes](app:review), and **Run now** needs Read-only off in [Settings › Vault](app:settings/vault).

## Hold the summaries for you
With **Hold the summaries for me** on, each summary waits in [Changes](app:review) until you accept it. With it off, they're written at once; undo them in Recent runs or revert them in Changes. A scheduled one that fails a check (the note has unsaved edits, or the vault is read-only) is held instead. One that would change the note's system-note header is always held.

## Run a job now or stop it
**Run now** runs a job at once and opens its chat in [Ask](app:ask). The daily summary covers yesterday; click the day after **for** to pick an earlier one, and **×** to go back. A dated note, such as a meeting note, counts on its own date, not the day it was written up. While a job runs, its button becomes **Stop**. A stopped daily check keeps what it has read for next time.

## Check the wiki every day
**Daily check** (09:00 by default) checks wiki pages changed since the last run for contradictions and makes the fixes (held in [Changes](app:review) when a check fails). It also updates the page list in `index.md`. A bar shows its progress.

To also re-ingest changed sources, or ingest new ones as they arrive, use the **Ingest** switches in [Settings › AI assistants](app:settings/assistants).

## Look back at recent runs
**Recent runs** lists the last eight summary runs:

- **Open** shows the run's chat in Ask.
- **Show** opens the note it wrote.
- **Undo** puts the note back, if it hasn't been edited since. The line in `log.md` stays as a record.

Reverting a run in [Changes](app:review) does the same, and Recent runs marks it undone. An assistant sees the same list and chats, plus the weekly review preparation's last chat.

## Keep your tools' sessions out of the summaries
The summaries put each project's work under **Work done** or **Personal** by its area: area Personal is personal, any other is work, done projects included. A session counts for a project when its folder is named after it (`orbit-app` for Orbit App) or its messages are plainly about it. To move a project's work across, change its area on [Projects](app:projects).

The daily summary counts your Claude Code sessions as your work. If a tool of yours starts sessions itself (a script, another app), list it under **Your tools that start Claude Code sessions** and its sessions count as automated instead.

Each tool has a **Name** and one or both of:

- **Folder contains**: the session's working folder path contains this text.
- **First message starts with**: the session's first message opens with these words, ignoring case.

With both, a session must match both. A row is saved once it has a name and one match; **×** removes it. If your saved tools couldn't load, it says so with **Retry**, and nothing can be changed until they load. An assistant can list, add or remove tools when you ask.

**Folders that look like a tool's** lists folders where, in the last two weeks, at least three sessions ran and nearly all were one prompt and done. Your vault, temporary folders and folders where you hold conversations aren't listed. **It's a tool** adds a folder as a tool named after it; rename it if you like. Brainstead's own jobs are recognised without being listed.
