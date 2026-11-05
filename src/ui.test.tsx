// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useRef } from "react";
import { Tooltips, useDebounced, useDismiss } from "./ui";

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

describe("useDismiss", () => {
  function Box({ onClose, keep }: { onClose: () => void; keep?: string }) {
    const ref = useRef<HTMLDivElement>(null);
    useDismiss(ref, onClose, true, keep);
    return (
      <>
        <div ref={ref}>inside</div>
        <div className="winctl">
          <button type="button">Minimise</button>
        </div>
        <p>outside</p>
      </>
    );
  }

  it("closes on a click outside, but not on what it keeps, such as the window's own buttons", () => {
    vi.useFakeTimers();
    const onClose = vi.fn();
    render(<Box onClose={onClose} keep=".winctl" />);
    act(() => vi.advanceTimersByTime(1));
    fireEvent.mouseDown(screen.getByText("inside"));
    fireEvent.mouseDown(screen.getByRole("button", { name: "Minimise" }));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.mouseDown(screen.getByText("outside"));
    expect(onClose).toHaveBeenCalledTimes(1);
    cleanup();
    render(<Box onClose={onClose} />);
    act(() => vi.advanceTimersByTime(1));
    fireEvent.mouseDown(screen.getByRole("button", { name: "Minimise" }));
    expect(onClose).toHaveBeenCalledTimes(2);
    cleanup();
    vi.useRealTimers();
  });
});
