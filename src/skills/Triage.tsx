// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Triage bookmarks (the vault's `triage-bookmarks` skill, §4): every bookmark with how long its
// note has sat, the model's suggestion for each, and a decision per bookmark: Keep, Ingest into the wiki
// (a new wiki page, revertable in Changes), Make a task, Archive, or Remove (a missing one). Only
// `Me. Bookmarks.md` changes, a line at a time, each undoable.

import { useEffect, useMemo, useState } from "react";
import { api, BookmarkRow, BookmarkSuggestion } from "../api";
import { filedPage } from "../Ask";
import { makePage } from "../knowledge";
import { captureStamp } from "../Capture";
import { Icon } from "../icons";
import { nav, openDoc, useViewState } from "../nav";
import { useVaultVersion } from "../state";
import { reportEditError, undoAction } from "../taskModel";
import { toast } from "../Toast";
import { TopBar } from "../TopBar";
import { keyLabel } from "../platform";

export type Decision = "keep" | "promote" | "task" | "archive" | "remove";

/** Each choice: its button's label (triage_bookmarks names them so too), icon and colour. */
export const LABEL: Record<Decision, [string, string, string]> = {
  keep: ["Keep", "pin", ""],
  promote: ["Ingest into the wiki", "wiki", "acc"],
  task: ["Make a task", "tasks", ""],
  archive: ["Archive", "someday", ""],
  remove: ["Remove", "x", "red"],
};

/** What each choice does, for its button's tooltip. */
const TIP: Record<Decision, string> = {
  keep: "Keep the bookmark as it is (K)",
  promote: "Make a wiki page from the note (revertable in Changes); the bookmark goes (I)",
  task: "Make a task from the note; the bookmark goes (T)",
  archive: "Drop the bookmark; the note stays where it is (A)",
  remove: "Remove the bookmark to the missing note (⌫)",
};

const KEYS: [string, Decision][] = [
  ["k", "keep"],
  ["i", "promote"],
  ["t", "task"],
  ["a", "archive"],
  ["Backspace", "remove"],
];

