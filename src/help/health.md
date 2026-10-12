---
title: Knowledge health
kind: screen
screens: [health]
order: 15
summary: Checks the wiki for missing pages, broken links, stale dates and other problems, and fixes the safe ones for you.
---
Knowledge health checks the wiki and its sources whenever the vault changes. **Need a decision** counts the issues only you can settle; the sidebar and Today's **Wiki issues to decide** show the same number. From the second day, a sparkline beside it shows the trend; hover it for the highest count.

## Read the checks
The checks come in two groups.

- **Wiki checks**: missing pages (linked but never written), broken sources, orphan pages (nothing links to them), missing cross-links, stale `updated:` dates, unlogged writes (pages changed after `log.md` was last written), sources not yet ingested, and images nothing uses.
- **More checks**: possible duplicates, stale pages others rely on, sources changed since they were cited, pages not in the page shape, pages with no Current state, claims with no citation (found by the [contradictions](app:contradictions) check), and system notes missing their header.

A system note's header is the "This is a system note" callout at the top of notes such as the To Do list, which Brainstead looks for. **Fix** puts it back as Brainstead last saw it.

Stale pages, claims with no citation and pages with no Current state are worth a look but aren't counted (their icon is grey). Claims with no citation are listed one row per page: click it to open the page with those lines highlighted, or **Ignore** them all.

Hover a check for what it looks for; click it to see its issues, and an issue to open its page. **Check now** runs every check again.

## Fix the safe issues
Some issues have one right answer, so Brainstead fixes them without asking:

- adding a missing `[[ ]]` link at the first mention
- setting `updated:` to the file's date
- adding a missing line to `log.md`
- putting back a system note's header

Click **Fix** on one issue, or **Fix safe issues** in the top bar for all of them. `⌘Z` undoes each fix.

## Deal with the rest
- **Missing pages**: **Create** makes the page in `wiki/entities/` (from `Templates/Wiki page.md` if there is one) and opens it. **Link to…** points the link at an existing page instead, everywhere it's used: type part of a page name and pick it. It's listed in Changes, with Revert. Each row says which pages link it. When a note's name is a slip away from the missing link (as after a note was renamed: "meting" for "meeting"), the row asks **Did you mean** that note, and **Link to it** points the links there in one click. Linking also points the claims on those pages made from the old name at the note, so a later ingest doesn't write the old name again; Revert puts them back with the link.
- **A missing page or broken source whose file is in the Trash** (a transcript trashed after its meeting note, say) says **In the Trash since** the day. If a meeting note was made from it (same title and day; a transcript's Part 1 and Part 2 count as one meeting), the row asks **Did you mean** that note, with **Link to it**. **Restore from Trash** puts the file back instead; there's no **Create** for a file in the Trash. An assistant reading the checks is told the same: In the Trash since, and Did you mean. When Brainstead trashes a transcript at the end of making its meeting note, it points the links to it at the note first, so nothing is left missing.
- **Broken sources** (a page's `sources:` lists a file that isn't there): **Link to…** points it at a file that's there, with **Did you mean** and **Restore from Trash** as for missing pages.
- **Sources changed since they were cited**: **Ingest again** reads the source again, so the pages citing it catch up.
- **Orphan pages**: `index.md`'s catalogue links every wiki page, so an orphan is one it hasn't caught up with yet (the daily check does). **Add them to index.md** brings it up to date now.
- **Possible duplicates**: **Open the other** compares them; **Not duplicates** stops the pair being flagged.
- **Anything else** (an orphan page, a stale page, a broken source…): **Ignore** hides it until its page (or, for a claim, its line) changes; **Show again** lists ignored ones again. Stale pages and claims with no citation also have **Ignore all**, at the top of their list.

A fixed issue leaves the list as soon as its fix is made, and the checks rerun within a second of any change to the vault, wherever it was made (Changes, Ask, another editor).
- **Images nothing uses**: **Move to the Trash**, after asking; you can restore them.
- **Sources not yet ingested**: **Ingest** starts the same ingest as [Sources](app:sources); its page changes are listed in [Changes](app:review). A source that can't be ingested, such as an SVG or a zip file, shows **Can't ingest**.

## Put pages in the page shape
Every entity and concept page has the same layout: the opening text, **Current state**, its topics (such as Architecture), a **Timeline**, then **See also**. Each Timeline entry is a heading starting with its date, newest first, with a line under it naming the note it came from:

```
### 2026-10-02 — Steerco: launch moves to 28 November
Source: [[Meeting. Orbit App Steerco - 2026-10-02]]
```

**Pages not in the page shape** lists pages laid out another way. **Reshape pages** fixes every page it can, without the AI, after asking; **Reshape** on a row does one page. It:

- moves sections whole
- turns a dated heading such as "2 Oct 2026 — Steerco" into "2026-10-02 — Steerco"
- turns a summing-up section such as Status (Apr 2026) into Current state, with "As of Apr 2026." under it
- keeps the newest of several dated summing-up sections as Current state and moves the rest to the Timeline
- merges two entries from the same note on the same date, and two See also lists

It checks that no text is lost before saving. Each page is one change in [Changes](app:review); revert them one by one or with **Revert all**.

A page that needs you says why: a heading whose year it can't tell, two summing-up sections, two entries citing the same note on different dates, or text in the Timeline that isn't an entry. Fix that (usually a heading) and it reshapes by itself, or click **Reshape anyway** and check the result in Changes.

## Write the missing Current states
**Pages with no Current state** lists pages with a Timeline but no Current state; they aren't counted as needing a decision. **Write 5** has the assistant's cheap model write one for five pages, so you can check them in [Changes](app:review) first; **Write all** does the rest. It uses only the page's opening text and newest Timeline entries, and leaves alone a page with nothing current to say. Each page is one change, with **Revert all** for the run. **Stop** ends the run after the pages in progress.

## Fix with Ask
**Fix with Ask** starts a chat asking the assistant to fix the issues that need judgement. Its edits are listed in [Changes](app:review) with Revert; safe fixes are made at once and `⌘Z` undoes them. The button is greyed out when nothing needs a decision.

## Side cards and contradictions
On the right, **Wiki pages by type** and **Wiki pages by tag** are pie charts; click a slice or row to open the [Wiki](app:wiki) filtered to it. **Pending sources** lists sources not yet cited and those changed since. **Contradictions** in the top bar opens the report of wiki pages that disagree.

## The daily check
Turn on **Daily check** in [Settings › Jobs & schedule](app:settings/jobs) to have Brainstead, at the time you set (09:00 to start), look for contradictions in pages that changed and update the list of pages in `index.md`. It catches up after sleep. **Also refresh pages whose sources changed**, in [Settings › AI assistants](app:settings/assistants), has it ingest those sources again. Its notification also says how many pages have left the page shape, for **Reshape pages**. **Run now** runs it straight away and becomes **Stop** while it runs.
