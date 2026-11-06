// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const resolved: Record<string, string> = { "Orbit App": "wiki/entities/Orbit App.md", OA: "wiki/entities/Orbit App.md" };
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `asset://${p}`,
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) => {
    if (cmd === "links_resolve") return (args.targets as string[]).map((t) => resolved[t] ?? null);
    if (cmd === "tasks_query" || cmd === "tasks_all")
      return [
        {
          path: "A.md",
          title: "A",
          line: 3,
          lineText: "- [ ] Ship it 📅 2026-10-03 #followup",
          status: " ",
          text: "Ship it 📅 2026-10-03 #followup",
          done: false,
          due: "2026-10-03",
          scheduled: null,
          doneOn: null,
          created: null,
          rank: null,
          tags: ["followup"],
          heading: null,
        },
      ];
    if (cmd === "asset_find") return args.name === "diagram.png" ? "images/diagram.png" : null;
    return null;
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
const openUrl = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl, openPath: vi.fn() }));
vi.mock("mermaid", () => ({ default: { initialize: vi.fn(), render: vi.fn(async () => ({ svg: "<svg data-test='mmd'></svg>" })) } }));

import { imageSource, Markdown } from "./Markdown";
import { nav } from "../nav";
import { ErrorBoundary } from "../ErrorBoundary";
import { settings } from "../store";

afterEach(cleanup);

const md = (content: string, q?: string) => render(<Markdown content={content} path="Note.md" root="/vault" q={q} />);

