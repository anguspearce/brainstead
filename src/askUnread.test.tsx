// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AskScreen } from "./Ask";
import { ask, blankChat } from "./askState";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("An answer that lands while the window is closed", () => {
  it("stays unread (the sidebar's Ask badge) until the window is back", () => {
    // Closing the window only hides it: the Ask screen is still there, on the summary's tab.
    Element.prototype.scrollIntoView ??= () => {};
    const focus = vi.spyOn(document, "hasFocus").mockReturnValue(false);
    const c = { ...blankChat("claude:sonnet"), title: "Daily summary · 2026-10-12", review: true, unread: true };
    ask.set({ chats: [c], active: c.id });
    render(<AskScreen />);
    expect(ask.get().chats[0].unread).toBe(true);
    focus.mockReturnValue(true);
    act(() => void window.dispatchEvent(new Event("focus")));
    expect(ask.get().chats[0].unread).toBe(false);
  });
});
