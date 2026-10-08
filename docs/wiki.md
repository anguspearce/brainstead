# The wiki and sources

Brainstead keeps a wiki in your vault's `wiki/` folder: a page per person, team, product or idea, built from your sources, with every claim quoted from where it came from. The AI writes the pages and Brainstead checks every quote against its source. Each change is made at once and listed in Changes, with Revert.

It follows Andrej Karpathy's [LLM Wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) pattern: raw material in `sources/`, pages the model maintains in `wiki/`, a catalogue in `index.md` and a record in `log.md`. Brainstead adds checked quotes, Changes, Knowledge health and a contradictions check.

[Sources](#sources) · [Ingest](#ingest) · [Changes](#changes) · [Wiki pages](#wiki-pages) · [Knowledge health](#knowledge-health) · [Contradictions](#contradictions) · [Graph](#graph) · [Meeting notes and the other tools](#meeting-notes-and-the-other-tools)

## Sources

On Windows, ⌘ is Ctrl and ⌥ is Alt, so Sources' ⌥⌘0 is Ctrl+Shift+0.

<a href="images/index.md#the-wiki-and-sources"><picture><source media="(prefers-color-scheme: dark)" srcset="images/sources-dark.png"><img alt="Sources: captured emails and documents with their status, and the ingest runs" src="images/sources-light.png"></picture></a>

Sources (⌥⌘0) are the files in `sources/`: emails, Teams chats and transcripts captured from the browser, PDFs, Word, PowerPoint and Excel files, images, and notes. They are the record, so Brainstead doesn't edit them, except to correct misheard names (a change you can revert).

- Drag files onto Sources, or click **Import**, to copy them in.
- The **Outlook** and **Teams** Chrome extensions capture a thread, chat or meeting transcript into `sources/` with one click. Set them up in Settings › Capture extensions. Each capture also waits in the Inbox.
- Each source is **New**, **Changed** (since the pages citing it were written) or **Ingested**.
- A row's button is what the source usually becomes: **Meeting note** for a Teams transcript, **Draft reply** for an email or chat thread, otherwise **Ingest**. Its ▾ has the rest: **Make and ingest a meeting note** or **Draft a reply** where they fit, **Ingest into the wiki**, **Ask about this source** and **Compare with…**. A file's right-click menu offers the same.
- Transcripts, emails and chats are known by their names (`Teams. Transcript. …`, `Email. …`, `Teams. Chat. …`), in `sources/` or any folder under it.
- The **Ingest runs** pane lists each run's steps, the latest first and older ones as you scroll. **Search runs** finds them by name, state, model or day, and **See N changes** opens Changes at the run's first change.
- PDFs open with page navigation, zoom and find. Word, PowerPoint and Excel files show their text; **Quick Look** (`Space`, Mac only) shows how they look. Images show the text read from them, which search uses too (Mac only for now).

<a href="images/index.md#the-wiki-and-sources"><picture><source media="(prefers-color-scheme: dark)" srcset="images/source-provenance-dark.png"><img alt="A source's provenance: when it was ingested and by which model, and the wiki pages that cite it with the passages they cite" src="images/source-provenance-light.png"></picture></a>

Beside a source, its provenance says when it was ingested, by which model, and which wiki pages cite it, with the passages they cite.

## Ingest

**Ingest** reads a source and has the AI draft changes to the wiki pages it mentions, each claim with a quote from the source. Brainstead checks every quote. A change with none of its quotes found is dropped; one with some missing is kept and flagged. The source is added to each page's `sources:` list, and misheard names in it are fixed. A run shows its steps as it goes, with **Stop**.

Ingests wait in one queue, whoever started them, and run one at a time, oldest source first, so a newer source has the last word on a page.

Brainstead, not the AI, decides where text goes on a page:

- Each source gets one dated entry in the page's **Timeline**, newest first, with a `Source:` line. Ingesting the same source again replaces its entry.
- When the source changes what's true now, a new **Current state** goes under the page's opening text. A new topical section goes above the Timeline.
- The checked facts behind a change (what, its value, as of when, and the quote) are kept with the page. They're part of the change, so Revert takes them back too.
- A change that would rewrite a section the AI saw only part of is dropped, so nothing is lost on a long page.

An image is ingested from the text read from it. Claude Code and Codex also see the picture; Copilot and Antigravity don't, so an image with no text needs one of the first two. Its changes are kept even when a quote isn't in that text, each flagged in Changes to check against the picture. On Windows, reading text in images and showing images to a model aren't available yet, so the text in an image is not read yet: a note about an image should carry what it says.

An ingest you start makes its changes at once. One the daily check starts holds any change that fails a check for you instead. To ingest each capture as it lands, turn on **Ingest new sources as they arrive** in Settings › AI assistants.

## Changes

<a href="images/index.md#the-wiki-and-sources"><picture><source media="(prefers-color-scheme: dark)" srcset="images/review-dark.png"><img alt="Changes: what assistants and runs changed, grouped by run, one open with its diff and the quotes it rests on" src="images/review-light.png"></picture></a>

Changes (⌥⌘5) lists every change assistants and runs made to the vault, newest first, grouped by run ("Daily check, 5 Oct: 6 changes to 4 pages"). Each has its diff, why, who made it, the quotes it rests on and **Revert**. **Search changes** narrows the list by title, page, source or model; `↑` `↓` step through what it found and `Esc` clears it.

- A change you started (Ask, a terminal, a run started by hand) is made at once, and flagged if a check fails.
- A scheduled run's change (the daily check and its ingests, the scheduled summaries) is made only when it passes every check, and held otherwise: a quote not found or read from an image, text rewritten or removed in one of your own notes, unsaved edits to the page, an open contradiction on it, or properties that wouldn't read.
- A change to a template, one that adds code that runs when a note is shown, or one that changes a [system note's header](notes.md#system-notes) is always held. Renames, moves to the Trash and restores from it are made at once, unless they touch a template or bring back code that runs. An assistant can accept a held change only while you're there, and only one held for a failed check.
- **Held for you** is at the top, each with why: **Accept** (`A`), **Reject** (`R`), **Edit before accepting**, and **Accept all** or **Reject all** for a run (`⌘↩` accepts the run). With a search on, a run's Accept all, Reject all and Revert all act only on the changes the search shows, as their confirmations count. A held change applies to the page as it is when you accept it. The sidebar counts only held changes.
- **Revert** undoes a change and keeps later edits. If its lines were edited since, it says so and offers the page as it was, to copy. **Revert all** undoes a whole run. A page's side pane lists its agent changes, with Revert.
- History is kept 90 days or 500 MB, whichever comes first (Settings › AI assistants).

## Wiki pages

<a href="images/index.md#the-wiki-and-sources"><picture><source media="(prefers-color-scheme: dark)" srcset="images/wiki-page-dark.png"><img alt="A wiki page: its current state with numbered citations, and the Sources and Linked from cards" src="images/wiki-page-light.png"></picture></a>

Every entity and concept page has one layout: opening text, **Current state**, topical sections, a **Timeline** of dated entries (`### YYYY-MM-DD — title`, or a month, quarter or half) newest first, each with a `Source:` line, and **See also**. Summaries are left as they are.

The Wiki (⌥⌘9) lists the pages by type and tag with their health: **Held**, **Stale**, **Check** or **Healthy**. On a page:

- A link to a source shows as a small number that opens the source at the passage cited; the **Sources** card lists them.
- The **Facts** card lists the page's checked facts: the latest value of each, what it replaced, and its source. Older pages may have none until they're next ingested.
- A banner says when another page disagrees.

**New wiki page** makes an entity or concept page you write yourself.

## Knowledge health

<a href="images/index.md#the-wiki-and-sources"><picture><source media="(prefers-color-scheme: dark)" srcset="images/health-dark.png"><img alt="Knowledge health: the checks, the issues needing a decision, and the side cards" src="images/health-light.png"></picture></a>

Knowledge health checks the wiki whenever the vault changes: missing pages and links, broken or changed sources, duplicates, stale pages, claims with no citation and more. A small chart beside **Need a decision** shows that count day by day. Claims with no citation are listed one row a page, and open the page with those lines highlighted.

- Pie charts show the pages by type and by tag; a slice opens the Wiki filtered to it.
- **Fix safe issues** fixes the ones with one right answer (a missing link, a date, a `log.md` line, a system note's lost header), undoably.
- **Ignore** hides an issue until its page changes; **Show again** brings a check's ignored issues back. Stale pages and claims with no citation also have **Ignore all**.
- A fixed issue leaves the list at once, and the checks rerun within a second of any change to the vault. Checks only worth a look (stale pages, claims with no citation, pages with no Current state) have a grey icon, not amber.
- Missing pages can be created, or linked to an existing page with **Link to…**. Each says which pages link it. When a note's name is a slip away (after a rename, say), the row asks **Did you mean** that note, and **Link to it** points the links there.
- A missing page or source whose file is in the Trash says **In the Trash since** the day, with **Restore from Trash** instead of **Create**.
- A source changed since it was cited has **Ingest again**, so its pages catch up. Orphan pages have **Add them to index.md**, which brings the catalogue up to date now. Duplicates can be compared or dismissed.
- **Pages not in the page shape** lists pages laid out another way. **Reshape pages** fixes the ones it can, without the AI, and checks that nothing is lost before it saves. Each page is one change in Changes, and you can revert the whole run at once. A page it can't fix says why; fix it, or **Reshape anyway**. The screen's help lists exactly what it changes.
- **Pages with no Current state** lists pages with a Timeline but no Current state. **Write 5** and **Write all** have a cheap model write one from the page's opening and newest Timeline entries. Each page is a change in one run in Changes.
- **Fix with Ask** hands the issues that need judgement to an assistant; its fixes are listed in Changes.

## Contradictions

The contradictions check groups claims about the same thing on the pages that changed, and has the AI judge the ones whose values differ: **Real** (with its severity), **Newer supersedes older**, **Not a conflict** or **Unclear**. When it has a fix for a real one, the fix is made and listed in Changes. You can **Mark resolved**, **Ignore**, or **Save report as note**. The [daily check](day-to-day.md#summaries-and-jobs) runs it on each day's changed pages.

## Graph

<a href="images/index.md#the-wiki-and-sources"><picture><source media="(prefers-color-scheme: dark)" srcset="images/graph-dark.png"><img alt="The graph: a wiki page at the centre with the pages around it, coloured by type" src="images/graph-light.png"></picture></a>

The **Graph** draws how pages link: a page's neighbourhood, or the whole wiki. Entities are blue, sources and summaries orange, concepts green, notes purple, as on the Wiki's pie chart.

## Meeting notes and the other tools

- **Meeting notes** turns a Teams transcript into a meeting or 1-1 note from your template, with names spelt as your vault spells them. The note is made (listed in Changes) and ingested, and the transcript moves to the Trash; both are on by default in Settings › AI assistants, and a transcript is kept if either fails. However a transcript is moved to the Trash (by this, from the Meeting notes card, the Inbox, a Still in the Inbox strip, Sources, the file menu or command palette, or an assistant's trash_note), links to it are first pointed at its meeting note, when it has one. A transcript leaves the Inbox once its note is made and ingested, or once a note held in Changes is accepted there; reverting the note in Changes puts a transcript that wasn't trashed back in the Inbox.
  - Start it from **Meeting note** on a transcript in Sources, or `M` (or **Make and ingest a meeting note** on the right-click menu) in the Inbox. If Brainstead is sure of the note's type, name and date, it does it all in one step; otherwise it opens Meeting notes at the question.
  - Tick several (or **Tick all**) and **Make and ingest all** asks once for their dates, then does each. The card always says where a transcript is (being written, failed, held in Changes, its note there already) and leads with the step that's left.
  - The note is dated with the meeting's day. If the Teams extension couldn't read it, the capture day is used and you confirm it first.
- **Fix name** corrects a misspelt name across every note and wiki page at once (sources are left alone), adds the wrong spelling as an alias, and remembers it for future ingests. One ⌘Z undoes it all.
- **Draft reply** drafts an answer to an email or Teams thread, drawing on the vault, for you to copy and send. Type what to change in **Ask for a change** under the reply ("shorter") and press `↩`; it keeps your own edits, and **Undo** puts back the reply as it was. While the thread is still in the Inbox, a **Still in the Inbox** strip takes it out from there: **Done, keep in Sources**, **Ingest and take out** or **Move to the Trash**.
- **Triage bookmarks** goes through your bookmarked notes with a suggestion for each: keep, update or drop.
- **Compare**, in the sidebar's Tools, has two tabs:
  - **Doc check** checks a document against the version in force of the one it should follow, or whether a revision took in your feedback. The version in force comes from your register of governing documents, the note `Me. Canonical Docs.md`, which Doc check can start for you.
  - **Any two documents** compares A and B: their differences, or what changed between two versions. **Compare with…** on a file's right-click menu or a source's ▾ opens it with that file as A.
  - Both first look at the documents' openings (**Check they're about the same thing**) and stop when they aren't. Each step is ticked off with its time. Checks run in the background, with **Stop** (which keeps nothing); the sidebar shows a spinner while one runs and a badge for those that finished while you were elsewhere. Results are kept until either document changes from the version they read.

Assistants do all of this with Brainstead's tools, named as the screens are: for example `import_sources`, `facts`, `lint`, `ignore_issue`, `health_issue`, `page_shape`, `reshape_pages`, `write_current_state`, `list_contradictions` and `contradictions`.
