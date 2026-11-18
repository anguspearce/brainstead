---
title: Settings › Teams and Outlook sync
kind: screen
screens: [settings/sync]
order: 36
summary: Read your own Teams chats, meeting transcripts, Inbox mail and calendar into the vault on their own, without clicking capture.
---
With **Teams and Outlook sync** on, Brainstead reads your own Teams chats, meeting transcripts, Inbox mail and calendar and writes what's new into the vault's `sources/` folder — the same files the [capture extensions](app:settings/capture) make, listed in the [Inbox](app:inbox) and ingested like any capture, but without a button press. It reads only; it never sends, replies or changes anything in Teams or Outlook. macOS and Windows.

## How it signs in
It uses the tokens your open Teams tab already has. While Teams is open in your browser, the Teams extension hands those tokens to Brainstead every few minutes; Brainstead keeps the latest set in your computer's secure store (the Keychain on macOS, Credential Manager on Windows) and uses only those — it never mints its own, so its access never exceeds what your browser already has. So:

- **Keep Teams open in your browser** (Chrome) and the **Teams capture** extension loaded. That's what keeps the tokens fresh.
- **For mail, keep Outlook on the web open too**, with the **Outlook capture** extension loaded. Mail is read with the token your Outlook tab holds, which the Outlook extension hands over every 15 minutes; Teams' own page doesn't reliably keep one that can read mail.
- **The extension keeps a background Teams tab fresh.** It stops Chrome putting the tab to sleep, and when a token the sync uses is about to run out (Teams only renews one when it next needs it), it reloads the tab so Teams fetches new ones, then hands them over. It never reloads a tab you're looking at or one playing sound (a call), and at most once every 45 minutes.
- **Keep Chrome from freezing the Teams tab.** Chrome's Energy Saver (on by default when a laptop is unplugged) freezes background tabs, and a frozen Teams tab can't hand anything over, so the sign-in goes stale on battery. Once: open Chrome's Settings › Performance and add `teams.cloud.microsoft` and `outlook.cloud.microsoft` (and `teams.microsoft.com`, `outlook.office.com`) under **Always keep these sites active**.
- If the last sign-in came from an older copy of the Teams extension than this version of Brainstead ships, the Sign-in card says so: reload the extension at chrome://extensions. The note clears at the next hand-over, within 15 minutes; the card checks every minute while it's open. Chrome only picks up an updated unpacked extension when it's reloaded, and an old copy may not keep the sign-in fresh.
- **The dot in the top bar** shows the sign-in at a glance while the sync is on: green when Teams and Outlook are both connected, amber when something needs you (gone stale, signed out, an old extension), grey when not signed in. Hover over it to see what; click it to come here.
- If one service's token has gone stale, the Sign-in card says which (**Gone stale: transcripts**, say); the rest of the sync carries on. **Gone stale: mail** usually means no Outlook tab is open in Chrome.
- If Microsoft turns a token down (you were signed out of Teams or Outlook, or the session ended), the Sign-in card says **Signed out:** and which services. Sign in again in that Chrome tab; the extension hands the new sign-in over within 15 minutes (reload the extension to do it at once), and the note clears.
- If you close Teams for a while the tokens go stale and the pane shows **Not signed in** — reopen Teams and it comes back within a few minutes.
- It is your own access, used only on this computer. The calls go to Microsoft and nowhere else.

## First sync
The first time it runs — and the first time it sees each new chat — it loads only the last few days of history, so a big mailbox or a long chat history doesn't flood the Inbox all at once. Set how many days with **On first sync, load the last N days** (0 loads everything). After that it only picks up what's new.

