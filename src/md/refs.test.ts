// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import {
  collectRefs,
  hostOf,
  isExternalHref,
  refPatterns,
  refUrl,
  rowProblem,
  sampleRef,
  tokenize,
  validPatterns,
  type RefPattern,
} from "./refs";
import { fromLabel, gatherRefs, noteRefs } from "./refUses";
import { bodyRefs, propertyText } from "./noteRefs";
import propertyCases from "../../tests/fixtures/property-text.json";
import { parseInline, plainOf, taskRefs } from "./inline";
import { settings } from "../store";

const ORBIT: RefPattern = { prefix: "ORBIT-", url: "https://tickets.example.com/browse/{key}" };
const patterns = [ORBIT, { prefix: "HELP-", url: "https://help.example.com/t/{num}" }];

const links = (text: string, p: RefPattern[] = patterns) =>
  tokenize(text, { patterns: p })
    .filter((s) => "href" in s)
    .map((s) => (s as { text: string; href: string }).text);

describe("tokenize", () => {
  it("finds a key as the prefix and its digits, on word boundaries", () => {
    expect(links("See ORBIT-12, then (ORBIT-7) and ORBIT-9.")).toEqual(["ORBIT-12", "ORBIT-7", "ORBIT-9"]);
    expect(links("XORBIT-12 ORBIT-12b ORBIT_12 ORBIT- ORBIT-12-x #ORBIT-12 -ORBIT-12")).toEqual([]);
  });

  it("matches prefixes case-sensitively, and only those set", () => {
    expect(links("orbit-12 Orbit-12")).toEqual([]);
    // ACME-02 and OQ-41 are text unless their prefix is a pattern.
    expect(links("ACME-02 and OQ-41")).toEqual([]);
    expect(links("ACME-02 and OQ-41", [...patterns, { prefix: "ACME-", url: "https://x.example.com/{num}" }])).toEqual(["ACME-02"]);
    expect(links("ORBIT-12", [])).toEqual([]);
  });

  it("makes the address from the template", () => {
    const [seg] = tokenize("ORBIT-12", { patterns });
    expect(seg).toEqual({ text: "ORBIT-12", href: "https://tickets.example.com/browse/ORBIT-12", kind: "ref" });
    expect(refUrl("HELP-345", patterns)).toBe("https://help.example.com/t/345");
    expect(refUrl("HELP-x", patterns)).toBeNull();
    expect(refUrl("NOPE-1", patterns)).toBeNull();
  });

  it("reads [label](url) links and bare URLs, leaving the closing punctuation out of a URL", () => {
    const segs = tokenize("Read [the doc](https://example.com/doc), or https://www.example.com/a/b). Mail [me](mailto:maya@example.com).", {
      patterns: [],
    });
    expect(segs.filter((s) => "href" in s)).toEqual([
      { text: "the doc", href: "https://example.com/doc", kind: "link" },
      { text: "example.com", href: "https://www.example.com/a/b", kind: "url" },
      { text: "me", href: "mailto:maya@example.com", kind: "link" },
    ]);
    // The words around them are kept in order.
    expect(segs.map((s) => s.text).join("")).toBe("Read the doc, or example.com). Mail me.");
  });

  it("keeps a closing bracket that belongs to the URL", () => {
    const [, s] = tokenize("x https://example.com/a_(b)", { patterns: [] });
    expect(s).toMatchObject({ href: "https://example.com/a_(b)" });
  });

  it("never reads a key inside a link or a URL", () => {
    const text = "[ORBIT-12](https://tickets.example.com/browse/ORBIT-12) and https://example.com/ORBIT-12 and ORBIT-13";
    const segs = tokenize(text, { patterns });
    expect(segs.filter((s) => "href" in s).map((s) => s.text)).toEqual(["ORBIT-12", "example.com", "ORBIT-13"]);
    expect(
      tokenize(text, { patterns, links: false })
        .filter((s) => "href" in s)
        .map((s) => s.text),
    ).toEqual(["ORBIT-13"]);
  });

  it("is plain text with nothing to find", () => {
    expect(tokenize("Just words, 12 of them.", { patterns })).toEqual([{ text: "Just words, 12 of them." }]);
    expect(tokenize("http:// and https:// alone", { patterns: [] })).toEqual([{ text: "http:// and https:// alone" }]);
  });

  it("takes the longest prefix and escapes what it is given", () => {
    const p = [
      { prefix: "A-", url: "https://a.example.com/{num}" },
      { prefix: "A-B.", url: "https://b.example.com/{num}" },
    ];
    expect(refUrl("A-B.5", p)).toBe("https://b.example.com/5");
    expect(links("A-B.5 A-1 A-BX5", p)).toEqual(["A-B.5", "A-1"]);
  });
});

