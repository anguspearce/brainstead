# Teams capture — Chrome / Edge extension

Sends the open Microsoft Teams **chat / channel thread** *or* **meeting transcript** to Brainstead, which writes it as a markdown file under `sources/`. The filename prefix depends on what the extension detected on the active tab:

| Source | Filename written |
|---|---|
| Chat / channel thread | `sources/Teams. Chat. <title> - <date>.md` |
| Meeting recap → Transcript tab | `sources/Teams. Transcript. <title> - <date>.md` |

A chat's `<date>` is the day it was captured. A transcript's is the meeting's day when the extension finds it on the recap page (`meeting-date.js`: the header's date text, a `<time datetime>`, aria-labels and titles, then the page title, in several English formats; a future date or one whose weekday disagrees is dropped). The file then has a `**Meeting:** YYYY-MM-DD HH:MM` line above `**Captured:**`, which the Meeting notes screen uses for the note's date. Without it, the name has the capture day and the screen asks you to confirm the date.

From there, Ingest on the Sources screen folds it into the wiki.

## Why this exists

Microsoft Graph isn't available, so this extension runs in your already signed-in Teams tab and hands the visible conversation to Brainstead on click: no continuous scraping, no second login, no API.

It's the [Outlook capture](../outlook-capture/README.md) extension ported to Teams. Same shape, same ingest afterwards.

## Install (unpacked)

