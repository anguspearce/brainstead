import { describe, expect, it } from "vitest";
import { BURST_MS, SETTLE_MS, strayKey, type StrayState } from "./strayKeys";

describe("strayKey", () => {
  it("lets a lone key through once the window has settled", () => {
    const s: StrayState = { focusedAt: 0, lastKey: -Infinity };
    expect(strayKey(s, SETTLE_MS + 10)).toBe(false);
  });
  it("ignores keys just after the window comes back to the front", () => {
    const s: StrayState = { focusedAt: 1000, lastKey: -Infinity };
    expect(strayKey(s, 1000 + SETTLE_MS - 1)).toBe(true);
  });
  it("ignores a key that follows another quickly, as typing a password does", () => {
    const s: StrayState = { focusedAt: -Infinity, lastKey: -Infinity };
    expect(strayKey(s, 10_000)).toBe(false);
    expect(strayKey(s, 10_000 + BURST_MS - 50)).toBe(true);
    expect(strayKey(s, 10_000 + BURST_MS + 100)).toBe(true);
    expect(strayKey(s, 20_000)).toBe(false);
  });
});
