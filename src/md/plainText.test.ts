// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { dropLeadingHeading, plainSegments, plainText } from "./plainText";

describe("plainText", () => {
  it("drops emphasis and keeps the words", () => {
    expect(plainText("**Source:** the *launch* plan, ~~old~~ and ==new== `code`")).toBe("Source: the launch plan, old and new code");
  });
  it("turns links into their text", () => {
    expect(plainText("See [[Orbit App]] and [[Orbit App#Launch|the launch]] and [the brief](Briefs/Brief.md)")).toBe(
      "See Orbit App and the launch and the brief",
    );
  });
  it("drops task boxes, bullets and inline fields", () => {
    expect(plainText("- [ ] Call Maya about the launch [effort:: 2] (due:: 2026-10-01)")).toBe("Call Maya about the launch");
    expect(plainText("- [x] Done thing")).toBe("Done thing");
  });
  it("cuts a URL to its domain", () => {
    expect(plainText("Read https://www.example.com/a/b?c=1 today")).toBe("Read example.com today");
  });
  it("drops Templater code", () => {
    expect(plainText('Date: <% tp.date.now("YYYY-MM-DD") %> done')).toBe("Date: done");
  });
  it("drops Templater code the snippet cut in half", () => {
    expect(plainText('tp.file.title, "") -%>\n# Weekly plan <% tp.date.now(')).toBe("Weekly plan");
    expect(plainText('"") -%>')).toBe("");
  });
  it("leaves snake_case and maths alone", () => {
    expect(plainText("set max_retry_count to 2 * 3 * 4")).toBe("set max_retry_count to 2 * 3 * 4");
  });
  it("drops headings and block ids", () => {
    expect(plainText("## Plan ^abc123")).toBe("Plan");
  });
});

describe("plainSegments", () => {
  it("keeps the matched words marked", () => {
    expect(
      plainSegments([
        { text: "**", hit: false },
        { text: "Source", hit: true },
        { text: ":** [[Orbit App|Orbit]]", hit: false },
      ]),
    ).toEqual([
      { text: "Source", hit: true },
      { text: ": Orbit", hit: false },
    ]);
  });
});

describe("dropLeadingHeading", () => {
  const seg = (...xs: [string, boolean][]) => xs.map(([text, hit]) => ({ text, hit }));
  it("drops the heading's words from the passage's start, hits and all", () => {
    expect(dropLeadingHeading(seg(["Launch", true], [" prep Book the room", false]), "Launch prep")).toEqual(seg(["Book the room", false]));
    expect(dropLeadingHeading(seg(["Still open - Lena's plan", false]), "Daily summary 2026-11-19 › Still open")).toEqual(
      seg(["Lena's plan", false]),
    );
  });
  it("keeps a passage that only starts like the heading, or is nothing but it", () => {
    expect(dropLeadingHeading(seg(["Statusline is set", false]), "Status")).toEqual(seg(["Statusline is set", false]));
    expect(dropLeadingHeading(seg(["Status", false]), "Status")).toEqual(seg(["Status", false]));
    expect(dropLeadingHeading(seg(["Other words", false]), "Status")).toEqual(seg(["Other words", false]));
  });
});
