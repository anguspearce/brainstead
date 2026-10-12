// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { CheckRun } from "./api";
import { mergeRun, runPlace } from "./docChecks";

const run = (over: Partial<CheckRun> = {}): CheckRun => ({
  id: "doccheck-1",
  candidate: "Orbit plan v2.md",
  doc: "Orbit plan v1.md",
  mode: "differences",
  status: "running",
  started: "2026-10-12T09:00:00+02:00",
  steps: [],
  error: null,
  result: null,
  ...over,
});
const step = (name: string, status = "running") => ({ name, status, ms: 0 });

describe("mergeRun", () => {
  it("keeps the steps that arrived before doc_check_start answered", () => {
    let l = mergeRun([], run({ steps: [step("Read the document", "done"), step("Read Orbit plan v1.md")] }));
    l = mergeRun(l, run());
    expect(l).toHaveLength(1);
    expect(l[0].steps.map((s) => s.name)).toEqual(["Read the document", "Read Orbit plan v1.md"]);
  });

  it("never moves a run that ended back to running", () => {
    for (const status of ["done", "failed", "stopped"]) {
      const ended = mergeRun([], run({ status, steps: [step("Read the document", "done")] }));
      expect(mergeRun(ended, run())).toBe(ended);
      expect(mergeRun(ended, run({ steps: [step("Read the document", "done"), step("Compare them", "running")] }))).toBe(ended);
    }
  });

  it("takes later copies in place, and a new run first", () => {
    const older = run({ id: "doccheck-0", status: "done" });
    let l = mergeRun([older], run());
    expect(l.map((x) => x.id)).toEqual(["doccheck-1", "doccheck-0"]);
    l = mergeRun(l, run({ steps: [step("Read the document")] }));
    l = mergeRun(l, run({ status: "stopped", steps: [step("Read the document", "failed")] }));
    expect(l.map((x) => `${x.id} ${x.status}`)).toEqual(["doccheck-1 stopped", "doccheck-0 done"]);
  });
});

describe("runPlace", () => {
  it("opens Compare with A, B and the mode, a file from disk included", () => {
    expect(runPlace(run({ candidate: "/Users/maya/Desktop/Orbit plan.pdf", mode: "changes" }))).toEqual({
      screen: "compare",
      view: { "compare.a": "/Users/maya/Desktop/Orbit plan.pdf", "compare.b": "Orbit plan v1.md", "compare.mode": "changes" },
    });
  });

  it("opens Doc check with the document, the mode and the governing document", () => {
    expect(runPlace(run({ candidate: "sources/Acme brief.pdf", doc: "Acme policy", mode: "standard" }))).toEqual({
      screen: "doccheck",
      view: { "doccheck.candidate": "sources/Acme brief.pdf", "doccheck.mode": "standard", "doccheck.doc": "Acme policy" },
    });
    // Feedback mode has no governing document: the one chosen on the screen stays.
    expect(runPlace(run({ candidate: "sources/Acme brief.pdf", doc: "", mode: "callouts" })).view).toEqual({
      "doccheck.candidate": "sources/Acme brief.pdf",
      "doccheck.mode": "callouts",
    });
  });
});
