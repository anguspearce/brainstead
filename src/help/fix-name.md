---
title: Fix a name everywhere
kind: screen
screens: []
order: 25
summary: Correct a name that was spelt wrong across all your notes at once, and stop it coming back.
---
When a transcript gets a name wrong, such as "Lina" for Lena, Fix a name everywhere corrects it in every note in one go. Open it from **Fix a name everywhere…** in ⌘K.

## Type the two names
Type the name as it was written in **Written as** and the right one in **Correct spelling**. Brainstead lists every file with the wrong name, showing each line with the wrong spelling struck out and the right one beside it. If the correct name only adds a surname ("Sam" to "Sam Carter"), a surname already there isn't doubled. Web addresses, the place a markdown link points to and tags are left as they are: they name a page, a file or a tag, which the fix doesn't rename.

## See what happens to each file
Each file is marked with what the fix will do:

- **Rewrite**: a note, corrected in place. Its modification time is kept.
- **Add as alias**: the person's wiki page gets the wrong spelling as an alias, so links and searches still find it. If that page's properties don't read as YAML the alias can't be added: the fix stops, names the file and changes nothing.
- **Left alone**: sources (the raw record) and templates.
- **Guarded: someone else**: a file about someone whose real name is that spelling.
- **Skipped**: a conflicted copy of this file exists.

A file whose name holds the wrong spelling is marked **In the file name: rename it separately**. Untick any file you want left as it is.

## Protect someone with that name
If the wrong spelling is also someone else's real name, type part of the file names about them, such as "Lina Park", in the box below the names. Separate several with commas. Those files are never touched.

**Ask about each file** starts every note unticked, so you tick each one after reading it. An ingest then never applies the correction on its own.

## Apply it
Leave **Remember this correction** on so future captures, transcripts and ingests spell it right (the list is kept in Brainstead's app data, not the vault). **Where it's from** records where you learnt it. Click **Apply**. The notes, the alias and the remembered correction are one change: `⌘Z` undoes it all.

An assistant can fix a name the same way, with the same list and choices. It names the files to change; the rest are left alone. Its changes are listed in [Changes](app:review) with Revert, one a file, made or held together, and the correction is remembered once they're made. Nothing happens while the two spellings are the same.
