---
title: Ask your vault
kind: guide
screens: []
order: 7
summary: Chat with an AI assistant about your vault, check its sources, keep chats in tabs, save one and pick a model.
---
Ask is a chat with an AI assistant on this computer. It reads and searches your vault but never edits a file itself.

## Ask your first question
Open [Ask](app:ask) with `⌘6`. Type a question, such as "what did Maya decide about the Orbit App launch?", and press `↩` (`⇧↩` starts a new line). While it works you see what it reads and searches; **Stop** or `Esc` ends the answer where it is. Or click **Ask** beside `?` on most screens to start a chat about what you're looking at.

## Check where an answer comes from
Start a message with `/wiki` to have the answer drawn from the [Wiki](app:wiki) and your sources, citing the passages it rests on. Under any answer, **Copy** copies it, **Save as note** makes it a note, and **File this answer** makes a long answer a new wiki page. Anything an assistant changes is listed in [Changes](app:review), where you can revert it.

## Keep chats in tabs
Each chat is a tab: click **+** or press `⌘T` for a new one. A dot on a tab means an answer you haven't seen. Closing a tab doesn't delete the chat; **History** in [Ask](app:ask) finds, reopens, renames and pins your chats.

## Save a chat to the vault
A chat stays outside the vault until you click **Save** in [Ask](app:ask)'s top bar. It's saved as `Chat. <title>.md` and keeps up to date as the chat goes on; the button becomes **Saved**, which opens the note. Saving needs Read-only off in [Settings › Vault](app:settings/vault).

## Choose the model
Pick the model with the button beside **Send** before your first message. A chat keeps the model it started with, so start a new chat to use another, or click **Ask another model** under an answer. New chats start with the model set in [Settings › AI assistants](app:settings/assistants), which also shows which assistants were found.

## Work in your own repos
Ask works on the vault. To run Claude Code, Codex or a shell in a repo or folder of your own, open the [Workspace](app:workspace) with `⌘7`, add the folder your repos are in, and start a session there. It reads the vault but changes it only through Brainstead, so its changes are in [Changes](app:review) too.

## Ask about Brainstead itself
Ask how to do something in Brainstead, such as "how do I rename a note?", and the assistant answers from this help, saying so when the help doesn't cover it. Try it in [Ask](app:ask) now. What an assistant may change, and what's held for you, is in [Assistants and Brainstead's tools](help:assistants).
