// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What the vault takes on disk, as Settings › Vault shows it and app_status says it: one rule for both.

import { useEffect, useState } from "react";
import { api, DiskUsage, IndexStats, VaultUsage } from "./api";
import { useVaultVersion } from "./state";
import { fmtBytes, fmtCount } from "./ui";

/** The vault's size: every file but those in dot folders and excluded folders. */
export const vaultBytes = (u: VaultUsage) => u.bytes - u.excludedBytes;

/** The vault's size by part, biggest first, leaving out parts with nothing in them. "Other files"
 * is what the index doesn't keep: attachments, scripts, the schema files, conflict copies. */
export function vaultParts(s: IndexStats, u: VaultUsage): [string, number][] {
  const other = Math.max(0, vaultBytes(u) - u.imageBytes - s.bytes);
  const parts: [string, number][] = [
    ["notes", s.noteBytes],
    ["wiki", s.wikiBytes],
    ["sources", s.sourceBytes],
    ["templates", s.templateBytes],
    ["images", u.imageBytes],
    ["other files", other],
  ];
  return parts.filter(([, b]) => b > 0).sort((a, b) => b[1] - a[1]);
}

/** One line for app_status: the total, its parts, the excluded folders and Brainstead's own data. */
export function diskUsageText(s: IndexStats, d: DiskUsage): string {
  const u = d.vault;
  const vault = u
    ? `the vault takes ${fmtBytes(vaultBytes(u))} in ${fmtCount(u.files - u.excluded)} files (${vaultParts(s, u)
        .map(([l, b]) => `${l} ${fmtBytes(b)}`)
        .join(", ")})` + (u.excluded ? `; excluded folders ${fmtBytes(u.excludedBytes)} more, not counted` : "")
    : "no vault is open";
  return `${vault}; Brainstead's own data, outside the vault, ${fmtBytes(d.appDataBytes)}, of which Changes' history ${fmtBytes(d.changesBytes)}`;
}

/** The disk usage, read again whenever the vault changes. */
export function useDiskUsage(): DiskUsage | null {
  const v = useVaultVersion();
  const [d, setD] = useState<DiskUsage | null>(null);
  useEffect(() => {
    let live = true;
    void api
      .diskUsage()
      .then((x) => live && setD(x))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [v]);
  return d;
}
