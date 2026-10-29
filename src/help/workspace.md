---
title: Workspace
kind: screen
screens: [workspace]
order: 7
summary: Your own Claude Code, Codex or shell sessions in your repos and folders, which change the vault only through Brainstead.
---
The Workspace runs your own Claude Code, Codex or a shell in any folder or git repo on this computer, each in a real terminal in a tab. A session can't change the vault itself: it changes it only through Brainstead's tools, so every change is in [Changes](app:review) with Revert. Go to the Workspace with `⌘7`.

## Add your folders
Click **Add folder…** at the bottom of the left column and pick the folder your repos are in, such as `~/repos`. With nothing added yet, **Add ~/repos** does it in one click. Each git repo in it is listed with its branch, an amber dot when it has uncommitted changes, and the project it belongs to. Hover over a folder's heading and click its cross to stop listing it; nothing on disk changes.

**Pin** on a folder keeps it at the top under **Pinned**. Folders you've started sessions in are under **Recent**, newest first.

A folder inside the vault is shown greyed: the vault changes only through Brainstead, so use [Ask](app:ask) there.

## Start a session
Click a folder. **New session** offers **Claude Code**, **Codex** or **Shell**. For Claude Code or Codex you can type a first message; for Claude Code you can also name a git worktree to work in, so the session's changes stay off your branch. Click **Start**, or **Open in your terminal** to start the same thing in your own terminal app instead.

Under **Past Claude Code sessions** are the earlier sessions in that folder, newest first. **Resume** picks one up here where it left off; **Resume in your terminal** picks it up in your terminal app.

## Work in a session
Each session is a tab at the top. A spinner means it's working, a blue dot that it's waiting for you, and a grey dot that it finished while you weren't looking. A Codex or shell session can't say how it stands, so it shows a green ring while it runs. The sidebar's **Workspace** shows how many sessions are waiting for you.

Keys you type in the terminal go to the program, so `?`, `Esc` and Ctrl keys don't do what they do elsewhere in Brainstead. On a Mac, `⌘` shortcuts still work; on Windows, Ctrl+Shift ones do.

Closing a tab stops its session. A session that has ended says how, and **Restart** starts it again in its place; a Claude Code session carries on where it left off.

## Beside a session
The panel beside a session shows:

- **Project**: the project this folder belongs to, with its next actions. Click it to open it in [Projects](app:projects). A folder belongs to the project whose **Folders** name it (or a folder it's in), else to the project named like the folder, so `orbit-app` belongs to Orbit App.
- **Files changed**: the files the session has changed. A file in the vault opens; any other copies its path.
- **Wrap up** (Claude Code): asks it to tick off the tasks it did, add new ones, and record what it found and decided in the vault, all through Brainstead's tools.
- **Open in your terminal**: moves the session to your terminal app, where it carries on.
- **Copy command**: the command that starts it in any terminal, guarded the same way.
- **Stop** ends the session's program; **Restart** starts an ended one again.

## Sessions in your own terminal
A session started from **Open in your terminal**, **Resume in your terminal** or a copied command runs guarded the same way: it can read the vault but changes it only through Brainstead's tools. While it runs it shows as a tab here too, with how it stands and the files it changed, but you type into it in your terminal.

## Link a folder to a project
On [Projects](app:projects), a project's **Folders** lists the folders its work happens in. Add one with the folder button, or type a path such as `~/repos/orbit-app` or just a folder's name. A session started in a folder knows its project, so it can record findings and tasks there, and Changes names the folder and project.

## What assistants can do here
An assistant can list the folders and sessions, add and pin folders, start a Claude Code or Codex session, type into one, and stop one, as the screen does. It can't start or type into a session when nobody is watching. A shell session isn't guarded, so an assistant can't start one or type into one: start a shell yourself here. Nor can it type into a session in your own terminal. See [Assistants and Brainstead's tools](help:assistants).
