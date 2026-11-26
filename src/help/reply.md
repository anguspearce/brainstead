---
title: Draft reply
kind: screen
screens: [reply]
order: 23
summary: Have the AI draft a reply to an email or Teams thread, drawing on your vault, and put it in Outlook or Teams unsent, or copy it, for you to send yourself.
---
Draft reply reads a thread, checks whether it answered what was asked, and drafts a reply grounded in your vault. Brainstead never sends anything: it leaves the reply in Outlook or Teams unsent, or you copy it there, and you press Send. Open it from **Draft reply** under Assistant in the sidebar, **Draft a reply** in a captured thread's top bar, `D` on an email or chat capture in the [Inbox](app:inbox), ⌘K, or Ask's `/` picker when your vault has a draft-reply skill.

## Choose the thread
Pick a captured Outlook or Teams thread beside **Thread**, newest first, each named by subject and day ("Re Orbit App launch date · Fri 2 Oct"; Teams chats start "Teams:"). Threads come from the capture extensions (see [Sources](app:sources)). For a thread you haven't captured, choose **Paste a thread…** and paste the messages, newest last, with who said what. **Open the capture** opens the captured file.

## Draft the reply
Choose a tone, **Brief**, **Warm** or **Formal**, and click **Draft**. The AI looks up the thread's people and topics in your vault and writes a plain-text reply. Changing the tone drafts it again; **Redraft** redrafts in the same tone.

Under the thread, a verdict says whether the question was answered, partly answered or not, point by point.

## Finish and send it yourself
Edit the draft in place. **To fill in** lists the gaps the reply leaves for you, and a line names the vault pages it drew on.

**Put in Outlook** (on an email) saves the reply, as it stands with your edits, as a reply-all draft on the thread's latest message in Outlook, above the quoted history, and opens it in the browser for you to check and send. It uses the Outlook sign-in [Sync](app:settings/sync) has, so it needs Outlook on the web open, as the sync does. Click it again and you get a second draft.

**Put in Teams** (on a chat the sync brought in) opens the chat in Chrome, and the Teams extension types the reply into its message box, where it waits for you to press Send. It types only once it sees that chat is the one open, never into another chat; check it's the right one anyway. It's also copied, so if the box stays empty (the extension's icon shows `!` and says why), paste it. It needs the Teams extension 1.2.1 or later, and a chat you captured by hand with the extension's button can't be found again this way: copy its reply instead.

**Copy reply** copies plain text; **Copy as rich text** keeps the paragraphs for an email. When the reply commits you to something, **Add task** captures that follow-up as a task, linked to the captured thread.

An assistant can draft a reply for you in the same tones, **Brief** unless you ask for another, and revise it as you ask, as **Ask for a change** does. When you ask, it can also put the reply in Outlook or Teams, as **Put in Outlook** and **Put in Teams** do; never in a run nobody is watching. Nothing is ever sent.

While the thread is still in the [Inbox](app:inbox), a **Still in the Inbox** strip under the reply takes it out without going back: **Done, keep in Sources** (the usual one once you've replied), **Ingest and take out**, or **Move to the Trash**. **Copy reply** also offers **Take it out of the Inbox** in its message.

The draft, the changes asked for and Undo are kept while you work, unless you choose another thread or the thread's own file changes. To change the reply, type what you want in **Ask for a change** under it ("shorter", "mention the 14 November date", "less formal") and press `↩`. It revises the reply as it stands, keeping your own edits, and looks in your vault again if it needs to. Each request shows above the box; **Undo** puts back the reply as it was before the last one. **Redraft** starts afresh. An answer that comes back after you've chosen another thread is dropped.