describe("patterns", () => {
  it("drops rows that can't open a page", () => {
    expect(
      validPatterns([
        { prefix: " ORBIT- ", url: " https://x.example.com/{key} " },
        { prefix: "", url: "https://x.example.com/{key}" },
        { prefix: "BAD-", url: "javascript:alert(1)" },
        { prefix: "FILE-", url: "file:///etc/passwd" },
        { prefix: "ORBIT-", url: "https://dup.example.com/{key}" },
      ]),
    ).toEqual([{ prefix: "ORBIT-", url: "https://x.example.com/{key}" }]);
    expect(validPatterns(undefined)).toEqual([]);
  });

  it("shows the link a template makes for a sample key", () => {
    expect(sampleRef(ORBIT)).toEqual({ key: "ORBIT-12", url: "https://tickets.example.com/browse/ORBIT-12" });
    expect(sampleRef({ prefix: "", url: "" })).toBeNull();
  });

  it("knows an address that leaves the app", () => {
    expect(["https://a.example.com", "http://a.example.com", "mailto:m@example.com"].every(isExternalHref)).toBe(true);
    expect(["Note.md", "wiki:Orbit", "#top", "javascript:x"].some(isExternalHref)).toBe(false);
    expect(hostOf("https://www.example.com/x")).toBe("example.com");
  });
});

describe("collectRefs", () => {
  it("gathers each once from text, a list or a property value", () => {
    expect(collectRefs(["ORBIT-12", "ORBIT-14 and ORBIT-12", 3, null], { patterns }).map((r) => r.key)).toEqual(["ORBIT-12", "ORBIT-14"]);
    expect(collectRefs("ORBIT-12", { patterns })).toEqual([
      { key: "ORBIT-12", url: "https://tickets.example.com/browse/ORBIT-12", kind: "ref" },
    ]);
    expect(collectRefs({ a: "ORBIT-12" }, { patterns })).toEqual([]);
  });
});

describe("what is never a key", () => {
  const keys = (t: string) => links(t);

  it("is anything inside a markdown link or an image, whatever it points at", () => {
    expect(keys("[ORBIT-12](Note.md) [ORBIT-13](#heading) [ORBIT-14][ref] ![ORBIT-15](pic.png)")).toEqual([]);
    expect(keys('[ORBIT-12](Note.md "a title") and ORBIT-13')).toEqual(["ORBIT-13"]);
    // A bare [label] is text, as View draws it without a definition.
    expect(keys("[ORBIT-12] and [ORBIT-13 x")).toEqual(["ORBIT-12", "ORBIT-13"]);
    // Nor in code of any length, nor in a wikilink.
    expect(keys("`ORBIT-1` ``a `ORBIT-2` b`` [[Bug ORBIT-3]] ![[ORBIT-4.png]] ORBIT-5")).toEqual(["ORBIT-5"]);
    // An unclosed one is plain text, and what is in it counts.
    expect(keys("`ORBIT-1 and [[ORBIT-2")).toEqual(["ORBIT-1", "ORBIT-2"]);
  });

  it("has no `/` before it, as in a nested tag or a path", () => {
    expect(keys("#team/ORBIT-12 docs/ORBIT-13.md and ORBIT-14")).toEqual(["ORBIT-14"]);
  });
});

