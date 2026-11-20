// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// One open file's editing state: what was read from disk (text and version), what's in the editor,
// the draft kept in Rust while they differ, and what to do when the file changes underneath.
// Saving never overwrites an outside change: Rust refuses, and the change is merged in when it
// touches other lines, else the user chooses.

import type { Draft, EditError } from "../api";
import { merge3 } from "./merge";

export interface SessionIO {
  save: (path: string, content: string, base: string) => Promise<string>;
  draftWrite: (d: Draft) => Promise<void>;
  draftRead: (path: string) => Promise<Draft | null>;
  draftDiscard: (path: string) => Promise<void>;
}

export type Notice =
  | { kind: "draft"; draft: Draft; stale: boolean }
  /** The file changed outside and the change overlaps the unsaved edits. */
  | { kind: "conflict"; theirs: { content: string; version: string } }
  | { kind: "merged" }
  | { kind: "saved" };

export interface SessionView {
  dirty: boolean;
  saving: boolean;
  notice: Notice | null;
}

/** The session of the note open in the editor, for actions on that note from elsewhere. */
const open = new Set<EditSession>();
export const openSession = (path: string): EditSession | null => [...open].reverse().find((s) => s.path === path) ?? null;

const isEditError = (e: unknown): e is EditError => typeof e === "object" && !!e && "code" in e;

export class EditSession {
  /** The file as last read or saved. */
  base: { text: string; version: string };
  text: string;
  private view: SessionView = { dirty: false, saving: false, notice: null };
  private timer: ReturnType<typeof setTimeout> | undefined;
  private subs = new Set<() => void>();
  /** An outside version the user has already said to keep their own text over. */
  private kept: string | null = null;
  /** A draft from before is on offer (its banner): it stays as it is, neither overwritten nor
   *  thrown away, until the user restores or discards it. */
  private heldDraft = false;
  /** Versions our own saves replaced: a re-read that started before a save finished brings one
   *  back, and it isn't an outside change. */
  private superseded: string[] = [];
  /** An outside change reported while a save was under way, looked at when it's done. */
  private during: { content: string; version: string } | null = null;

  constructor(
    readonly path: string,
    disk: { content: string; version: string },
    private io: SessionIO,
    /** Puts text in the editor (a reload or a merge). */
    private setText: (t: string) => void,
    private draftDelay = 1000,
  ) {
    this.base = { text: disk.content, version: disk.version };
    this.text = disk.content;
    open.add(this);
  }

  /** The file is being renamed or trashed: the unsaved edits go into the draft now, and the
   *  session lets go of the file (no more drafts, no question on leaving). */
  detached = false;
  async detach() {
    clearTimeout(this.timer);
    if (this.dirty && !this.heldDraft)
      await this.io.draftWrite({ path: this.path, content: this.text, base: this.base.version, at: Date.now() }).catch(() => {});
    this.detached = true;
  }
  /** The rename or trash failed: the file is this session's again. */
  reattach() {
    this.detached = false;
    if (this.dirty) this.edited(this.text);
  }

  get state(): SessionView {
    return this.view;
  }
  subscribe = (f: () => void) => {
    this.subs.add(f);
    return () => {
      this.subs.delete(f);
    };
  };
  private set(p: Partial<SessionView>) {
    this.view = { ...this.view, ...p };
    this.subs.forEach((f) => f());
  }
  get dirty() {
    return this.text !== this.base.text;
  }

  /** On opening: a draft left from before (a crash, a restart, leaving without saving). */
  async checkDraft() {
    const d = await this.io.draftRead(this.path).catch(() => null);
    if (!d) return;
    if (d.content === this.base.text) await this.io.draftDiscard(this.path).catch(() => {});
    else {
      this.heldDraft = true;
      this.set({ notice: { kind: "draft", draft: d, stale: d.base !== this.base.version } });
    }
  }

  restoreDraft(d: Draft) {
    this.heldDraft = false;
    this.setText(d.content);
    this.edited(d.content);
    this.set({ notice: null });
  }

  async discardDraft() {
    // Once the note was edited the draft on disk is those edits, not the one on offer: it stays.
    const onDisk = this.heldDraft || !this.dirty;
    this.heldDraft = false;
    if (onDisk) await this.io.draftDiscard(this.path).catch(() => {});
    if (this.view.notice?.kind === "draft") this.set({ notice: null });
  }

