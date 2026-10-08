// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The right-click menu for a vault file: ask about it, ingest it, bookmark it; copy title, path or
// markdown; reveal in Finder; rename with link rewrite; discard an unsaved draft; move to the
// Trash. On an open document (when it passes `doc`) also copy as rich text and export PDF. Copying happens in Rust, so it isn't lost
// when WebKit decides the click was too long ago.

import { useEffect, useState } from "react";
import { api, FileSummary } from "./api";
import { Icon } from "./icons";
import { nav } from "./nav";
import { askAboutNote } from "./Ask";
import { runSourceAction, SOURCE_ACTION, sourceActions } from "./sourceActions";
import { canIngest as ingestOffered } from "./Lists";
import { copyRich, errText, exportPdf, fullPath, openRename, toggleBookmark, trashFile, useBookmarked } from "./notes/actions";
import { Popover } from "./ui";
import { toast } from "./Toast";
import { fileManager, isMac, keyLabel } from "./platform";
import { useFeatures } from "./features";

export interface MenuState {
  at: DOMRect | null;
  file: FileSummary | null;
  open: (at: DOMRect, f: FileSummary) => void;
  openAt: (e: React.MouseEvent, f: FileSummary) => void;
  close: () => void;
}

/** A system note by its name (src-tauri/core/src/rename.rs `is_system`): the notes other features
 *  find by name, the vault's CLAUDE.md, index.md and log.md, and the summaries' monthly notes. A
 *  note that only says it's one is refused when you try. */
export function isSystemNote(path: string): boolean {
  return (
    [
      "Me. To Do List.md",
      "Me. Scratchpad.md",
      "Me. Bookmarks.md",
      "Me. Smart Lists.md",
      "Me. Canonical Docs.md",
      "CLAUDE.md",
      "index.md",
      "log.md",
    ].includes(path) || /^Me\. (Daily|Weekly) (Summaries|Reviews) - \d{4}-\d{2}\.md$/.test(path)
  );
}

export function useFileMenu(): MenuState {
  const [st, setSt] = useState<{ at: DOMRect | null; file: FileSummary | null }>({ at: null, file: null });
  return {
    ...st,
    open: (at, file) => setSt({ at, file }),
    openAt: (e, file) => {
      if ((e.target as HTMLElement).closest("input, textarea")) return;
      e.preventDefault();
      setSt({ at: new DOMRect(e.clientX, e.clientY, 0, 0), file });
    },
    close: () => setSt({ at: null, file: null }),
  };
}

/** `doc`: the open document's rendered element, for Copy rich text and Export PDF. `readFrom`: on a
 *  paragraph of the open note, reads it aloud from there. */