1. Open Brainstead once: it registers its capture helper with Chrome (and Edge, Brave and Chromium when they're installed). Settings › Capture extensions shows whether it's connected.
2. Chrome → `chrome://extensions` → switch on **Developer mode**.
3. **Load unpacked** → choose this folder (`extensions/teams-capture/`). Settings › Capture extensions has a Reveal in Finder button for it (Show in Explorer on Windows).
4. Pin the extension to the toolbar.

The manifest carries a fixed public key, so the extension's ID is the same on every computer, and Brainstead's helper accepts only that ID (Chrome checks it against the helper's manifest). There's nothing to copy or paste.

## Use

1. Open one of:
   - A chat or a channel thread in Teams web, or
   - A meeting's **Recap → Transcript** tab.
2. Click the toolbar icon → **Ingest current conversation**.
3. The extension **auto-scrolls** the conversation upward in viewport-sized chunks, capturing what's visible at each pass and merging by stable per-message id. It stops when `scrollTop` reaches 0 (top of chat), two passes in a row reveal no new messages, or a 300-second wall-clock timeout fires. Live progress (`Pass 3: 142 messages so far…`) shows in the popup while it's open, and on the toolbar badge while it isn't.

The capture runs in `background.js` (an MV3 service worker), not the popup. Chrome destroys a popup's JS context the moment it loses focus, and a long scroll loop made that easy to hit: the page-side work would finish and then resolve into a context that no longer existed, throwing the whole transcript away. Now closing the popup or switching tabs is safe, and reopening shows where the capture got to. Run state lives in `chrome.storage.session`; the popup is a renderer.
4. The popup shows `✓ Wrote sources/Teams. Chat. <title> - YYYY-MM-DD.md` (or `Teams. Transcript.`) on success. If it couldn't reach the very top within the time budget, the status notes it so you can re-run on a scrolled-up view.
5. In Brainstead, the new file is on Sources, marked New, and in the Inbox.

### Why auto-scroll

Teams **virtualises** the message list in both directions — older messages that scroll off-screen are torn out of the DOM, and newer ones drop out when you scroll up. So there's no DOM state where the whole chat is loaded at once; a complete capture must be multi-pass. The extension does that work for you on a single click, rather than asking you to keep clicking after each manual scroll.

For **transcripts** the scroll direction is reversed — Teams renders the recap chronologically (oldest utterance first, newest at the bottom), so the loop scrolls **down** in viewport-sized chunks and stops when `scrollTop + clientHeight` reaches the bottom of the focus zone (`#scrollToTargetTargetedFocusZone`). The chat path's "Load earlier messages" button click isn't applicable here — the recap pre-virtualises the entire transcript.

### Mode detection

The extractor picks chat vs transcript by DOM markers, not URL — Teams uses client-side routing without updating the address bar on a recap → transcript navigation (the URL stays pinned at `https://teams.microsoft.com/v2/`). Presence of the transcript scroll container (`#scrollToTargetTargetedFocusZone` / `[data-testid="scroll-to-target-targeted-focus-zone"]`) or `[id^="entry-"]` rows triggers transcript mode; otherwise the chat path runs.

### Why the iframe makes this fiddly

Teams loads the meeting Recap → Transcript tab into a sandboxed `about:blank` iframe (`#xplatIframe` / `name="RecapxPlatIframe"`). That breaks two assumptions you'd otherwise rely on:

- **`chrome.scripting.executeScript` with `allFrames: true` doesn't reach about:blank iframes.** Chrome's automatic frame walk skips them; only the top frame gets the injection. The popup works around this by enumerating every frame in the tab via `chrome.webNavigation.getAllFrames(...)` and passing the explicit frame IDs to `executeScript` — that DOES inject into about:blank frames. Requires the `webNavigation` permission in `manifest.json`.
- **The sandbox attribute blocks reciprocal DOM access despite `allow-same-origin`.** The parent frame's content script sees `iframe.contentDocument === null`; the iframe's content script can't read `window.top.document` either. Each frame's extractor runs in its own isolated world. The popup's per-frame result picker handles the resulting split: it picks the transcript-mode result for the messages and borrows the chat-mode result's title (since the meeting title sits in the top frame's DOM, where the iframe can't see it).
- **`host_permissions` must include `<all_urls>`.** That's the only pattern that matches `about:blank`, so it's required for executeScript to be allowed to inject there. Specific Teams patterns stay in the list as documentation, but `<all_urls>` is what actually unblocks the recap iframe.
- **Only Teams' own frames are read.** Because `<all_urls>` would let the extractor run in any frame, `background.js` passes `executeScript` only the top frame, frames on a Teams host, and `about:blank` / `about:srcdoc` frames whose parent is one of those. A third-party frame embedded in Teams (an app tab, a link preview) is never injected and can't supply the capture.

## What it captures

### Chat mode

- **Title** — tries three sources in order: (1) a `TITLE_SELECTORS` match anywhere in the document, but filtered against a UI-chrome blacklist (generic strings like "Message list", "Conversation actions" can't win even though they sit inside the main pane), (2) a derived "Chat with …" name built from the unique senders in the visible messages, (3) `document.title`. We rely only on specific `data-tid` attributes here — generic `role="heading"` selectors are intentionally NOT in the list because Teams uses them for accessibility section headings like "Message list" that would beat the actual chat name. The BFF strips the " - Microsoft Teams" suffix Teams adds, plus filename-illegal characters (`/ \ : * ? " < > |`, control chars, leading/trailing dots), and caps the result at 80 chars.
- The URL of the conversation (so the markdown links back).
- For each message in the visible thread: sender, timestamp (ISO when `<time datetime=…>` is present, otherwise whatever string Teams renders), and the body text.

### Transcript mode

- **Meeting title** — the recap header doesn't expose a `data-tid` on the title, so we heuristic-match: a `span[title][class*="fui-StyledText"]` *outside* the transcript scroll container whose `title` attribute equals its `innerText` (FluentUI's pattern for "show full text on hover when truncated"). Of the matches, the longest one wins. Falls back to `document.title` with Teams chrome stripped.
- The URL (will always be `https://teams.microsoft.com/v2/` for transcripts — Teams' client-side router doesn't update the address bar — but recorded as-is for traceability).
- For each utterance: **speaker** (from the row's `[class*="itemDisplayName-"]` if it has its own header, else parsed from the row's `aria-label`), **time** (short form like `0:14` from the visible `[id^="Header-timestamp-"]` when present, otherwise condensed from the long-form `1 minute 2 seconds` in the aria-label), and **body** (the `[id^="sub-entry-"]` text).
- "Meeting events" — the rows announcing "Maya started transcription", "Recording started", etc. — are skipped by filtering on the `[class*="eventSpeakerName-"]` / `[class*="eventText-"]` markers that distinguish them from utterance rows.

Teams renders the author name AND the timestamp on a **parent** "message group" container, not inside each individual `[data-tid="chat-pane-message"]` wrapper, so a naive `querySelector` from the message wrapper finds nothing. The extractor handles this by building a document-ordered index of every author-header (and timestamp) element up front, then for each message finding the closest preceding entry — falling back to parsing the wrapper's `aria-label` (Teams accessibility labels embed both the author and the time in a comma-separated form like `"Wayne Davies, Yesterday 09:40, …"`), and finally carrying the previous sender / timestamp forward (Teams collapses consecutive messages from the same author into one group with a single visible header).

If the structured extraction finds nothing — selectors stale, or the conversation hadn't fully rendered — the extension falls back to the user's text selection, or `document.body.innerText`. Better to capture an ugly transcript than fail silently.

## When the selectors break

Teams web is React-driven and ships frequent UI updates. If the structured capture stops finding messages or the title:

1. Open Teams, right-click a message (or transcript utterance, or the title) → **Inspect**.
2. Note the wrapping element's selector — usually a `[data-tid="..."]` attribute for chat, an `[id^="..."]` prefix or `[class*="..."]` suffix-stable class for transcripts.
3. Edit `extractor.js` and add the new selector to the relevant list. The lists are grouped by mode:
   - **Chat**: `MESSAGE_SELECTORS`, `AUTHOR_SELECTORS`, `TIMESTAMP_SELECTORS`, `BODY_SELECTORS`, `TITLE_SELECTORS`.
   - **Transcript**: `TRANSCRIPT_ROW_SELECTOR`, `TRANSCRIPT_BODY_SELECTOR`, `TRANSCRIPT_TIME_SELECTOR`, `TRANSCRIPT_SPEAKER_SELECTOR`, `TRANSCRIPT_EVENT_MARKER`, `TRANSCRIPT_SCROLL_SELECTORS`.
4. Reload the extension on the `chrome://extensions` page.

The meeting date reader is a pure function with tests over sample text and HTML (`meeting-date.test.js`, run by Vitest in `make check`). Add a sample there when Teams shows the date a new way.

The selector lists are intentionally fan-out — adding a new one without removing the old ones lets the extension keep working for both old and new versions during a rollout window.

## Message to Brainstead

```
chrome.runtime.sendNativeMessage("com.wayned.brainstead", {
  "type":  "teams",
  "url":   "https://teams.microsoft.com/v2/chat/19:...",
  "title": "Chat with Lena Fischer",
  "kind":  "chat" | "transcript",        // optional, defaults to "chat"
  "meetingDate":  "2026-10-02",          // optional, transcripts: the meeting's day (local)
  "meetingStart": "10:00",               // optional, transcripts: its start (local)
  "messages": [
    { "sender": "Lena Fischer", "date": "2026-05-18T09:40:00Z", "body": "..." },
    ...
  ]
}

→ { "ok": true,
  "filename": "Teams. Chat. Chat with Lena Fischer - 2026-05-18.md",
  "path":     "sources/Teams. Chat. Chat with Lena Fischer - 2026-05-18.md" }
```

`date` is optional on each message. When `kind: "transcript"`:

- The filename prefix is `Teams. Transcript.` instead of `Teams. Chat.`.
- A valid `meetingDate` replaces the capture day in the filename and adds `**Meeting:** <date> <start>` above `**Captured:**`.
- The header line in the rendered markdown is `[Open meeting recap](url)` instead of `[Open in Teams](url)`.
- Per-utterance `date` carries the meeting offset (e.g. `"0:14"` or `"1:05:12"`), not an ISO timestamp.

## Limits / caveats

- **Single conversation per click.** No bulk export, no scheduled scrape — by design. The auto-scroll loop is bounded (300 s wall-clock, 300 passes max) and user-initiated; it's not background scraping.
- **Long chats may not reach the top in one go.** If the 300 s timeout fires before `scrollTop` reaches 0, the popup says so — scroll up to roughly where it stopped and re-run; the new capture will include the older history.
- **Attachments + reactions are ignored.** Bodies only. Mentions and code blocks come through as visible text (formatting flattened).
- **No file-level dedup between captures.** Capturing the same conversation twice on the same day writes a second file with a free name (`… - 2026-08-19-2.md`); nothing is overwritten. Within a single capture, messages are deduped by stable id across passes.
- **Local-only.** Chrome starts Brainstead's helper over native messaging; no port is opened and nothing is reachable from another device.

## Roadmap

- Optional: capture attachment names (not the bytes) so Brainstead's ingest can surface them in the wiki page.
- Optional: preserve mention markup (`@Person`) and code-block fences in the markdown output.
- Optional: a "stop" button in the popup that aborts the scroll loop early (return whatever's captured so far).
