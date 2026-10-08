# Outlook capture — Chrome / Edge extension

Sends Outlook on the web threads to Brainstead, which writes them as markdown under `sources/`. One click captures the open thread; a second button captures every thread you have **checked** in the message list, either as one file each or as a single combined document. From there, Ingest on the Sources screen folds them into the wiki.

## Why this exists

Microsoft Graph isn't available, so this extension runs in your already signed-in Outlook tab and hands threads to Brainstead on click: no background polling, no scheduled job, no second login. Multi-thread capture is bounded (25 threads at most, 10 by default) and runs in the foreground with the popup open, so it stays a deliberate action rather than an export.

## Install (unpacked)

1. Open Brainstead once: it registers its capture helper with Chrome (and Edge, Brave and Chromium when they're installed). Settings › Capture extensions shows whether it's connected.
2. Chrome → `chrome://extensions` → switch on **Developer mode**.
3. **Load unpacked** → choose this folder (`extensions/outlook-capture/`). Settings › Capture extensions has a Reveal in Finder button for it (Show in Explorer on Windows).
4. Pin the extension to the toolbar.

The manifest carries a fixed public key, so the extension's ID is the same on every computer, and Brainstead's helper accepts only that ID (Chrome checks it against the helper's manifest). There's nothing to copy or paste.

## Use: one thread

1. Open a thread in Outlook on the Web.
2. Click the toolbar icon → **Ingest current thread**.
3. The popup shows `✓ Wrote sources/Email. Thread. <subject> - YYYY-MM-DD.md` on success.
4. In Brainstead, the new file is on Sources, marked New, and in the Inbox.

## Use: several checked threads

1. In the inbox, a folder, or a search-results list, tick the checkbox on each thread you want. (Checkboxes appear on hover at the left of each row; `Ctrl`/`Cmd`-click a row works too.)
2. Click the toolbar icon. The **Selected threads** section reports how many it found. Opening the popup scans the list: it scrolls the message list top to bottom to find checked rows below the fold, then returns the scroll position to where you had it. You'll see the list move briefly. That scan is the only way to see them, because OWA removes off-screen rows from the DOM entirely.
3. Choose **One file each** or **One combined file** (name it if you want; without a name the file is called `Email. Threads. <first subject> +N more - <date>.md`).
4. Set the cap if 10 isn't right, then click **Ingest N selected threads**.
5. Each thread reports as it lands. Failures are listed individually and don't abort the run.

**You can close the popup.** The run belongs to the service worker, so switching tabs, switching windows or closing the popup doesn't interrupt it. The toolbar badge shows `3/9` while it works and `✓` (or `2!`) when it's done; reopening the popup shows the live log. Only closing the browser, or Chrome killing the worker, stops a run — and a run interrupted that way is reported as such, with whatever it already wrote left in `sources/`.

Not every row in a mail list is a conversation. Meeting invitations render a calendar card, and SharePoint / OneDrive "shared with you" mails render an actionable-message card; neither has the message-body DOM the thread extractor reads. Those are still captured, as the reading pane's visible text, and are filed as **`Email. Item. <subject> - <date>.md`** with a `**Capture:**` line in the file saying so. Calling a meeting invite a "Thread" in the file list would be a lie, hence the different prefix.

Because checkbox selection only exposes a ~30-word preview per row, the extension has to open each thread to read its bodies. So a batch run:

- **clears your checkbox selection** (OWA drops it the moment a row is clicked),
- **marks the captured threads read**,
- leaves the last thread of the batch open in the reading pane,
- takes roughly 3-6 seconds per thread.

None of that is recoverable by the extension, which is the honest reason for the cap: a run you abandon half-way still cost you your selection.

## Where the work runs

`background.js` (an MV3 service worker) owns every run; `popup.js` only sends commands and renders state.

That split is not decoration. Chrome destroys a popup's JS context the moment it loses focus, so when the walk lived in the popup, switching tabs mid-run killed it: the remaining threads went uncaptured, and in combined mode every thread captured so far was discarded, since that mode posts once at the end. Even a single capture was lost that way, because `await chrome.scripting.executeScript(...)` resolved the transcript into a context that no longer existed.

The worker's run state lives in `chrome.storage.session` and is the single source of truth, so the popup renders the same thing whether it was open throughout, opened half-way, or opened after the run finished. Progress is also broadcast for a popup that happens to be listening (it rejects harmlessly when none is) and mirrored onto the toolbar badge for when it isn't.

Two details worth keeping if you touch it:

- **No `chrome.alarms` keepalive.** A service worker idles out after 30s, but each `chrome.*` call resets that timer and an in-flight one holds it open. This loop is awaiting `executeScript` or a localhost `fetch` almost continuously, with a worst-case gap of one `openRowAndSettle` (~8s), so an alarm would buy nothing and cost a permission.
- **`active` is the concurrency authority, and it is assigned synchronously.** Guarding on `status === "running"` read back from storage is a check-then-act across an `await`; two quick clicks both passed it and two runs interleaved over the same tab. `test-harness/test-worker.mjs` covers this.

## Titles, and why the list row wins

Subject extraction tries four sources in order and reports which one it used (`titleSource`): the quoted chain's `Subject:` line, a heading inside the focused message wrapper, the first short heading in the reading pane, then `document.title`.

That last one is a weak fallback, and it used to be documented here as "the focused-thread subject". It isn't, in this build: `outlook.office.com` sets the title to `Inbox - <mailbox> - Outlook`, so any mail with no quoted reply chain produced a file called `Email. Thread. Inbox - <mailbox> - <date>.md`, and several such mails in one batch collided into `… -2.md`, `… -3.md`. The batch flow therefore overrides the title with the **list row's** subject whenever `titleSource` is `document-title`, since the row always has it. The single-thread button has no row to read, which is why the reading-pane heading source was added.

## What it captures

- **Subject** — tries three sources in order: (1) a `Subject:` line from the quoted-chain headers in the message history (always present in any reply/forward thread, parsed out of the history blob), (2) a heading inside the focused message wrapper via `SUBJECT_SELECTORS` (scoped to the wrapper, never document-wide — OWA's sidebar has its own `role="heading"` elements like "Navigation pane" that would otherwise win), (3) `document.title` as last resort, which OWA reliably sets to the focused-thread subject. Leading `Re:` / `Fw:` / `Fwd:` / `Forward:` prefixes — stacked or not — are stripped after extraction. The result is used both as the markdown heading and to derive the filename, with the BFF stripping filename-illegal characters (`/ \ : * ? " < > |`, control chars, leading/trailing dots) and capping at 80 chars.
- The URL of the thread (so the markdown links back).
- For each message in the conversation: sender, recipients (when present), date (when present), and the body text.
- **Non-conversations:** a meeting invite or card-only mail yields `kind: "reading-pane"` and one message whose body is the reading pane's text. Scoped to the reading pane, *not* `document.body.innerText`, which would drag in the navigation pane, folder list and every visible row of the message list.
- **For a batch:** the checked rows from the message list, read via `list-extractor.js`. `[role="option"]` rows carry `aria-selected="true"` when checked and a stable `data-convid` used to re-find each row after the previous click re-rendered the list. Ordering comes from DOM order, because `aria-posinset` is unreliable (a three-row selection reports `1`, `0`, `0`). OWA also appends `"N conversations selected"` to the focused row's aria-label; the popup cross-checks that against the rows it can see and warns when they disagree.

If the structured extraction finds nothing — e.g. OWA rolled out a new DOM and the selectors are stale — the extension falls back to whatever you have selected on the page, or the document's visible text. Better to capture an ugly transcript than fail silently; you can then update the selectors in `extractor.js`.

## Testing without OWA

`test-harness/` holds two manual tests that drive a fake, virtualised OWA list in a real Chrome. They exercise the shipped source (`list-extractor.js` verbatim, and `openRowAndSettle` lifted out of `popup.js` by source slicing), so they can't drift into testing a copy:

```
node extensions/outlook-capture/test-harness/test-scan.mjs     # counting across the fold
node extensions/outlook-capture/test-harness/test-walk.mjs     # clicking a row below the fold
node extensions/outlook-capture/test-harness/test-worker.mjs   # the run outliving the popup
```

`test-worker.mjs` needs no browser at all: it loads `background.js` into a `node:vm` with a fake `chrome`, and every progress broadcast **rejects** — which is exactly what happens with no popup listening. A run that completes under those conditions is the property we care about. It also covers the per-thread cap, one bad row not aborting the rest, a refusing BFF, the double-start race, and an interrupted run being reported rather than left spinning.

They are not part of `npm test` because the behaviour under test needs real layout — `scrollHeight`, `clientHeight` and `innerText` are all meaningless in jsdom — and because `puppeteer-core` isn't a dependency of this repo. It's resolved from the globally-installed `openwolf` package, or `PUPPETEER_DIR`; Chrome comes from `CHROME_PATH` or a platform default. Run them after any OWA DOM change, before blaming the selectors.

## When the popup says nothing at all

If the popup sits on **"Checking the message list…"** and the *Ingest selected threads* button stays greyed out, that is **not** a selection problem, and no amount of re-ticking checkboxes will fix it. That string is the static placeholder in `popup.html`; the moment `scan()` starts it is replaced with "Scanning the message list for checked threads…". Seeing the placeholder means the popup never got as far as the scan.

Everything in the popup goes through the service worker, and `init()` awaits `owa:get-state` before anything else. If the worker does not answer, that await is where it stops. Two ways in:

1. **The worker isn't running** - failed to register, or errored on startup. `sendMessage` rejects with "Could not establish connection. Receiving end does not exist."
2. **`readState()` threw** - e.g. `chrome.storage.session` unavailable in that context.

Both used to be silent: `owa:get-state` was `void readState().then(respond)` with no rejection path, so `respond` was never called after the listener had already returned `true`; Chrome then closes the channel and rejects the popup's `sendMessage`, which `void init()` swallowed. The popup now catches it and says so, naming the underlying error. `test-harness/test-worker.mjs` covers the "answers even when reading state throws" contract.

To diagnose the worker itself: open the extensions page (`chrome://extensions`, or `edge://extensions`), find this extension, and click **service worker** to open its console. Errors there are the root cause. **Reload** re-registers it.

## When the selectors break

OWA is React-driven and ships frequent UI updates. If the structured capture stops finding messages:

1. Open the thread in OWA, right-click a message → **Inspect**.
2. Note the wrapping element's selector — usually `[role="document"]`, `[role="article"]`, or a `[data-testid="..."]`.
3. Edit `extractor.js` and add the new selector to `MESSAGE_SELECTORS` / `SENDER_SELECTORS` / `BODY_SELECTORS` / `DATE_SELECTORS` / `RECIPIENTS_SELECTORS`.
4. Reload the extension on the `chrome://extensions` page.

For the **list** selectors (`list-extractor.js`), tick two or three threads and run this in the OWA console. It dumps every attribute of each checked row, which is how the current `ROW_SELECTORS` / `CHECKBOX_SELECTORS` were derived:

```js
[...document.querySelectorAll('[role="option"]')]
  .filter(r => r.getAttribute('aria-selected') === 'true'
            || r.querySelector('[aria-checked="true"]'))
  .map(r => ({ attrs: [...r.attributes].map(a => `${a.name}=${a.value}`).join(' | '),
               text: r.innerText.slice(0, 60) }));
```

If a batch run reports **"clicking the row changed nothing in the reading pane"** for every row, the click target is the thing that moved: `openRowAndSettle` in `popup.js` clicks the row element, then waits for either of two signals. It already falls back from `.click()` to a synthesised `pointerdown`/`mousedown`/`mouseup`/`click` sequence.

The two settle signals matter for diagnosis:

1. **The reading pane's text changed.** Strongest evidence, and what any mail with readable content gives, including a meeting invitation (its calendar card has no message body but does have pane text, which is why `PANE_SELECTORS` fans out past `[aria-label="Message body"]`).
2. **The selection collapsed to just the clicked row, and the pane has no text at all.** Last resort for a genuinely empty pane. The "pane is empty" half of that condition is load-bearing: with virtualisation, "only one row is selected" can be true simply because the other selected rows are unmounted, so on its own this signal would settle on the *previous* thread and capture it twice.

So "opened, but nothing readable in the reading pane" means the click landed and even `READING_PANE_SELECTORS` found no text: that is the case to take a DOM dump of.

The selector lists are intentionally fan-out — adding a new one without removing the old ones lets the extension keep working for both old and new versions during a rollout window.

## Messages to Brainstead

```
chrome.runtime.sendNativeMessage("com.wayned.brainstead", {
  "type":  "outlook",
  "url":   "https://outlook.office.com/...",
  "title": "Re: Budget approval — Q3",
  "kind":  "thread",          // or "reading-pane" for a non-conversation; optional
  "messages": [
    { "sender": "alice@…", "recipients": "bob@…", "date": "Mon 10 Mar 12:34", "body": "..." },
    ...
  ]
}

→ { "ok": true,
  "filename": "Email. Thread. Re Budget approval — Q3 - 2026-05-18.md",
  "path":     "sources/Email. Thread. Re Budget approval — Q3 - 2026-05-18.md" }
```

`recipients` and `date` are optional on each message. `kind` is optional and defaults to a threaded capture; `reading-pane` switches the filename prefix to `Email. Item.` and adds the `**Capture:**` note. Batch threads carry their own `kind`, so a mixed batch marks only the non-threaded members.

The **one-file-each** batch mode needs no endpoint of its own: the worker sends one `outlook` message per thread as it walks them, so each file lands as soon as it is captured and a stall on thread 7 keeps threads 1-6. Only the combined mode needs a second message:

```
{
  "type":  "outlook-batch",
  "label": "Northwind renewal",   // optional
  "threads": [ { "url": …, "title": …, "messages": [ … ] }, … ]   // 1-25
}

→ { "ok": true,
  "filename": "Email. Threads. Northwind renewal - 2026-08-19.md",
  "path":     "sources/Email. Threads. Northwind renewal - 2026-08-19.md",
  "threads":  3 }
```

In the combined document each thread is a `##` heading (with its own Outlook link) and messages demote to `###`, so the outline stays one heading deep per nesting level in both shapes. More than 25 threads is refused, not silently cut short.

## Limits / caveats

- **Bounded, foreground batches only.** No background job, no scheduled scrape, nothing that runs without the popup open, by design. The cap is 10 by default and 25 hard, enforced in both the popup and Brainstead.
- **Virtualisation is handled, but only for rows OWA has rendered at least once.** The message list is virtualised: off-screen rows do not exist in the DOM. Both the count and the walk scroll the list to deal with that (each row records the `scrollTop` it was found at, and the walk scrolls back there to re-create it before clicking). What this still can't reach is a row OWA has never rendered, i.e. one in a page it hasn't lazy-loaded yet. The popup cross-checks its count against OWA's own "N conversations selected" and tells you when the two disagree rather than quietly capturing fewer.
- **Attachments are ignored.** Bodies only. If you need an attachment, save it manually into `sources/` alongside.
- **No content deduplication.** Ingesting the same thread twice writes a second file rather than overwriting: Brainstead picks a free name (`… - 2026-08-19-2.md`). Nothing compares bodies, so two captures of an evolving thread both stick around.
- **Image inline content is text-only.** OWA's image proxies aren't followed; you'll see alt text or nothing.
- **Local-only.** Chrome starts Brainstead's helper over native messaging; no port is opened and nothing is reachable from another device.

## Roadmap

- Optional: capture attachment URLs (not the bytes) so the `/ingest` skill can surface them in the wiki page.
- Optional: dedupe by content hash so a re-capture updates rather than accumulates.
- Optional: scroll the virtualised list to reach checked rows that have been unmounted, instead of reporting them as missing.

The Teams-web port shipped as a sibling extension — see [`../teams-capture/`](../teams-capture/README.md).
