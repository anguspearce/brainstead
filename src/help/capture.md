---
title: Quick capture
kind: screen
screens: []
order: 6
summary: A small window over any app for catching a task or a Scratchpad note in a few keystrokes.
---
Quick capture opens a small box over whatever app you're in. Type, press `↩`, and you're back where you were.

## Open Quick capture
Press `⌃⌥Space` from any app. It works with the main window closed, as long as Brainstead is running. Or click **Capture** in the top bar of [Today](app:today), or type the task in ⌘K and choose "Capture … as a task".

To change it, click the shortcut under Quick capture in [Settings › General](app:settings/general) and press the new keys. **Reset to ⌃⌥Space** puts it back. The pane says if another app uses the keys; ⌥Space and ⌘⌥Space are often taken by launchers.

## Choose Task or Scratchpad note
The box captures a **Task** (for the To Do list) or a **Scratchpad note**; `Tab` or a click below the box switches.

- A task is one line. It goes on the To Do list as `- [ ] your text`, at the top of the `#### Other` section.
- A Scratchpad note can run to several lines (`⇧↩` for a new line). It goes to the Scratchpad as a block headed with the date and time, such as `## 2026-10-03 14:30`, newest first. A line that starts with `#` or `##` is kept as text, written `\#`, so the note stays one block; code blocks are left alone.

Both then wait in the [Inbox](app:inbox) to be clarified. Press `↩` to save, or `Esc` to close without saving.

## Add dates and details as you type
In a task, a few words turn into the task format when you save:

- `due:`, `defer:`, `start:` or `created:` followed by a word (today, tomorrow, mon, next week, +3d, 2026-10-05) become dates. `due:` on its own opens a date picker.
- `@calls` becomes the context `#context/calls`, and `effort:15m` becomes `[effort:: 15m]`.
- `#followup`, `#waiting-for` and `#someday-maybe` put it straight on those lists.

`[[` suggests notes to link and `#` suggests tags; `↑` and `↓` choose and `↩` or Tab inserts. An empty box shows a hint listing these.

An assistant can capture a task or Scratchpad note for you when you ask, with the same shorthand. It's listed in [Changes](app:review), with Revert.

## If a capture is refused
While **Read-only** is on in [Settings › Vault](app:settings/vault), captures are refused with a message and a button to that setting.

It's also refused when a sync conflict copy (such as `Me. To Do List 1.md`) sits beside the file; sort out the copy first.
