---
title: Weekly review
kind: screen
screens: [weekly]
order: 5
summary: A guided GTD weekly review over your real lists, step by step, saved in the week's own review note when you finish.
---
The weekly review walks you through eleven steps in GTD's three phases over your real lists, so you can act on what you see as you go.

## Start or carry on a review
From the review's time on its day until the end of the next day, the To process card on [Today](app:today) shows **Weekly review day** with **Start**. Or start it any time from ⌘K with "Start the weekly review". The day and time (Friday 16:00 by default) are in [Settings › Jobs & schedule](app:settings/jobs).

**Weekly review** in the sidebar, under Projects, opens a start page. It shows:

- the week the review is for;
- when it's scheduled, with **Change**;
- how its suggestions stand: ready, being prepared, or when they'll be prepared;
- a link to this week's review note, once you've finished it.

**Start the review** goes to step 1. Opening the start page prepares nothing.

**Pause** in the top bar goes back to Today and keeps your progress. While paused, the To process card shows **Carry on**, and the start page shows your step, with **Carry on** and **Start over**. Start over drops the progress, notes and decisions and begins at step 1, asking first if there's progress to lose.

A new review covers the week just ended on a Monday or Tuesday, otherwise this week. A paused review keeps its week until the next review is due.

## Go through the steps
The steps are listed on the left by phase; click any to jump to it, which marks the step you were on done. **Next** marks the current step done and goes on, and **Back** returns. The top bar shows the step, about how long is left and a progress bar.

- **Get clear**: Collect loose ends (a capture box), Empty the inbox (its count and Open the Inbox), Process this week's notes (notes whose name carries a date this week, such as `Idea. Pricing - 2026-10-01`, other than meetings; older notes you only edited aren't listed).
- **Get current**: Next actions (tick or change them in place; as in [Tasks](app:tasks), the tasks of on-hold and someday projects are left out), Look back: this week's meetings (meeting, 1-1 and interview notes dated this week), Look ahead: check your calendar, Projects, Waiting for.
- **Get creative**: Someday / maybe, Links to pages not written yet, New ideas (a capture box).

## Suggestions prepared from your week
With an AI assistant set up, Brainstead reads the week ahead of the review and prepares suggestions for the steps. Starting the review prepares them only if the week has none yet. Without an assistant the steps work as normal. The suggestions leave out on-hold and someday projects' tasks, as the step does, except those due or deferred in the next two weeks; the preparation reads the week its own way, so its list of next actions needn't match the step's.

A line under the step's title says when they were prepared, with **Prepare again** for a fresh set. A bar shows while they're being prepared; if it fails you see why, with **Retry**. If the model stops part way, what came before is kept; if nothing can be read, the run's chat in Ask shows what it wrote.

Each suggestion is a row above the step's usual content: what's suggested, the note it came from (hover for the quote, click to open it), an action and **Skip**. The actions are **Add task**, **Tick**, **Defer to** a date, **Waiting for**, **Someday**, **Change wording**, **Clarify as…** and **Link**. Collect loose ends and New ideas show prompts to think about instead.

Nothing changes until you click. An accepted suggestion does the app's usual undoable action, with Undo on the message. A new task goes under Other on the To Do list, or into its project. A link is listed in [Changes](app:review) with Revert. What you accept or skip stays hidden after a pause, and what you accept goes in the review's log. The [Inbox](app:inbox) shows the inbox suggestions too.

In [Ask](app:ask) you can ask for help, such as "go through this week's suggestions with me". Once the review is started, the assistant can read the suggestions, accept or skip one, or prepare them again (for a paused review, its own week).

## Act on what you find
Each step lists the real items with their usual actions:

- **Projects** shows active projects with no next action or nothing for two weeks. Type a next action to add it, or click **Someday** or **Done** to change the status. A project moved to Someday takes its tasks out of Next actions.
- **Waiting for** lists those tasks. **Draft nudges in Ask** asks [Ask](app:ask) to draft a follow-up for each, to copy into Outlook or Teams. Nothing is sent.
- **Someday / maybe** lists those tasks, and someday projects with **Start now**.
- **Links to pages not written yet** lists links to missing pages, with a way to [Knowledge health](app:health) to create or link them.

Brainstead can't read your calendar, so Look ahead asks you to open Outlook and capture what you find.

## Finish and save
Type anything worth keeping in **Notes for this week's review** at any step; it's kept with your progress. On the last step, **Finish and save** writes the week's own note, such as `Me. Weekly Review - 2026-W40`, in the vault's top folder. It holds the week's dates, when you finished, the steps done, the actions you took and your notes. You go back to Today, and **Open** on the message shows the note. ⌘Z undoes the save.

An assistant can do what the buttons do: start, move on, add notes and finish.

The note is yours to edit, move or delete. Finishing the same week again rewrites it, but keeps anything under its **Added later** heading. If you remove that heading, the whole old note is kept under a new **Added later**. The scheduled weekly summary reads the note but never writes to it.

The pane on the right shows this week's weekly summary, once written, with how many notes and meetings are dated this week, and **Your review: open** once you've finished.

## Resize the panes
Drag the line beside the steps, or beside the side column, to resize them; a double-click on it puts it back ([pane widths](help:settings-general)).
