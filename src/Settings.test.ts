// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => p }));
import type { CliInfo } from "./api";
import { foundDetail } from "./Settings";

describe("Found on this computer", () => {
  const ollama: CliInfo = { cli: "ollama", path: null, version: null, models: [], efforts: ["low", "medium", "high"] };

  it("shows a refused OLLAMA_HOST as Rust's note, never as missing", () => {
    const note =
      "OLLAMA_HOST is gpu-box:11434. Brainstead reaches Ollama only at localhost, 127.0.0.1 or ::1, any port; set it to one of those.";
    expect(foundDetail({ ...ollama, note, refused: true })).toBe(note);
    // Not running, with no note from Rust: the row says so.
    expect(foundDetail(ollama)).toBe("Ollama isn't running, or isn't installed.");
  });

  it("counts an Ollama server's models beside its address", () => {
    const up = { ...ollama, path: "http://localhost:11434", models: [{ id: "ollama:qwen3:14b", name: "qwen3:14b", efforts: [] }] };
    expect(foundDetail(up)).toBe("1 model · http://localhost:11434");
  });
});
