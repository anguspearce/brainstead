# Capture extensions

Chrome (and Edge, Brave, Chromium) extensions for Brainstead. Two send what's open in Outlook on the web or Microsoft Teams to Brainstead, which writes it into the vault's `sources/` folder, ready to ingest; the third hands the open Slack tab's sign-in over for Slack sync. Microsoft Graph isn't used.

- [`outlook-capture/`](outlook-capture/README.md): the open email thread, or the threads ticked in the message list (one file each, or one combined file).
- [`teams-capture/`](teams-capture/README.md): the open chat or channel thread, or a meeting's Recap → Transcript, named with the meeting's day when the recap page shows it.
- [`slack-capture/`](slack-capture/README.md): Slack sync's sign-in, handed over from an open Slack tab; it captures nothing itself.

They reach Brainstead by Chrome native messaging: Chrome starts Brainstead's helper (the app's own program, `com.wayned.brainstead`), which writes the file through the same safe write as the app. It works while Brainstead is closed and opens no port. Brainstead registers the helper with each installed browser when it starts, allowing only these three extensions, whose IDs are fixed by the public key in their manifests.

Install: open Brainstead once, then load each folder unpacked on `chrome://extensions` with Developer mode on. Settings › Capture extensions shows whether each is connected and its last capture. After an update, open that page (it refreshes the folder) and click reload on the extension's card in `chrome://extensions`.

Tests: `extensions/**/*.test.js` run under Vitest with the app's (`make check`); `outlook-capture/test-harness/` runs with `node`.
