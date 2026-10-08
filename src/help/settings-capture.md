---
title: Settings › Capture extensions
kind: screen
screens: [settings/capture]
order: 35
summary: Set up the Outlook and Teams browser extensions that capture emails, chats and transcripts into the vault.
---
Two browser extensions send what's open in Outlook on the web or Teams into the vault's `sources/` folder, ready to ingest and listed in the [Inbox](app:inbox). They work even while Brainstead is closed.

## What each extension captures
- **Outlook capture**: the open email thread in Outlook on the web, or the threads ticked in the list.
- **Teams capture**: the open Teams chat or channel thread, or a meeting's transcript (named with the meeting's day when the recap page shows it).

Captures are refused while the vault is read-only; turn that off in [Settings › Vault](app:settings/vault).

## Install the extensions
1. In Chrome, Edge, Brave, Vivaldi or Chromium, open `chrome://extensions` and switch on Developer mode.
2. Click Load unpacked and choose an extension's folder. **Show in Finder** (**Show in Explorer** on Windows) on its row shows the folder. The first time Brainstead starts on Windows it registers itself for Chrome, Edge, Brave and Chromium, so captures reach it.
3. Pin the extension to the toolbar.
4. Open an email thread or a Teams chat and click the extension's button.

After an update, open this page, then click reload on the extension's card in `chrome://extensions`. Each extension's row shows the version in its folder; the card in `chrome://extensions` shows the version the browser has loaded, so when they match, the reload took. The line under the steps lists the browsers that can reach Brainstead; if none is listed, install one and open this page again.

## Check recent captures
**Recent captures** lists what arrived, or what was refused and why. **Open** opens a captured source. An assistant can read this pane too. To ingest each one as it arrives, switch on **Ingest new sources as they arrive** in [Settings › AI assistants](app:settings/assistants).

## Quick capture lives elsewhere
The `⌃⌥Space` capture box doesn't need the browser; see [Quick capture](help:capture).