## What syncs
- **Chats**: every 1:1 and group chat, one source per day for each, **topped up** in place as that day gains more messages. A file shared in a chat is listed under its message as a link to where it lives in OneDrive or SharePoint; the file itself isn't brought in.
- **Channels**: off unless you choose them. **Show my channels** lists yours; tick the ones to sync.
- **Meeting transcripts**: for meetings in your calendar, fetched after the meeting ends, with anything typed in the meeting chat folded in after the transcript. A transcript becomes a meeting note as usual (Settings › AI assistants).
- **Mail**: one source per email **thread**, holding the whole conversation including your own replies, topped up as the thread grows. A thread that forked (two people answering the same message) is written branch by branch, each under a **Branch** heading saying whose message it answers, so each conversation reads on its own; one that carries on under a new subject starts with a **Continues** link to the earlier thread. A new Inbox message brings its thread in; mail Outlook files under Other, and anything from a sender or domain in **Mail to skip**, are left out. Use **Don't sync this sender** on a synced email (its right-click menu) to add one, or type one here. When you keep moving the same sender's mail, or the same chat, to the Trash (three times or more), **Suggested from what you delete** offers to skip it: **Skip** puts the sender on Mail to skip or the chat on **Chats to skip**, and **Not this one** keeps it syncing and stops the suggestion. Nothing is skipped until you choose; **Remove** on Chats to skip syncs a chat again.
- **Email attachments**: with **Bring in email attachments** on (the default), a PDF, Word, PowerPoint, Excel or text file attached to a synced email, up to 30 MB, is brought into `sources/` as its own source (`Email. Attachment. <name> - <date>`), lands in the Inbox and is ingested like any capture. The thread lists each message's attachments under it, linking to the file by its path in `sources/`, so you, and an assistant reading the thread, can go straight to it. Pictures in the body (signature logos) are left out; other attachments (images, invitations, attached emails, anything over 30 MB, shared OneDrive links) are listed by name with why they weren't brought in. Each attachment is downloaded once, however often its thread grows.

- **After you remove one**: a chat day or email thread you've moved to the Trash (or deleted, or whose Trash was emptied) isn't brought back. If it gets new messages, they start a fresh source (the whole thread, or the whole day) that lands in the Inbox, so nothing new is missed. Attachments already brought in are linked again, not downloaded twice, and one you've removed is listed as removed. A source renamed outside Brainstead counts as removed too, so its next new message starts a fresh copy.

## Slack
With **Slack sync** on (under Slack, in this pane) and Slack open in a browser tab with the **Slack sync** extension loaded (Settings › [Capture extensions](app:settings/capture)), Brainstead reads your own Slack conversations every 15 minutes, read-only, into `sources/`: your DMs and group DMs, the channels you switch on under **Show my channels**, and threads that mention you. Each conversation's day is one source, `Slack. Chat. <name> - <date>`, with each thread's replies under its first message; a thread that gets more replies later updates its day's source rather than starting a new one. The Slack desktop app can stay open alongside the tab. The first sync loads the same number of days as Teams. **Sign out** forgets the Slack sign-in on this computer. Slack chats you keep deleting are suggested for **Chats to skip**, as Teams chats are.

## Keeping the wiki in step
A chat day or a thread that's already been ingested and then gets more messages shows as **Changed** in Sources. With the daily check's **ingest changed sources again** on (Settings › AI assistants), the wiki catches up about once a day rather than re-ingesting on every new message; otherwise ingest it when you like from Sources.

## Sync now
The sync runs on its own every so often. **Sync now** runs a pass at once and says what it added and any problems it hit. A problem (a Microsoft service that didn't answer) is shown and the rest of the pass still runs — it never stops captures.

## Ask an assistant to pull a topic
With sync on, you can ask an assistant (in Ask, or any connected assistant) to gather a topic on demand — for example, “read the last week's emails, Teams chats and meeting transcripts about the Fit programme and write a note on how it's going”. It pulls the matching sources into the vault (reusing what's already there and fetching only what's missing), then reads them to write your note.

## Turning it off
Switch **Sync Teams and Outlook** off to stop it, or **Sign out** to forget the sign-in. The capture extensions still work by hand either way.