  /** The editor's text changed. */
  edited(text: string) {
    this.text = text;
    if (this.view.dirty !== this.dirty) this.set({ dirty: this.dirty });
    clearTimeout(this.timer);
    // New edits while an older draft is on offer: they are the unsaved work now, and take the
    // draft's place on disk, or leaving ("Keep as draft"), a crash or a quit would lose them. The
    // older draft stays on offer in the banner until the note is closed.
    if (this.heldDraft && this.dirty) this.heldDraft = false;
    if (this.heldDraft || this.detached) return;
    if (this.dirty) this.timer = setTimeout(() => void this.writeDraft(), this.draftDelay);
    else void this.io.draftDiscard(this.path).catch(() => {});
  }

  async writeDraft() {
    clearTimeout(this.timer);
    if (!this.dirty || this.heldDraft || this.detached) return;
    await this.io.draftWrite({ path: this.path, content: this.text, base: this.base.version, at: Date.now() }).catch(() => {});
  }

  /** The file as it is on disk now (after a change outside, or a re-read). */
  diskChanged(theirs: { content: string; version: string }, onSave = false) {
    if (theirs.version === this.base.version) return;
    if (!onSave && this.view.saving) {
      this.during = theirs;
      return;
    }
    if (!onSave && this.superseded.includes(theirs.version)) return;
    if (theirs.content === this.base.text) {
      this.base = { text: theirs.content, version: theirs.version };
      return;
    }
    if (!this.dirty) {
      this.base = { text: theirs.content, version: theirs.version };
      this.text = theirs.content;
      this.setText(theirs.content);
      return;
    }
    const m = merge3(this.base.text, this.text, theirs.content);
    if (m.ok) {
      this.base = { text: theirs.content, version: theirs.version };
      this.setText(m.text);
      this.edited(m.text);
      this.set({ notice: { kind: "merged" } });
    } else if (onSave || this.kept !== theirs.version) this.set({ notice: { kind: "conflict", theirs } });
  }

  /** Throws the EditError when Rust refuses for any reason other than an outside change. */
  async save(): Promise<boolean> {
    if (!this.dirty || this.view.saving) return !this.dirty;
    const text = this.text;
    this.during = null;
    this.set({ saving: true });
    try {
      const before = this.base.version;
      const version = await this.io.save(this.path, text, before);
      if (version !== before) this.superseded = [...this.superseded.slice(-7), before];
      this.base = { text, version };
      if (!this.dirty) {
        clearTimeout(this.timer);
        if (!this.heldDraft) await this.io.draftDiscard(this.path).catch(() => {});
      }
      // Edits made while saving keep their draft timer (edited() set it).
      const notice = this.view.notice?.kind === "draft" ? this.view.notice : this.dirty ? null : ({ kind: "saved" } as const);
      this.set({ saving: false, dirty: this.dirty, notice });
      this.catchUp();
      return true;
    } catch (e) {
      this.set({ saving: false });
      this.catchUp();
      if (isEditError(e) && e.code === "changed" && e.current) {
        this.diskChanged(e.current, true);
        return false;
      }
      throw e;
    }
  }

  /** An outside change that came in during the save, now it's done. */
  private catchUp() {
    const t = this.during;
    this.during = null;
    if (t) this.diskChanged(t);
  }

  /** Throws away the unsaved edits for the file as it is on disk. */
  reload(theirs: { content: string; version: string } = { content: this.base.text, version: this.base.version }) {
    clearTimeout(this.timer);
    this.base = { text: theirs.content, version: theirs.version };
    this.text = theirs.content;
    this.setText(theirs.content);
    if (!this.heldDraft) void this.io.draftDiscard(this.path).catch(() => {});
    this.set({ dirty: false, notice: this.view.notice?.kind === "draft" ? this.view.notice : null });
  }

  /** Keeps the edits (as a draft) and puts the notice away; saving stays refused until a reload. */
  keepMine() {
    if (this.view.notice?.kind === "conflict") this.kept = this.view.notice.theirs.version;
    void this.writeDraft();
    this.set({ notice: null });
  }

  dismiss() {
    if (this.view.notice?.kind === "draft") return;
    this.set({ notice: null });
  }

  /** Leaving: the draft is written now, not in a second. */
  close() {
    clearTimeout(this.timer);
    if (this.dirty) void this.writeDraft();
    this.subs.clear();
    open.delete(this);
  }
}