describe("Markdown", () => {
  it("links wikilinks, marks ghost links and follows headings", async () => {
    const { container } = md("See [[Orbit App]], [[OA|the app]], [[Orbit App#Current state]] and [[Nowhere]].");
    await waitFor(() => expect(container.querySelector("a.ghost")).toBeTruthy());
    const links = [...container.querySelectorAll("a.wl")];
    expect(links.map((a) => a.textContent)).toEqual(["Orbit App", "the app", "Orbit App › Current state", "Nowhere"]);
    expect(links[3].className).toContain("ghost");
    expect(links[0].className).not.toContain("ghost");
  });

  it("drops the frontmatter, ranks and block ids, and gives headings ids", () => {
    const { container } = md("---\ntitle: x\n---\n# Top Heading\n- [ ] task ^rank-1024\n\nPara with a block. ^q3\n");
    expect(container.textContent).not.toContain("title: x");
    expect(container.textContent).not.toContain("rank-1024");
    expect(container.querySelector("#top-heading")).toBeTruthy();
    expect(container.querySelector('[id="^q3"]')?.textContent).toBe("Para with a block.");
  });

  it("folds the system note", () => {
    md("> **This is a system note** — Master task list.\n\nBody");
    expect(screen.getByText("System note").closest("details")).toBeTruthy();
  });

  it("draws callouts with their type's look, title and fold", () => {
    const { container } = md(
      "> [!warning] Mind the **gap**\n> Body text\n\n> [!TIP]\n> Untitled\n\n> [!faq]- Closed\n> Hidden\n\n> [!odd]\n> > [!quote] Inner\n\n> Plain quote",
    );
    const boxes = [...container.querySelectorAll(".callout")];
    expect(boxes.map((b) => b.getAttribute("data-tone"))).toEqual(["amber", "teal", "amber", "blue", "grey"]);
    expect(boxes[0].querySelector(".callout-title")?.textContent).toBe("Mind the gap");
    expect(boxes[0].querySelector(".callout-body")?.textContent).toBe("Body text");
    expect(boxes[1].querySelector(".callout-title")?.textContent).toBe("Tip");
    expect(boxes[2].tagName).toBe("DETAILS");
    expect((boxes[2] as HTMLDetailsElement).open).toBe(false);
    expect(boxes[3].querySelector(".callout")).toBe(boxes[4]);
    expect(container.textContent).not.toContain("[!");
    expect(container.querySelectorAll("blockquote")).toHaveLength(1);
  });

  it("ticks a task inside a callout on its own line", () => {
    const onTick = vi.fn();
    const { container } = render(<Markdown content={"# T\n\n> [!todo] Week\n> - [ ] Call Lena"} path="T.md" root="/v" onTick={onTick} />);
    (container.querySelector(".callout input.tick") as HTMLElement | null)?.click();
    expect(onTick).toHaveBeenCalledWith(3, true);
  });

  it("keeps code as code and highlights search words outside it", () => {
    const { container } = md("Launch date here.\n\n```js\nconst launch = 1; // [[Not a link]]\n```", "launch");
    expect(container.querySelector("mark.search-hit")?.textContent).toBe("Launch");
    expect(container.querySelector("pre mark")).toBeNull();
    expect(container.querySelector("pre a")).toBeNull();
  });

  it("renders math, task queries and mermaid", async () => {
    const { container } = md("$$\nx^2\n$$\n\n```tasks\nnot done\n```\n\n```mermaid\ngraph LR\n A-->B\n```");
    await waitFor(() => expect(container.querySelector(".katex")).toBeTruthy());
    await waitFor(() => expect(screen.getByText("Ship it")).toBeTruthy());
    await waitFor(() => expect(container.querySelector("[data-test=mmd]")).toBeTruthy());
  });

  it("draws images at the width their markdown gives, which the PDF prints", async () => {
    const { container } = md(
      'A ![[diagram.png|300]] B ![chart|250](images/c.png) C <img height="287" width="717" src="/api/vault-assets/images/p.png" />',
    );
    await waitFor(() => expect(container.querySelectorAll("img")).toHaveLength(3));
    const imgs = [...container.querySelectorAll("img")];
    expect(imgs.map((i) => i.getAttribute("width"))).toEqual(["300", "250", "717"]);
    expect(imgs.map((i) => i.alt)).toEqual(["diagram.png", "chart", ""]);
  });

  it("opens a mermaid diagram full screen and closes it with Escape", async () => {
    md("```mermaid\ngraph LR\n A-->B\n```");
    fireEvent.click(await screen.findByRole("button", { name: "Full screen" }));
    const full = await screen.findByRole("dialog", { name: "Diagram" });
    await waitFor(() => expect(full.querySelector("[data-test=mmd]")).toBeTruthy());
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Diagram" })).toBeNull());
  });

  it("answers a task query for the note it's drawn from, not the place on screen", async () => {
    nav.go({ screen: "doc", path: "Elsewhere.md" });
    const { container } = md("```tasks\nexplain\npath includes {{query.file.path}}\n```");
    await waitFor(() => expect(container.querySelector(".tqexplain")?.textContent).toContain("path includes Note.md"));
  });

  it("removes scripts and handlers from raw HTML", () => {
    const { container } = md('<img src="x.png" onerror="alert(1)"><script>alert(2)</script><b>kept</b>');
    expect(container.querySelector("script")).toBeNull();
    expect(container.innerHTML).not.toContain("onerror");
    expect(container.querySelector("b")?.textContent).toBe("kept");
  });

  it("maps the previous app's image URLs back to the vault", async () => {
    const { container } = md(
      '<img height="287" width="717" src="/api/vault-assets/images/Pasted-image-1.png" />\n\n<img src="/api/vault-assets/by-name/diagram.png">',
    );
    await waitFor(() => expect(container.querySelectorAll("img").length).toBe(2));
    const [a, b] = [...container.querySelectorAll("img")];
    expect(a.getAttribute("src")).toBe("asset:///vault/images/Pasted-image-1.png");
    expect(a.getAttribute("width")).toBe("717");
    await waitFor(() => expect(b.getAttribute("src")).toBe("asset:///vault/images/diagram.png"));
  });

  it("finds embedded images by name", async () => {
    const { container } = md("![[diagram.png]] and ![[missing.png]]");
    await waitFor(() => expect(container.querySelector("img")?.getAttribute("src")).toBe("asset:///vault/images/diagram.png"));
    await waitFor(() => expect(container.querySelector(".ghostimg")?.textContent).toContain("missing.png"));
  });

  it("ticks a task in View by its line in the file, past the properties", () => {
    const ticks: [number, boolean][] = [];
    const src = "---\ntags: [a]\n---\n# T\n\n- [ ] one\n- [x] two ✅ 2026-10-01\n";
    const { container } = render(<Markdown content={src} path="A.md" root="/vault" onTick={(l, d) => ticks.push([l, d])} />);
    const boxes = container.querySelectorAll<HTMLInputElement>("input[type=checkbox]");
    expect(boxes).toHaveLength(2);
    boxes[0].click();
    boxes[1].click();
    expect(ticks).toEqual([
      [5, true],
      [6, false],
    ]);
  });

  it("draws task dates as icons with their meaning; in code and plain text only the emoji becomes an icon", () => {
    const { container } = md(
      "- [ ] ship 📅 2020-01-02 ⏳ 2999-01-01 `📅 2020-01-03`\n- [x] old ✅ 2026-10-01\n\nNot a task 📅 2020-01-04\n",
    );
    const chips = [...container.querySelectorAll(".tdate")];
    expect(chips.map((c) => c.className.split(" ")[1])).toEqual(["due", "scheduled", "done"]);
    expect(chips[0].className).toContain("is-late");
    expect(chips[0].getAttribute("title")).toContain("must be done by");
    expect(container.textContent).not.toContain("📅 2020-01-02");
    expect(container.querySelector("code")?.textContent).toBe(" 2020-01-03");
    expect(container.querySelector("code .femoji")).toBeTruthy();
    expect(container.textContent).toContain("Not a task  2020-01-04");
    expect(container.textContent).not.toMatch(/📅/u);
  });

  it("strikes through a done task's text, not its open subtasks", () => {
    const { container } = md("- [x] parent done\n  - [ ] child open\n- [ ] other");
    const struck = [...container.querySelectorAll(".tdone")].map((e) => e.textContent?.trim());
    expect(struck).toEqual(["parent done"]);
  });

  it("draws #tags, but not in code, links or headings", () => {
    const { container } = md("# Title\n\nA #todo and #area/sub-1 here, `#code`, [x](#frag) and #123.");
    expect([...container.querySelectorAll(".mdtag")].map((e) => e.textContent)).toEqual(["#todo", "#area/sub-1"]);
  });
  it("draws ==highlights==, across bold and links, but not in code or around spaces", () => {
    const { container } = md("A ==plain== and ==**bold** [[Note]] too==, `==code==`, a == b == c and ==open.");
    const marks = [...container.querySelectorAll("mark.hl")];
    expect(marks.map((e) => e.textContent)).toEqual(["plain", "bold Note too"]);
    expect(marks[1].querySelector("strong")).not.toBeNull();
    expect(container.textContent).toContain("==code==");
    expect(container.textContent).toContain("a == b == c and ==open.");
  });
});

