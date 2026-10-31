---
title: Settings › General
kind: screen
screens: [settings, settings/general]
order: 30
summary: The switch-over checklist, the menu bar and opening at login, the quick capture shortcut, your name and the app's appearance.
---
General holds the settings that affect the whole app. Open Settings with `⌘,`.

## Move over from another notes app
**Moving over from another notes app** lists what's left before you stop using another app on the same vault. It shows only when that app's `scripts/` folder is in the vault. Brainstead ticks some items itself:

- **Brainstead can write to the vault**: read-only is off in [Settings › Vault](app:settings/vault).
- **Brainstead runs the daily and weekly summaries**: on in [Settings › Jobs & schedule](app:settings/jobs).
- **Your tools that start sessions are listed** (optional): so the daily summary doesn't count them as your work. See the [Jobs & schedule help](help:settings-jobs).
- **The Outlook extension captures here** and **The Teams extension captures here**: a capture has arrived from each.
- **The other app's skills and scripts are retired**: its skills (`.claude/skills`) and scripts (`scripts/`) let an agent write to the vault directly, bypassing [Changes](help:review). Once **The other app is stopped** is ticked, **Retire them** copies your name corrections out, then:
  - moves both folders to the [Trash](app:trash), where you can restore them;
  - adds a section to the vault's `CLAUDE.md` telling agents to change the vault only through Brainstead's tools.

  `⌘Z` undoes the `CLAUDE.md` section; restore the folders from the Trash. An assistant can retire them when you ask, but only once **The other app is stopped** is ticked. The checklist stays afterwards.

You tick the rest yourself: the other app's summaries off, its browser extensions removed, and the app stopped. An assistant can read the checklist and tick or untick these three when you tell it.

## Start at login, and the menu bar
- **Show in the menu bar** (on by default) puts Brainstead's icon and small window in the menu bar.
- **Only in the menu bar when the window is closed** takes Brainstead out of the Dock when you close its window. It only appears while **Show in the menu bar** is on, and an assistant can't turn it on otherwise.
- **Open at login** starts Brainstead when you log in. It only appears for the installed app, not one run from a folder. It's the same switch as in the menu-bar window and, on a Mac, System Settings › General › Login Items; on Windows it adds Brainstead to your startup apps. If macOS asks you to approve it, System Settings opens there; until you do, it shows as off.

- **Offer Getting started when Brainstead opens** (on by default) shows a welcome with **Open Getting Started** each time the app opens on a vault. Ticking **Don't ask me again** in it turns this off. The guides are always in the help: press `?` and click **Getting started**.

What the two together do is in [The menu bar](help:menubar).

## Change the quick capture shortcut
The [Quick capture](help:capture) shortcut has a default, shown on its button (Ctrl+Shift+Space on Windows). Click the shortcut button and press the new keys, including at least one modifier. `Esc` or a click elsewhere keeps the old one. **Reset to ⌃⌥Space** puts it back. If macOS won't take a shortcut, a message says why. An assistant can change it when you ask, with the same checks.

## Set your name
Type your first name under **Your name**, as Teams writes it. Today greets you by it. In meeting notes made from transcripts it tells Brainstead which speaker is you, so your actions become tasks and a meeting with one other person counts as a 1-1.

## Choose light or dark
**Appearance** sets the app to **System**, **Light** or **Dark**. Changing it also sets documents back to following the app; to give notes their own light or dark, use [Settings › Notes](app:settings/notes).

## Use the Windows accent colour
On Windows, **Accent colour** is **App** (Brainstead's blue) or **System**, the colour from Windows Settings › Personalisation › Colours. With System, buttons, links, the selected row, the focus ring and checked boxes take that colour in light and dark, and follow it within a few seconds when you change it in Windows, or when you next come back to Brainstead. Where text on it would be hard to read, the colour is made darker or lighter, keeping its hue: a yellow shows as a deep gold. It's kept for this computer. A Mac doesn't show the setting. An assistant can read and change it when you ask (`accent`, `app` or `system`).

## Scrollbars on Windows
On Windows, scrollbars stay out of sight until the pointer is over a list or page, or it scrolls. Then a thin line shows where you are, and it widens under the pointer so you can drag it; it goes again a second after scrolling stops. If you'd rather see them all the time, turn on **Always show scrollbars** in Windows Settings › Accessibility › Visual effects. Brainstead then shows every scrollbar, and notices the change when you come back to its window. There's no setting for it in Brainstead itself. A Mac keeps its own scrollbars.

## Make everything bigger or smaller
**Interface size** scales the whole window together, from 80% to 150% in steps of 10: the sidebar, lists, buttons, popovers, dialogs and the text of notes. **Smaller** and **Bigger** step it and the window follows at once; **Reset to 100%** puts it back. `⇧⌘=` and `⇧⌘-` change it from any screen and `⇧⌘0` resets it; on a Mac the **View** menu has the same three. It's kept for this computer, not in the vault, so each machine has its own. A note's [Text size](help:note) (`⌘=` `⌘-` `⌘0`, 12 to 24 px) is separate and comes on top: 120% with 18 px text shows a 21.6 px body. The quick capture and menu-bar windows are sized to what they hold and stay at 100%. An assistant can read and change it when you ask (`interfaceSize`, a percentage).

## Resize the panes
Every pane can be dragged wider or narrower: the sidebar, a screen's list on the left (Tasks, Inbox, Wiki, Search, Settings, Weekly review, Meeting notes, Contradictions, Compare and Doc check), its list on Projects and Changes, the right-hand pane or column (Today, Tasks, Weekly review, Meeting notes, Sources, Graph, Knowledge health, Activity, a note's **Linked from**, a project's side column), and the columns of every table (Notes, Wiki, Sources, Templates, Trash, Activity's log and Doc check's register). Rest the pointer on the line between two panes, or on a column heading's right edge, and drag; the line shows in the accent colour while you're on it. Each pane has a least and a greatest width, and the panes can never cover one another: when the window is too narrow for them, or the sidebar is wide, the flexible pane gives way first, then the panes at their usual widths, then the dragged ones. A wider sidebar lays a screen out as a narrower window would. A double-click on a line puts that pane back to its usual width, as does `↩` when the line has the focus; `←` and `→` move it 8 px, `⇧←` and `⇧→` 40 px. In a table, one column (**Title**, **Name**, **Item** or **Document**) takes the rest; a column before it resizes by its heading's right edge, and a column after it by its heading's left edge. **Pane widths** here says how many are resized, and **Reset pane widths** puts every one back.

**Lock pane widths** keeps the screens' panes and the sidebar as they are, and **Lock column widths** every table's columns: their lines are hidden and don't answer the pointer or the keys, and the widths you set stay. **Reset pane widths** still works while they're locked. The widths and locks are kept for this computer, not in the vault. An assistant can read them and change one when you ask (`paneWidths`, px by pane; `resizeLocks.panes` and `resizeLocks.columns`); while a type is locked, it's refused a width and told which lock to turn off.
