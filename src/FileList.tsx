// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The table the Notes, Wiki and Sources lists share: sortable columns, an in-list search that
// moves between matches with the arrow keys (the previous app's SearchNavInput), and the right-click
// menu.

import { iconizeEmoji } from "./md/fieldEmoji";
import { plainSegments, plainText } from "./md/plainText";
import { ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { api, FileSummary, Layer, SearchHit } from "./api";
import { FileMenu, useFileMenu } from "./FileMenu";
import { Icon } from "./icons";
import { openDoc, useViewState } from "./nav";
import { settings, useStore } from "./store";
import { fmtCount, SearchBox, Seg, TableBand, useDebounced } from "./ui";

/** Rows in bands, one per value of `of` (A–Z, rows with none last): Notes by type. */
export interface Grouping<T> {
  of: (r: T) => string | null;
  /** The band of rows with no value: "No type". */
  none: string;
  icon: string;
  /** What a band's count says: "3 notes of this type". */
  tip: (n: number, key: string | null) => string;
}

export interface Column<T> {
  key: string;
  label: string;
  value: (r: T) => string | number | null;
  render?: (r: T, hit?: SearchHit) => ReactNode;
  width?: string;
  align?: "right";
}

/** Sorts with empty values last whichever way round, as the previous app's DataTable does. */
export function sortRows<T>(rows: T[], col: Column<T> | undefined, dir: 1 | -1): T[] {
  if (!col) return rows;
  return [...rows].sort((a, b) => {
    const x = col.value(a),
      y = col.value(b);
    const ex = x === null || x === "",
      ey = y === null || y === "";
    if (ex || ey) return ex === ey ? 0 : ex ? 1 : -1;
    const c =
      typeof x === "number" && typeof y === "number"
        ? x - y
        : String(x).localeCompare(String(y), "en", { numeric: true, sensitivity: "base" });
    return c * dir;
  });
}

/** In-list search, ranked by the index in the list's layers: the same query finds the same files
 *  as the Search page. A search that fails says why, with a way to run it again. */
function useListSearch(q: string, layers: Layer[]) {
  const [hits, setHits] = useState<Map<string, SearchHit> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tries, setTries] = useState(0);
  useEffect(() => {
    let live = true;
    const t = q.trim();
    setError(null);
    if (t.length < 2) {
      setHits(null);
      return;
    }
    api
      .search(t, layers, 500)
      .then((r) => live && setHits(new Map(r.hits.map((h) => [h.file.path, h]))))
      .catch((e) => {
        if (!live) return;
        setHits(new Map());
        setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [q, layers.join(","), tries]); // eslint-disable-line react-hooks/exhaustive-deps
  return { hits, error, retry: () => setTries((n) => n + 1) };
}

/** Rows drawn at first, and added each time the end of the list comes into view. */
const PAGE = 100;

export function FileList<T extends FileSummary>({
  rows,
  columns,
  layers,
  noun,
  defaultSort,
  filters,
  empty,
  placeholder,
  group,
}: {
  rows: T[] | null;
  columns: Column<T>[];
  layers: Layer[];
  noun: [string, string];
  defaultSort: [string, 1 | -1];
  filters?: ReactNode;
  empty?: ReactNode;
  placeholder: string;
  group?: Grouping<T>;
}) {
  // What's typed, and the debounced query everything else follows.
  const [input, setQ] = useViewState(`list:${noun[1]}:q`, "");
  const q = useDebounced(input);
  // The user's sort is kept (with the history entry); an index search shows best matches first
  // until a column is clicked, and the user's sort comes back when the search is cleared.
  const [chosen, setSort] = useViewState<[string, 1 | -1]>(`list:${noun[1]}:sort`, defaultSort);
  // Whether search results follow the switch by the box (false once a column is clicked during a
  // search, until the search changes); kept too, so a restart shows the same order.
  const [ranked, setRanked] = useViewState(`list:${noun[1]}:ranked`, true);
  const [active, setActive] = useState(0);
  const { hits, error: searchError, retry } = useListSearch(q, layers);
  const menu = useFileMenu();
  const table = useRef<HTMLDivElement>(null);
  const searching = q.trim().length >= 2;
  // While searching: best matches first, or the latest first (the switch by the box, kept per list).
  const canLatest = columns.some((c) => c.key === "mtime");
  const order = useStore(settings).searchOrder?.[noun[1]];
  const latest = canLatest && order === "latest";
  const [sk, sd]: [string, 1 | -1] =
    searching && ranked ? (latest ? ["mtime", -1] : ["_rank", -1]) : chosen[0] === "_rank" ? defaultSort : chosen;
  const sort = useMemo((): [string, 1 | -1] => [sk, sd], [sk, sd]);

  const sorted = useMemo(() => {
    if (!rows) return [];
    // Ranked by the search while searching (a click on a column sorts the matches instead).
    if (searching && hits) {
      const m = rows.filter((r) => hits.has(r.path));
      return sort[0] === "_rank"
        ? m.sort((a, b) => hits.get(b.path)!.score - hits.get(a.path)!.score)
        : sortRows(
            m,
            columns.find((c) => c.key === sort[0]),
            sort[1],
          );
    }
    return sortRows(
      rows,
      columns.find((c) => c.key === sort[0]),
      sort[1],
    );
  }, [rows, hits, sort, columns, searching]);
  // Grouped, each band keeps the order within it; the arrow keys follow the order shown.
  const groups = useMemo(() => {
    if (!group) return null;
    const m = new Map<string, T[]>();
    for (const r of sorted) {
      const k = group.of(r) ?? "";
      m.set(k, [...(m.get(k) ?? []), r]);
    }
    return [...m].sort(([a], [b]) => ((a === "") !== (b === "") ? (a === "" ? 1 : -1) : a.localeCompare(b)));
  }, [sorted, group]);
  const shown = useMemo(() => (groups ? groups.flatMap(([, rs]) => rs) : sorted), [groups, sorted]);

  const lastQ = useRef(q);
  useEffect(() => {
    setActive(0);
    if (q !== lastQ.current) setRanked(true);
    lastQ.current = q;
  }, [q]); // eslint-disable-line react-hooks/exhaustive-deps
  const idx = Math.min(active, Math.max(0, shown.length - 1));
  // Rows are drawn PAGE at a time: more as the end comes into view, or as the arrow keys reach it.
  const [limit, setLimit] = useState(PAGE);
  useEffect(() => {
    setLimit(PAGE);
  }, [shown]);
  if (idx >= limit) setLimit(idx + PAGE);
  const end = useRef<HTMLDivElement>(null);
  const more = limit < shown.length;
  useEffect(() => {
    const el = end.current;
    if (!el || !more) return;
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && setLimit((l) => l + PAGE), { rootMargin: "400px" });
    io.observe(el);
    return () => io.disconnect();
  }, [more, limit]);
  useEffect(() => {
    table.current?.querySelector(`[data-row="${idx}"]`)?.scrollIntoView({ block: "nearest" });
  }, [idx]);

  const key = (e: React.KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    // Esc clears the box whatever it found, nothing included.
    if (e.key === "Escape") {
      if (input) {
        setQ("");
        e.preventDefault();
      }
      return;
    }
    const n = shown.length;
    if (!n) return;
    if (e.key === "ArrowDown") setActive((idx + 1) % n);
    else if (e.key === "ArrowUp") setActive((idx - 1 + n) % n);
    else if (e.key === "Enter") openDoc(shown[idx].path, searching ? { q: q.trim() } : {});
    else return;
    e.preventDefault();
  };
  const row = (r: T, i: number) => {
    const hit = hits?.get(r.path);
    return (
      <div
        key={r.path}
        data-row={i}
        role="row"
        className={`frow tb-row ${i === idx && searching ? "sel" : ""}`}
        onClick={() => openDoc(r.path, searching ? { q: q.trim() } : {})}
        onContextMenu={(e) => menu.openAt(e, r)}
      >
        {columns.map((c) => (
          <div key={c.key} role="gridcell" className={`fcell ${c.align ?? ""}`} style={{ flex: c.width ?? "1" }}>
            {c.render ? c.render(r, hit) : (c.value(r) ?? "—")}
          </div>
        ))}
      </div>
    );
  };
  const total = rows?.length ?? 0;

  return (
    <div className="flist">
      <div className="ftools">
        <SearchBox value={input} onChange={setQ} onKeyDown={key} placeholder={placeholder} className="listq">
          {searching && (
            <span className="faint nav">
              {shown.length ? `${idx + 1}/${shown.length}` : "0"}
              <span className="kbd">↑↓</span>
            </span>
          )}
        </SearchBox>
        {canLatest && searching && (
          <Seg
            label="Search results order"
            value={latest ? "latest" : "ranked"}
            options={[
              ["ranked", "Best match", "Search results with the best matches first"],
              ["latest", "Latest", "Search results with the most recently changed first"],
            ]}
            onChange={(v) => {
              setRanked(true);
              settings.update({ searchOrder: { ...settings.get().searchOrder, [noun[1]]: v } });
            }}
          />
        )}
        {filters}
        <div className="sp" />
        <span className="faint count">
          {shown.length === total
            ? `${fmtCount(total)} ${total === 1 ? noun[0] : noun[1]}`
            : `${fmtCount(shown.length)} of ${fmtCount(total)} ${noun[1]}`}
        </span>
      </div>
      <div className={`ftable ${groups ? "grouped" : ""}`} ref={table} role="grid" aria-rowcount={shown.length}>
        <div className="frow fhead tb-head" role="row">
          {columns.map((c) => (
            <button
              key={c.key}
              type="button"
              role="columnheader"
              className={`fcell ${c.align ?? ""}`}
              style={{ flex: c.width ?? "1" }}
              aria-sort={sort[0] === c.key ? (sort[1] === 1 ? "ascending" : "descending") : "none"}
              title={`Sort by ${c.label.toLowerCase()}${sort[0] === c.key ? "; click again to reverse" : ""}`}
              onClick={() => {
                setRanked(false);
                setSort(sort[0] === c.key ? [c.key, sort[1] === 1 ? -1 : 1] : [c.key, 1]);
              }}
            >
              {c.label}
              {sort[0] === c.key && <Icon name={sort[1] === 1 ? "chevup" : "chevdown"} size={11} />}
            </button>
          ))}
        </div>
        {rows === null && <div className="fempty faint">Loading…</div>}
        {rows !== null && searching && searchError && (
          <div className="fempty err" role="alert">
            The search didn't run: {searchError}{" "}
            <button type="button" className="btn sm" title="Run the search again" onClick={retry}>
              Retry
            </button>
          </div>
        )}
        {rows !== null && shown.length === 0 && !(searching && searchError) && (
          <div className="fempty faint">{searching ? `Nothing matches “${q.trim()}”.` : (empty ?? "Nothing here yet.")}</div>
        )}
        {groups && group
          ? (() => {
              // The bands of the rows drawn so far (rows are drawn PAGE at a time); counts are the whole band's.
              let at = 0;
              return groups.map(([k, rs]) => {
                const from = at;
                at += rs.length;
                if (from >= limit) return null;
                return (
                  <div key={k} className="tb-group" role="rowgroup">
                    <TableBand icon={group.icon} label={k || group.none} none={!k} n={rs.length} tip={group.tip(rs.length, k || null)} />
                    {rs.slice(0, limit - from).map((r, j) => row(r, from + j))}
                  </div>
                );
              });
            })()
          : shown.slice(0, limit).map((r, i) => row(r, i))}
        {more && (
          <div ref={end} className="fempty faint">
            Loading more…
          </div>
        )}
      </div>
      <FileMenu menu={menu} />
    </div>
  );
}

/** A search hit's passage, with the matched words marked. */
export function Snippet({ hit }: { hit?: SearchHit }) {
  if (!hit?.snippet.length) return null;
  return (
    <div className="snip">
      {hit.heading && <span className="snh">{plainText(hit.heading)} · </span>}
      {plainSegments(hit.snippet).map((s, i) =>
        s.hit ? <mark key={i}>{iconizeEmoji(s.text)}</mark> : <span key={i}>{iconizeEmoji(s.text)}</span>,
      )}
    </div>
  );
}

/** Modification time as "2 Oct 2026". */
export const fmtDay = (ms: number) => new Date(ms).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
