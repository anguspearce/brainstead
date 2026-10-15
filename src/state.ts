// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What the window knows about the vault and the app's permissions, kept current from Rust's events.

import { api, Permissions, VaultChanges, VaultStatus } from "./api";
import { settings, Store, useStore } from "./store";

export const EMPTY_STATUS: VaultStatus = {
  state: "none",
  vaultPath: null,
  error: null,
  stats: {
    files: 0,
    notes: 0,
    wiki: 0,
    sources: 0,
    templates: 0,
    tasks: 0,
    openTasks: 0,
    links: 0,
    unresolvedLinks: 0,
    updatedAt: 0,
    lastSyncMs: 0,
  },
};

export const vaultStatus = new Store<VaultStatus>(EMPTY_STATUS);
/** Bumped whenever files in the vault change (or the index is rebuilt), with what changed, so views reload. */
export const vaultVersion = new Store<{ n: number; changes: VaultChanges | null }>({ n: 0, changes: null });
export const useVaultVersion = () => useStore(vaultVersion).n;
export const permissions = new Store<Permissions | null>(null);

/** The vault is still being opened or indexed: what can't be read yet isn't a failure, and views
 *  reload when it's ready (the version bump above). */
export function useVaultOpening(): boolean {
  const st = useStore(vaultStatus).state;
  const path = useStore(settings).vaultPath;
  return st === "indexing" || (st === "none" && !!path);
}

let checking = 0;
/** Asks again, e.g. when the window comes back from System Settings. Only the latest answer counts. */
export async function recheckPermissions() {
  const n = ++checking;
  const p = await api.permissions().catch(() => null);
  if (n === checking && p) permissions.set(p);
}

export async function startStatus() {
  // Every change of status is an event, so once one has come the first read's answer is the
  // older of the two (it can arrive after an event the vault opening sent meanwhile).
  let heard = false;
  await api.onIndexStatus((s) => {
    heard = true;
    const was = vaultStatus.get().state;
    vaultStatus.set(s);
    // A finished (re)index changes everything.
    if (s.state === "ready" && was !== "ready") vaultVersion.set({ n: vaultVersion.get().n + 1, changes: null });
  });
  await api.onVaultChanged((c) => vaultVersion.set({ n: vaultVersion.get().n + 1, changes: c }));
  const first = await api.vaultStatus();
  if (!heard) vaultStatus.set(first);
}
