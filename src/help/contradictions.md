---
title: Contradictions
kind: screen
screens: [contradictions]
order: 20
summary: Find places where wiki pages disagree about the same thing, and settle them.
---
Contradictions finds wiki pages that disagree about the same fact, such as two launch dates for Orbit App. Open it from **Contradictions** in [Knowledge health](app:health)'s top bar.

## Run a check
Click **Check · changed pages only**. Brainstead reads the claims and facts on each wiki page that changed since the last check, and the AI judges the ones that disagree. **Stop** ends a check part way and keeps what it has read for next time. The daily check in [Settings › Jobs & schedule](app:settings/jobs) can run this for you each day.

## Read a clash
Real contradictions come first, by severity, then unclear and unjudged ones. Each clash shows the judge's verdict: **Real** (with its severity), **Newer supersedes older**, **Not a conflict**, **Unclear** or **Not judged yet**. Once you've dealt with it, it shows **Resolved** or **Ignored**.

Select one to see each page's claim side by side, with its quote and "as of" date. Click a page's name to open it. **Judge's view** says what the fix should be and which page is right.

## Settle it
When the judge has a fix, it is made to the page that's wrong (out-of-date pages included) and listed in [Changes](app:review) with Revert. From the daily check, a fix that fails a check is held there for you instead. **The fix is in Changes** takes you there.

Once a fix is applied, that clash stops being flagged; revert the fix and it is flagged again. A fix that couldn't be made or held (the vault is read-only, say) is noted in Brainstead's log, and the daily check's line says how many and why.

**Mark resolved** when you've dealt with it, or **Ignore** when it isn't real; either way it stops being flagged.

## Keep a record
**Save report as note** saves the findings as `Contradictions - <date>.md`, with each clash's claims, verdict and fix.

When you ask, an assistant can read the findings in the same order, with each fix and whether it's made or waiting in Changes. It can also mark a clash resolved or ignored, and save the report once the check has finished. Its saved report is listed in [Changes](app:review).

## Resize the panes
Drag the line between the list and the claims to resize them; a double-click on it puts it back ([pane widths](help:settings-general)).