describe("highlighter colours", () => {
  it("keeps a mark's colour, and draws == as yellow", () => {
    const { container } = md('A <mark class="hl-red">red</mark>, <mark class="hl-teal">teal</mark> and ==yellow==.');
    expect([...container.querySelectorAll("mark")].map((m) => m.className)).toEqual(["hl-red", "hl-teal", "hl"]);
  });
});

describe("Tasks-format emoji outside tasks", () => {
  it("draws them as icons in prose and inline code, never as emoji", () => {
    const { container } = md("> Closed tasks (`✅ YYYY-MM-DD`) and due dates 📅 here.");
    expect(container.textContent).not.toMatch(/[✅📅]/u);
    expect(container.querySelectorAll(".femoji").length).toBe(2);
  });
});

describe("raw HTML that would trip the renderer", () => {
  it("draws notes with bad data-wiki, data-tdate and malformed % instead of throwing", async () => {
    const err = vi.spyOn(console, "error").mockImplementation(() => {});
    const { container } = md(
      'A <a data-wiki="{not json">bad link</a>, a <span data-tdate="nope">bad date</span>, <span data-tdate="1">num</span>, <img src="pic%E0%A4%A.png" alt="pic"> and [odd](100%25%zz.md).',
    );
    await waitFor(() => expect(container.textContent).toContain("bad link"));
    expect(container.textContent).toContain("bad date");
    expect(container.textContent).toContain("odd");
    expect(container.querySelector(".prose")).not.toBeNull();
    expect(container.textContent).not.toContain("couldn’t be drawn");
    (container.querySelector("a.wl") as HTMLAnchorElement | null)?.click();
    err.mockRestore();
  });
  it("imageSource keeps a malformed % as it is", () => {
    expect(imageSource("a%zz.png")).toEqual({ path: "a%zz.png" });
    expect(imageSource("wiki:b%E0.png")).toEqual({ name: "b%E0.png" });
  });
  it("a note that fails to draw says so in its place rather than blanking the window", () => {
    const err = vi.spyOn(console, "error").mockImplementation(() => {});
    const Boom = () => {
      throw new Error("kaboom");
    };
    const { container } = render(
      <ErrorBoundary fallback={(e) => <p className="faint">This couldn’t be drawn: {e.message}</p>}>
        <Boom />
      </ErrorBoundary>,
    );
    expect(container.textContent).toContain("kaboom");
    err.mockRestore();
  });
});

