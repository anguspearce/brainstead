// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (cmd: string, _a?: Record<string, unknown>): Promise<unknown> => {
  if (cmd === "trash_move") return { id: "t1", basename: "x" };
  if (cmd === "meeting_transcripts")
    return [{ path: TRANSCRIPT, inferred: { type: "Meeting", name: "Offsite", date: "2026-10-06", exists: true } }];
  if (cmd === "doc_read") return { content: "", meta: { backlinks: [{ path: "wiki/entities/Offsite.md" }] } };
  return 1;
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (c: string, a?: Record<string, unknown>) => invoke(c, a),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const TRANSCRIPT = "sources/Teams. Transcript. Offsite - 2026-10-06.md";

import { fileAction, fullPath, loadImagesForPrint, trashFile } from "./actions";
import { settings } from "../store";

describe("Move to the Trash from the file menu", () => {
  it("points a transcript's links at its meeting note first, and trashes anything else as it is", async () => {
    await trashFile(TRANSCRIPT);
    const cmds = invoke.mock.calls.map(([c]) => c);
    expect(cmds.indexOf("health_link_ghost")).toBeGreaterThan(-1);
    expect(cmds.indexOf("health_link_ghost")).toBeLessThan(cmds.indexOf("trash_move"));
    expect(invoke.mock.calls.find(([c]) => c === "health_link_ghost")![1]).toMatchObject({
      to: "Meeting. Offsite - 2026-10-06.md",
      pages: ["wiki/entities/Offsite.md"],
    });
    invoke.mockClear();
    await trashFile("notes/Plan.md");
    expect(invoke.mock.calls.map(([c]) => c)).not.toContain("health_link_ghost");
    expect(invoke.mock.calls.find(([c]) => c === "trash_move")![1]).toEqual({ path: "notes/Plan.md" });
  });
});

describe("Copy path", () => {
  it("copies the file's full path on disk, not its path in the vault", async () => {
    settings.update({ vaultPath: "/Users/maya/Orbit notes/" });
    expect(fullPath("Projects/Plan.md")).toBe("/Users/maya/Orbit notes/Projects/Plan.md");
    invoke.mockClear();
    await fileAction("copy-path", { path: "Projects/Plan.md", title: "Plan" });
    expect(JSON.stringify(invoke.mock.calls.at(-1))).toContain("/Users/maya/Orbit notes/Projects/Plan.md");
  });

  it("takes a Windows vault folder's separator", () => {
    settings.update({ vaultPath: "C:\\Users\\maya\\Orbit notes\\" });
    expect(fullPath("Projects/Plan.md")).toBe("C:\\Users\\maya\\Orbit notes\\Projects\\Plan.md");
  });
});

describe("Export PDF", () => {
  it("loads lazy images before printing, so ones below the fold are in the PDF", async () => {
    const box = document.createElement("div");
    box.innerHTML = `<img loading="lazy" src="a.png"><img loading="lazy" src="b.png">`;
    const decode = vi.fn(async () => {});
    for (const img of box.querySelectorAll("img")) {
      Object.defineProperty(img, "complete", { value: false });
      img.decode = decode;
    }
    await loadImagesForPrint(box, 50);
    expect([...box.querySelectorAll("img")].map((i) => i.loading)).toEqual(["eager", "eager"]);
    expect(decode).toHaveBeenCalledTimes(2);
    // One that never loads doesn't hold the export up.
    box.querySelector("img")!.decode = () => new Promise(() => {});
    await loadImagesForPrint(box, 20);
  });
});
