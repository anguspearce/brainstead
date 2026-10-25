---
title: Settings › Teams and Outlook sync
kind: screen
screens: [settings/sync]
order: 36
summary: Read your own Teams chats, meeting transcripts, Inbox mail and calendar into the vault on their own, without clicking capture.
---
With **Teams and Outlook sync** on, Brainstead reads your own Teams chats, meeting transcripts, Inbox mail and calendar and writes what's new into the vault's `sources/` folder — the same files the [capture extensions](app:settings/capture) make, listed in the [Inbox](app:inbox) and ingested like any capture, but without a button press. It reads only; it never sends, replies or changes anything in Teams or Outlook. macOS only for now.

## How it signs in
It uses the sign-in your open Teams tab already has. While Teams is open in your browser, the Teams extension hands that sign-in to Brainstead; Brainstead keeps it in the Mac's Keychain and refreshes it itself. So:

- Keep the **Teams capture** extension loaded, and open Teams in your browser at least once a day.
- If the sign-in lapses, the pane says **Not signed in** — open Teams in the browser and it comes back within a few minutes.
- It is your own access, used only on this Mac. The calls go to Microsoft and nowhere else.

## First sync
The first time it runs — and the first time it sees each new chat — it loads only the last few days of history, so a big mailbox or a long chat history doesn't flood the Inbox all at once. Set how many days with **On first sync, load the last N days** (0 loads everything). After that it only picks up what's new.

## What syncs
- **Chats**: every 1:1 and group chat, one source a day for each. A day's messages are written once the day is over, or once the chat has been quiet for a few hours.
- **Channels**: off unless you choose them. **Show my channels** lists yours; tick the ones to sync.
- **Meeting transcripts**: for meetings in your calendar, fetched after the meeting ends. A transcript becomes a meeting note as usual (Settings › AI assistants).
- **Mail**: new messages in your Inbox. Mail Outlook files under Other is skipped, as is anything from a sender or domain in **Mail to skip**. Use **Don't sync this sender** on a synced email (its right-click menu) to add one, or type one here.

## Sync now
The sync runs on its own every so often. **Sync now** runs a pass at once and says what it added and any problems it hit. A problem (a Microsoft service that didn't answer) is shown and the rest of the pass still runs — it never stops captures.

## Ask an assistant to pull a topic
With sync on, you can ask an assistant (in Ask, or any connected assistant) to gather a topic on demand — for example, “read the last week's emails, Teams chats and meeting transcripts about the Fit programme and write a note on how it's going”. It pulls the matching sources into the vault (reusing what's already there and fetching only what's missing), then reads them to write your note.

## Turning it off
Switch **Sync Teams and Outlook** off to stop it, or **Sign out** to forget the sign-in. The capture extensions still work by hand either way.
