// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";
import type { Draft } from "../api";
import { nav, place, setLeaveGuard } from "../nav";
import { EditSession, openSession, SessionIO } from "./session";

function make(disk = { content: "a\nb\nc\n", version: "v1" }, io: Partial<SessionIO> = {}) {
  const drafts = new Map<string, Draft>();
  const full: SessionIO = {
    save: vi.fn(async () => "v2"),
    draftWrite: vi.fn(async (d: Draft) => void drafts.set(d.path, d)),
    draftRead: vi.fn(async (p: string) => drafts.get(p) ?? null),
    draftDiscard: vi.fn(async (p: string) => void drafts.delete(p)),
    ...io,
  };
  let shown = disk.content;
  const s = new EditSession("Note.md", disk, full, (t) => (shown = t), 0);
  // The editor reports what it was given, as CodeMirror does.
  const type = (t: string) => {
    shown = t;
    s.edited(t);
  };
  return { s, io: full, drafts, type, shown: () => shown };
}

describe("EditSession", () => {
  it("is dirty only while the text differs from disk, and saves against the version read", async () => {
    const { s, io, type } = make();
    type("a\nB\nc\n");
    expect(s.state.dirty).toBe(true);
    expect(await s.save()).toBe(true);
    expect(io.save).toHaveBeenCalledWith("Note.md", "a\nB\nc\n", "v1");
    expect(s.state.dirty).toBe(false);
    expect(s.base.version).toBe("v2");
    type("a\nb\nc\n");
    type("a\nB\nc\n");
    expect(s.state.dirty).toBe(false);
  });

  it("keeps a draft while dirty, and drops it on save", async () => {
    const { s, drafts, type } = make();
    type("x\n");
    await s.writeDraft();
    expect(drafts.get("Note.md")?.content).toBe("x\n");
    expect(drafts.get("Note.md")?.base).toBe("v1");
    await s.save();
    expect(drafts.has("Note.md")).toBe(false);
  });

  it("offers a draft left from before, unless it's what's on disk", async () => {
    const m = make();
    m.drafts.set("Note.md", { path: "Note.md", content: "old edit\n", base: "v0", at: 1 });
    await m.s.checkDraft();
    expect(m.s.state.notice).toMatchObject({ kind: "draft", stale: true });
    m.s.restoreDraft((m.s.state.notice as { draft: Draft }).draft);
    expect(m.shown()).toBe("old edit\n");
    expect(m.s.state.dirty).toBe(true);

    const n = make();
    n.drafts.set("Note.md", { path: "Note.md", content: "a\nb\nc\n", base: "v1", at: 1 });
    await n.s.checkDraft();
    expect(n.s.state.notice).toBeNull();
    expect(n.drafts.has("Note.md")).toBe(false);
  });

  it("keeps a recovered draft until the note is edited; new edits then take its place, the offer staying", async () => {
    const m = make();
    const old = { path: "Note.md", content: "old edit\n", base: "v0", at: 1 };
    m.drafts.set("Note.md", old);
    await m.s.checkDraft();
    // A change on disk and a dismissed notice leave it alone.
    m.s.diskChanged({ content: "z\n", version: "v9" });
    m.s.dismiss();
    await m.s.writeDraft();
    expect(m.drafts.get("Note.md")).toEqual(old);
    expect(m.s.state.notice).toMatchObject({ kind: "draft" });
    // Typing is new unsaved work: it's what a draft keeps now (leaving with "Keep as draft"
    // would lose it otherwise), and the older draft can still be restored from the banner.
    m.type("typing\n");
    await m.s.writeDraft();
    expect(m.drafts.get("Note.md")?.content).toBe("typing\n");
    expect(m.s.state.notice).toMatchObject({ kind: "draft", draft: old });
    // Discarding the offer puts the banner away and leaves the new edits' draft.
    await m.s.discardDraft();
    expect(m.s.state.notice).toBeNull();
    expect(m.drafts.get("Note.md")?.content).toBe("typing\n");

    // Untouched, Discard deletes the draft itself.
    const n = make();
    n.drafts.set("Note.md", old);
    await n.s.checkDraft();
    await n.s.discardDraft();
    expect(n.drafts.has("Note.md")).toBe(false);
  });

  it("keeps the draft of edits made while a save was under way", async () => {
    let finish: (v: string) => void = () => {};
    const m = make(undefined, { save: vi.fn(() => new Promise<string>((r) => (finish = r))) });
    m.type("one\n");
    const saving = m.s.save();
    m.type("two\n");
    finish("v2");
    await saving;
    expect(m.s.state.dirty).toBe(true);
    await new Promise((r) => setTimeout(r, 5));
    expect(m.drafts.get("Note.md")?.content).toBe("two\n");
  });

  it("ignores a re-read from before the save, and catches up with an outside change made during it", async () => {
    let finish: (v: string) => void = () => {};
    const m = make(undefined, { save: vi.fn(() => new Promise<string>((r) => (finish = r))) });
    m.type("mine\n");
    const saving = m.s.save();
    finish("v2");
    await saving;
    // A read that started before the save comes back with the old file.
    m.s.diskChanged({ content: "a\nb\nc\n", version: "v1" });
    expect(m.shown()).toBe("mine\n");
    // An outside change reported mid-save is looked at once the save is done.
    m.type("mine 2\n");
    const again = m.s.save();
    m.s.diskChanged({ content: "theirs\n", version: "v7" });
    finish("v3");
    await again;
    expect(m.shown()).toBe("theirs\n");
  });

  it("lets go of a renamed or trashed note: draft written once, none after, back if that fails", async () => {
    const m = make();
    expect(openSession("Note.md")).toBe(m.s);
    m.type("edit\n");
    await m.s.detach();
    expect(m.drafts.get("Note.md")?.content).toBe("edit\n");
    m.drafts.delete("Note.md");
    m.s.close();
    await m.s.writeDraft();
    expect(m.drafts.has("Note.md")).toBe(false);
    expect(openSession("Note.md")).not.toBe(m.s);
    const n = make();
    n.type("again\n");
    await n.s.detach();
    n.drafts.delete("Note.md");
    n.s.reattach();
    await new Promise((r) => setTimeout(r, 5));
    expect(n.drafts.get("Note.md")?.content).toBe("again\n");
  });

  it("reloads quietly when clean, merges when the change is elsewhere", () => {
    const m = make();
    m.s.diskChanged({ content: "a\nb\nc\nd\n", version: "v2" });
    expect(m.shown()).toBe("a\nb\nc\nd\n");
    m.type("A\nb\nc\nd\n");
    m.s.diskChanged({ content: "a\nb\nc\nd\ne\n", version: "v3" });
    expect(m.shown()).toBe("A\nb\nc\nd\ne\n");
    expect(m.s.base.version).toBe("v3");
    expect(m.s.state.notice).toEqual({ kind: "merged" });
  });

  it("never overwrites a clashing outside change", async () => {
    const m = make(undefined, {
      save: vi.fn(async () => {
        throw { code: "changed", message: "changed", current: { content: "a\nTHEIRS\nc\n", version: "v9" } };
      }),
    });
    m.type("a\nMINE\nc\n");
    expect(await m.s.save()).toBe(false);
    expect(m.s.state.notice).toMatchObject({ kind: "conflict" });
    expect(m.shown()).toBe("a\nMINE\nc\n");
    m.s.keepMine();
    expect(m.s.state.notice).toBeNull();
    // The same outside version doesn't nag again, but saving still refuses and says why.
    m.s.diskChanged({ content: "a\nTHEIRS\nc\n", version: "v9" });
    expect(m.s.state.notice).toBeNull();
    expect(await m.s.save()).toBe(false);
    expect(m.s.state.notice).toMatchObject({ kind: "conflict" });
    m.s.reload({ content: "a\nTHEIRS\nc\n", version: "v9" });
    expect(m.shown()).toBe("a\nTHEIRS\nc\n");
    expect(m.s.state.dirty).toBe(false);
  });

  it("passes other refusals (read-only) to the caller", async () => {
    const m = make(undefined, {
      save: vi.fn(async () => {
        throw { code: "read-only", message: "read-only" };
      }),
    });
    m.type("z\n");
    await expect(m.s.save()).rejects.toMatchObject({ code: "read-only" });
    expect(m.s.state.dirty).toBe(true);
  });
});

describe("leave guard", () => {
  it("holds a move until it's let go", () => {
    nav.go({ screen: "notes" });
    let held: (() => void) | null = null;
    setLeaveGuard((go) => {
      held = go;
      return true;
    });
    nav.go({ screen: "tasks" });
    expect(place.get().place.screen).toBe("notes");
    setLeaveGuard(null);
    held!();
    expect(place.get().place.screen).toBe("tasks");
  });
});
