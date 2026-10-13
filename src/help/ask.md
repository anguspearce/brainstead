---
title: Ask
kind: screen
screens: [ask]
order: 6
summary: Chat with an AI assistant on this computer about your vault, in tabs; it never writes a file itself.
---
Ask is a chat about your vault with an AI assistant installed on this computer, signed in with your own account. It reads and searches the vault but never edits a file itself. Go to Ask with `⌥⌘7`.

## Ask from any screen
Most screens have an **Ask** button beside `?`. It starts a new chat about what you're looking at: the selected task, project, Inbox item, change, clash or page, or the screen itself when nothing is selected. Its tooltip says which. The question is begun for you ("About the task “Call Sam…”: ") to finish and send.

**Ask about this source**, on a source's ▾ in [Sources](app:sources), a file's right-click menu or `A` on a capture in the [Inbox](app:inbox), starts a new chat begun "About [[the source]]: ". Started from the Inbox, the chat shows a **Still in the Inbox** strip while the capture waits there, to take it out without going back: **Done, keep in Sources**, **Ingest and take out** (**Make and ingest a meeting note** for a transcript) or **Move to the Trash**. A new chat takes the strip away.

## Start and manage chats
Each chat is a tab. Click **+** (New chat) or press `⌘T` for a new one. A dot on a tab means an answer you haven't seen yet, and the sidebar's **Ask** shows how many open chats have one. An answer that arrives while the window is closed counts as unseen, even on the tab you left open, until you're back. Closing the last tab opens a new, empty one. Closing a tab doesn't delete the chat: it stays in **History**, where you can find a chat by title, reopen it, rename it, pin it or move it to the Trash. Saved and pinned chats are kept; closed chats that aren't saved or pinned are deleted once 20 newer ones are in History. The daily or weekly summary shows as a tab marked with a calendar while it runs, without taking you to Ask.

A chat stays in Brainstead's app data folder, out of the vault, until you click **Save** in the top bar. That saves it to the vault as `Chat. <title>.md` at the top, and from then on the note keeps up to date as the chat goes on. The button becomes **Saved**, which opens the note. History marks each chat "saved" or "not saved", with a save button on the unsaved ones. A chat gets a short title after the first answer.

The chats of background runs (the daily and weekly summaries, the weekly review's preparation) stay out of the vault the same way until you save one.

An assistant can save a chat for you when you ask, an open one as **Save** does or a closed one as History's **Save** does.

Save needs the vault to be writable. While it's read-only, a saved chat's new turns are kept in the app data folder and the note is left as it was. The next turn after read-only is off brings the note up to date.

## Choose the assistant and model
Ask works with Claude Code, Codex, Antigravity and Copilot, whichever are installed and signed in. Pick the model with the button beside **Send** before your first message. Each chat keeps the model it started with: to use another, start a new chat. New chats start with the model set in [Settings › AI assistants](app:settings/assistants), which also shows which assistants were found. For Claude Code, the top bar shows how much of the model's context the chat has used.

## Write a message
An empty chat offers three example questions and Brainstead's own skills; click one to type it into the box. With no assistant installed, it says **No assistant found** and what to install.

Press `↩` to send and `⇧↩` for a new line; the line under the box lists the keys. In the message box:

- `/` lists skills and workflows. Brainstead's own, such as `/wiki` (answer from the vault with citations), work with every assistant; the vault's skills in `.claude/skills` run only in Claude Code.
- A skill marked **Opens screen** is one Brainstead does on a screen of its own; picking it goes there.
- `[[` suggests a note to link and `#` a tag.
- `↑` and `↓` step through messages you sent before, in any chat.
- Ask how to do something in Brainstead and the assistant answers from this help, saying so when the help doesn't cover it.
- After an answer, the empty box offers a next message in grey. Press `→` (or `Tab`) to type it in, then change it or send it with `↩`. Turn this off, or choose its model, in [Settings › AI assistants](app:settings/assistants).

## Stop an answer or queue the next message
While the assistant works you see what it is reading and searching. Click **Stop** or press `Esc` to stop the answer where it is. Messages you send while it is answering wait under **Queued** and are sent in order; click the cross on one to take it out of the queue.

## What Ask can and can't change
Claude Code, Codex and Copilot change the vault only through Brainstead's tools, which do what the screens do. Their changes are made at once and listed in [Changes](app:review), where you can revert them; `⌘Z` undoes one straight after. Starting a run, such as an ingest (the AI reading a source into the wiki), happens at once too. A few kinds of change are held for you instead: see [Assistants and Brainstead's tools](help:assistants). An assistant can also read your daily and weekly summaries ("what did I do yesterday?") and work through the [Weekly review](app:weekly) with you.

## Act on an answer
Under each answer:

- **Copy**: rich text where formatting pastes, markdown where it doesn't.
- **File this answer**: makes a long answer a new wiki page (a concept or an entity), listed in Changes.
- **Add tasks** and **Update the wiki**: the assistant adds the tasks or makes the wiki changes the answer supports; each is listed in Changes.
- **Save as note**: the question and answer as a new note, `Ask. <question> - <date>.md`.
- **Ask another model**: the same question in a new chat with a model you pick.
