---
title: Projects
kind: screen
screens: [projects]
order: 4
summary: Every project note by status and area, with its next actions, waiting-fors and a flag when one is stuck.
---
A project is a note named `Project. <name>.md`. Its properties hold its status, area and outcome. Its tasks are those written in it plus any task elsewhere that links it. In the daily and weekly summaries, area Personal goes under **Personal** and any other area under **Work done**, done projects included.

## Find a project
**Active**, **On hold**, **Someday** and **Completed** in the top bar switch by status, each with its count. The list groups projects by area (no area last). Under each project's name, a grey line says how many next actions and waiting-fors it has and when the note or any of its tasks last changed. Click a project to open it on the right, and click it again to let it go. Click an area's name to fold its projects away or show them again; what's folded is remembered.

To put areas or projects in your own order, point at a row and drag it by the grip on its left: an area among the areas, a project among its area's projects. **No area** stays last, and areas and projects you haven't moved come after the ones you have, A to Z. The order is kept in Brainstead's settings on this Mac, not in your notes.

To rename an area, point at its band and click the pencil, type the new name and press `↩`. It changes the area on every project under it, whatever its status, undone together with **Undo** in the toast. The area keeps its place in your order, and undoing the rename puts the old name back where it was. A name already in use merges the two, where that area was.

An active project that's stuck says **No next action** in red, and one that's quiet says **Quiet** in amber when nothing has changed for two weeks; an assistant sees the same flags. The sidebar counts active projects, in bold when any has no next action.

## Work on a project
Click a project to open it. Edit its name, **Done looks like** (the outcome) and **Area** in place (each has a pencil): click, type and press `↩`. Renaming the project renames its note, as **Rename** in the file menu does: links to it follow, and **Undo** in the toast (or ⌘Z) puts the old name back. To change anything else in the note, click the pencil on the project's row, or **Edit** under its tasks: the project note opens in Edit.

**Next actions** lists its open tasks. Tick them, drag them into order (an assistant can too), use each task's ⋯ menu or the task-list keys. Type in **Add a next action** and press `↩` to add one under the note's `## Next actions` heading; `@calls`, `effort:15m` and `due:fri` work as in capture.

**Waiting for** and **Someday / maybe** list those tasks when there are any. Each waiting-for has **Draft nudge**, which asks [Ask](app:ask) for a follow-up to copy into Outlook or Teams.

The side shows the project note, notes that link it, wiki pages it links, recent activity and what's done (cancelled tasks struck through).

## Change its status
Under the tasks are **Mark complete** (or **Make active**), **Edit** and ⋯. They change only the note's status:

- **Mark complete** moves it to Completed.
- **Make active** brings it back to Active.
- **Put on hold** (on ⋯) moves an active project to On hold.
- **Someday** (on ⋯) parks it on Someday.

**Ask about this project** (on ⋯) opens [Ask](app:ask) with the project note attached. **Move to the Trash** (on ⋯) says how many tasks are in the note and how many elsewhere link it, then moves it to the [Trash](app:trash). **Undo** in the toast, or restoring it, brings it back; until then, tasks elsewhere link a missing note.

## Find projects in your notes
At the top of the **Active** projects, **Find tasks and projects** has an assistant read your last 90 days of notes for work they keep coming back to that isn't a project yet. Each suggestion has a name, what done looks like, the notes it rests on and up to three next actions. Suggested tasks show at the top of [Tasks](app:tasks).

- **Accept** makes the project note with its first actions, revertable in [Changes](app:review).
- **Edit** changes its name or outcome first. A name can't contain `/ \ : * ? " < > | [ ] # ^` or start with a dot.
- **Skip** leaves it out for good.

## Start a new project
**New project** asks for a **Name**, what **Done looks like** and an **Area** (areas in use are offered). **Make project** writes `Project. <name>.md` with its properties and `## Next actions`, `## Waiting for` and `## Notes` headings. ⌘Z undoes it.

Or start one from ⌘K, or from an [Inbox](app:inbox) item with `P`.

## Link tasks to a project
Add the project's link to a task's line, as in `- [ ] Draft the launch post [[Project. Orbit App launch]]`, or use Project in its ⋯ menu or the task pane on [Tasks](app:tasks). Rows show the project as a chip; click it to open the project.
