# Slack sync — Chrome / Edge extension

Hands your open Slack tab's sign-in to Brainstead, so its Slack sync can read your own Slack conversations into `sources/` (D-20261118-01). It captures nothing itself and has no button to press: it hands over every ten minutes, when the browser starts, and when you click **Hand over now** in its popup.

What it hands over, to Brainstead's local helper only (Chrome native messaging, `com.wayned.brainstead`):

- each workspace's web client token (`xoxc-…`), read from the Slack page's `localConfig_v2` in localStorage (`slack-reader.js`);
- the Slack session cookie (`d`, `xoxd-…`), read with `chrome.cookies`.

Brainstead keeps them in the login Keychain and uses them only to read: your DMs and group DMs, the channels you choose in Settings › Teams and Outlook sync › Slack, and threads that mention you. Each conversation's day becomes `sources/Slack. Chat. <name> - <date>.md`, with each thread's replies under its first message.

Slack only needs to be open in a browser tab; the Slack desktop app can stay open alongside it. Mute the tab's notifications if you don't want each alert twice.

## Install (unpacked)

1. Open Brainstead once, so it registers its helper with your browsers.
2. Open `chrome://extensions`, switch on Developer mode, click **Load unpacked** and choose this folder (Settings › Capture extensions › Slack sync › Show in Finder).
3. Open Slack in a tab at `https://app.slack.com` and sign in.
4. Switch on **Slack sync** in Settings › Teams and Outlook sync.

The extension's ID is fixed by the public key in `manifest.json`, so Brainstead's helper accepts it without anything to copy.

## Files

| File | What |
|---|---|
| `manifest.json` | The extension; its `key` fixes its ID. |
| `background.js` | Hands the sign-in over on a timer. |
| `slack-reader.js` | Reads the workspaces' tokens in the page; tested by `slack-reader.test.js`. |
| `popup.html`, `popup.js` | The last hand-over, and Hand over now. |
