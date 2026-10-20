// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const open = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: (...a: unknown[]) => open(...a) }));
const vaultCreate = vi.fn();
vi.mock("./api", async (orig) => {
  const m = await orig<typeof import("./api")>();
  return { ...m, api: { ...m.api, vaultCreate: (...a: unknown[]) => vaultCreate(...a) } };
});

import { chooseVault, Welcome } from "./Permissions";
import { settings } from "./store";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  open.mockReset();
  vaultCreate.mockReset();
});

describe("Welcome", () => {
  it("creates a new vault where the user says, and opens it read-write", async () => {
    const commit = vi.spyOn(settings, "commit").mockResolvedValue(undefined as never);
    open.mockResolvedValue("/Users/maya/Documents");
    vaultCreate.mockResolvedValue("/Users/maya/Documents/Orbit notes");
    render(<Welcome />);
    expect(screen.getByText(/Getting Things Done, notes and an LLM wiki/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Create a new vault/ }));
    const name = screen.getByRole("textbox", { name: "Name of the new vault" }) as HTMLInputElement;
    expect(name.value).toBe("Brainstead");
    fireEvent.change(name, { target: { value: "Orbit notes" } });
    fireEvent.click(screen.getByRole("button", { name: "Choose where…" }));
    await waitFor(() => expect(commit).toHaveBeenCalledWith({ vaultPath: "/Users/maya/Documents/Orbit notes", readOnly: false }));
    expect(vaultCreate).toHaveBeenCalledWith("/Users/maya/Documents", "Orbit notes");
  });

  it("shows why a vault couldn't be made", async () => {
    const commit = vi.spyOn(settings, "commit").mockResolvedValue(undefined as never);
    open.mockResolvedValue("/Users/maya");
    vaultCreate.mockRejectedValue("“Brainstead” already has files in it. Choose another name, or open it with Choose folder.");
    render(<Welcome />);
    fireEvent.click(screen.getByRole("button", { name: /Create a new vault/ }));
    fireEvent.click(screen.getByRole("button", { name: "Choose where…" }));
    expect(await screen.findByText(/already has files in it/)).toBeTruthy();
    expect(commit).not.toHaveBeenCalled();
  });
});

describe("chooseVault", () => {
  it("opens a different folder read-only", async () => {
    settings.set({ ...settings.get(), vaultPath: "/Users/maya/New", readOnly: false });
    const commit = vi.spyOn(settings, "commit").mockResolvedValue(undefined as never);
    open.mockResolvedValue("/Users/maya/Notes");
    await chooseVault();
    expect(commit).toHaveBeenCalledWith({ vaultPath: "/Users/maya/Notes", readOnly: true });
  });
});
