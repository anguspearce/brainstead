---
title: Sources
kind: screen
screens: [sources]
order: 12
page: Preview a source
summary: The raw material the wiki is built from, and where you ingest it into the wiki.
---
Sources are the files in your vault's `sources/` folder: captured emails and chats, transcripts, documents, images and notes. They are the record, so Brainstead doesn't edit them, except that an ingest (the AI reading a source into the wiki) corrects misheard names, listed in [Changes](app:review) with Revert. Go to Sources with `⌥⌘0`.

## Add a source
- Drag files from Finder (or Explorer) onto the Sources screen, or click **Import** and pick them. Each is copied into `sources/`, renamed if the name is taken. Folders, files already in the vault and files over 200 MB are refused.
- Capture from the browser with the Outlook and Teams extensions (set up in [Settings › Capture extensions](app:settings/capture)). A capture lands in `sources/` as an `Email. …` or `Teams. …` note, and also waits in the [Inbox](app:inbox).

Nothing can be added while Read-only is on in [Settings › Vault](app:settings/vault).

## See what needs ingesting
Each source has a status:

- **New**: no wiki page cites it.
- **Changed**: it changed after the pages citing it were written.
- **Ingested**.

Filter with **All**, **New**, **Changed** and **Ingested** above the list. **Search sources** searches their text and names with the same rules as [Search](help:search). Kind says what each file is; Folder says which folder under `sources/` it's in (hover for the full path).

Markdown, text, PDF, Word, PowerPoint and Excel files and images (not SVG) can be ingested. A file that can't be ingested has no **Ingest** button.

## Ingest a source
- Click **Ingest** on a source's row, or **Re-ingest** on one already ingested.
- A Teams transcript's row has **Meeting note** instead: it writes the meeting note, ingests that and trashes the transcript ([Meeting note from a transcript](app:meeting)). An email thread or Teams chat has **Draft reply** ([Draft reply](app:reply)).
- Each row's ▾ lists everything that can be done with the source: **Make and ingest a meeting note** or **Draft a reply** where they fit, **Ingest into the wiki**, **Ask about this source** (a chat about it in [Ask](app:ask)) and **Compare with…** ([Compare two documents](app:compare), with this source as A). The [Inbox](app:inbox) and a file's right-click menu offer the same.
- **Ingest** in the top bar ingests every new and changed source, one after another. Teams transcripts get meeting notes instead.
- You can also choose **Ingest into the wiki** from a note's ⋯ or right-click menu, or press `I` on a capture in the [Inbox](app:inbox).

Ingests run one at a time, oldest source first, so a newer source has the last word on a page.

The AI drafts changes to the wiki pages the source mentions, and every quote is checked against the source. What the source adds goes in as:

- a dated entry in the page's Timeline, newest first, naming the source (ingesting the source again replaces its entry);
- a new Current state under the opening text, when what's true now changes;
- a topical section above the Timeline.

On a long page the AI rewrites only a section it saw in full; "left out by the checks" says when it tried otherwise.

A change is dropped only when none of its quotes is found in the source. One with some quotes missing is kept and flagged. A change from an image is never dropped for a missing quote (the AI may read what text recognition missed); it's kept and flagged.

The changes are made at once, with a line in `log.md`, and listed in [Changes](app:review), where you can revert them. Only when the daily check runs on its schedule is a flagged change held there for you instead. The source is added to each page's `sources:`.

To ingest new sources as soon as they arrive, turn on **Ingest new sources as they arrive** in [Settings › AI assistants](app:settings/assistants).

## Follow an ingest run
The **Ingest runs** pane on the right shows each run's steps, with **Stop** while it runs or waits. An assistant can stop one too. When it's done, **See N changes** (or **Review N held changes**) opens Changes at the run's first change, and "left out by the checks" lists what was dropped and why. The pane shows the latest runs; scroll down for older ones. **Search runs** at its top finds runs by the source or meeting note's name, state (Done, Failed, Stopped), model or day; `↓` and `↑` step through what it found, and `Esc` or ✕ clears it.

## Preview a source
Open a source to read it.

- **PDF**: a bar with page n of N, previous and next, and zoom from 50% to 300% (click the percentage to fit the width again). **Find in this file** lists the pages that match; `↩` goes to the next match and `⇧↩` the one before.
- **Word, PowerPoint and Excel**: the text Brainstead reads from them, which search, ingest and quotes use. To see the file as it looks in its app, press **Quick Look** or `Space` (Mac only for now).
- **Image**: shown with **Text in the image** folded under it; click it to see the text Brainstead read, which search and ingest use.
- **Open in its app** opens the file in its usual app. A file Brainstead can't show has both buttons.

When you ingest an image, Claude Code and Codex see the picture as well as its text; Copilot and Antigravity get the text only. So an image with no text needs Claude Code or Codex. A change from an image is flagged in [Changes](app:review), to check against the picture.

## See where a source is used
Beside the preview, the provenance card, **Cited by N pages**, says when the source was last ingested and by which model, and lists the wiki pages that cite it and the passages they cite; **Details** adds the file's hash and passages indexed. **Re-ingest** and **Reveal** (in Finder, or Explorer on Windows) are at the foot, with **Move to the Trash** at the right. An assistant can read the same card.

A source's top bar has **Ask**, which starts a chat about it. A captured thread has **Draft a reply**, and other documents have **Doc check**. On the Sources list, **Meeting notes** opens [Meeting note from a transcript](app:meeting).
