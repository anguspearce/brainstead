// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  accelLabel,
  captureDefault,
  dragRegion,
  fileManager,
  isMod,
  isOptCmd,
  keyLabel,
  modLabel,
  platform,
  setPlatform,
  strayCtrl,
  vaultFile,
} from "./platform";

const start = platform();
afterEach(() => setPlatform(start));

const ev = (o: Partial<KeyboardEvent>) => ({ metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...o });

describe("platform helpers", () => {
  it("leaves labels alone on macOS", () => {
    setPlatform("macos");
    expect(keyLabel("⇧⌘R")).toBe("⇧⌘R");
    expect(modLabel).toBe("⌘");
    expect(fileManager).toBe("Finder");
    expect(document.documentElement.dataset.platform).toBe("macos");
  });
  it("uses Cmd as the modifier on macOS and Ctrl elsewhere", () => {
    setPlatform("macos");
    expect(isMod(ev({ metaKey: true }))).toBe(true);
    expect(isMod(ev({ ctrlKey: true }))).toBe(false);
    expect(strayCtrl(ev({ ctrlKey: true }))).toBe(true);
    expect(isOptCmd(ev({ metaKey: true, altKey: true }))).toBe(true);
    setPlatform("windows");
    expect(isMod(ev({ ctrlKey: true }))).toBe(true);
    expect(isMod(ev({ metaKey: true }))).toBe(false);
    expect(strayCtrl(ev({ ctrlKey: true }))).toBe(false);
    expect(isOptCmd(ev({ ctrlKey: true, shiftKey: true }))).toBe(true);
    expect(isOptCmd(ev({ ctrlKey: true }))).toBe(false);
  });
  it("words the glyphs on Windows", () => {
    setPlatform("windows");
    expect(modLabel).toBe("Ctrl");
    expect(fileManager).toBe("Explorer");
    expect(keyLabel("⇧⌘R")).toBe("Ctrl+Shift+R");
    expect(keyLabel("⌘K")).toBe("Ctrl+K");
    expect(keyLabel("⌘1")).toBe("Ctrl+1");
    expect(keyLabel("⌥⌘1")).toBe("Ctrl+Shift+1");
    expect(keyLabel("⌥⌘C")).toBe("Ctrl+Alt+C");
    expect(keyLabel("⌃⌥Space")).toBe("Ctrl+Shift+Space");
    expect(keyLabel("⌘↩")).toBe("Ctrl+Enter");
    expect(keyLabel("⇧↩")).toBe("Shift+Enter");
    expect(keyLabel("⌫ Remove")).toBe("Backspace Remove");
    expect(keyLabel("⌘,")).toBe("Ctrl+,");
    expect(keyLabel("⌘.")).toBe("Ctrl+.");
    expect(keyLabel("⌥-click")).toBe("Alt-click");
    expect(keyLabel("Save (⌘S)")).toBe("Save (Ctrl+S)");
    expect(keyLabel("⌘+ ⌘− ⌘0")).toBe("Ctrl++ Ctrl+− Ctrl+0");
    expect(keyLabel("⇧⌘P · Space")).toBe("Ctrl+Shift+P · Space");
    expect(keyLabel("Nothing to change")).toBe("Nothing to change");
  });
  it("names the file manager per platform", () => {
    setPlatform("linux");
    expect(fileManager).toBe("Files");
  });
});

describe("key label fixture", () => {
  const fixture = JSON.parse(readFileSync(resolve(__dirname, "../tests/fixtures/keylabels.json"), "utf8")) as {
    labels: { mac: string; windows: string }[];
    captureDefault: { macos: string; windows: string };
  };
  it("labels every pair on Windows", () => {
    setPlatform("windows");
    for (const { mac, windows } of fixture.labels) expect(keyLabel(mac)).toBe(windows);
  });
  it("is the identity on macOS", () => {
    setPlatform("macos");
    for (const { mac } of fixture.labels) expect(keyLabel(mac)).toBe(mac);
  });
  it("matches the capture default per platform", () => {
    setPlatform("macos");
    expect(captureDefault()).toBe(fixture.captureDefault.macos);
    setPlatform("windows");
    expect(captureDefault()).toBe(fixture.captureDefault.windows);
  });
  it("keeps a recorded Key prefix on macOS and strips it elsewhere", () => {
    setPlatform("macos");
    expect(accelLabel("Control+Alt+KeyK")).toBe("⌃⌥KeyK");
    setPlatform("windows");
    expect(accelLabel("Control+Alt+KeyK")).toBe("Ctrl+Alt+K");
  });
});

describe("vaultFile", () => {
  it("joins with the root's own separator", () => {
    expect(vaultFile("/srv/Notes", "img/a.png")).toBe("/srv/Notes/img/a.png");
    expect(vaultFile("/srv/Notes/", "/img/a.png")).toBe("/srv/Notes/img/a.png");
    expect(vaultFile("C:\\Users\\maya\\Notes", "img/a b.png")).toBe("C:\\Users\\maya\\Notes\\img\\a b.png");
    expect(vaultFile("C:\\Notes\\", "a.png")).toBe("C:\\Notes\\a.png");
    expect(vaultFile("\\\\server\\share\\Notes", "x/y.md")).toBe("\\\\server\\share\\Notes\\x\\y.md");
  });
});

describe("dragRegion", () => {
  it("is where the window has no title bar: macOS and Windows", () => {
    setPlatform("macos");
    expect(dragRegion()).toEqual({ "data-tauri-drag-region": true });
    setPlatform("windows");
    expect(dragRegion()).toEqual({ "data-tauri-drag-region": true });
    setPlatform("linux");
    expect(dragRegion()).toEqual({});
  });
});