describe("brackets in addresses", () => {
  const urls = (t: string) =>
    tokenize(t, { patterns: [] })
      .filter((s) => "href" in s)
      .map((s) => (s as { href: string }).href);

  it("keeps a closing bracket of a bare URL only when it closes one inside it", () => {
    expect(urls("(see https://example.com/a)")).toEqual(["https://example.com/a"]);
    expect(urls("see https://example.com/wiki/A_(b) now")).toEqual(["https://example.com/wiki/A_(b)"]);
    expect(urls("(https://example.com/wiki/A_(b))")).toEqual(["https://example.com/wiki/A_(b)"]);
    expect(urls("https://example.com/a))")).toEqual(["https://example.com/a"]);
  });

  it("stops a bare URL at a table's bar", () => {
    expect(urls("| https://example.com/a | b |")).toEqual(["https://example.com/a"]);
  });

  it("allows one level of balanced brackets in a link's address, as GFM does", () => {
    const l = (t: string) => tokenize(t, { patterns: [] }).filter((s) => "href" in s);
    expect(l("[wiki](https://example.com/A_(b)) after")).toMatchObject([{ text: "wiki", href: "https://example.com/A_(b)" }]);
    expect(l('[wiki](<https://example.com/a b> "t")')).toMatchObject([{ href: "https://example.com/a%20b" }]);
    // Two levels is no link's address; the bare address inside it is still one.
    expect(l("[deep](https://example.com/A_((b)))").map((s) => "kind" in s && s.kind)).toEqual(["url"]);
  });
});

describe("a long text", () => {
  const quick = (text: string) => {
    const t0 = performance.now();
    tokenize(text, { patterns });
    parseInline(text, patterns);
    return performance.now() - t0;
  };

  it("is scanned in a time that follows its length, whatever it is made of", () => {
    for (const unit of ["[", "[a](", "[[", "`", "``a", "![", "https://", "ORBIT-", "[a]", "(", "**"]) {
      expect(quick(unit.repeat(20000)), unit).toBeLessThan(1500);
    }
    expect(quick(`[${"a".repeat(20000)}`)).toBeLessThan(1500);
    // Runs of an opener with a far closer, as 40,000 characters of `[[x ` and a `]]` at the end.
    expect(quick(`${"[[x ".repeat(10000)}]]`)).toBeLessThan(1500);
    expect(quick(`${"[[".repeat(20000)}x]]`)).toBeLessThan(1500);
    expect(quick("[a](https://example.com/x) ORBIT-1 ".repeat(5000))).toBeLessThan(2000);
  });
});

describe("the patterns in force", () => {
  it("are one array until the setting changes", () => {
    settings.update({ referenceLinks: [ORBIT] });
    const a = refPatterns();
    expect(a).toEqual([ORBIT]);
    expect(refPatterns()).toBe(a);
    settings.update({ referenceLinks: [] });
    expect(refPatterns()).toBe(refPatterns());
    expect(refPatterns()).toEqual([]);
    const list = [ORBIT];
    expect(validPatterns(list)).toBe(validPatterns(list));
  });

  it("hold a prefix once, by the rule Settings and the settings tool use", () => {
    const rows = [
      { prefix: "ORBIT-", url: "https://a.example.com/{key}" },
      { prefix: " ORBIT- ", url: "https://b.example.com/{key}" },
      { prefix: "", url: "https://c.example.com" },
      { prefix: "X-", url: "ftp://nope" },
    ];
    expect(rows.map((_, i) => rowProblem(rows, i))).toEqual([
      null,
      "An earlier row has this prefix already.",
      "Needs a prefix, such as ORBIT-.",
      "The address must start with http:// or https:// and have no spaces.",
    ]);
    expect(validPatterns(rows)).toEqual([{ prefix: "ORBIT-", url: "https://a.example.com/{key}" }]);
    // A row that doesn't work holds no prefix for a later one.
    const later = [
      { prefix: "A-", url: "nope" },
      { prefix: "A-", url: "https://a.example.com/{key}" },
    ];
    expect(later.map((_, i) => !!rowProblem(later, i))).toEqual([true, false]);
  });
});

