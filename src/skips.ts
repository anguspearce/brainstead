// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Skips suggested from what the user deletes (src-tauri/src/sync/skips.rs, D-20261118-15):
// accepting one puts its sender or chat on the sync's skip list, as Settings › Teams and Outlook sync
// and the MCP's teams_sync both do it.

import { api, SkipSuggestion } from "./api";
import { settings } from "./store";

/** Skip: the sender on Mail to skip, or the chat on Chats to skip; its count starts again. */
export async function acceptSkip(s: SkipSuggestion): Promise<void> {
  const cur = settings.get();
  if (s.kind === "chat") {
    const have = cur.syncIgnoreChats ?? [];
    if (!have.includes(s.value)) settings.update({ syncIgnoreChats: [...have, s.value] });
  } else {
    const have = cur.syncIgnoreSenders ?? [];
    if (!have.includes(s.value)) settings.update({ syncIgnoreSenders: [...have, s.value] });
  }
  await api.skipSuggestionForget(s.key);
}

/** A chat taken off Chats to skip: synced again, and its deletes counted afresh. */
export async function unskipChat(id: string): Promise<void> {
  settings.update({ syncIgnoreChats: (settings.get().syncIgnoreChats ?? []).filter((x) => x !== id) });
  await api.skipSuggestionForget(`chat:${id}`);
}
