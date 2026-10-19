// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { greeting, isReviewDay, todaySummary } from "./Today";

describe("Today's greeting", () => {
  it("follows the hour", () => {
    expect(greeting(8)).toBe("Good morning");
    expect(greeting(12)).toBe("Good afternoon");
    expect(greeting(18)).toBe("Good evening");
  });
  it("adds the first name from Settings when there is one", () => {
    expect(greeting(9, "Maya Patel")).toBe("Good morning, Maya");
    expect(greeting(9, "  ")).toBe("Good morning");
    expect(greeting(20, undefined)).toBe("Good evening");
  });
});

describe("Today's summary sentence", () => {
  const say = (n: Parameters<typeof todaySummary>[0]) =>
    todaySummary(n)
      .map((p) => p.text + p.after)
      .join("");
  it("says overdue and due today always, the others when there are any", () => {
    expect(say({ overdue: 8, due: 0, deferred: 2, waiting: 4 })).toBe(
      "8 overdue, nothing due today, 2 deferred until now and 4 waiting for others.",
    );
    expect(say({ overdue: 0, due: 0, deferred: 0, waiting: 0 })).toBe("Nothing overdue and nothing due today.");
    expect(say({ overdue: 0, due: 3, deferred: 0, waiting: 1 })).toBe("Nothing overdue, 3 due today and 1 waiting for others.");
  });
  it("makes each count a jump to its band, overdue in red", () => {
    const p = todaySummary({ overdue: 2, due: 0, deferred: 0, waiting: 1 });
    expect(p.map((x) => x.to)).toEqual(["today-overdue", undefined, "today-waiting"]);
    expect(p[0].tone).toBe("red");
  });
});

describe("the weekly review reminder", () => {
  // 2026-10-02 is a Friday.
  const at = (day: number, h: number, m = 0) => new Date(2026, 9, day, h, m);
  it("shows from the review's time on its day and all the next day", () => {
    expect(isReviewDay("fri", "16:00", at(2, 15, 59))).toBe(false);
    expect(isReviewDay("fri", "16:00", at(2, 16, 0))).toBe(true);
    expect(isReviewDay("fri", "16:00", at(3, 0, 5))).toBe(true);
    expect(isReviewDay("fri", "16:00", at(3, 23, 59))).toBe(true);
    expect(isReviewDay("fri", "16:00", at(4, 9, 0))).toBe(false);
    expect(isReviewDay("fri", "16:00", at(1, 17, 0))).toBe(false);
  });
  it("follows another day and time, and defaults to Friday 16:00", () => {
    expect(isReviewDay("sun", "09:30", at(4, 9, 30))).toBe(true);
    expect(isReviewDay("sun", "09:30", at(5, 20, 0))).toBe(true);
    expect(isReviewDay("sun", "09:30", at(3, 20, 0))).toBe(false);
    expect(isReviewDay(undefined, undefined, at(2, 16, 30))).toBe(true);
    expect(isReviewDay(undefined, undefined, at(2, 10, 0))).toBe(false);
  });
});
