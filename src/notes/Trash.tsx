// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Trash: files moved to the vault's `.trash/` folder, in the layout the previous app uses
// (bff/src/services/trash.ts), so either app can put them back. Restore, restore as, delete
// forever and empty, each permanent step confirmed in an app dialog.

import { useCallback, useEffect, useMemo, useState } from "react";
import { api, Layer, TrashEntry } from "../api";
import { Icon } from "../icons";
import { openDoc } from "../nav";
import { useVaultVersion } from "../state";
import { reportEditError } from "../taskModel";
import { toast } from "../Toast";
import { TopBar } from "../TopBar";
import { ago, Dialog, fmtBytes, SearchBox, Seg } from "../ui";
import { errText } from "./actions";
import { folderOf } from "./filename";
import { useFeatures } from "../features";

const LABEL: Record<Layer, string> = { note: "Note", wiki: "Wiki", source: "Source", template: "Template" };

type Filter = "all" | Layer;

const fold = (t: string) => t.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
/** Whether every word of `q` is in the entry's name or the folder it was in. */
export function trashMatches(e: TrashEntry, q: string): boolean {
  const hay = fold(`${e.basename} ${e.originalRel}`);
  return fold(q)
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

/** "Today 08:40", "Yesterday", "Tue", "28 Sep": when it was deleted. */
export function deletedWhen(iso: string, now = new Date()): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((day(now) - day(d)) / 86_400_000);
  if (days === 0) return `Today ${d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })}`;
  if (days === 1) return "Yesterday";
  if (days > 1 && days < 7) return d.toLocaleDateString("en-GB", { weekday: "short" });
  return d.toLocaleDateString("en-GB", {
    day: "numeric",
    month: "short",
    ...(d.getFullYear() !== now.getFullYear() && { year: "numeric" }),
  });
}

type Confirm = { what: "delete"; ids: string[] } | { what: "empty" } | { what: "restore-as"; entry: TrashEntry };

