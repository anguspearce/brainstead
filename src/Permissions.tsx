// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// First run: Full Disk Access, then the vault folder, or a new vault made with the example notes
// (the archived build plan, §2, "Permissions at startup"). The same Full Disk Access card is in
// Settings › Permissions.

import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "./api";
import { errText } from "./notes/actions";
import { Icon, Mark } from "./icons";
import { permissions, recheckPermissions } from "./state";
import { settings, useStore } from "./store";
import { useFeatures } from "./features";

/** Checks again whenever the window comes back to the front (from System Settings, say). */
export function useRecheckOnFocus() {
  useEffect(() => {
    const f = () => void recheckPermissions();
    window.addEventListener("focus", f);
    return () => window.removeEventListener("focus", f);
  }, []);
}

/** Asks for the vault folder with the system's open panel. A different folder opens read-only:
 * it's the user's own notes, which Brainstead hasn't been trusted with yet. */
export async function chooseVault(): Promise<boolean> {
  const cur = settings.get().vaultPath;
  const dir = await open({ directory: true, title: "Choose your notes vault", defaultPath: cur ?? undefined }).catch(() => null);
  if (typeof dir !== "string") return false;
  await settings.commit(dir === cur ? { vaultPath: dir } : { vaultPath: dir, readOnly: true });
  return true;
}

/** Asks where to put a new vault called `name`, makes it with the example notes and opens it,
 * read-write: nothing in it is the user's yet. False when the panel was cancelled; throws the
 * reason it couldn't be made. */
export async function createVault(name: string): Promise<boolean> {
  const parent = await open({ directory: true, title: `Choose where to put “${name.trim()}”` }).catch(() => null);
  if (typeof parent !== "string") return false;
  const dir = await api.vaultCreate(parent, name);
  await settings.commit({ vaultPath: dir, readOnly: false });
  return true;
}

/** "Create a new vault": a name, then where to put it. */
function NewVault({ close }: { close: () => void }) {
  const [name, setName] = useState("Brainstead");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const go = () => {
    if (!name.trim() || busy) return;
    setErr(null);
    setBusy(true);
    createVault(name)
      .catch((e) => setErr(errText(e)))
      .finally(() => setBusy(false));
  };
  return (
    <div className="card perm">
      <div className="pt">Create a new vault</div>
      <p className="muted">
        Brainstead makes a folder with a few example notes that show what it can do; delete them when you&apos;re done. A new vault opens
        with Read-only off, as nothing in it is yours yet.
      </p>
      <div className="row">
        <label className="inp grow">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && go()}
            aria-label="Name of the new vault"
            placeholder="Name"
            spellCheck={false}
            autoFocus
          />
        </label>
        <button type="button" className="btn ghost" title="Go back without making a vault" onClick={close}>
          Cancel
        </button>
        <span title={name.trim() ? "Choose the folder to put the new vault in" : "Type a name first"}>
          <button type="button" className="btn pri" disabled={!name.trim() || busy} onClick={go}>
            Choose where…
          </button>
        </span>
      </div>
      {err && <p className="err">{err}</p>}
    </div>
  );
}

export function FullDiskAccessCard({ compact }: { compact?: boolean }) {
  const p = useStore(permissions);
  const features = useFeatures();
  const granted = p?.fullDiskAccess === true;
  const unknown = p?.fullDiskAccess == null;
  if (!features.fullDiskAccess) return null;
  return (
    <div className="card perm">
      <div className="permhead">
        <Icon name="shield" size={18} style={{ color: granted ? "var(--green)" : "var(--amber)" }} />
        <div className="t">
          <div className="pt">Full Disk Access</div>
          <div className="faint">{granted ? "Granted." : unknown ? "Couldn't be checked on this Mac." : "Not granted yet."}</div>
        </div>
        {!granted && (
          <>
            <button
              type="button"
              className="btn"
              title="Check again whether Brainstead has Full Disk Access"
              onClick={() => void recheckPermissions()}
            >
              Check again
            </button>
            <button
              type="button"
              className="btn pri"
              title="Open Privacy & Security › Full Disk Access in System Settings"
              onClick={() => void api.openFullDiskAccess()}
            >
              Open System Settings
            </button>
          </>
        )}
      </div>
      {!compact && (
        <p className="muted">
          If your vault lives in a cloud-synced (File Provider) folder, without Full Disk Access macOS asks separately for each protected
          place Brainstead reads, or blocks it. Turn Brainstead on under Privacy &amp; Security › Full Disk Access; if macOS offers to quit
          and reopen it, say yes.
        </p>
      )}
    </div>
  );
}

export function Welcome() {
  const p = useStore(permissions);
  const s = useStore(settings);
  useRecheckOnFocus();
  const [creating, setCreating] = useState(false);
  const needsFda = !!p?.applies && p.fullDiskAccess === false && !s.skippedFullDiskAccess;
  return (
    <div className="welcome drag" data-tauri-drag-region>
      <div className="wcard">
        <div className="brand">
          <Mark size={34} />
          <b>Brainstead</b>
        </div>
        <p className="tagline">Getting Things Done, notes and a wiki kept by AI, over plain markdown files you own.</p>
        {needsFda ? (
          <>
            <h2 className="h2">First, Full Disk Access</h2>
            <FullDiskAccessCard />
            <p className="faint">
              Without it, choose the vault folder yourself next. Brainstead can then read the vault, but macOS may still block files it
              keeps elsewhere, and you'll be asked again after some updates.
            </p>
            <div className="row">
              <button
                type="button"
                className="btn ghost"
                title="Skip Full Disk Access and choose the vault folder yourself"
                onClick={() => settings.update({ skippedFullDiskAccess: true })}
              >
                Continue without it
              </button>
            </div>
          </>
        ) : (
          <>
            <h2 className="h2">Choose your vault</h2>
            <p className="muted">
              The folder of markdown notes you use with other apps. Brainstead indexes it into its own database, outside the folder, and
              opens it <b>read-only</b>: nothing in the vault is changed until you turn that off in Settings. New to it? Create a new vault
              with a few example notes.
            </p>
            <div className="row">
              <button
                type="button"
                className="btn pri lg"
                title="Pick the folder of markdown notes to open; it opens read-only"
                onClick={() => void chooseVault()}
              >
                <Icon name="folder" size={14} />
                Choose folder…
              </button>
              <button
                type="button"
                className="btn lg"
                title="Make a new vault folder with example notes that show what Brainstead can do"
                onClick={() => setCreating(true)}
              >
                <Icon name="plus" size={14} />
                Create a new vault
              </button>
            </div>
            {creating && <NewVault close={() => setCreating(false)} />}
          </>
        )}
      </div>
    </div>
  );
}