describe("a template in View", () => {
  it("explains a Templater name on hover, as the editor does", async () => {
    const { container } = render(<Markdown content={"Today <% tp.date.now() %>\n"} path="Templates/Day.md" root="" templater />);
    const now = [...container.querySelectorAll<HTMLElement>("[data-tp]")].find((e) => e.textContent === "now");
    expect(now).toBeTruthy();
    fireEvent.mouseOver(now!);
    await waitFor(() => expect(container.querySelector(".tpview")?.textContent).toMatch(/date/i));
    // The box stays while the pointer is on it, so its Docs link can be clicked…
    const box = container.querySelector<HTMLElement>(".tpview")!;
    fireEvent.mouseOver(box);
    expect(box.querySelector("a.qdocs")).toBeTruthy();
    await new Promise((r) => setTimeout(r, 400));
    expect(container.querySelector(".tpview")).toBeTruthy();
    // …and goes a moment after it leaves.
    fireEvent.mouseLeave(container.querySelector(".prose")!);
    await waitFor(() => expect(container.querySelector(".tpview")).toBeNull());
  });
});

describe("reference keys in View", () => {
  const set = (on: boolean) =>
    settings.update({ referenceLinks: on ? [{ prefix: "ORBIT-", url: "https://tickets.example.com/browse/{key}" }] : [] });
  afterEach(() => set(false));
  const body = [
    "Blocked on ORBIT-12 and ORBIT-14.",
    "",
    "Not `ORBIT-98` in code, [ORBIT-97](https://example.com/x) in a link, or XORBIT-96.",
    "",
    "```",
    "ORBIT-95",
    "```",
  ].join("\n");

  it("turn a configured key into a link that opens in the browser, outside code and links", () => {
    set(true);
    const { container } = md(body);
    const refs = [...container.querySelectorAll<HTMLAnchorElement>("a[data-ref]")];
    expect(refs.map((a) => a.textContent)).toEqual(["ORBIT-12", "ORBIT-14"]);
    expect(refs[0].getAttribute("href")).toBe("https://tickets.example.com/browse/ORBIT-12");
    expect(refs[0].className).toContain("mdref");
    fireEvent.click(refs[1]);
    expect(openUrl).toHaveBeenLastCalledWith("https://tickets.example.com/browse/ORBIT-14");
    // The text of what was left alone is as written.
    expect(container.querySelector("code")?.textContent).toBe("ORBIT-98");
    expect(container.querySelector("pre")?.textContent).toContain("ORBIT-95");
    expect(container.textContent).toContain("XORBIT-96");
    expect(container.querySelectorAll("a").length).toBe(3);
  });

  it("are not keys inside a link to a note or a heading, a reference link or an image, or after a slash", () => {
    set(true);
    const { container } = md(
      "[ORBIT-1](Note.md) [ORBIT-2](#top) [ORBIT-3][ref] ![ORBIT-4](pic.png) #team/ORBIT-5 docs/ORBIT-6.md and ORBIT-7",
    );
    expect([...container.querySelectorAll("a[data-ref]")].map((a) => a.textContent)).toEqual(["ORBIT-7"]);
  });

  it("change nothing with no pattern set", () => {
    const { container } = md(body);
    expect(container.querySelector("a[data-ref]")).toBeNull();
    expect(container.textContent).toContain("Blocked on ORBIT-12 and ORBIT-14.");
  });
});