describe("the words of a link", () => {
  it("are plain: no backticks, no bold or italic marks", () => {
    const texts = (t: string) =>
      tokenize(t, { patterns: [] })
        .filter((s) => "href" in s)
        .map((s) => s.text);
    expect(texts("[**Spec**](https://example.com/s)")).toEqual(["Spec"]);
    expect(texts("[`cmd` and *this* and **that**](https://example.com/s)")).toEqual(["cmd and this and that"]);
    expect(collectRefs("[**Spec**](https://example.com/s)", { patterns: [] }).map((r) => r.key)).toEqual(["Spec"]);
    expect(
      collectRefs(["[**Spec**](https://example.com/s)", "[a * b * c](https://example.com/t)"], { patterns: [] }).map((r) => r.key),
    ).toEqual(["Spec", "a * b * c"]);
  });
});

describe("a task's row and what is listed for it", () => {
  it("keeps every character of the text, a private-use one among them", () => {
    const odd = "\uE000";
    const text = `a${odd}b **${odd}ORBIT-1${odd}** and *${odd}* c${odd}`;
    expect(plainOf(parseInline(text, patterns))).toBe(text.replace(/\*/g, ""));
    // The same when the text holds every private-use character there is.
    let all = "";
    for (let c = 0xe000; c <= 0xf8ff; c++) all += String.fromCharCode(c);
    // Nothing to stand for a link, so no bold around it, but not a character lost.
    expect(plainOf(parseInline(`${all} **ORBIT-2**`, patterns))).toBe(`${all} **ORBIT-2**`);
    expect(taskRefs(`${all} **ORBIT-2**`, patterns).map((r) => r.key)).toEqual(["ORBIT-2"]);
  });

  const text =
    "Ship **ORBIT-12** and `ORBIT-9`, see [[Bug ORBIT-8]], [**the spec**](https://example.com/s), ![ORBIT-7](a.png) and https://www.example.com/x. ORBIT-13";

  it("draws a link with bold or code in its words as one link, and an image as its alt text", () => {
    const tree = parseInline("[**Spec** and `cmd`](https://example.com/s) ![diagram ORBIT-1](a.png)", patterns);
    expect(tree.map((n) => n.t)).toEqual(["a", "text", "image"]);
    const a = tree[0];
    expect(a.t === "a" && a.children.map((c) => c.t)).toEqual(["strong", "text", "code"]);
    expect(plainOf(tree)).toBe("Spec and cmd diagram ORBIT-1");
  });

  it("lists exactly the links the row draws", () => {
    expect(taskRefs(text, patterns).map((r) => [r.key, r.kind])).toEqual([
      ["ORBIT-12", "ref"],
      ["the spec", "link"],
      ["example.com", "url"],
      ["ORBIT-13", "ref"],
    ]);
  });
});

describe("gatherRefs", () => {
  it("lists each address once with where it is written", () => {
    const r = (key: string) => ({ key, url: `https://tickets.example.com/browse/${key}`, kind: "ref" as const });
    const uses = gatherRefs([
      { where: "Project note", values: [[r("ORBIT-9")], []] },
      { where: "Tracker", values: [[r("ORBIT-12"), r("ORBIT-9")]] },
      { where: "task", plural: "tasks", values: [[r("ORBIT-12")], [r("ORBIT-12")], []] },
    ]);
    expect(uses.map((u) => [u.key, u.from.map(fromLabel)])).toEqual([
      ["ORBIT-9", ["Project note", "Tracker"]],
      ["ORBIT-12", ["Tracker", "2 tasks"]],
    ]);
  });
});

