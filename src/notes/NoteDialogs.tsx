// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Mounted once in App: shows the open note dialog (dialogs.ts) and opens New note on ⌘N.

import { useEffect } from "react";
import { useStore } from "../store";
import { noteDialog, openNewNote } from "./dialogs";
import { NewNoteDialog } from "./NewNote";
import { RenameDialog } from "./Rename";
import { isMod, strayCtrl } from "../platform";

export function NoteDialogs() {
  const d = useStore(noteDialog);
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (isMod(e) && !e.shiftKey && !e.altKey && !strayCtrl(e) && e.key === "n") {
        e.preventDefault();
        openNewNote();
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, []);
  if (!d) return null;
  return d.kind === "new" ? <NewNoteDialog initial={d.template} /> : <RenameDialog key={d.path} path={d.path} title={d.title} />;
}
