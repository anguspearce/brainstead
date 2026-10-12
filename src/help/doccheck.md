---
title: Doc check
kind: screen
screens: [doccheck]
order: 24
summary: Check a document against a policy or standard it must follow, or check whether a revision took in your feedback.
---
Doc check reads a document against a governing document, such as a policy or standard it must follow, and lists where they differ. It uses the version your register marks as canonical (in force). It changes nothing unless you save the findings as a note. Open it from **Compare** in the sidebar (its **Doc check** tab), **Doc check** in a source's top bar, or **Compare** in ⌘K. To compare any two documents with neither in force, use the **Any two documents** tab ([Compare two documents](app:compare)).

Before checking, Doc check reads only the opening of the document and of the version in force, and stops there if they aren't about the same thing.

## Set up the register
The register is the note `Me. Canonical Docs.md`: a table with one row per version of each governing document. Its columns are key, title, version, status, path and aliases.

- Status is canonical (in force), draft or superseded.
- Path is a file in the vault, a `~/…` path or a full path.

**Start the register** creates it with an empty table; **Edit register** opens it. The screen shows the register's rows, marks a file that isn't there, and shows any problem reading the table.

## Check a document
Choose a source beside **Check** (type part of its name to find it), or **Choose a file…** to pick any file on disk (PDF, Word, PowerPoint, Excel, markdown or text). Then pick a mode:

- **Against the version in force**: choose the governing document. Brainstead uses only its canonical row and won't guess a version from a file name. It refuses when no version, or more than one, is marked canonical.
- **Against my earlier feedback**: for a revised document, the AI finds your feedback on the last version and checks each point against the revision.

Click **Check**. If it can't run yet, it says why.

A check runs in the background, and a long document can take minutes. Its steps are ticked off as it goes (reading the document, reading the version in force, **Check they're about the same thing**, comparing them), each with its time and a clock on the one going. **Stop** ends it at once: nothing more is asked of the AI and nothing is kept. You can leave the page meanwhile: **Compare** in the sidebar shows a spinner while a check runs and a badge for those that finished while you were elsewhere, and a message says when it's done, with **Open** to come back to it, the document, mode and governing document chosen as they were.

Every finished check is kept on this computer (not in your vault). Choosing the same document, governing document and mode again shows it straight away, saying when it was checked. While neither document has changed since it was read, it still stands; if either has, it says so, and **Check again** gets findings that match them. **Save as note** puts the findings in your vault.

## Read the findings
The verdict comes first, with a summary, the counts and which version was used. Each finding is a **Conflict** (both texts quoted), **Not covered**, **Superseded term**, **Beyond scope** or **Agrees**. Important ones are marked **Material**. **Show** lists the points that agree.

**Copy findings** copies them as markdown. **Save as note** saves them as `Doc check. <name> - <date>.md`.

An assistant can check a document the same way and read the register. When you ask, it can save its findings as a note and start the register; both are listed in [Changes](app:review), with Revert.