describe("a note's references", () => {
  const body = [
    "# Notes",
    "",
    "The plan is ORBIT-12, `ORBIT-90` and ``a `ORBIT-91` b``.",
    "",
    "````",
    "```",
    "ORBIT-92",
    "```",
    "````",
    "",
    "    ORBIT-93 in indented code",
    "",
    "<!-- ORBIT-94 -->",
    "",
    "%% ORBIT-95 %%",
    "",
    "See [[Bug ORBIT-96]], ![ORBIT-97](pic.png), [ORBIT-98](Other.md) and [the spec](https://example.com/spec).",
    "",
    "| Where | Link |",
    "|---|---|",
    "| ORBIT-14 | https://example.com/a |",
    "",
    "Bare https://www.example.com/page. [ORBIT-99][ref]",
    "",
    "- [ ] A task about ORBIT-15",
    "- [x] Done, ORBIT-16",
    "  - A note under it, ORBIT-17",
    "",
    "[ref]: https://example.com/ref",
    "",
  ].join("\n");

  it("are read from the parse View uses, not from the text", () => {
    const got = bodyRefs(body, patterns).map((r) => [r.key, r.kind]);
    // The `%%` comment isn't hidden in View, so its key is drawn as a link there and counts here.
    expect(got).toEqual(
      expect.arrayContaining([
        ["ORBIT-12", "ref"],
        ["ORBIT-95", "ref"],
        ["the spec", "link"],
        ["ORBIT-14", "ref"],
        ["example.com", "url"],
        ["ORBIT-17", "ref"],
        // A link by reference is listed by its words, which hold no key of their own.
        ["ORBIT-99", "link"],
      ]),
    );
    const keys = got.filter(([, kind]) => kind === "ref").map(([k]) => k);
    for (const never of [
      "ORBIT-90",
      "ORBIT-91",
      "ORBIT-92",
      "ORBIT-93",
      "ORBIT-94",
      "ORBIT-96",
      "ORBIT-97",
      "ORBIT-98",
      "ORBIT-99",
      "ORBIT-15",
      "ORBIT-16",
    ])
      expect(keys, never).not.toContain(never);
    // The bare address in the table cell stops at its bar.
    expect(bodyRefs(body, patterns).map((r) => r.url)).toContain("https://example.com/a");
  });

  it("lists the keys of the properties, the text and the open tasks, but not tags and aliases", () => {
    const md = `---\ntickets: [ORBIT-12, ORBIT-18]\ntags:\n  - ORBIT-19\naliases: [ORBIT-20]\nsummary: Mentions ORBIT-12\n---\n${body}`;
    // The properties as the index parses them.
    const properties = { tickets: ["ORBIT-12", "ORBIT-18"], tags: ["ORBIT-19"], aliases: ["ORBIT-20"], summary: "Mentions ORBIT-12" };
    expect(propertyText(properties)).toEqual(["ORBIT-12", "ORBIT-18", "Mentions ORBIT-12"]);
    const task = (text: string, done = false) => ({ text, done }) as never;
    const uses = noteRefs(md, [task("A task about ORBIT-15 and ORBIT-12"), task("Done, ORBIT-16", true)], patterns, properties);
    expect(uses.find((u) => u.key === "ORBIT-12")!.from.map(fromLabel)).toEqual(["property", "text", "1 task"]);
    const keys = uses.map((u) => u.key);
    expect(keys).toEqual(expect.arrayContaining(["ORBIT-18", "ORBIT-15", "ORBIT-14"]));
    for (const never of ["ORBIT-19", "ORBIT-20", "ORBIT-16"]) expect(keys, never).not.toContain(never);
  });

  it("are none with no pattern set", () => {
    expect(noteRefs(body, [], [])).toEqual([]);
  });
});

