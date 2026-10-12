---
title: Tasks
kind: screen
screens: [tasks]
order: 3
summary: Every task in the vault, live from the notes it's written in, as GTD lists you can tick, date, filter and reorder.
---
A task is a `- [ ]` line in any note. Tasks lists them all, and each change rewrites only that one line in its note.

## Choose a list
Each list on the left shows its count:

- **Next actions**: open tasks without `#followup`, `#waiting-for` or `#someday-maybe`, and the sidebar's count. Deferred tasks stay off until their day, and tasks still in the [Inbox](app:inbox) until you clarify them.
- **Follow-ups**: tasks tagged `#followup`.
- **Waiting for**: tasks tagged `#waiting-for`.
- **Deferred**: tasks with a defer date, soonest first.
- **Someday / maybe**: tasks tagged `#someday-maybe`.
- **Done this week** and **Done last week**: ticked tasks by done date, weeks starting on Monday.

In Next actions, Follow-ups, Waiting for and Someday / maybe, drag rows by their grip to set your own order (not while grouped). The order is saved at the end of each line as `^rank-1024`, and the list is renumbered when it needs room. An assistant moves a task the same way.

## Use the keys
- `↑` and `↓` choose a task, carrying on into the next list.
- `↩` puts the task's words in the pane to change.
- `Space` ticks or unticks it.
- `⌫` deletes it (`⌘Z` puts it back).
- `/` goes to **Search tasks**.

They work on [Today](app:today), [Projects](app:projects) and the [Weekly review](app:weekly) too, where `↩` opens the task's note, but not in task blocks inside a note.

## Tick, defer and undo
Tick a task to mark it done; it gets a done date (`✅ 2026-10-03`). Untick it to remove it. ⌘Z undoes up to five task changes, even after a restart, as long as the line hasn't changed since.

Right-click a row, or click its ⋯ button, for the task's menu:

- **Due**: Today, Tomorrow, Mon or Next week.
- **Defer until**: Tomorrow, Mon, Next week or 2 weeks. A deferred task stays off Next actions until that day, then shows on [Today](app:today).
- **Pick…** for a calendar, **Clear** to remove.
- **More dates…** shows the start and created dates. A start date doesn't hide the task; it only makes it less urgent until then.
- Priority, context, effort and project, with quick chips and **Find…** or **Other…**.
- **Rename…** changes the task's words in the menu (`↩` keeps, `Esc` goes back); its dates and tags stay.
- **Mark in progress** (its box shows a dot) and **Back to to-do**.
- **Waiting for someone**, **Cancel task** and **Reopen** on a cancelled task.
- **Open** with the note's name, and **Delete task**, which ⌘Z puts back.

## Use the task pane
Click a task to show it in the pane on the right. Click its words to change them (`↩` keeps, `Esc` drops; dates and tags stay). You can also tick it and set its dates, priority, context, effort and project, as in the menu. Hover the ⓘ beside a field for what it means. Context offers the ones in use plus calls, computer, errands, office and personal, or **Other…** for a new one.

**Lives in** shows the note, line and heading; click it to open the note there. The ⋯ button at the foot has **Open note**, **Cancel task** (or **Reopen**) and **Delete task** (⌘Z undoes). **Ask about this** opens [Ask](app:ask) about the task. **Waiting for someone** adds `#waiting-for`; **No longer waiting** removes it.

## Group, filter and save lists
**Group by** in the top bar (**None**, **Project**, **Context** or **Due**) groups the list, with a band and count per group. Grouped lists can't be dragged.

**Search tasks** narrows every list to tasks with all the words you type, in their text, note, heading, project, tags or context. The counts on the left follow, and `Esc` clears it. Context chips (**All**, `@calls`, `@office`…) show one context; click the chip again, or **All**, to show all. The effort menu shows tasks of 15 minutes, 30 minutes or an hour or less.

**Save this list…** keeps a context, effort and grouping under a name, under Saved on the left. Its x forgets it; the tasks stay. An assistant can use the same search, grouping and filters.

## Find tasks in your notes
At the top of **Next actions**, **Find tasks and projects** has an assistant read your last 90 days of notes (not the wiki, sources, templates or project notes) for things you'd do, or are waiting on, that aren't tasks yet. It runs only when you start it, here, in [Projects](app:projects) or from `⌘K`, with progress and **Stop** on its row.

Each suggestion names its note: hover for the quote, click to open it. Brainstead checks the quote is in that note and leaves out anything that's already a task.

- **Accept** adds the task to its project's Next actions, or the To Do list, and lists it in [Changes](app:review), where you can revert it.
- **Edit** lets you reword it first (`↩` adds it).
- **Skip** leaves it out.

Neither comes back. Suggested projects are at the top of [Projects](app:projects). Its model is set under Models by job in [Settings › AI assistants](app:settings/assistants).

## Add a task
**New task** opens the capture box and puts the task under Other on the To Do list, with the list's tag and chosen context: `#followup` in Follow-ups, say, or `#context/calls` with `@calls` chosen. With neither, it waits in the [Inbox](app:inbox); the tooltip says which. As you type:

- `due:`, `defer:`, `start:` or `created:` and a word (today, tomorrow, fri, next week, +3d, 2026-10-05) become dates. `due:` on its own opens a date picker.
- `@calls` becomes the context tag `#context/calls`.
- `effort:15m` becomes `[effort:: 15m]`. Efforts read 15m, 1h, 1h30m or 2d (a day is 8 hours).
- `[[` links a note and `#` adds a tag, with suggestions from the vault.

## How tasks are written
Tasks are plain markdown, so other editors read them too.

- Dates are emoji before the date: `📅` due, `⏳` deferred until, `🛫` start, `➕` created, `✅` done.
- Priority is one emoji: `🔺` highest, `⏫` high, `🔼` medium, `🔽` low, `⏬` lowest.
- A context is a tag like `#context/calls`, and effort a field like `[effort:: 30m]`.
- A task belongs to a project when it's in the project's note or links `[[Project. Orbit App launch]]`.
- A recurring task carries a rule such as `🔁 every week`. Tick it and the next one is added above, with its dates moved on.
- Tasks inside quotes and callouts (`> - [ ] Call Maya`) work like any other.

Rows show these as chips, not emoji; in its project's own note a row shows the project chip, not the note's name. If the tasks can't be read, the list says **Couldn't load the tasks.** with **Retry**.

An assistant can change tasks as you can here; each change is listed in [Changes](app:review) with Revert. A task it adds gets the list's tag, as **New task** does, and one with no project waits in the [Inbox](app:inbox).
