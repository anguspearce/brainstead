---
title: Assistants and the MCP server
kind: guide
screens: []
order: 14
summary: Use the AI assistants already on your computer, in Ask or in Terminal, with Brainstead's tools and the same checks the screens use.
---
Brainstead has no model of its own. It runs the assistants you've installed, signed in with your own account.

## Check which assistants are found
Open [Settings › AI assistants](app:settings/assistants). **Found on this computer** lists Claude Code, Codex, Antigravity and Copilot, with each one's version. If one is missing, **How to install** opens its instructions. Sign in to it in Terminal, then click **Look again**.

## Choose the models
In [Settings › AI assistants](app:settings/assistants), **New chats use** sets the model a new chat in [Ask](app:ask) starts with. **Models by job** gives the summaries, ingests, meeting notes and other background work a model of their own.

## Add Brainstead's tools to a terminal assistant
Brainstead's tools are an MCP server, the standard way to add tools to an assistant. In [Settings › AI assistants](app:settings/assistants), **Brainstead's tools** shows the command for each assistant found; click **Copy** and run it once in Terminal. Ask's chats have the tools already. The assistant can then search, read and change the vault as the screens do.

## Know what an assistant can change
An assistant never edits a file itself: it asks the app, which makes each change with the screens' checks and lists it in [Changes](app:review) with Revert. A change to a template, a system note's header or one adding code that runs is always held for you. **Empty the Trash** and **Delete forever** stay with you. [Assistants and Brainstead's tools](help:assistants) has the full list.

## Give Brainstead Full Disk Access
If your vault is in a cloud-synced folder, open [Settings › Permissions](app:settings/permissions) and click **Open System Settings**. Turn Brainstead on under Full Disk Access, so it and its assistants can read the vault without asking at every turn.