describe("every surface", () => {
  const keys = (refs: { key: string }[]) => refs.map((r) => r.key);

  it("never pairs backticks across a blank line, as a code span can't cross one", () => {
    const doc = "Use the ` key.\n\nSee ORBIT-12.\n\nRun `make`.";
    expect(links(doc)).toEqual(["ORBIT-12"]);
    expect(keys(bodyRefs(doc, patterns))).toEqual(["ORBIT-12"]);
    // In one paragraph a code span still runs over a line break.
    expect(links("A `b\nORBIT-12` c")).toEqual([]);
  });

  it("finds a key in underscore emphasis, but not one a word runs into", () => {
    const text = "see _ORBIT-12_ and __ORBIT-13__, not snake_ORBIT-14, ORBIT-15_x or _ORBIT-16_b";
    expect(links(text)).toEqual(["ORBIT-12", "ORBIT-13"]);
    expect(keys(taskRefs(text, patterns))).toEqual(["ORBIT-12", "ORBIT-13"]);
    expect(keys(bodyRefs(text, patterns))).toEqual(["ORBIT-12", "ORBIT-13"]);
  });

  it("draws a link whose words hold balanced brackets as one link", () => {
    const text = "[see [x] ORBIT-12](https://a.example.com) and ORBIT-13";
    expect(links(text)).toEqual(["see [x] ORBIT-12", "ORBIT-13"]);
    expect(keys(taskRefs(text, patterns))).toEqual(["see [x] ORBIT-12", "ORBIT-13"]);
    expect(keys(bodyRefs(text, patterns))).toEqual(["see [x] ORBIT-12", "ORBIT-13"]);
  });

  it("draws a link whose words are an image as the image's alt text, and lists it by those words", () => {
    const text = "Check [![logo](x.png)](https://example.com)";
    expect(links(text)).toEqual(["logo"]);
    expect(keys(taskRefs(text, patterns))).toEqual(["logo"]);
    expect(keys(bodyRefs(text, patterns))).toEqual(["logo"]);
  });

  it("lists a link written by reference to a web address, as View draws it", () => {
    const md =
      "See [Spec][s], [ORBIT-3][] and [Notes][n].\n\n[s]: https://example.com/spec\n[orbit-3]: https://example.com/o\n[n]: Notes.md\n";
    expect(bodyRefs(md, patterns).map((r) => [r.key, r.url, r.kind])).toEqual([
      ["Spec", "https://example.com/spec", "link"],
      ["ORBIT-3", "https://example.com/o", "link"],
    ]);
  });

  it("reads a task's continuation lines and a footnote as the note's text, as View links them", () => {
    expect(keys(bodyRefs("- [ ] Call Maya ORBIT-11\n  about ORBIT-12\n\n[^1]: ticket ORBIT-44\n", patterns))).toEqual([
      "ORBIT-12",
      "ORBIT-44",
    ]);
  });

  it("opens a link's address as CommonMark reads it, escapes out and an angle-bracket one encoded", () => {
    const text = "[x](https://a.example.com/a\\_b) and [a](<https://x.example.com/a b>)";
    const hrefs = tokenize(text, { patterns }).flatMap((s) => ("href" in s ? [s.href] : []));
    expect(hrefs).toEqual(["https://a.example.com/a_b", "https://x.example.com/a%20b"]);
    expect(taskRefs(text, patterns).map((r) => r.url)).toEqual(hrefs);
    // The References card lists the note's text with the same addresses.
    expect(bodyRefs(text, patterns).map((r) => r.url)).toEqual(hrefs);
  });

  it("lists a link whose words are another address by its words, as a task row does", () => {
    const text = "[https://a.example.com](https://b.example.com/x) and https://c.example.com/y and www.d.example.com";
    const got = bodyRefs(text, patterns).map((r) => [r.key, r.url, r.kind]);
    expect(got).toEqual([
      ["https://a.example.com", "https://b.example.com/x", "link"],
      ["c.example.com", "https://c.example.com/y", "url"],
      ["d.example.com", "http://www.d.example.com", "url"],
    ]);
    expect(taskRefs("[https://a.example.com](https://b.example.com/x)", patterns).map((r) => [r.key, r.url, r.kind])).toEqual([got[0]]);
  });

  it("holds a row to having {key} or {num} in its address", () => {
    const rows = [{ prefix: "A-", url: "https://a.example.com/tickets" }];
    expect(rowProblem(rows, 0)).toBe("The address needs {key} or {num} where the key goes.");
    expect(validPatterns(rows)).toEqual([]);
    expect(rowProblem([{ prefix: "A-", url: "https://a.example.com/t/{num}" }], 0)).toBeNull();
  });

  it("holds a row to a prefix the scan can find, not one read as code, a link or an address first", () => {
    for (const prefix of ["`T-", "[T-", "![T-", "[[T-", "https://t.example.com/T-", "http://T-"]) {
      const rows = [{ prefix, url: "https://t.example.com/{key}" }];
      expect(rowProblem(rows, 0), prefix).toBe("A prefix can't start with `, [ or a web address: those are read as code or a link.");
      expect(validPatterns(rows), prefix).toEqual([]);
    }
    for (const prefix of ["T[1-", "!T-", "h-"]) {
      const rows = [{ prefix, url: "https://t.example.com/{key}" }];
      expect(rowProblem(rows, 0), prefix).toBeNull();
      expect(keys(collectRefs(`see ${prefix}12`, { patterns: validPatterns(rows) })), prefix).toEqual([`${prefix}12`]);
    }
  });

  it("counts a task line once, by the app's reading of its status, and only an open one", () => {
    const md = "- [-] dropped ORBIT-5\n- [/] doing ORBIT-6\n- [ ] open ORBIT-7\n\nText ORBIT-8.\n";
    expect(keys(bodyRefs(md, patterns))).toEqual(["ORBIT-8"]);
    const task = (text: string, done: boolean) => ({ text, done }) as never;
    const uses = noteRefs(md, [task("dropped ORBIT-5", true), task("doing ORBIT-6", false), task("open ORBIT-7", false)], patterns);
    expect(uses.map((u) => [u.key, u.from.map(fromLabel)])).toEqual([
      ["ORBIT-8", ["text"]],
      ["ORBIT-6", ["1 task"]],
      ["ORBIT-7", ["1 task"]],
    ]);
  });

  it("reads a note saved with CRLF line endings as one saved with LF", () => {
    const lf = "# Note\n\n- [ ] Call Maya about ORBIT-2\n- [x] Done ORBIT-3\n  about ORBIT-4\n";
    const crlf = lf.replace(/\n/g, "\r\n");
    expect(keys(bodyRefs(crlf, patterns))).toEqual(keys(bodyRefs(lf, patterns)));
    const task = (text: string, done: boolean) => ({ text, done }) as never;
    const tasks = [task("Call Maya about ORBIT-2", false), task("Done ORBIT-3", true)];
    expect(noteRefs(crlf, tasks, patterns).map((u) => [u.key, u.from.map(fromLabel)])).toEqual([
      ["ORBIT-4", ["text"]],
      ["ORBIT-2", ["1 task"]],
    ]);
  });

  it("lists only the keys a task's row draws, not those in the fields it shows as chips", () => {
    const task = (text: string) => ({ text, done: false, path: "Projects/Project. Orbit.md" }) as never;
    const texts = ["Fix login \u{1F194} ORBIT-12", "Fix login ⛔ ORBIT-15", "Fix login ^ORBIT-14", "Fix [effort:: ORBIT-16] ORBIT-17"];
    expect(noteRefs("", texts.map(task), patterns).map((u) => u.key)).toEqual(["ORBIT-17"]);
  });

  it("reads a note's properties by the rule Rust reads a project's (tests/fixtures/property-text.json)", () => {
    for (const c of propertyCases.cases)
      expect([...propertyText(c.properties as Record<string, unknown>)].sort(), c.name).toEqual([...c.text].sort());
  });
});
