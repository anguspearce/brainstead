// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Renders every note in the fixture vault, and in a real vault when BRAINSTEAD_VAULT names one
// (read-only: files are only read), and fails on any error. Stage 2's "every note renders".

/// <reference types="node" />

import { cleanup, render } from "@testing-library/react";
import fs from "node:fs";
import path from "node:path";
import { afterAll, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `asset://${p}`,
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) =>
    cmd === "links_resolve" ? (args.targets as string[]).map(() => null) : cmd === "tasks_query" ? [] : null,
  ),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(), openPath: vi.fn() }));
vi.mock("mermaid", () => ({ default: { initialize: vi.fn(), render: vi.fn(async () => ({ svg: "<svg></svg>" })) } }));

import { Markdown } from "./Markdown";

const SKIP_DIRS = new Set(["images", "scripts", "node_modules"]);

function notes(root: string): string[] {
  const out: string[] = [];
  const walk = (dir: string) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      if (e.name.startsWith(".")) continue;
      const p = path.join(dir, e.name);
      if (e.isDirectory()) {
        if (dir === root && SKIP_DIRS.has(e.name)) continue;
        walk(p);
      } else if (/\.(md|txt)$/i.test(e.name)) out.push(p);
    }
  };
  walk(root);
  return out;
}

const fixture = path.resolve(import.meta.dirname, "../../tests/fixtures/vault");
const vaults = [fixture, ...(process.env.BRAINSTEAD_VAULT ? [process.env.BRAINSTEAD_VAULT] : [])];

describe.each(vaults)("every note in %s renders", (root) => {
  const errors: string[] = [];
  const spy = vi.spyOn(console, "error").mockImplementation((...a) => errors.push(a.map(String).join(" ").slice(0, 300)));
  afterAll(() => spy.mockRestore());
  it("without errors", () => {
    const files = notes(root);
    expect(files.length).toBeGreaterThan(5);
    const failed: string[] = [];
    for (const f of files) {
      const rel = path.relative(root, f).split(path.sep).join("/");
      errors.length = 0;
      try {
        const { unmount } = render(<Markdown content={fs.readFileSync(f, "utf8")} path={rel} root={root} />);
        unmount();
      } catch (e) {
        failed.push(`${rel}: ${String(e).slice(0, 200)}`);
      }
      if (errors.length) failed.push(`${rel}: ${errors[0]}`);
    }
    cleanup();
    expect(failed).toEqual([]);
  }, 600_000);
});