export function TrashScreen() {
  const v = useVaultVersion();
  const [rows, setRows] = useState<TrashEntry[] | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [blocked, setBlocked] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState<Confirm | null>(null);

  const load = useCallback(() => {
    api
      .trashList()
      .then((r) => setRows([...r].sort((a, b) => b.deletedAt.localeCompare(a.deletedAt))))
      .catch((e) => {
        setRows([]);
        toast(errText(e), undefined, "bad");
      });
  }, []);
  useEffect(load, [load, v]);

  const shown = useMemo(
    () => (rows ?? []).filter((r) => (filter === "all" || r.layer === filter) && trashMatches(r, q)),
    [rows, filter, q],
  );
  const total = (rows ?? []).reduce((n, r) => n + r.sizeBytes, 0);
  const picked = shown.filter((r) => sel.has(r.id));
  const quickLook = (r: TrashEntry) => api.trashQuickLook(r.id).catch((e) => toast(errText(e), undefined, "bad"));
  // Space previews the one selected item, as in Finder (where Quick Look exists).
  const canLook = useFeatures().quickLook;
  useEffect(() => {
    if (!canLook) return;
    const k = (e: KeyboardEvent) => {
      if (e.key !== " " || picked.length !== 1 || (e.target as HTMLElement)?.closest?.("input, textarea, button, [contenteditable]"))
        return;
      e.preventDefault();
      void quickLook(picked[0]);
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  });
  const count = (l: Layer) => (rows ?? []).filter((r) => r.layer === l).length;

  const restore = async (es: TrashEntry[]) => {
    const stuck = new Set(blocked);
    let done = 0;
    let last = "";
    for (const e of es) {
      try {
        last = await api.trashRestore(e.id, null);
        stuck.delete(e.id);
        done++;
      } catch (err) {
        if (typeof err === "object" && err && "code" in err && err.code === "exists") stuck.add(e.id);
        else reportEditError(err);
      }
    }
    setBlocked(stuck);
    setSel(new Set());
    load();
    if (done === 1) toast("Put back", { label: "Open", run: () => openDoc(last) });
    else if (done > 1) toast(`Put back ${done} files`);
    if (es.some((e) => stuck.has(e.id))) toast("A file now exists where it was. Use Restore as… to choose another name.", undefined, "bad");
  };

  const forget = async (ids: string[]) => {
    for (const id of ids) await api.trashDelete(id).catch(reportEditError);
    setSel(new Set());
    load();
  };

  const toggle = (id: string) => {
    const s = new Set(sel);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    setSel(s);
  };
  const allOn = shown.length > 0 && picked.length === shown.length;

  return (
    <main className="main">
      <TopBar title="Trash" sub={rows ? `${rows.length} item${rows.length === 1 ? "" : "s"} · ${fmtBytes(total)}` : undefined}>
        <Seg<Filter>
          label="Show"
          value={filter}
          onChange={setFilter}
          options={[
            ["all", "All", "Everything in the Trash"],
            ...(["note", "wiki", "source", "template"] as Layer[])
              .filter((l) => count(l))
              .map((l): [Filter, string, string] => [l, `${LABEL[l]}s · ${count(l)}`, `Only the ${LABEL[l].toLowerCase()}s in the Trash`]),
          ]}
        />
        <span title={rows?.length ? "Delete everything in the Trash for good; asks first" : "The Trash is already empty"}>
          <button type="button" className="btn" disabled={!rows?.length} onClick={() => setConfirm({ what: "empty" })}>
            <Icon name="trash" size={14} />
            Empty the Trash{total ? ` · frees ${fmtBytes(total)}` : ""}
          </button>
        </span>
      </TopBar>
      <div className="body trash">
        {!!rows?.length && (
          <div className="ftools">
            <SearchBox
              value={q}
              onChange={setQ}
              onKeyDown={(e) => e.key === "Escape" && setQ("")}
              placeholder="Search the Trash"
              className="listq"
            >
              {q.trim() && (
                <span className="faint nav">
                  {shown.length} of {rows.length}
                </span>
              )}
            </SearchBox>
          </div>
        )}
        {picked.length > 0 && (
          <div className="trsel">
            <b>{picked.length} selected</b>
            <span className="faint">· {fmtBytes(picked.reduce((n, r) => n + r.sizeBytes, 0))}</span>
            <span className="grow" />
            <button
              type="button"
              className="btn sm"
              title="Put the selected files back where they were"
              onClick={() => void restore(picked)}
            >
              <Icon name="back" size={12} />
              Restore
            </button>
            <button
              type="button"
              className="btn sm danger"
              title="Delete the selected files for good; asks first"
              onClick={() => setConfirm({ what: "delete", ids: picked.map((r) => r.id) })}
            >
              <Icon name="x" size={12} />
              Delete forever
            </button>
          </div>
        )}
        {!rows ? (
          <div className="trempty">
            <p className="faint">Loading…</p>
          </div>
        ) : rows.length === 0 ? (
          <div className="trempty">
            <Icon name="trash" size={28} />
            <p className="muted">The Trash is empty.</p>
          </div>
        ) : (
          <div className="trlist" role="table" aria-label="Trash">
            <div className="trrow tb-head" role="row">
              <button
                type="button"
                className={`cb ${allOn ? "on" : ""}`}
                role="checkbox"
                aria-checked={allOn}
                aria-label="Select all"
                title={allOn ? "Clear the selection" : "Select every item shown"}
                onClick={() => setSel(allOn ? new Set() : new Set(shown.map((r) => r.id)))}
              >
                {allOn && <Icon name="check" size={11} />}
              </button>
              <span>Type</span>
              <span>Name</span>
              <span>Was in</span>
              <span>Deleted</span>
              <span className="r">Size</span>
              <span />
            </div>
            {shown.length === 0 && <p className="muted trnone">Nothing in the Trash matches “{q.trim()}”.</p>}
            {shown.map((r) => {
              const on = sel.has(r.id);
              const stuck = blocked.has(r.id);
              return (
                <div key={r.id} role="row" className={`trrow tb-row ${on ? "sel" : ""} ${stuck ? "stuck" : ""}`}>
                  <button
                    type="button"
                    className={`cb ${on ? "on" : ""}`}
                    role="checkbox"
                    aria-checked={on}
                    aria-label={`Select ${r.basename}`}
                    title={on ? "Unselect" : "Select"}
                    onClick={() => toggle(r.id)}
                  >
                    {on && <Icon name="check" size={11} />}
                  </button>
                  <span className={`lb ${r.layer}`}>{LABEL[r.layer] ?? r.layer}</span>
                  <div className="ell">
                    <div className="ell trname">{r.basename.replace(/\.md$/i, "")}</div>
                    {stuck && (
                      <div className="trwarn">
                        <Icon name="info" size={12} />
                        Can’t restore — a file now exists at the original path
                      </div>
                    )}
                  </div>
                  <span className="mono faint ell">{folderOf(r.originalRel) || "/"}</span>
                  <span className="faint" title={ago(new Date(r.deletedAt).getTime())}>
                    {deletedWhen(r.deletedAt)}
                  </span>
                  <span className="faint r">{fmtBytes(r.sizeBytes)}</span>
                  <span className="tracts">
                    {canLook && (
                      <button
                        type="button"
                        className="ibtn"
                        aria-label={`Quick Look ${r.basename}`}
                        title="Quick Look (Space)"
                        onClick={() => void quickLook(r)}
                      >
                        <Icon name="eye" size={14} />
                      </button>
                    )}
                    {stuck ? (
                      <button
                        type="button"
                        className="btn sm"
                        title="Put it back under a different path, since its old one is taken"
                        onClick={() => setConfirm({ what: "restore-as", entry: r })}
                      >
                        Restore as…
                      </button>
                    ) : (
                      <>
                        <button type="button" className="btn sm ghost" title="Put it back where it was" onClick={() => void restore([r])}>
                          Restore
                        </button>
                        <button
                          type="button"
                          className="btn sm ghost"
                          title="Put it back under a different path"
                          onClick={() => setConfirm({ what: "restore-as", entry: r })}
                        >
                          Restore as…
                        </button>
                      </>
                    )}
                  </span>
                </div>
              );
            })}
          </div>
        )}
        <div className="trfoot">
          <Icon name="shield" size={14} />
          <span>
            Items stay in the vault’s .trash folder until you empty it, so sync clients and other apps see them too. Restore puts a file
            back where it was.
          </span>
        </div>
      </div>
      {confirm?.what === "restore-as" && (
        <RestoreAs
          entry={confirm.entry}
          onClose={() => setConfirm(null)}
          onDone={(p) => {
            setConfirm(null);
            const b = new Set(blocked);
            b.delete(confirm.entry.id);
            setBlocked(b);
            load();
            toast("Put back", { label: "Open", run: () => openDoc(p) });
          }}
        />
      )}
      {(confirm?.what === "delete" || confirm?.what === "empty") && (
        <ConfirmForever
          n={confirm.what === "empty" ? (rows?.length ?? 0) : confirm.ids.length}
          empty={confirm.what === "empty"}
          onClose={() => setConfirm(null)}
          onConfirm={async () => {
            const c = confirm;
            setConfirm(null);
            if (c.what === "empty") {
              const n = await api.trashEmpty().catch((e) => (reportEditError(e), null));
              if (n !== null) toast(`Deleted ${n} file${n === 1 ? "" : "s"} for good`);
              load();
            } else await forget(c.ids);
          }}
        />
      )}
    </main>
  );
}

function ConfirmForever({ n, empty, onClose, onConfirm }: { n: number; empty: boolean; onClose: () => void; onConfirm: () => void }) {
  return (
    <Dialog onClose={onClose} width={440} label={empty ? "Empty the Trash" : "Delete forever"}>
      <div className="confirm">
        <h2 className="h2">{empty ? "Empty the Trash?" : `Delete ${n === 1 ? "this file" : `${n} files`} forever?`}</h2>
        <p className="muted">
          {empty
            ? `All ${n} file${n === 1 ? "" : "s"} in the vault’s .trash folder will be deleted.`
            : "It will be deleted from the vault’s .trash folder."}{" "}
          This can’t be undone.
        </p>
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" title="Keep the files (Esc)" onClick={onClose} autoFocus>
            Cancel
          </button>
          <button
            type="button"
            className="btn lg danger-pri"
            title={empty ? "Delete every file in the Trash; this can’t be undone" : "Delete for good; this can’t be undone"}
            onClick={onConfirm}
          >
            {empty ? "Empty the Trash" : "Delete forever"}
          </button>
        </div>
      </div>
    </Dialog>
  );
}

function RestoreAs({ entry, onClose, onDone }: { entry: TrashEntry; onClose: () => void; onDone: (path: string) => void }) {
  const dot = entry.originalRel.lastIndexOf(".");
  const suggested =
    dot > 0 ? `${entry.originalRel.slice(0, dot)} (restored)${entry.originalRel.slice(dot)}` : `${entry.originalRel} (restored)`;
  const [to, setTo] = useState(suggested);
  const [err, setErr] = useState<string | null>(null);
  const go = async () => {
    try {
      onDone(await api.trashRestore(entry.id, to.trim()));
    } catch (e) {
      setErr(errText(e));
    }
  };
  return (
    <Dialog onClose={onClose} width={520} label="Restore as">
      <form
        className="confirm"
        onSubmit={(e) => {
          e.preventDefault();
          void go();
        }}
      >
        <h2 className="h2">Restore as</h2>
        <p className="muted">Where to put “{entry.basename}” back, as a path in the vault.</p>
        <span className="inp mono">
          <input autoFocus value={to} onChange={(e) => setTo(e.target.value)} aria-label="Vault path" />
        </span>
        {err && (
          <div className="nnerr" role="alert">
            <Icon name="info" size={14} />
            <span>{err}</span>
          </div>
        )}
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" title="Leave it in the Trash (Esc)" onClick={onClose}>
            Cancel
          </button>
          <span title={to.trim() ? "Put the file back at this path" : "Type a path to restore to"}>
            <button type="submit" className="btn pri lg" disabled={!to.trim()}>
              Restore
            </button>
          </span>
        </div>
      </form>
    </Dialog>
  );
}
