// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Inbox list's divider and the Inbox's own keys: Enter on the divider puts the list back to its
// width and nothing else; Enter anywhere else applies the open item's suggestion (a vault change).

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { InboxItem } from "./api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn(async () => {}) }));

const capture: InboxItem = {
  kind: "capture",
  path: "sources/Orbit App plan.pdf",
  line: 0,
  text: "Orbit App plan",
  lineText: "Added to Sources",
  stamp: null,
};

// Every call the screen makes answers with nothing, but for the Inbox, its projects, the weekly
// preparation's suggestion for the capture (keep it in Sources) and the write that suggestion makes.
const known: Record<string, (...a: unknown[]) => unknown> = {
  inboxList: async () => [capture],
  projectsList: async () => [],
  weekprepStatus: async () => ({
    prep: { suggestions: [{ text: "A plan to keep", action: { do: "clarify", item: capture, becomes: "reference", text: "" } }] },
  }),
  inboxCaptureDone: vi.fn(async () => undefined),
};
const api = vi.hoisted(
  () =>
    new Proxy({} as Record<string, unknown>, {
      get: (_, k: string) =>
        known[k] ?? (k.startsWith("on") ? async () => () => {} : /(List|Transcripts)$/.test(k) ? async () => [] : async () => undefined),
    }),
);
vi.mock(import("./api"), async (orig) => ({ ...(await orig()), api: api as never }));

const { InboxScreen } = await import("./Inbox");

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("the Inbox list's divider", () => {
  it("takes its own Enter, so the open item's suggestion isn't applied", async () => {
    render(<InboxScreen />);
    // The suggestion is there (its Accept button) before the divider has the keyboard.
    await screen.findByTitle(/^Do what the AI suggests/);
    const divider = screen.getByRole("separator", { name: "Resize the Inbox list" });
    divider.focus();
    fireEvent.keyDown(divider, { key: "Enter" });
    await new Promise((r) => setTimeout(r, 50));
    expect(known.inboxCaptureDone).not.toHaveBeenCalled();
    // Enter off the divider is the suggestion's key, and does write: the control for the above.
    fireEvent.keyDown(document.body, { key: "Enter" });
    await waitFor(() => expect(known.inboxCaptureDone).toHaveBeenCalledWith(capture.path));
  });
});
