---
title: Scheduled jobs
kind: guide
screens: []
order: 15
summary: Let Brainstead write the daily and weekly summaries, prepare the weekly review and check the wiki each day.
---
Jobs run while Brainstead is running, even with its window closed. One missed while the computer slept catches up next time.

## Turn on the summaries
Open [Settings › Jobs & schedule](app:settings/jobs) and turn on **Brainstead runs the daily and weekly summaries**. Then switch on **Daily summary** (06:30) and **Weekly summary** (Friday 16:00), and change the times if you like. They need Read-only off in [Settings › Vault](app:settings/vault).

## Hold the summaries for you
To read each summary before it goes into the vault, turn on **Hold the summaries for me** in [Settings › Jobs & schedule](app:settings/jobs). Each one then waits in [Changes](app:review) until you accept it.

## Prepare the weekly review
In [Settings › Jobs & schedule](app:settings/jobs), set **Weekly review** to the day and time you do it. **Prepare the weekly review** then reads the week 4 hours before and saves suggestions for each step of the [Weekly review](app:weekly).

## Check the wiki every day
Turn on **Daily check** in [Settings › Jobs & schedule](app:settings/jobs). Each day it checks the pages that changed for contradictions and makes the fixes; a fix that fails a check is held in [Changes](app:review). To refresh pages whose sources changed too, use the **Ingest** switches in [Settings › AI assistants](app:settings/assistants).

## Run a job now, or undo one
**Run now** runs a job at once and opens its chat in [Ask](app:ask); while it runs, the button becomes **Stop**. **Recent runs** in [Settings › Jobs & schedule](app:settings/jobs) lists the last summaries, with **Open**, **Show** and **Undo**.
