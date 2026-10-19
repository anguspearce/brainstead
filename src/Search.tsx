// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Search page: the whole vault on the index, with the previous app's query syntax, layer
// filters, passages, and the saved searches from Me. Smart Lists.md down the side.

import { useEffect, useRef, useState } from "react";
import { api, Layer, SearchHit, SearchResults, SmartList } from "./api";
import { FileMenu, useFileMenu } from "./FileMenu";
import { Snippet } from "./FileList";
import { LAYER_LABEL } from "./Doc";
import { Icon } from "./icons";
import { nav, openDoc, place, useViewState } from "./nav";
import { useVaultVersion } from "./state";
import { useStore } from "./store";
import { askQuote, TopBar } from "./TopBar";
import { Dialog, fmtCount, SearchBox, Seg, useDebounced } from "./ui";
import { reportEditError, undoLast } from "./taskModel";
import { toast } from "./Toast";

/** The parts of the vault Search filters on (and a saved search keeps; saved_searches takes the same). */
export const LAYERS: Layer[] = ["note", "wiki", "source"];

/** Results for a query that's already debounced (see useDebounced). */
export function useSearch(q: string, layers: Layer[], limit = 200): SearchResults | null {
  return useSearchOf(q, layers, limit)?.r ?? null;
}

/** What a search came back with: the hits, or (with none) why it failed and a way to try again. */
export interface SearchOutcome {
  q: string;
  r: SearchResults;
  error?: string;
  retry?: () => void;
}

/** As useSearch, with the query (trimmed) the results answer: they lag behind what's typed. */
export function useSearchOf(q: string, layers: Layer[], limit = 200): SearchOutcome | null {
  const [r, setR] = useState<SearchOutcome | null>(null);
  const [tries, setTries] = useState(0);
  const v = useVaultVersion();
  useEffect(() => {
    let live = true;
    const t = q.trim();
    if (t.length < 2) {
      setR(null);
      return;
    }
    api
      .search(t, layers, limit)
      .then((x) => live && setR({ q: t, r: x }))
      .catch((e) => live && setR({ q: t, r: { hits: [], total: 0, ms: 0 }, error: String(e), retry: () => setTries((n) => n + 1) }));
    return () => {
      live = false;
    };
  }, [q, layers.join(","), limit, v, tries]); // eslint-disable-line react-hooks/exhaustive-deps
  return r;
}

/** The date ranges, as days back from today ("" for any time). */
const WHEN: [string, string][] = [
  ["", "Any time"],
  ["7", "Past week"],
  ["31", "Past month"],
  ["92", "Past 3 months"],
  ["365", "Past year"],
];

/** Today less `days`, as YYYY-MM-DD. */
export function sinceOf(days: string, now = new Date()): string {
  const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - Number(days), 12);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

const dayOf = (h: SearchHit) => h.file.date ?? new Date(h.file.mtime).toISOString().slice(0, 10);

/** Hits newest or oldest first (by the file's date, or the day it last changed), or as ranked. */
export function sortHits(hits: SearchHit[], sort: "best" | "newest" | "oldest"): SearchHit[] {
  if (sort === "best") return hits;
  const s = [...hits].sort((a, b) => dayOf(a).localeCompare(dayOf(b)) || a.file.mtime - b.file.mtime);
  return sort === "newest" ? s.reverse() : s;
}