export function TriageScreen() {
  const v = useVaultVersion();
  const [rows, setRows] = useState<BookmarkRow[] | null>(null);
  const [sugg, setSugg] = useState<Map<string, BookmarkSuggestion>>(new Map());
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState("");
  // Decided this session (kept ones stay in the file, so they're remembered here).
  const [done, setDone] = useViewState<Record<string, Decision>>("triage.done", {});
  const [open, setOpen] = useState<string | null>(null);

  useEffect(() => {
    void api
      .bookmarksStatus()
      .then(setRows)
      .catch((e) => setError(String(e)));
  }, [v]);

  const todo = useMemo(() => (rows ?? []).filter((r) => !done[r.target]), [rows, done]);
  const cur = (rows ?? []).find((r) => r.target === open) ?? todo[0] ?? null;
  const decided = Object.keys(done).length;
  const total = decided + todo.length;

  const suggest = async () => {
    const items = todo.filter((r) => r.path).map((r) => ({ target: r.target, path: r.path! }));
    if (!items.length) return;
    setAsking(true);
    setError("");
    try {
      const out = await api.bookmarksSuggest(items);
      setSugg((m) => new Map([...m, ...out.map((s) => [s.target, s] as [string, BookmarkSuggestion])]));
    } catch (e) {
      setError(String(e));
    } finally {
      setAsking(false);
    }
  };

  const decide = async (r: BookmarkRow, d: Decision) => {
    const s = sugg.get(r.target);
    try {
      if (d === "promote") {
        const name = (s?.page ?? r.title).trim();
        const kind = s?.kind ?? "concept";
        const body = [s?.summary, `From [[${r.target}]].`].filter(Boolean).join("\n\n");
        await makePage(
          `wiki/${kind === "entity" ? "entities" : "concepts"}/${name}.md`,
          filedPage(name, kind, body),
          `New page from a bookmark: ${name}`,
          {
            kind: "chat",
            label: "Triage",
          },
        );
      }
      if (d === "task") await api.capture("task", s?.task ?? `Read [[${r.target}]]`, captureStamp());
      if (d === "keep") await api.bookmarkKeep(r.target);
      else await api.bookmarkRemove(r.target);
      setDone({ ...done, [r.target]: d });
      setOpen(null);
      const what = { keep: "Kept", promote: "Made a wiki page for", task: "Made a task of", archive: "Archived", remove: "Removed" }[d];
      toast(`${what} “${r.title}”`, d === "keep" ? undefined : undoAction, "ok");
      if (d === "promote") toast("The new page is made; it's in Changes, with Revert", { label: "Changes", run: () => nav.go("review") });
    } catch (e) {
      reportEditError(e);
    }
  };

  // K I T A ⌫ decide the open bookmark, ↩ takes the suggestion.
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (!cur || e.metaKey || e.ctrlKey || e.altKey) return;
      if ((e.target as HTMLElement)?.closest?.("input, textarea, [contenteditable]")) return;
      const s = sugg.get(cur.target);
      const hit = KEYS.find(([key]) => key === e.key || key === e.key.toLowerCase());
      if (e.key === "Enter" && (s || cur.missing)) void decide(cur, cur.missing ? "remove" : s!.decision);
      else if (hit) void decide(cur, hit[1]);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  });

  const sub = rows ? `${todo.length} to go` : undefined;
  return (
    <main className="main">
      <TopBar title="Triage bookmarks" sub={sub}>
        {total > 0 && (
          <span className="faint small">
            {decided} of {total}
          </span>
        )}
        <span
          title={
            asking
              ? "Reading the bookmarked notes…"
              : todo.some((r) => r.path)
                ? "Have the assistant read each bookmarked note and suggest what to do with it"
                : "No bookmarked notes left to suggest for"
          }
        >
          <button type="button" className="btn" disabled={asking || !todo.some((r) => r.path)} onClick={() => void suggest()}>
            <Icon name="cpu" size={14} />
            {asking ? "Reading them…" : "Suggest for all"}
          </button>
        </span>
      </TopBar>
      <div className="body triage">
        <p className="muted">
          Your bookmarks, oldest untouched first. Suggestions read each note; nothing changes until you choose. Ages are from when each note
          last changed, so they're at most that old.
        </p>
        {error && <p className="err">{error}</p>}
        {rows && !rows.length && <p className="faint">No bookmarks. Bookmark a note from its top bar.</p>}
        <div className="card bmlist">
          {[...(rows ?? [])]
            .sort((a, b) => Number(!!done[a.target]) - Number(!!done[b.target]) || (b.days ?? 1e9) - (a.days ?? 1e9))
            .map((r) => {
              const d = done[r.target];
              const s = sugg.get(r.target);
              const isOpen = !d && cur?.target === r.target;
              return (
                <div key={r.target} className={`bmrow ${d ? "done" : ""} ${isOpen ? "open" : ""}`}>
                  <button
                    type="button"
                    className="bmhead"
                    title={d ? `Done: ${LABEL[d][0]}` : "Show this bookmark's choices"}
                    onClick={() => !d && setOpen(r.target)}
                    disabled={!!d}
                  >
                    <span className="grow">
                      <span className="tt">{r.title}</span>
                      <span className="faint small">
                        {r.missing
                          ? "The note isn't there any more"
                          : r.days != null
                            ? `untouched ${r.days} ${r.days === 1 ? "day" : "days"}`
                            : ""}
                        {r.stale && !r.missing ? " · stale" : ""}
                      </span>
                    </span>
                    {d ? (
                      <span className="chip green">
                        <Icon name="check" size={11} />
                        {LABEL[d][0]}
                      </span>
                    ) : r.missing ? (
                      <span className="chip red">Link missing</span>
                    ) : s ? (
                      <span className={`chip ${LABEL[s.decision][2]}`}>
                        <Icon name={LABEL[s.decision][1]} size={11} />
                        {LABEL[s.decision][0]}
                      </span>
                    ) : null}
                  </button>
                  {isOpen && (
                    <div className="bmbody">
                      {s && (
                        <div className="bmwhy">
                          {s.summary && <p>{s.summary}</p>}
                          <p className="faint small">
                            {s.why}
                            {s.decision === "promote" && s.page && ` · as the ${s.kind ?? "concept"} “${s.page}”`}
                            {s.decision === "task" && s.task && ` · “${s.task}”`}
                          </p>
                        </div>
                      )}
                      <div className="bmkeys">
                        {(s || r.missing) && (
                          <button
                            type="button"
                            className="key rec"
                            title={`Do what's suggested: ${LABEL[r.missing ? "remove" : s!.decision][0].toLowerCase()} (↩)`}
                            onClick={() => void decide(r, r.missing ? "remove" : s!.decision)}
                          >
                            <span className="kbd">↩</span>
                            Accept suggestion
                          </button>
                        )}
                        {(r.missing ? (["remove"] as Decision[]) : (["keep", "promote", "task", "archive"] as Decision[])).map((x) => (
                          <button key={x} type="button" className="key" title={TIP[x]} onClick={() => void decide(r, x)}>
                            <span className="kbd">
                              {KEYS.find(([, d]) => d === x)![0]
                                .replace("Backspace", "⌫")
                                .toUpperCase()}
                            </span>
                            {LABEL[x][0]}
                          </button>
                        ))}
                        {r.path && (
                          <button type="button" className="btn sm ghost" title="Open the bookmarked note" onClick={() => openDoc(r.path!)}>
                            Open
                          </button>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
        </div>
        <p className="faint small">
          K Keep · I Ingest into the wiki · T Make a task · A Archive · {keyLabel("⌫ Remove · ↩ accept the suggestion · ⌘Z")} undoes
        </p>
      </div>
    </main>
  );
}
