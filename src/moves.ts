// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Following a file that was renamed or trashed, whoever did it (src-tauri/src/notes.rs `moved`):
// an assistant, a run, a revert in Changes, or a screen here. The open note goes with it, its
// unsaved edits into the draft where they now belong, and Today's recent list and the note's look
// follow. What the Rename and Trash dialogs already did (src/drafts.ts withDraftHandedOver) is
// left alone.

import { api } from "./api";
import { moveDocLook } from "./docLook";
import { openSession } from "./editor/session";
import { renameInProjectOrder } from "./gtd";
import { nav, place } from "./nav";
import { Recent, recentDocs } from "./recent";
import { settings } from "./store";

export async function fileMoved(m: { from: string; to: string | null; draft: string }) {
  // Unsaved edits still in an editor that didn't let go of the file: into the draft, which goes with it.
  const s = openSession(m.from);
  if (s && !s.detached) {
    await s.detach();
    const d = await api.draftRead(m.from).catch(() => null);
    if (d) {
      await api.draftWrite({ ...d, path: m.draft }).catch(() => {});
      await api.draftDiscard(m.from).catch(() => {});
    }
  }
  const recent = recentDocs();
  if (recent.some((r) => r.path === m.from)) {
    const next: Recent[] = m.to
      ? recent.filter((r) => r.path !== m.to).map((r) => (r.path === m.from ? { ...r, path: m.to! } : r))
      : recent.filter((r) => r.path !== m.from);
    settings.update({ recent: next });
  }
  if (m.to) {
    moveDocLook(m.from, m.to);
    renameInProjectOrder("projects", m.from, m.to);
  }
  const { place: here, canBack } = place.get();
  if (here.screen !== "doc" || here.path !== m.from) return;
  if (m.to) nav.replace({ ...here, path: m.to });
  else if (canBack) nav.back();
  else nav.go("notes");
}

export function startMoves() {
  void api.onFileMoved((m) => void fileMoved(m));
}
