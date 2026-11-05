// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import moment from "moment";
import { cancelled, completion, inQuote, waitingToggled, withPriority } from "./edits";
import { parseTask } from "./fields";

describe("waiting for", () => {
  it("adds the tag after the text, before the fields and block id", () => {
    expect(waitingToggled("- [ ] Quote from Acme 📅 2026-10-09 ^abc").lines).toEqual([
      "- [ ] Quote from Acme #waiting-for 📅 2026-10-09 ^abc",
    ]);
    expect(waitingToggled("  * [ ] Sign-off ^x1").lines).toEqual(["  * [ ] Sign-off #waiting-for ^x1"]);
    expect(waitingToggled("- [ ] Plain").lines).toEqual(["- [ ] Plain #waiting-for"]);
  });

  it("takes it out again, leaving the rest as it was", () => {
    expect(waitingToggled("- [ ] Quote from Acme #waiting-for 📅 2026-10-09").lines).toEqual(["- [ ] Quote from Acme 📅 2026-10-09"]);
    expect(waitingToggled("- [ ] Quote #waiting-for/acme").lines).toEqual(["- [ ] Quote #waiting-for/acme #waiting-for"]);
  });
});

describe("tasks in quotes and callouts", () => {
  const today = moment("2026-10-03", "YYYY-MM-DD");
  it("reads the task inside the markers", () => {
    expect(parseTask("> - [ ] Call Lena 📅 2026-10-05")?.dates.due).toBe("2026-10-05");
    expect(parseTask("> > * [/] Nested")?.status).toBe("/");
    expect(parseTask("> [!todo] Title")).toBeNull();
  });
  it("puts the markers back on every line an edit writes", () => {
    expect(inQuote("> - [ ] Call Lena", (l) => cancelled(l, today)).lines).toEqual(["> - [-] Call Lena ❌ 2026-10-03"]);
    expect(inQuote("> > - [ ] Call Lena", (l) => withPriority(l, "high")).lines).toEqual(["> > - [ ] Call Lena ⏫"]);
    expect(inQuote("> - [ ] Bins 🔁 every week 📅 2026-10-03", (l) => completion(l, today))?.lines).toEqual([
      "> - [ ] Bins 🔁 every week 📅 2026-10-10",
      "> - [x] Bins 🔁 every week 📅 2026-10-03 ✅ 2026-10-03",
    ]);
    expect(inQuote("> - [ ] Plain", (l) => completion(l, today))).toBeNull();
    expect(inQuote("- [ ] Outside", waitingToggled).lines).toEqual(["- [ ] Outside #waiting-for"]);
  });
  it("leaves a priority emoji inside a code span alone when it sets or clears the priority", () => {
    const doc = "- [ ] Document the `⏫` marker";
    expect(withPriority(doc, "high").lines).toEqual(["- [ ] Document the `⏫` marker ⏫"]);
    // Its real priority changes; the one in the code stays.
    expect(withPriority(`${doc} 🔽`, "highest").lines).toEqual(["- [ ] Document the `⏫` marker 🔺"]);
    expect(withPriority(`${doc} 🔽`, "none").lines).toEqual([doc]);
    expect(withPriority(doc, "none").lines).toEqual([doc]);
    // A field emoji in code isn't where the fields start.
    expect(withPriority("- [ ] Use `📅` here 📅 2026-10-09", "low").lines).toEqual(["- [ ] Use `📅` here 🔽 📅 2026-10-09"]);
  });
});
