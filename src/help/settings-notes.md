---
title: Settings › Notes
kind: screen
screens: [settings/notes]
order: 31
summary: Spelling and grammar checks, reading aloud, how documents look by default, where templates find their scripts, and ticket keys that open in your browser.
---
Notes sets how you write and how notes and wiki pages look.

## Check spelling and grammar
On Windows, spelling is Windows' own spell checker, the one Edge and Office use, and grammar is checked by Harper, which runs on the PC: nothing you write leaves it.

**Check spelling** underlines unknown words in red as you write; click one for guesses, Learn or Ignore. **Check grammar** underlines grammar problems in blue; click one for the explanation and fixes. Both are on by default.

**Spelling language** picks which English to check against, such as colour or color. It lists the English dictionaries macOS has and starts as macOS is set. An assistant uses the same names, such as British English.

## Read notes aloud
**Read aloud** sets the **Voice** (English voices, best first), the **Speed** (0.5× to 2×), and **Highlight each word** (off tints the whole paragraph instead). **Test** says a sentence with those settings. On a Mac the voices are macOS's (download better ones in System Settings › Accessibility › Spoken Content); on Windows they're Windows' own, natural voices first where it has them (add more in Windows Settings › Time & language › Speech).

## Choose how documents look
**Document look** sets the theme and colour for notes, wiki pages and their PDFs. The themes are Brainstead, Business, Editorial, Modern, Minimal, Report and Technical; hover over one to preview it. The swatches set the accent colour, and the sample shows the result.

To change one note's look, do it from that note's screen. If any notes have their own look, **Notes with their own look** says how many, and **Use the defaults for all** resets them. An assistant can list those notes and reset them too, when you ask.

## Make documents light or dark
**Light or dark** sets documents to **As the app**, **Light** or **Dark**, so you can read light notes in a dark app or the other way round. PDFs always print light. Changing **Appearance** in [Settings › General](app:settings/general) sets this back to **As the app**.

## Point templates at their scripts
New note runs the Templater templates in the vault's `Templates` folder. Their user scripts (`tp.user`) are the `.js` files in the folder named here, `Templates/scripts` by default. Type a vault folder; it's saved when you leave the box. **Templater docs ↗** opens Templater's documentation in your browser.

## Open ticket keys in your browser
**Reference links** turns a ticket or issue key such as `ORBIT-12` into a link that opens in your browser. Press **Add a prefix**, type the prefix (`ORBIT-`) and the address its keys open, with `{key}` standing for the whole key and `{num}` for its digits: `https://tickets.example.com/browse/{key}` opens `https://tickets.example.com/browse/ORBIT-12`. The row shows what a sample key opens, and an address must start with `http://` or `https://`. **Remove** takes a row out. A row needs a prefix and a working address, and a prefix can be used once: a repeated one says so and is ignored. There are none until you add them, and then nothing is a reference key.

A key is the prefix followed by digits, and the prefix is case-sensitive. Before it there is no letter, digit, `_`, `-`, `#` or `/`, so `XORBIT-12`, `#team/ORBIT-12` and `docs/ORBIT-12.md` have no key. After it comes no letter, digit or `_`, and no `-` followed by one, so `ORBIT-12b` and `ORBIT-12-b` have none. `ALB-02` is plain text unless `ALB-` is a prefix here. A key is never inside code, a wikilink, an image or a markdown link (`[ORBIT-12](Note.md)`), whatever the link points at.

Once a prefix is set, keys are links in task rows, in a note in View and in Edit (`Ctrl`-click, `⌘`-click on a Mac), in a property such as `tickets: [ORBIT-12]` (a link chip; the values of `tags` and `aliases` never are), and in Dataview tables. The note's **References** card, the project's **Tickets and links** card and an assistant's lists of them ([A note](help:note), [Projects](help:projects)) appear only then. With no prefix set they are absent. Then `[label](https://...)` links and bare web addresses in task rows open in your browser, and a property that is a web address is a link that shows the address as written; task rows also draw image syntax as its alt text and handle double-backtick code and code or wikilinks inside italics. An assistant reads and sets the list through the `referenceLinks` setting.
