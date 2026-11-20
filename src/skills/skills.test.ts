// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { groupSkills, skillGroup } from "../Ask";
import { findingsMarkdown } from "./DocCheck";
import { isThread, replyTo } from "./Reply";

describe("skill screens", () => {
  it("groups the / picker", () => {
    expect(["wiki-query", "daily-review", "draft-reply", "triage-bookmarks", "odd"].map(skillGroup)).toEqual([
      "Wiki",
      "Reviews",
      "Writing",
      "Inbox",
      "Other",
    ]);
    expect(
      groupSkills([{ name: "odd" }, { name: "ingest" }, { name: "weekly-review" }, { name: "daily-review" }]).map((s) => s.name),
    ).toEqual(["daily-review", "weekly-review", "ingest", "odd"]);
  });

  it("knows a captured thread", () => {
    expect(isThread("sources/Email. Thread. Re launch - 2026-10-02.md")).toBe(true);
    expect(isThread("sources/Teams. Chat. Standup - 2026-10-02.md")).toBe(true);
    expect(isThread("sources/Teams. Transcript. Sync - 2026-10-02.md")).toBe(false);
  });

  it("writes the findings as a note", () => {
    const md = findingsMarkdown("proposal.docx", {
      verdict: "Needs changes",
      summary: "Two conflicts.",
      against: {
        key: "om",
        title: "Acme Operating Model",
        version: "v2.3",
        status: "canonical",
        path: "",
        resolvedPath: "",
        exists: true,
        aliases: [],
        line: 1,
      },
      excluded: [
        {
          key: "om",
          title: "Acme Operating Model",
          version: "v2.2",
          status: "superseded",
          path: "",
          resolvedPath: "",
          exists: true,
          aliases: [],
          line: 2,
        },
      ],
      findings: [
        { kind: "diverges", title: "Funding", where: "§3 vs §4", candidate: "per project", canonical: "annual capacity", material: true },
        { kind: "aligned", title: "Pod size", where: "", candidate: "", canonical: "", material: false },
      ],
    });
    expect(md).toContain("Checked against Acme Operating Model v2.3 (in force).");
    expect(md).toContain("Not used: Acme Operating Model v2.2 (Superseded).");
    expect(md).toContain("## Conflict (material): Funding");
    expect(md).toContain("## Agrees\n\n- Pod size");
  });
});

describe("Put in Outlook / Put in Teams", () => {
  it("goes where the thread came from, and only for a captured email or chat", () => {
    expect(replyTo("sources/Email. Thread. Re Orbit App launch date - 2026-10-02.md")).toBe("Outlook");
    expect(replyTo("sources/Mail/Email. Steerco minutes - 2026-10-02.md")).toBe("Outlook");
    expect(replyTo("sources/Teams. Chat. Launch crew - 2026-10-02.md")).toBe("Teams");
    expect(replyTo("sources/Teams. Transcript. Steerco - 2026-10-02.md")).toBeNull();
    expect(replyTo("sources/Roadmap.md")).toBeNull();
    expect(replyTo(null)).toBeNull();
  });
});
