// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Tooltips, useDebounced } from "./ui";

describe("useDebounced", () => {
  it("waits for typing to pause, but clears at once", () => {
    vi.useFakeTimers();
    const { result, rerender } = renderHook(({ v }) => useDebounced(v, 250), { initialProps: { v: "" } });
    rerender({ v: "b" });
    rerender({ v: "bl" });
    rerender({ v: "blu" });
    expect(result.current).toBe("");
    act(() => vi.advanceTimersByTime(249));
    expect(result.current).toBe("");
    act(() => vi.advanceTimersByTime(1));
    expect(result.current).toBe("blu");
    rerender({ v: "" });
    expect(result.current).toBe("");
    vi.useRealTimers();
  });
});

describe("Tooltips", () => {
  it("hides when the pointer leaves the page, as onto a window over it", () => {
    vi.useFakeTimers();
    render(
      <>
        <button type="button" title="Minimise">
          -
        </button>
        <Tooltips />
      </>,
    );
    fireEvent.mouseOver(screen.getByRole("button"));
    act(() => vi.advanceTimersByTime(450));
    expect(screen.getByRole("tooltip").textContent).toBe("Minimise");
    fireEvent.mouseOut(screen.getByRole("button"), { relatedTarget: null });
    expect(screen.queryByRole("tooltip")).toBeNull();
    cleanup();
    vi.useRealTimers();
  });
});
