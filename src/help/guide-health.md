---
title: Knowledge health
kind: guide
screens: []
order: 6
summary: Keep the wiki in good order: fix the safe issues, decide the rest, settle contradictions and let the daily check watch for you.
---
Keep the wiki accurate and well linked as it grows.

## Open Knowledge health
Go to [Knowledge health](app:health). It checks the wiki whenever the vault changes. **Need a decision** counts the issues only you can settle. Click a check to see its issues, and an issue to open its page.

## Fix the safe issues
Click **Fix safe issues** in the top bar of [Knowledge health](app:health). Brainstead adds missing links, corrects `updated:` dates, adds missing `log.md` lines and puts back lost system note headers. `⌘Z` undoes each fix.

## Decide the rest
- Missing pages: click **Create** or **Link to…**.
- Possible duplicates: **Not duplicates** if they aren't.
- Nothing to fix, such as a stale page: **Ignore** hides it until its page changes. Stale pages and claims with no citation also have **Ignore all**.
- The rest: **Fix with Ask**. The assistant makes the fixes, each listed in [Changes](app:review) with Revert.

## Settle contradictions
Click **Contradictions** in [Knowledge health](app:health)'s top bar, then **Check · changed pages only**. Real contradictions come first. Their fixes are listed in [Changes](app:review); when you're done, mark each one resolved or ignored on [Contradictions](app:contradictions).

## Turn on the daily check
In [Settings › Jobs & schedule](app:settings/jobs), turn on **Daily check**. Each day it looks for contradictions in the pages that changed. To have it ingest changed sources again too, turn on **Also refresh pages whose sources changed** in [Settings › AI assistants](app:settings/assistants).

## Fix a name spelt wrong
When a transcript gets a name wrong, such as "Lina" for Lena, choose **Fix a name everywhere…** in `⌘K`. Type **Written as** and **Correct spelling**, check the files it lists, and click **Apply**. Sources are left alone, the person's wiki page gets the wrong spelling as an alias, and **Remember this correction** keeps later ingests right. The fix is logged in [Activity](app:activity), and `⌘Z` undoes it all. [Fix a name everywhere](help:fix-name) has the details.
