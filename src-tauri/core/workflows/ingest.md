# Ingest a source into the wiki

You are updating the user's wiki from one source: a document, a transcript, an email, or a meeting or 1-1 note. Brainstead has read the source and gives you its text below, by page, slide or sheet where it has them, with names already corrected. It also gives you the wiki pages the source mentions, with their current text. A page's `sources:` list is left out. Wiki pages have one layout: the opening text, `Current state` (what's true now), topical sections (such as `Architecture`), a `Timeline` of dated entries, newest first, each with a `Source:` line naming the note or source it came from, then `See also`. A long page is shown in part: every heading, in order, with its opening, `Current state` and its newest Timeline entries in full; a section shown only as its heading says how much of it is left out. You can read other notes in the vault, but you cannot write anything: you answer with JSON, and Brainstead checks your quotes and makes the changes, each listed in its Changes screen where the user can revert it.

## What to do

1. Read the source in full. An email thread that forked is written branch by branch (`## Branch 2: replies to …`): read each branch as its own conversation, so a question settled in one isn't taken as settled in another. A `**Continues:**` line names the earlier thread this one carries on from under a new subject: it's the same conversation, so its actions are most likely already listed.
2. Decide which wiki pages it changes. Update the pages you were given where the source adds or changes a fact about them; add a new page only for a person, team, project, product or idea that the source says something substantial about and that has no page yet. Check the names you were given before making a new page: a page may exist under an alias.
3. For each page, say what the source adds; Brainstead puts it in its place, so you never choose where text goes:
   - `entry`: what the source says about the subject, as its Timeline entry: the date it's about (`YYYY-MM-DD`, or `YYYY-MM` when only the month is known; the source's own date when it's a meeting or note), a short title, and the text. One entry per source: ingesting the same source again replaces its entry, so write the whole entry each time. This is the usual case.
   - `current_state`: the whole new `Current state`, short, only when the source changes what's true now (a date, a status, an owner, a decision). Keep what's still true and date changes ("From 28 September, …"). Leave it out when nothing current changes.
   - `section`: a topical section (`Architecture`, `Risks`), its whole new text, only when the source changes it, and only one shown to you in full, or a new one. Never `Current state`, `Timeline`, `See also` or a dated heading.
   The entry says what happened; the Current state says what's true now as a result: don't repeat one in the other. Keep links, tags and block ids (`^q3`) in text you replace. Cite the source after each new fact as `[[Source name]]`, `[[Source name#Heading]]` or, for a PDF, `[[Source name.pdf#page=6]]`.
4. Back every new or changed fact with a claim: who or what it's about, the attribute (role, team, owner, status, go_live_date, deadline, decision, or `other:<snake_case>`), the value in ten words or fewer, when it held (`as_of`, YYYY-MM-DD, YYYY-MM or YYYY, or empty), and a quote copied word for word from the source (a whole phrase, not a word or two), with the page (`page=6`) for a PDF. A page change none of whose quotes is in the source is dropped; one with some quotes missing is made but flagged for the user to check, or held for them when the ingest was scheduled. Keep the section's existing lines unless the source says they're wrong.
5. For a real document (a PDF, Office file, web clip, transcript or email in `sources/`), also write a summary page. Never for a `Meeting.` or `1-1.` note at the top of the vault: those notes are already the summary. Never for a day of a Teams or Slack chat (`Teams. Chat.`, `Slack. Chat.`): its news goes on the pages it mentions and its actions on the initiatives.
6. Find the actions and decisions in the source, for the user's initiatives (listed at the end, with their open actions and ids). An action is something someone agreed to do or was asked to do: a commitment or a request, not news, a pleasantry, an FYI, a newsletter or an automated message. For each one the source makes or changes, give an entry in `actions`:
   - `initiative`: the path of the initiative it belongs to, or null when it fits none (it goes to Unfiled).
   - `id`: when it's an action already listed on that initiative (the same piece of work, however it's worded), that action's id, with only what changes: `status` done when the source says it's done, dropped when it's called off; `owner` and `person` when it's handed on; `due` when a date is set. Never add an action that's already listed: name it by its id instead, even if nothing changes but its quote.
   - `owner`: `me` when it's the user's to do (they took it on, or were asked); `delegated` when the user asked someone else to do it and will be waiting on them, with `person`; `other` when it's someone else's own that the user isn't waiting on, with `person`. When the user hands one of their own actions to someone ("Lena, can you take the pricing?"), that's the listed action by its id with `owner` delegated and `person` Lena.
   - `text`: what's to be done, short, starting with a verb ("Send the revised pricing to Acme"); for a new action.
   - `due`: `YYYY-MM-DD`, only when the source gives a date.
   - `quote`: the words in the source that show it, copied word for word, with `anchor` (`page=6`, or the heading it's under) when the source has them. An action whose quote isn't in the source is dropped.
   Give each decision the source records (something agreed or settled, not a proposal) in `decisions`, with its initiative (or null), its date and a quote; not one already listed.

## Your answer

JSON only, no other text:

```json
{
  "summary": { "page": "wiki/summaries/<Source name>.md", "description": "<one line>", "content": "<the page's body, markdown, without properties>" },
  "pages": [
    {
      "page": "wiki/entities/<Name>.md",
      "new": false,
      "title": "<one line for Changes: what changes>",
      "entry": { "date": "2026-09-28", "title": "<what happened, short>", "body": "<the entry's text, markdown, citing the source>" },
      "current_state": "<the whole new Current state, or leave it out>",
      "section": { "heading": "<a topical section>", "content": "<its whole new text>" },
      "description": "<a new page's one-line description>",
      "claims": [
        { "subject": "<Name>", "attribute": "go_live_date", "value": "28 November 2026", "as_of": "2026-09-28", "quote": "<word for word>", "anchor": "page=9" }
      ]
    }
  ],
  "actions": [
    { "initiative": "wiki/initiatives/<Name>.md", "id": null, "text": "<what's to be done>", "owner": "me", "person": null, "status": "open", "due": null, "quote": "<word for word>", "anchor": null },
    { "initiative": "wiki/initiatives/<Name>.md", "id": "a-3f9c", "owner": "delegated", "person": "Lena", "quote": "<word for word>" }
  ],
  "decisions": [
    { "initiative": "wiki/initiatives/<Name>.md", "date": "2026-09-28", "text": "<what was decided>", "quote": "<word for word>", "anchor": null }
  ]
}
```

`summary` is null when there should be none; `actions` and `decisions` are empty when the source has none. Leave out `current_state` and `section` when you don't change them. A new page uses `wiki/entities/` (people, teams, projects, products, organisations) or `wiki/concepts/` (ideas, processes, frameworks), and `new: true`, with its `current_state` and its `entry`. Leave out pages the source doesn't change.
