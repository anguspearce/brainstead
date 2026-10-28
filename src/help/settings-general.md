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

## Make everything bigger or smaller
**Interface size** scales the whole window together, from 80% to 150% in steps of 10: the sidebar, lists, buttons, popovers, dialogs and the text of notes. Drag the slider and the window follows; **Reset to 100%** puts it back. `⇧⌘=` and `⇧⌘-` change it from any screen and `⇧⌘0` resets it; on a Mac the **View** menu has the same three. It's kept for this computer, not in the vault, so each machine has its own. A note's [Text size](help:note) (`⌘=` `⌘-` `⌘0`, 12 to 24 px) is separate and comes on top: 120% with 18 px text shows a 21.6 px body. The quick capture and menu-bar windows are sized to what they hold and stay at 100%. An assistant can read and change it when you ask (`interfaceSize`, a percentage).
