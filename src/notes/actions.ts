// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What the note editor's toolbar and the file menu can do to a file: copy it as rich text, export
// it as a PDF, rename it, move it to the Trash. The clipboard and the PDF are done in Rust (§5.3).

import { withDraftHandedOver } from "../drafts";
import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { splitTablesForPrint } from "./printTables";
import { api, EditError } from "../api";
import { nav, place } from "../nav";
import { useVaultVersion } from "../state";
import { reportEditError, undoLast } from "../taskModel";
import { toast } from "../Toast";
import { noteDialog } from "./dialogs";
import { features } from "../features";
import { fileManager } from "../platform";
import { isTranscriptPath, trashTranscript } from "../meetingFlow";
import { settings } from "../store";

/** An error from Rust as one line for a person. */
export const errText = (e: unknown) => (typeof e === "object" && e && "message" in e ? String((e as EditError).message) : String(e));

/** The rendered document as HTML and plain text, without the app's own controls. */
export function richOf(el: HTMLElement): { html: string; text: string } {
  const c = el.cloneNode(true) as HTMLElement;
  c.querySelectorAll("button, .tqhead, [data-no-copy]").forEach((n) => n.remove());
  c.querySelectorAll<HTMLInputElement>('input[type="checkbox"]').forEach((n) => n.replaceWith(n.checked ? "☑ " : "☐ "));
  return { html: c.innerHTML, text: (c.innerText ?? c.textContent ?? "").trim() };
}

/** Copies a rendered document (the read view's element). Rust inlines its vault images. */
export async function copyRich(el: HTMLElement): Promise<void> {
  if (!features.get().richCopy) return toast("Copying as rich text isn't available on this computer", undefined, "bad");
  const { html, text } = richOf(el);
  try {
    await api.copyRich(html, text);
    toast("Copied as rich text");
  } catch (e) {
    toast(errText(e), undefined, "bad");
  }
}

/** Loads every image on the page, lazy ones off the screen too, and waits for them (up to
 *  `ms`): WebKit's print doesn't load a lazy image, so the PDF would have none below the fold. */
export async function loadImagesForPrint(root: ParentNode = document, ms = 10_000): Promise<void> {
  const imgs = [...root.querySelectorAll("img")];
  for (const img of imgs) img.loading = "eager";
  const loaded = imgs.map((img) => (img.complete ? Promise.resolve() : img.decode().catch(() => {})));
  await Promise.race([Promise.all(loaded), new Promise((r) => setTimeout(r, ms))]);
}

/** Asks where to save, then prints the window's document (its print styles) to a PDF there. */
export async function exportPdf(title: string): Promise<void> {
  if (!features.get().exportPdf) return toast("Export as PDF isn't available on this computer", undefined, "bad");
  const dest = await save({ defaultPath: `${title}.pdf`, filters: [{ name: "PDF", extensions: ["pdf"] }] });
  if (!dest) return;
  // Long tables repeat their header on each page (WebKit's print doesn't): split for the print,
  // put back after.
  const { restore, forced } = splitTablesForPrint();
  toast("Exporting as PDF…");
  try {
    await loadImagesForPrint();
    await api.exportPdf(dest, title, forced);
    toast("Exported as PDF", { label: "Show", run: () => void api.reveal(dest).catch(() => {}) });
  } catch (e) {
    toast(errText(e), undefined, "bad");
  } finally {
    restore();
  }
}

export function openRename(path: string): void {
  noteDialog.set({ kind: "rename", path });
}

/** Moves a file to the vault's `.trash/`, with Undo. Leaves the document if it was open. A Teams
 *  transcript's links point at its meeting note first (trashTranscript). */
export async function trashFile(path: string): Promise<void> {
  try {
    const entry = await withDraftHandedOver(path, async () =>
      isTranscriptPath(path) ? (await trashTranscript(path)).entry! : api.trashMove(path),
    );
    const here = place.get().place;
    if (here.screen === "doc" && here.path === path) {
      if (place.get().canBack) nav.back();
      else nav.go("notes");
    }
    toast(`Moved “${entry.basename}” to the Trash`, {
      label: "Undo",
      run: () =>
        void api
          .trashRestore(entry.id, null)
          .then(() => toast("Put back"))
          .catch(reportEditError),
    });
  } catch (e) {
    reportEditError(e);
  }
}

/**
 * A vault path as the file's full path on disk, which is what Copy path copies. A vault path
 * always uses `/`; a Windows root (`C:\Users\…`) uses `\`, so the path takes the root's separator.
 */
export function fullPath(path: string): string {
  const raw = settings.get().vaultPath ?? "";
  const sep = /^([A-Za-z]:\\|\\\\)/.test(raw) ? "\\" : "/";
  const root = raw.replace(/[\\/]$/, "");
  return root ? `${root}${sep}${path.replaceAll("/", sep)}` : path;
}

/** The actions the Note menu, the toolbar and the file menu share. */
export type FileAction = "copy-markdown" | "copy-rich" | "copy-path" | "copy-title" | "export-pdf" | "rename" | "reveal" | "trash";

/** Runs one on a file; `el` is the open document's rendered element, for rich copy and PDF. */
export async function fileAction(a: FileAction, f: { path: string; title: string }, el?: HTMLElement | null): Promise<void> {
  try {
    switch (a) {
      case "copy-markdown":
        await api.copyFile(f.path);
        return toast("Markdown copied");
      case "copy-rich":
        if (el) await copyRich(el);
        return;
      case "copy-path":
        await api.copyText(fullPath(f.path));
        return toast("Path copied");
      case "copy-title":
        await api.copyText(f.title);
        return toast("Title copied");
      case "export-pdf":
        return await exportPdf(f.title);
      case "rename":
        return openRename(f.path);
      case "reveal":
        await api.reveal(f.path);
        return toast(`Shown in ${fileManager}`);
      case "trash":
        return await trashFile(f.path);
    }
  } catch (e) {
    toast(errText(e), undefined, "bad");
  }
}

/** Bookmarks or un-bookmarks a note (Me. Bookmarks.md), with Undo. */
export async function toggleBookmark(path: string): Promise<void> {
  try {
    const on = await api.bookmarkToggle(path);
    toast(on ? "Bookmarked" : "Bookmark removed", { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok");
  } catch (e) {
    reportEditError(e);
  }
}

/** Whether a note is in the bookmarks, kept current as the vault changes. */
export function useBookmarked(path: string | undefined): boolean {
  const [on, setOn] = useState(false);
  const v = useVaultVersion();
  useEffect(() => {
    if (!path) return;
    let live = true;
    api
      .bookmarks()
      .then((b) => live && setOn(b.some((x) => x.path === path)))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [path, v]);
  return on;
}