export function FileMenu({ menu, doc, readFrom }: { menu: MenuState; doc?: () => HTMLElement | null; readFrom?: () => void }) {
  const f = menu.file;
  const [hasDraft, setHasDraft] = useState(false);
  useEffect(() => {
    setHasDraft(false);
    if (!f || !menu.at) return;
    let live = true;
    api
      .draftRead(f.path)
      .then((d) => live && setHasDraft(!!d))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [f, menu.at]);
  const bookmarked = useBookmarked(menu.at ? f?.path : undefined);
  const features = useFeatures();
  if (!menu.at || !f) return null;
  const isText = /\.(md|txt)$/i.test(f.path);
  const system = isSystemNote(f.path);
  const systemTip = "A system note: other parts of the app look for it by name, so it can't be renamed or moved to the Trash";
  const canIngest = ingestOffered(f.path);
  const run = (fn: () => Promise<void>, done?: string) => () => {
    menu.close();
    fn()
      .then(() => done && toast(done))
      .catch((e) => toast(errText(e), undefined, "bad"));
  };
  const el = doc?.();
  // The Note menu's shortcuts (src-tauri/src/menu.rs) act on the open note, so they're shown
  // only on its menu, not on a list row's.
  const kbd = (k: string) => (doc ? <span className="kbd">{keyLabel(k)}</span> : null);
  const sc = (k: string) => (doc ? ` (${k})` : "");
  return (
    <Popover anchor={menu.at} onClose={menu.close} width={230}>
      <div className="menu" role="menu">
        {/* The note's own actions first; then copying; then out to a file or Finder; then changing
            the file; Move to the Trash on its own at the bottom. */}
        {readFrom && (
          <button
            type="button"
            role="menuitem"
            title={keyLabel("Read the note aloud from this paragraph (or ⌥-click it)")}
            onClick={() => {
              menu.close();
              readFrom();
            }}
          >
            <Icon name="speaker" size={14} />
            Read aloud from here
          </button>
        )}
        {isText && (
          <button
            type="button"
            role="menuitem"
            title="Open Ask with this note attached, to ask the model about it"
            onClick={() => {
              menu.close();
              askAboutNote(f.path);
            }}
          >
            <Icon name="ask" size={14} />
            Ask about this note
          </button>
        )}
        {/* The source's actions, as Sources and the Inbox offer them (a note's Ask is above). */}
        {sourceActions(f.path)
          .filter((a) => !(isText && a === "ask"))
          .map((a) => (
            <button
              key={a}
              type="button"
              role="menuitem"
              title={SOURCE_ACTION[a].tip(false)}
              onClick={() => {
                menu.close();
                runSourceAction(a, f.path);
              }}
            >
              <Icon name={SOURCE_ACTION[a].icon} size={14} />
              {SOURCE_ACTION[a].label(false)}
            </button>
          ))}
        {canIngest && (
          <button
            type="button"
            role="menuitem"
            title="Open Compare with this as document A, to pick the one to compare it with"
            onClick={() => {
              menu.close();
              nav.go({ screen: "compare", path: f.path });
            }}
          >
            <Icon name="shield" size={14} />
            Compare with…
          </button>
        )}
        {isText && (
          <button
            type="button"
            role="menuitem"
            title={bookmarked ? "Take this note off the sidebar's bookmarks" : "Add this note to the sidebar's bookmarks"}
            onClick={() => {
              menu.close();
              void toggleBookmark(f.path);
            }}
          >
            <Icon name="pin" size={14} />
            {bookmarked ? "Remove bookmark" : "Bookmark"}
          </button>
        )}
        {(isText || canIngest) && <div className="sep" />}
        {isText && (
          <button
            type="button"
            role="menuitem"
            title="Copy the note's markdown source to the clipboard"
            onClick={run(() => api.copyFile(f.path), "Markdown copied")}
          >
            <Icon name="copy" size={14} />
            Copy markdown
          </button>
        )}
        {el && features.richCopy && (
          <button
            type="button"
            role="menuitem"
            title="Copy the note as formatted text, to paste into email or a document"
            onClick={run(() => copyRich(el))}
          >
            <Icon name="copy" size={14} />
            Copy rich text
          </button>
        )}
        <button
          type="button"
          role="menuitem"
          title={keyLabel(`Copy the file's full path, ${fullPath(f.path)}${sc("⌥⌘C")}`)}
          onClick={run(() => api.copyText(fullPath(f.path)), "Path copied")}
        >
          <Icon name="copy" size={14} />
          Copy path
          {kbd("⌥⌘C")}
        </button>
        <button
          type="button"
          role="menuitem"
          title={`Copy the title, ${f.title}`}
          onClick={run(() => api.copyText(f.title), "Title copied")}
        >
          <Icon name="copy" size={14} />
          Copy title
        </button>
        <div className="sep" />
        {el && features.exportPdf && (
          <button
            type="button"
            role="menuitem"
            title={keyLabel(`Save the note as a PDF${sc("⌘P")}`)}
            onClick={run(() => exportPdf(f.title))}
          >
            <Icon name="export" size={14} />
            Export PDF…
            {kbd("⌘P")}
          </button>
        )}
        <button
          type="button"
          role="menuitem"
          title={keyLabel(`Show the file in a ${fileManager} window${sc("⌥⌘R")}`)}
          onClick={run(() => api.reveal(f.path), `Shown in ${fileManager}`)}
        >
          <Icon name="finder" size={14} />
          {isMac ? "Reveal in Finder" : `Show in ${fileManager}`}
          {kbd("⌥⌘R")}
        </button>
        <div className="sep" />
        <button
          type="button"
          role="menuitem"
          title={system ? systemTip : keyLabel(`Rename the file in its folder; links to it can be updated too${sc("⇧⌘R")}`)}
          disabled={system}
          onClick={() => {
            menu.close();
            openRename(f.path);
          }}
        >
          <Icon name="rename" size={14} />
          Rename…
          {kbd("⇧⌘R")}
        </button>
        {hasDraft && (
          <button
            type="button"
            role="menuitem"
            title="Throw away the unsaved changes kept for this note"
            onClick={run(() => api.draftDiscard(f.path), "Draft discarded")}
          >
            <Icon name="x" size={14} />
            Discard draft
          </button>
        )}
        <div className="sep" />
        <button
          type="button"
          role="menuitem"
          className="danger"
          title={system ? systemTip : "Move the file to Brainstead’s Trash, where it can be restored"}
          disabled={system}
          onClick={run(() => trashFile(f.path))}
        >
          <Icon name="trash" size={14} />
          Move to the Trash
        </button>
      </div>
    </Popover>
  );
}
