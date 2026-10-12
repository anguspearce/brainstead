---
title: Compare two documents
kind: screen
screens: [compare]
order: 25
summary: Compare any two documents side by side: where they differ, or what changed between two versions.
---
Compare reads two documents, A and B, with neither taking precedence, and lists how they differ. It's the **Any two documents** tab of **Compare** in the sidebar; the other tab, [Doc check](app:doccheck), checks a document against the version in force. Open it from the sidebar, **Compare with…** on a file's right-click menu or a source's ▾ on [Sources](app:sources) (that file is A), or **Compare two documents…** in ⌘K. It changes nothing unless you save the findings as a note.

## Choose the two documents
Pick **A** and **B** by typing part of their names, or **Choose a file…** for any file on disk (PDF, Word, PowerPoint, Excel, markdown or text). **Swap** exchanges them. Then choose what to look for:

- **Differences**: where they say different things, what's only in A, what's only in B, and what they agree on.
- **What changed**: from an earlier version (B) to a newer one (A), what was added, removed and changed. Rewording that keeps the meaning is left out.

Click **Compare**.

## Apples and pears
Before comparing anything, Compare reads only the opening of each document and asks whether they're about the same thing. If they aren't, it stops there and says so, so two unrelated documents cost a quick look rather than a full comparison. If that answer can't be read, it goes on and compares them, and the comparison can still say they're unrelated. Doc check does the same.

## While it runs
A comparison runs in the background, its steps ticked off as it goes (reading each document, the first look, comparing them) with a clock on the one going. **Stop** ends it at once: nothing more is asked of the AI and nothing is kept. You can leave the page: the sidebar shows a spinner beside **Compare**, then a badge when it's done, and a message offers **Open**, which comes back to it with both documents and the choice.

## The findings
Each finding is **Differs**, **Only in A**, **Only in B** or **Agrees** (or **Changed**, **Added**, **Removed**), with what each document says, quoted, and where. One that changes a decision, number, date, owner, scope or commitment is marked **Material**. **Copy findings** copies them as markdown; **Save as note** makes a note, `Compare. A vs B - YYYY-MM-DD.md`.

Every finished comparison is kept on this computer (not in your vault). Choosing the same two documents and the same choice again shows it at once; while neither has changed it still stands, and if either has, it says so and **Compare again** gets findings that match them.

An assistant can compare two documents too (`compare_docs`), and is given a kept comparison while it still stands. Two documents that aren't about the same thing aren't saved as a note, as the page offers no **Save as note** for them.
