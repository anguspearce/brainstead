// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS, Settings } from "./api";
import { SettingsStore } from "./store";

describe("SettingsStore", () => {
  it("saves nothing before the first read", async () => {
    const write = vi.fn(async (s: Settings) => s);
    const st = new SettingsStore(write, 10);
    st.update({ theme: "dark" });
    await st.flush();
    expect(write).not.toHaveBeenCalled();
    expect(st.get().theme).toBe("dark");
  });

  it("debounces changes into one save and adopts Rust's cleaned copy", async () => {
    vi.useFakeTimers();
    const write = vi.fn(async (s: Settings) => ({ ...s, excluded: s.excluded.map((x) => x.trim()) }));
    const st = new SettingsStore(write, 300);
    st.loadFrom(DEFAULT_SETTINGS);
    st.update({ excluded: [" old "] });
    st.update({ readOnly: false });
    expect(write).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(300);
    expect(write).toHaveBeenCalledTimes(1);
    expect(write.mock.calls[0][0]).toMatchObject({ excluded: [" old "], readOnly: false });
    expect(st.get().excluded).toEqual(["old"]);
    vi.useRealTimers();
  });

  it("saves nothing in a screenshot scene", async () => {
    const write = vi.fn(async (s: Settings) => s);
    const st = new SettingsStore(write, 0);
    st.loadFrom(DEFAULT_SETTINGS);
    st.frozen = true;
    await st.commit({ vaultPath: "/v" });
    expect(write).not.toHaveBeenCalled();
  });

  it("goes back to what Rust has when it refuses a change, so later changes still save", async () => {
    const write = vi.fn(async (s: Settings) => {
      if (s.excluded.includes("a/b")) throw new Error("A folder name can't have a slash.");
      return s;
    });
    const st = new SettingsStore(write, 0);
    st.loadFrom(DEFAULT_SETTINGS);
    await st.commit({ excluded: ["a/b"] });
    expect(st.get().excluded).toEqual(DEFAULT_SETTINGS.excluded);
    await st.commit({ theme: "dark" });
    expect(write.mock.calls[1][0]).toMatchObject({ theme: "dark", excluded: DEFAULT_SETTINGS.excluded });
    expect(st.get().theme).toBe("dark");
  });
});
