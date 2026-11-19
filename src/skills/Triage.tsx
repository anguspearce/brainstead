// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Triage bookmarks (the vault's `triage-bookmarks` skill, §4): every bookmark with how long its
// note has sat, the model's suggestion for each (the one suggestion component's row, with Accept,
// Edit and Skip, src/Suggestions.tsx), and a decision per bookmark: Keep, Ingest into the wiki
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
import { toTriage } from "../needsYou";
import { TableBand } from "../ui";
import { SuggestButton, SuggestChip, SuggestionRow } from "../Suggestions";
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

/** What a bookmark's row says under its title: how long its note has sat, or that it's gone. */
export const bookmarkNote = (r: BookmarkRow) =>
  (r.missing ? "The note isn't there any more" : r.days != null ? `untouched ${r.days} ${r.days === 1 ? "day" : "days"}` : "") +
  (r.stale && !r.missing ? " · stale" : "");

/** The Inbox's Bookmarks band (D-20261119-05): the bookmarks worth a triage (stale or missing, as
 *  Today's To process counts them), oldest first. A row opens Triage on it, where it's decided. */
export function BookmarksBand() {
  const v = useVaultVersion();
  const [rows, setRows] = useState<BookmarkRow[]>([]);
  useEffect(() => {
    let live = true;
    void api
      .bookmarksStatus()
      .then((b) => live && setRows(b.filter((r) => r.stale || r.missing)))
      .catch(() => live && setRows([]));
    return () => {
      live = false;
    };
  }, [v]);
  if (!rows.length) return null;
  const sorted = [...rows].sort((a, b) => Number(b.missing) - Number(a.missing) || (b.days ?? 1e9) - (a.days ?? 1e9));
  return (
    <div className="tb-group bmband" role="rowgroup">
      <TableBand icon="pin" label="Bookmarks" n={rows.length} tip={`${rows.length} bookmarks gone stale or missing, to triage`} />
      {sorted.map((r) => (
        <div key={r.target} role="row" className="irowc tb-row">
          <button
            type="button"
            className="irow"
            title="Triage it: keep it, ingest it into the wiki, make a task of it or archive it"
            onClick={() => nav.go({ screen: "triage", view: { "triage:open": r.target } })}
          >
            <span className="tt">{r.title}</span>
            <span className={`faint ${r.missing ? "bad" : ""}`}>{bookmarkNote(r)}</span>
          </button>
        </div>
      ))}
    </div>
  );
}

export function TriageScreen() {
  const v = useVaultVersion();
  const [rows, setRows] = useState<BookmarkRow[] | null>(null);
  const [sugg, setSugg] = useState<Map<string, BookmarkSuggestion>>(new Map());
  const [asking, setAsking] = useState(false);
  // Asked this visit: the button is Suggest again.
  const [asked, setAsked] = useState(false);
  const [error, setError] = useState("");
  // Decided this session (kept ones stay in the file, so they're remembered here).
  const [done, setDone] = useViewState<Record<string, Decision>>("triage.done", {});
  // The bookmark shown open: the Inbox's Bookmarks band opens Triage on the one clicked.
  const [open, setOpen] = useViewState<string | null>("triage:open", null, false);

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
    setAsked(true);
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

  /** `edited`: the suggestion's page or task as Edit changed it. */
  const decide = async (r: BookmarkRow, d: Decision, edited?: { page?: string; task?: string }) => {
    const found = sugg.get(r.target);
    const s = found && edited ? { ...found, page: edited.page || found.page, task: edited.task || found.task } : found;
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

  // Stale or missing ones first in the sub, as Today's To process counts them (src/needsYou.ts).
  const worth = toTriage(todo);
  // The bookmarks with a suggestion waiting: the chip counts them and opens the next one.
  const suggested = todo.filter((r) => sugg.has(r.target));
  const sub = rows ? `${worth ? `${worth} stale or missing · ` : ""}${todo.length} to go` : undefined;
  return (
    <main className="main">
      <TopBar title="Triage bookmarks" sub={sub}>
        {total > 0 && (
          <span className="faint small">
            {decided} of {total}
          </span>
        )}
        <SuggestChip
          n={suggested.length}
          busy={asking}
          onToggle={() => setOpen((suggested.find((r) => r.target !== cur?.target) ?? suggested[0]).target)}
        />
        <SuggestButton
          big
          again={asked}
          busy={asking}
          disabled={!todo.some((r) => r.path)}
          title={
            todo.some((r) => r.path)
              ? "Have the assistant read each bookmarked note and suggest what to do with it; nothing changes until you choose"
              : "No bookmarked notes left to suggest for"
          }
          onClick={() => void suggest()}
        />
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
                      <span className="faint small">{bookmarkNote(r)}</span>
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
                      {r.missing ? (
                        <SuggestionRow
                          id={r.target}
                          enterKey
                          text="The note isn't there any more: remove the bookmark"
                          accept={{ title: "Remove the bookmark to the missing note", run: () => void decide(r, "remove") }}
                        />
                      ) : (
                        s && (
                          <SuggestionRow
                            id={r.target}
                            enterKey
                            text={
                              <>
                                <div className="isugg-what">
                                  <Icon name="cpu" size={13} />
                                  Suggested: {LABEL[s.decision][0]}
                                </div>
                                {s.summary && <p>{s.summary}</p>}
                              </>
                            }
                            detail={
                              <>
                                {s.why}
                                {s.decision === "promote" && s.page && ` · as the ${s.kind ?? "concept"} “${s.page}”`}
                                {s.decision === "task" && s.task && ` · “${s.task}”`}
                              </>
                            }
                            accept={{
                              title: `Do what's suggested: ${LABEL[s.decision][0].toLowerCase()}`,
                              run: (e) => void decide(r, s.decision, e && { page: e.page, task: e.task }),
                            }}
                            edit={
                              s.decision === "promote"
                                ? {
                                    title: "Change the page's name before making it",
                                    fields: [{ key: "page", label: "The page", value: s.page ?? r.title }],
                                  }
                                : s.decision === "task"
                                  ? {
                                      title: "Change the task's words before making it",
                                      fields: [{ key: "task", label: "The task", value: s.task ?? "" }],
                                    }
                                  : undefined
                            }
                            skip={{
                              title: "Drop the suggestion; choose for this bookmark yourself",
                              run: () =>
                                setSugg((m) => {
                                  const n = new Map(m);
                                  n.delete(r.target);
                                  return n;
                                }),
                            }}
                          />
                        )
                      )}
                      <div className="bmkeys">
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
