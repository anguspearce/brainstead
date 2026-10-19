---
title: Today
kind: screen
screens: [today]
order: 1
summary: What needs you today: overdue and due tasks, deferred tasks that are back, what you're waiting for, and where you left off.
---
## Read the day at a glance
Under the greeting, one sentence sums up the day: "8 overdue, nothing due today, 2 deferred until now and 4 waiting for others." Overdue and due today are always there; the others only when there are any. Overdue is red, and each count is a link that jumps to its band, which lights up for a moment.

The bands below:

- **Overdue**: a due date before today.
- **Due today**.
- **Deferred until now**: a defer date of today or earlier. A task deferred to a later day stays off Today until then, even when it's due or overdue.
- **Coming up this week**: due in the next seven days, soonest first.
- **Waiting for**: tasks tagged `#waiting-for`, with how long each has waited (amber after a week).

On a wide window, Overdue and Due today are on the left and the other three beside them, so rows stay an easy length; on a narrower one they're one under the other. Empty bands are hidden, and "Nothing due today" shows when nothing is due or deferred. **All tasks** at the foot opens [Tasks](app:tasks). An assistant reading today's tasks gets the same bands and labels.

The first time, reading the vault can take a minute: a banner says so and the cards show "Loading …". If something can't be read, it says "Couldn't load …" with **Retry**.

## Work through today's tasks
Rows work as on [Tasks](help:tasks): tick, right-click or use ⋯, and click the note name to open it. The task-list keys work too; `↩` opens the task's note.

When today's tasks have contexts, chips (**All**, `@calls`, `@office`…) sit at the top of the table. Pick one to show only that context, Waiting for included; the counts follow. Click it again, or **All**, to see everything.

## Chase what you're waiting for
Waiting for rows show how long each has waited, such as "4 d waiting" ("since today" on the first day). The count starts from the task's created date (`➕ 2026-10-01`) and turns amber after a week. A task with no created date shows no count.

**Draft nudge**, on hover or focus, opens [Ask](app:ask) to draft a short follow-up to copy into Outlook or Teams. Nothing is sent.

## Capture without leaving the screen
The box under the counts is the same as Quick capture. Type a task and press `↩` for the To Do list, or press Tab for a Thought in the Scratchpad. `due: fri` and `defer: +3d` become dates as you type.

**Capture** in the top bar opens the floating Quick capture window. **Ask**, beside `?`, starts a chat in [Ask](app:ask) about your day.

## Use the To process card
The To process card shows:

- **Inbox to clarify**, with its count and **Clarify**, which opens the [Inbox](app:inbox). It says when the Inbox is clear.
- **Bookmarks to triage**, with **Triage**, when bookmarks are stale or missing.
- **Wiki issues to decide**, with **Check**, which opens [Knowledge health](app:health). It counts only issues you must decide, not ones a safe fix mends.
- **Weekly review day** with **Start**, from the review's time on its day until the end of the next day (Friday 16:00 unless changed in [Settings › Jobs & schedule](app:settings/jobs)). While a review is paused it shows **Weekly review, paused** with **Carry on**. Either goes straight into the [Weekly review](app:weekly).

## Read the daily and weekly summaries
The **Daily summary** card shows today's or the most recent daily summary, folded to one line: its date, and whether a run is going or when the next is due. Click the line to open or fold it; Brainstead remembers. **Open note** opens the summary note at that day's heading.

The **Weekly summary** card above it works the same way. Both show when **Brainstead runs the daily and weekly summaries** is on in [Settings › Jobs & schedule](app:settings/jobs), or when there's a summary to show.

## Pick up where you left off
**Changes held for you** counts the changes held in [Changes](app:review) and lists the first four; click one, or **Review**, to accept or reject them.

**Continue where you left off** shows up to six notes: unsaved ones first (marked Unsaved, with **Discard**), then notes you opened lately, then notes changed lately here or in another editor. **More in Activity** opens [Activity](app:activity).
