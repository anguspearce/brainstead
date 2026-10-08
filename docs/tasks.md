# Tasks and GTD

Brainstead runs [Getting Things Done](https://gettingthingsdone.com) (GTD) over the tasks already in your notes. A task is a `- [ ]` line in any note. Brainstead gathers them into lists, and each change rewrites only that one line, so other markdown editors see the same task.

[Today](#today) · [Quick capture](#quick-capture) · [The Inbox](#the-inbox) · [Tasks](#tasks) · [Projects](#projects) · [The weekly review](#the-weekly-review) · [How tasks are written](#how-tasks-are-written)

## Today

On Windows, ⌘ is Ctrl and ⌥ is Alt, so Today's ⌥⌘1 is Ctrl+Shift+1, and the other screen shortcuts follow it up to 0.

<a href="images/index.md#tasks-and-gtd"><picture><source media="(prefers-color-scheme: dark)" srcset="images/today-dark.png"><img alt="Today: the counts, the capture box, one table with Overdue, Due today, Deferred and Waiting for bands, and the right-hand cards" src="images/today-light.png"></picture></a>

Today (⌥⌘1) is what needs you now:

- One table with bands for **Overdue**, **Due today**, **Deferred** (until now) and **Waiting**. A task deferred to a later day stays off Today until then, even if it's due. Tick, date and change tasks in place; ⌘Z undoes.
- `@context` chips to show one context.
- A capture box, the same as Quick capture.
- **To process**: the Inbox, bookmarks to triage, wiki issues to decide, and the weekly review on its day.
- The latest **Weekly summary** and **Daily summary**.
- **Changes held for you** to accept or reject, and **Continue where you left off**.

A waiting-for row shows how many days it has waited. **Draft nudge** drafts a follow-up with Ask to copy into Outlook or Teams; nothing is sent.

## Quick capture

Press ⌃⌥Space in any app (change it in Settings › General).

- Type a task and press ↩. It goes on the To Do list under Other.
- Press Tab for a **Thought**. It goes to the Scratchpad under a date and time heading.
- As you type, `due: fri`, `defer: +3d`, `@calls` and `effort:15m` become the task format. `[[` links a note and `#` adds a tag.

Both kinds wait in the Inbox.

## The Inbox

<a href="images/index.md#tasks-and-gtd"><picture><source media="(prefers-color-scheme: dark)" srcset="images/inbox-dark.png"><img alt="The Inbox: captures, Scratchpad thoughts and tasks to clarify, one open with its choices" src="images/inbox-light.png"></picture></a>

The Inbox (⌥⌘2) lists what you've captured but not decided: emails and Teams chats from the browser extensions, sources added to `sources/` and not yet ingested, Scratchpad thoughts, and tasks under Other on the To Do list. Pick one (`J` and `K` move) and choose what it becomes:

| Key | Choice |
|---|---|
| `N` | Next action, with its project, context, effort and due date |
| `P` | A new project |
| `W` | Waiting for |
| `2` | Done now (under 2 min) |
| `S` | Someday / maybe |
| `R` | File as reference in a note, or keep a capture in Sources |
| `⌫` | Delete |

Tick several items (a section's header box ticks them all) to act on them together: make meeting notes, ingest, keep in Sources, mark done, move to Someday / maybe or delete. One that fails doesn't stop the rest: it stays ticked and the message says why. Next action, Waiting for and New project stay one at a time.

A capture offers only the choices that fit its kind. A transcript: make a meeting note (`M`), ingest (`I`), ask about it (`A`), keep in Sources (`R`) or delete. An email or chat thread: draft a reply (`D`), ingest, ask, Next action, New project, Waiting for, Someday / maybe, keep in Sources or delete. Any other capture: the same without the reply or Waiting for. A capture has no Done now, and only a thread can be Waiting for. **Ask about this source** starts a chat about it in Ask, where a **Still in the Inbox** strip takes it out when you're done. A transcript stays in the Inbox until its meeting note is made and ingested; if either fails it stays, to be chosen again. Suggest only suggests what the Inbox offers for that kind of item. **Suggest for all** suggests a choice for the first 20 items; ↩ accepts one. ⌘Z undoes each choice.

## Tasks

<a href="images/index.md#tasks-and-gtd"><picture><source media="(prefers-color-scheme: dark)" srcset="images/task-pane-dark.png"><img alt="Tasks: Next actions with a task open in the pane on the right, showing its dates, priority, context, effort and project" src="images/task-pane-light.png"></picture></a>

Tasks (⌥⌘3) has the GTD lists: **Next actions**, **Follow-ups**, **Waiting for**, **Deferred**, **Someday / maybe**, and **Done this week** and **last week**. Next actions leaves out deferred tasks until their day, and Inbox tasks until you clarify them.

- Tick to finish a task; a recurring one gets its next occurrence. ⌘Z undoes.
- The ⋯ menu, a right-click or the pane on the right renames a task, sets dates, priority, context, effort and project, marks a task in progress or waiting for, cancels, deletes or opens its note.
- Drag rows into your own order, kept in the file as `^rank-N`.
- Keys: `↑` `↓` choose, `↩` opens, `Space` ticks, `⌫` deletes (⌘Z puts it back), `/` searches. They work on Today, Projects and the weekly review too.
- **Search tasks** narrows every list to tasks matching all its words.
- Group by **Project**, **Context** or **Due**, filter by context and effort, and **Save this list…**.
- **New task** takes the current list's tag and context. From a list with neither, it waits in the Inbox.
- **Find tasks and projects** has an assistant read your last 90 days of notes for tasks and projects you haven't written down. Each suggestion quotes its note; **Accept**, **Edit** or **Skip** it. Accepted ones are listed in Changes with Revert. It runs only when you start it.

## Projects

<a href="images/index.md#tasks-and-gtd"><picture><source media="(prefers-color-scheme: dark)" srcset="images/projects-dark.png"><img alt="Projects: active projects by area with their next actions and waiting-fors, one open on the right" src="images/projects-light.png"></picture></a>

A project is a note named `Project. <name>.md`, with its status, area and outcome as properties. Its tasks are the ones written in it plus any that link it. Projects (⌥⌘4) lists them by status and area.

An active project is **Stuck** (red) with no next action, and **Quiet** (amber) when nothing has moved for two weeks. Add next actions, edit the outcome, area and status, and draft nudges for waiting-fors. **New project** writes the note with its headings.

- Click an area to fold its projects away; what's folded is remembered. Its pencil renames the area on every project under it; it keeps its place in the order, and undoing the rename (the toast's Undo, ⌘Z, Revert in Changes) puts it back where it was.
- Drag an area, or a project within its area, by its grip to put them in order. The order is a Brainstead setting, not written to your notes; ones never moved come after, A to Z.
- Click the project's name on its page to rename it; its note is renamed and links to it follow.
- The pencil on a project's row, or **Edit** on its page, opens the project note in Edit.

In the daily and weekly summaries, area Personal goes under **Personal** and any other area under **Work done** ([Day to day](day-to-day.md)).

## The weekly review

<a href="images/index.md#tasks-and-gtd"><picture><source media="(prefers-color-scheme: dark)" srcset="images/weekly-dark.png"><img alt="The weekly review: the steps by phase on the left, and the current step's real items to act on" src="images/weekly-light.png"></picture></a>

The weekly review walks you through GTD's three phases, Get clear, Get current and Get creative, in eleven steps over your real lists, acting on each item as you go.

- Open it from **Weekly review** in the sidebar, or **Start** on Today from the review's time (Friday 16:00 by default, in Settings › Jobs & schedule).
- **Pause** keeps your place.
- **Finish and save** writes the week's review note. ⌘Z undoes it.

On a Monday or Tuesday it covers the week just ended, otherwise this week. The scheduled weekly summary is separate, and reads your review note.

With an AI assistant set up, each step shows suggestions from your week, such as a task to add from a meeting, or one to tick, defer or reword. Nothing changes until you click, and each one can be undone. Assistants use `weekly_review`, `weekly_suggestion` and `weekly_step`.

## How tasks are written

Tasks follow the widely used Tasks format, so other markdown editors read them:

```markdown
- [ ] Call Sam about the launch date #context/calls [effort:: 15m] 📅 2026-10-06 ^rank-2048
- [ ] Security sign-off from Lena #waiting-for ➕ 2026-09-30
- [x] Send the steerco pack ✅ 2026-10-02
```

- Dates: `📅` due, `⏳` deferred until, `🛫` start, `➕` created, `✅` done, `❌` cancelled.
- Priority: `🔺` highest, `⏫` high, `🔼` medium, `🔽` low, `⏬` lowest.
- Repeat: a rule such as `🔁 every week`.
- A deferred task stays off Next actions until its day. A start date doesn't hide a task; it only makes it less urgent until then.
- A context is a `#context/<name>` tag, and effort a `[effort:: …]` field.
- `#followup`, `#waiting-for` and `#someday-maybe` put a task on those lists.
- A task belongs to a project when it's in the project's note or links it.
- Tasks inside quotes and callouts (`> - [ ] …`) count too.

The app shows dates and fields as icons and chips, never the emoji.