export function SearchScreen() {
  const { place: here } = useStore(place);
  const [typed, setQ] = useViewState("search:q", here.q ?? "");
  const q = useDebounced(typed);
  const [layers, setLayers] = useViewState<Layer[]>("search:layers", LAYERS);
  const [active, setActive] = useState(0);
  const [lists, setLists] = useState<SmartList[]>([]);
  const [saving, setSaving] = useState(false);
  // A date range adds `since:` to the query (unless it has its own); the sort reorders the hits.
  const [when, setWhen] = useViewState<string>("search:when", "");
  const [sort, setSort] = useViewState<"best" | "newest" | "oldest">("search:sort", "best");
  const sent = when && q.trim() && !/\bsince:/.test(q) ? `${q} since:${sinceOf(when)}` : q;
  const found = useSearchOf(sent, layers.length === LAYERS.length ? [] : layers);
  const r = found?.r ?? null;
  const menu = useFileMenu();
  const input = useRef<HTMLInputElement>(null);
  const v = useVaultVersion();

  useEffect(() => input.current?.focus(), []);
  // A query in the place (⌘K's "Search everything", back to an earlier search) wins over the remembered one,
  // but not the one this page put there itself below: by then the box may hold more, typed since.
  const wrote = useRef<string | null>(null);
  useEffect(() => {
    if (here.q !== undefined && here.q !== wrote.current && here.q !== typed) setQ(here.q);
  }, [here.q]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => setActive(0), [q, layers]);
  useEffect(() => {
    let live = true;
    api
      .smartLists()
      .then((l) => live && setLists(l))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [v]);

  // The query goes into the history, so back returns to it.
  useEffect(() => {
    const t = window.setTimeout(() => {
      if ((here.q ?? "") !== q) {
        wrote.current = q;
        nav.replace({ screen: "search", q });
      }
    }, 600);
    return () => window.clearTimeout(t);
  }, [q]); // eslint-disable-line react-hooks/exhaustive-deps

  const hits = sortHits(r?.hits ?? [], sort);
  const toggle = (l: Layer) => setLayers((ls) => (ls.includes(l) ? (ls.length > 1 ? ls.filter((x) => x !== l) : ls) : [...ls, l]));
  const key = (e: React.KeyboardEvent) => {
    if (e.key === "Escape" && typed) {
      setQ("");
      e.preventDefault();
      return;
    }
    if (!hits.length) return;
    if (e.key === "ArrowDown") setActive((a) => Math.min(hits.length - 1, a + 1));
    else if (e.key === "ArrowUp") setActive((a) => Math.max(0, a - 1));
    else if (e.key === "Enter") openDoc(hits[active].file.path, { q: q.trim() });
    else return;
    e.preventDefault();
  };

  return (
    <main className="main">
      <TopBar
        title="Search"
        ask={q.trim().length >= 2 ? { about: askQuote(q.trim()), prompt: `About “${q.trim()}”: ` } : { about: "your vault", prompt: "" }}
      >
        <button
          type="button"
          className="btn"
          disabled={q.trim().length < 2 || lists.some((l) => l.query === q.trim())}
          title="Keep this search in Saved searches (Me. Smart Lists.md)"
          onClick={() => setSaving(true)}
        >
          <Icon name="plus" size={14} />
          Save search
        </button>
      </TopBar>
      {saving && <SaveSearch query={q.trim()} layers={layers.length === LAYERS.length ? [] : layers} onClose={() => setSaving(false)} />}
      <div className="body split">
        <nav className="tagtree" aria-label="Saved searches">
          <div className="sect static">Saved searches</div>
          {lists.length === 0 && <p className="faint pad">None yet: Save search keeps one here (in Me. Smart Lists.md).</p>}
          {lists.map((l) => (
            <div key={l.name} className={`tnode saved ${q === l.query ? "on" : ""}`}>
              <button
                type="button"
                className="grow ell"
                title={l.query}
                onClick={() => {
                  setQ(l.query);
                  setLayers(l.layers.length ? l.layers : LAYERS);
                }}
              >
                <Icon name="search" size={13} />
                {l.name}
              </button>
              <button
                type="button"
                className="ibtn xs"
                aria-label={`Delete “${l.name}”`}
                title={`Delete the saved search “${l.name}” from Me. Smart Lists.md`}
                onClick={() =>
                  void api
                    .smartListDelete(l.name)
                    .then(() => toast(`Deleted “${l.name}”`, { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok"))
                    .catch(reportEditError)
                }
              >
                <Icon name="x" size={11} />
              </button>
            </div>
          ))}
        </nav>
        <div className="spage">
          {/* The box, its filters and the count stay put; only the results scroll. */}
          <div className="shead">
            <div className="sbar">
              <SearchBox
                value={typed}
                onChange={setQ}
                onKeyDown={key}
                inputRef={input}
                placeholder='Search everything — +must -not "exact phrase" tag:x'
                className="grow lg"
              />
              <div className="chips">
                {LAYERS.map((l) => (
                  <button
                    key={l}
                    type="button"
                    className={`chip f ${layers.includes(l) ? "on" : ""}`}
                    aria-pressed={layers.includes(l)}
                    title={
                      layers.includes(l)
                        ? `Leave ${LAYER_LABEL[l].toLowerCase()}s out of the results`
                        : `Include ${LAYER_LABEL[l].toLowerCase()}s in the results`
                    }
                    onClick={() => toggle(l)}
                  >
                    {LAYER_LABEL[l]}
                  </button>
                ))}
              </div>
              <select
                className="sel"
                aria-label="When"
                title="Only notes dated (or changed) in this time"
                value={when}
                onChange={(e) => setWhen(e.target.value)}
              >
                {WHEN.map(([v, l]) => (
                  <option key={v} value={v}>
                    {l}
                  </option>
                ))}
              </select>
              <Seg<"best" | "newest" | "oldest">
                label="Order"
                value={sort}
                onChange={setSort}
                options={[
                  ["best", "Best match", "The best matches first"],
                  ["newest", "Latest", "The latest first, by the note's date or the day it last changed"],
                  ["oldest", "Oldest", "The oldest first, by the note's date or the day it last changed"],
                ]}
              />
            </div>
            {found?.error && (
              <div className="scount err" role="alert">
                The search didn't run: {found.error}{" "}
                <button type="button" className="btn sm" title="Run the search again" onClick={found.retry}>
                  Retry
                </button>
              </div>
            )}
            {r && !found?.error && (
              <div className="faint scount">
                {r.total === 0
                  ? "No matches"
                  : `${fmtCount(r.total)} ${r.total === 1 ? "file" : "files"}${r.total > hits.length ? `, the best ${hits.length} shown` : ""}`}{" "}
                · {r.ms.toFixed(0)} ms
              </div>
            )}
          </div>
          {!r && (
            <div className="shelp faint">
              <p>
                Words match anywhere and in any form (<i>launch</i> finds <i>launching</i>). <span className="mono">+word</span> must be
                there, <span className="mono">-word</span> mustn't, <span className="mono">"two words"</span> together,{" "}
                <span className="mono">tag:hiring</span> by tag (and its sub-tags), <span className="mono">since:2026-09</span> and{" "}
                <span className="mono">before:2026-10-01</span> by the note's date (or the day it last changed),{" "}
                <span className="mono">AND</span> makes both sides required.
              </p>
            </div>
          )}
          <ol className="hits">
            {hits.map((h, i) => (
              <li key={h.file.path}>
                <button
                  type="button"
                  className={`hit ${i === active ? "on" : ""}`}
                  onMouseEnter={() => setActive(i)}
                  title={`Open ${h.file.path} with the matches marked`}
                  onClick={() => openDoc(h.file.path, { q: q.trim() })}
                  onContextMenu={(e) => menu.openAt(e, h.file)}
                >
                  <div className="hhead">
                    <span className={`lb ${h.file.layer}`}>{LAYER_LABEL[h.file.layer]}</span>
                    <b className="ell">{h.file.title}</b>
                    {h.file.type && <span className="chip">{h.file.type}</span>}
                    <span className="sp" />
                    {h.passages > 1 && <span className="faint">{h.passages} passages</span>}
                    {h.file.date && <span className="faint">{h.file.date}</span>}
                  </div>
                  <Snippet hit={h} />
                </button>
              </li>
            ))}
          </ol>
        </div>
      </div>
      <FileMenu menu={menu} />
    </main>
  );
}

/** Names a search and keeps it in Me. Smart Lists.md. */
function SaveSearch({ query, layers, onClose }: { query: string; layers: Layer[]; onClose: () => void }) {
  const [name, setName] = useState("");
  const save = () => {
    if (!name.trim()) return;
    api
      .smartListSave(name.trim(), query, layers)
      .then(() => {
        toast(`Saved “${name.trim()}”`, { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok");
        onClose();
      })
      .catch(reportEditError);
  };
  return (
    <Dialog onClose={onClose} width={420} label="Save search">
      <div className="confirm">
        <h2 className="h2">Save this search</h2>
        <p className="faint mono small">
          {query}
          {layers.length ? ` · ${layers.join(", ")}` : ""}
        </p>
        <label className="nnf">
          <span className="faint">Name</span>
          <span className="inp">
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && save()}
              placeholder="Launch notes"
            />
          </span>
        </label>
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" title="Close without saving the list" onClick={onClose}>
            Cancel
          </button>
          <span title={name.trim() ? "Save this search as a smart list (↩)" : "Give the list a name first"}>
            <button type="button" className="btn lg pri" disabled={!name.trim()} onClick={save}>
              Save
            </button>
          </span>
        </div>
      </div>
    </Dialog>
  );
}
