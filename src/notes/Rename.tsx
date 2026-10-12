// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Rename a file, with a preview of every link that will be rewritten to follow it (the previous app's
// RenameDialog and RenamePreviewBody). The name is built from type, title and date; the folder
// stays. Rust plans the rename, then makes exactly the planned changes or none.

import { withDraftHandedOver } from "../drafts";
import { useEffect, useMemo, useState } from "react";
import { api, RenamePlan } from "../api";
import { Icon } from "../icons";
import { nav, place } from "../nav";
import { reportEditError } from "../taskModel";
import { toast } from "../Toast";
import { Dialog, useDebounced } from "../ui";
import { moveDocLook } from "../docLook";
import { errText } from "./actions";
import { closeNoteDialog } from "./dialogs";
import { composeFilename, folderOf, parseName } from "./filename";

/** The vault path a rename to these parts gives, keeping the folder and extension. */
export function renamedPath(path: string, n: { type: string; title: string; date: string }): string {
  const ext = /\.txt$/i.test(path) ? ".txt" : ".md";
  const name = /\.(md|txt)$/i.test(path) ? composeFilename(n, ext) : n.title.trim();
  return name ? folderOf(path) + name : "";
}

/** The plan's changes by file, in the order Rust gave them. */
export function byFile(plan: RenamePlan): [string, RenamePlan["changes"]][] {
  const m = new Map<string, RenamePlan["changes"]>();
  for (const c of plan.changes) m.set(c.path, [...(m.get(c.path) ?? []), c]);
  return [...m];
}

const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;

/** Makes a planned rename, the note shown (or the project chosen) following it, with Undo; gives the
 *  new path. The Rename box and the name on a project's page both rename through this. */
export async function commitRename(path: string, plan: RenamePlan): Promise<string> {
  const now = await withDraftHandedOver(
    path,
    () => api.renameCommit(plan),
    (p) => p,
  );
  moveDocLook(path, now);
  const follow = (from: string, to: string) => {
    const at = place.get().place;
    if ((at.screen === "doc" || at.screen === "projects") && at.path === from) nav.replace({ ...at, path: to });
  };
  follow(path, now);
  // Undo is the app's undo (⌘Z does the same), back to the old name and the note shown under it.
  const name = (p: string) => p.split("/").pop() ?? p;
  toast(`Renamed to “${name(now)}”`, {
    label: "Undo",
    kbd: "⌘Z",
    run: () =>
      void api
        .undo()
        .then((r) => {
          toast(r ?? "Nothing to undo.");
          if (!r) return;
          moveDocLook(now, path);
          follow(now, path);
        })
        .catch(reportEditError),
  });
  return now;
}

export function RenameDialog({ path, title }: { path: string; title?: string }) {
  const isNote = /\.(md|txt)$/i.test(path);
  const start = useMemo(() => {
    const p = parseName(path);
    return { type: p.type ?? "", title: isNote ? p.title : (path.split("/").pop() ?? path), date: p.date ?? "" };
  }, [path, isNote]);
  const [n, setN] = useState(title ? { ...start, title } : start);
  const to = renamedPath(path, n);
  const dTo = useDebounced(to, 350);
  const [plan, setPlan] = useState<RenamePlan | { error: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState(true);

  useEffect(() => {
    if (!dTo || dTo === path) return setPlan(null);
    let live = true;
    setPlan(null);
    api
      .renamePreview(path, dTo)
      .then((p) => live && setPlan(p))
      .catch((e) => live && setPlan({ error: errText(e) }));
    return () => {
      live = false;
    };
  }, [path, dTo]);

  const ready = plan && "changes" in plan && plan.to === to ? plan : null;
  const files = ready ? byFile(ready) : [];

  const commit = async () => {
    if (!ready || busy) return;
    setBusy(true);
    try {
      await commitRename(path, ready);
      closeNoteDialog();
    } catch (e) {
      reportEditError(e);
      setBusy(false);
    }
  };

  const field = (k: keyof typeof n, label: string, wide = false) => (
    <label className={`nnf ${wide ? "wide" : ""}`}>
      <span>{label}</span>
      <span className="inp">
        <input autoFocus={k === "title"} value={n[k]} onChange={(e) => setN({ ...n, [k]: e.target.value })} />
      </span>
    </label>
  );

  return (
    <Dialog onClose={closeNoteDialog} width={640} label="Rename">
      <form
        className="rename"
        onSubmit={(e) => {
          e.preventDefault();
          void commit();
        }}
      >
        <div className="row">
          <h2 className="h2 grow">Rename {isNote ? "note" : "file"}</h2>
          <button type="button" className="ibtn" aria-label="Close" title="Close without renaming (Esc)" onClick={closeNoteDialog}>
            <Icon name="x" />
          </button>
        </div>
        {isNote ? (
          <div className="nnfields rnfields">
            {field("type", "Type")}
            {field("title", "Title", true)}
            {field("date", "Date")}
          </div>
        ) : (
          <div className="nnfields">{field("title", "Name", true)}</div>
        )}
        <div className="row nnname">
          <span className="faint">Filename</span>
          <span className="mono rnold">{path.split("/").pop()}</span>
          <Icon name="forward" size={12} />
          <span className="mono fname">{to ? to.split("/").pop() : "—"}</span>
        </div>
        {to && to !== path && (
          <div className="card rnplan">
            {plan && "error" in plan ? (
              <div className="nnerr" role="alert">
                <Icon name="info" size={14} />
                <span>{plan.error}</span>
              </div>
            ) : !ready ? (
              <div className="rnhead faint">Finding links…</div>
            ) : (
              <>
                <button
                  type="button"
                  className="rnhead"
                  onClick={() => setOpen(!open)}
                  aria-expanded={open}
                  title={
                    ready.changes.length
                      ? open
                        ? "Hide the links that will change"
                        : "Show the links that will change"
                      : "No other note links to this one"
                  }
                >
                  <Icon name="text" size={14} />
                  <b className="grow">
                    {ready.changes.length
                      ? `${plural(ready.changes.length, "link")} in ${plural(files.length, "file")} will be updated`
                      : "No other file links here"}
                  </b>
                  {ready.changes.length > 0 && <Icon name={open ? "chevup" : "chevdown"} size={14} />}
                </button>
                {open && (
                  <div className="rnfiles">
                    {files.map(([f, cs]) => (
                      <div key={f} className="rnfile">
                        <div className="row">
                          <b className="grow ell">{f.replace(/\.md$/i, "")}</b>
                          <span className="faint">{plural(cs.length, "link")}</span>
                        </div>
                        {cs.slice(0, 3).map((c) => (
                          <div key={c.line} className="rnba mono">
                            <s>{c.before.trim()}</s>
                            <b>{c.after.trim()}</b>
                          </div>
                        ))}
                        {cs.length > 3 && <div className="faint small">and {cs.length - 3} more</div>}
                      </div>
                    ))}
                  </div>
                )}
              </>
            )}
          </div>
        )}
        <div className="row nnfoot">
          <span className="grow" />
          <button type="button" className="btn lg" title="Close without renaming (Esc)" onClick={closeNoteDialog}>
            Cancel
          </button>
          <span
            title={
              busy
                ? "Renaming…"
                : !ready
                  ? "Change the name first, then wait for the link check"
                  : ready.changes.length
                    ? "Rename the file and rewrite every link to it"
                    : "Rename the file"
            }
          >
            <button type="submit" className="btn pri lg" disabled={!ready || busy}>
              {ready?.changes.length ? "Rename & update links" : "Rename"}
            </button>
          </span>
        </div>
      </form>
    </Dialog>
  );
}
