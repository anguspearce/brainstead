// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Library lists: Notes (type filters), Wiki (tag tree), Sources (ingested or pending) and
// Templates.

import { useEffect, useMemo, useState } from "react";
import { api, FileSummary, Layer, LintCheck, LintReport, SourceRow, WikiMeta } from "./api";
import { filedPage } from "./Ask";
import { Column, FileList, fmtDay, Grouping, Snippet } from "./FileList";
import { Icon } from "./icons";
import { ingest, RunsPane } from "./Ingest";
import { runSourceAction, SOURCE_ACTION, sourceActions } from "./sourceActions";
import { isTranscriptPath } from "./meetingFlow";
import { checkTemplate } from "./notes/templater";
import { changes, health, held, startKnowledge } from "./knowledge";
import { settings, useStore } from "./store";
import { openNewNote, setNewNoteCursor } from "./notes/dialogs";
import { nav, openDoc, useViewState } from "./nav";
import { reportEditError } from "./taskModel";
import { toast } from "./Toast";
import { sourceBadge } from "./SourcePreview";
import { useVaultVersion } from "./state";
import { useDraftPaths } from "./drafts";
import { IMAGE_EXT } from "./md/wikilinks";
import { TopBar } from "./TopBar";
import { Dialog, fmtBytes, fmtCount, Popover, Seg } from "./ui";
import { keyLabel } from "./platform";

function useFiles<T = FileSummary>(load: () => Promise<T[]>): T[] | null {
  const [rows, setRows] = useState<T[] | null>(null);
  const v = useVaultVersion();
  useEffect(() => {
    let live = true;
    let retry = 0;
    // A failed load (the index busy, the vault reopening) keeps what was shown and tries again,
    // rather than showing an empty list until the next change.
    const go = (wait: number) =>
      load()
        .then((r) => live && setRows(r))
        .catch(() => {
          if (!live) return;
          setRows((was) => was ?? []);
          if (wait < 8000) retry = window.setTimeout(() => go(wait * 2), wait);
        });
    void go(500);
    return () => {
      live = false;
      window.clearTimeout(retry);
    };
  }, [v]); // eslint-disable-line react-hooks/exhaustive-deps
  return rows;
}

const titleCol = <T extends FileSummary>(drafts?: Set<string>): Column<T> => ({
  key: "title",
  label: "Title",
  width: "3",
  value: (r) => r.title,
  render: (r, hit) => (
    <div className="ftitle">
      <span className="ell">
        {r.title}
        {drafts?.has(r.path) && (
          <span className="chip amber draftmark" title="Unsaved changes are kept as a draft: open the note to save or discard them">
            Draft
          </span>
        )}
      </span>
      <Snippet hit={hit} />
    </div>
  ),
});
const dateCol = <T extends FileSummary>(): Column<T> => ({ key: "date", label: "Date", width: "0 0 96px", value: (r) => r.date });
const modCol = <T extends FileSummary>(): Column<T> => ({
  key: "mtime",
  label: "Modified",
  width: "0 0 110px",
  value: (r) => r.mtime,
  render: (r) => <span className="faint">{fmtDay(r.mtime)}</span>,
});
const sizeCol = <T extends FileSummary>(): Column<T> => ({
  key: "size",
  label: "Size",
  width: "0 0 76px",
  align: "right",
  value: (r) => r.size,
  render: (r) => <span className="faint">{fmtBytes(r.size)}</span>,
});

/** The type chips: the five commonest types, then Other for untyped notes and the rest. */
export function typeChips(rows: FileSummary[], selected: string | null): [string, number][] {
  const counts = new Map<string, number>();
  for (const r of rows) if (r.type) counts.set(r.type, (counts.get(r.type) ?? 0) + 1);
  const top = [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, 5);
  if (selected && selected !== "other" && !top.some(([t]) => t === selected) && counts.has(selected))
    top.push([selected, counts.get(selected)!]);
  const inTop = new Set(top.map(([t]) => t));
  const other = rows.filter((r) => !r.type || !inTop.has(r.type)).length;
  return other ? [...top, ["other", other]] : top;
}

/** Notes in bands by type, when "Group by type" is on. */
const BY_TYPE: Grouping<FileSummary> = {
  of: (r) => r.type || null,
  none: "No type",
  icon: "note",
  tip: (n, t) => `${fmtCount(n)} ${n === 1 ? "note" : "notes"} ${t ? "of this type" : "with no type"}`,
};

export function NotesScreen() {
  const rows = useFiles(() => api.filesList("note"));
  const drafts = useDraftPaths();
  const [type, setType] = useViewState<string | null>("notes:type", null);
  const [byType, setByType] = useViewState("notes:group", false);
  const chips = useMemo(() => typeChips(rows ?? [], type), [rows, type]);
  const shown = useMemo(() => {
    if (!rows || !type) return rows;
    const named = new Set(chips.filter(([t]) => t !== "other").map(([t]) => t));
    return rows.filter((r) => (type === "other" ? !r.type || !named.has(r.type) : r.type === type));
  }, [rows, type, chips]);
  const columns: Column<FileSummary>[] = [
    {
      key: "type",
      label: "Type",
      width: "0 0 110px",
      value: (r) => r.type,
      render: (r) => (r.type ? <span className="chip">{r.type}</span> : <span className="faint">—</span>),
    },
    titleCol(drafts),
    dateCol(),
    { key: "path", label: "File", width: "2", value: (r) => r.path, render: (r) => <span className="faint ell mono">{r.path}</span> },
    modCol(),
    sizeCol(),
  ];
  return (
    <main className="main">
      <TopBar
        title="Notes"
        ask={
          type ? { about: `your ${type} notes`, prompt: `About my ${type} notes: ` } : { about: "your notes", prompt: "About my notes: " }
        }
      >
        <button type="button" className="btn pri" onClick={() => openNewNote()} title={keyLabel("New note (⌘N)")}>
          <Icon name="plus" size={14} />
          New note
        </button>
      </TopBar>
      <div className="body">
        <FileList
          rows={shown}
          columns={columns}
          layers={["note"]}
          noun={["note", "notes"]}
          defaultSort={["mtime", -1]}
          placeholder="Search notes"
          group={byType ? BY_TYPE : undefined}
          filters={
            <div className="chips">
              <button
                type="button"
                className={`chip f ${type === null ? "on" : ""}`}
                title="Show notes of every type"
                onClick={() => setType(null)}
              >
                All
              </button>
              {chips.map(([t, n]) => (
                <button
                  key={t}
                  type="button"
                  className={`chip f ${type === t ? "on" : ""}`}
                  title={
                    type === t
                      ? "Show notes of every type again"
                      : t === "other"
                        ? "Show only notes of the less common types, or none"
                        : `Show only notes whose type is ${t}`
                  }
                  onClick={() => setType(type === t ? null : t)}
                >
                  {t === "other" ? "Other" : t} <span className="faint">{fmtCount(n)}</span>
                </button>
              ))}
              <button
                type="button"
                className={`chip f ${byType ? "on" : ""}`}
                aria-pressed={byType}
                title={byType ? "Show the notes as one list again" : "Show the notes in a band per type, each with its count"}
                onClick={() => setByType(!byType)}
              >
                <Icon name="folder" size={11} />
                Group by type
              </button>
            </div>
          }
        />
      </div>
    </main>
  );
}

export interface TagNode {
  name: string;
  full: string;
  count: number;
  children: TagNode[];
}

/** Nested tags from `a/b/c`; a page counts once under every prefix of each tag it has. */
export function tagTree(rows: FileSummary[]): TagNode[] {
  const root: TagNode = { name: "", full: "", count: 0, children: [] };
  for (const r of rows) {
    const seen = new Set<string>();
    for (const tag of r.tags) {
      const parts = tag.replace(/^#/, "").split("/").filter(Boolean);
      let node = root;
      parts.forEach((p, i) => {
        const full = parts.slice(0, i + 1).join("/");
        let c = node.children.find((x) => x.name === p);
        if (!c) node.children.push((c = { name: p, full, count: 0, children: [] }));
        if (!seen.has(full)) {
          seen.add(full);
          c.count++;
        }
        node = c;
      });
    }
  }
  const sortRec = (ns: TagNode[]) => {
    ns.sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
    ns.forEach((n) => sortRec(n.children));
  };
  sortRec(root.children);
  return root.children;
}

export const hasTag = (r: FileSummary, sel: string) =>
  r.tags.some((t) => t.replace(/^#/, "") === sel || t.replace(/^#/, "").startsWith(`${sel}/`));

function TagTreeView({
  nodes,
  sel,
  onSel,
  depth = 0,
}: {
  nodes: TagNode[];
  sel: string | null;
  onSel: (t: string | null) => void;
  depth?: number;
}) {
  return (
    <>
      {nodes.map((n) => {
        const open = !!sel && (sel === n.full || sel.startsWith(`${n.full}/`));
        return (
          <div key={n.full}>
            <button
              type="button"
              className={`tnode ${sel === n.full ? "on" : ""}`}
              style={{ paddingLeft: 8 + depth * 14 }}
              title={sel === n.full ? "Show all pages again" : `Show pages tagged #${n.full}, and its sub-tags`}
              onClick={() => onSel(sel === n.full ? null : n.full)}
            >
              <Icon
                name="chevright"
                size={11}
                style={{ visibility: n.children.length ? "visible" : "hidden", transform: open ? "rotate(90deg)" : undefined }}
              />
              <span className="ell">{n.name}</span>
              <span className="n">{n.count}</span>
            </button>
            {open && n.children.length > 0 && <TagTreeView nodes={n.children} sel={sel} onSel={onSel} depth={depth + 1} />}
          </div>
        );
      })}
    </>
  );
}

/** A wiki page's health in the list: a change to it held for you, flagged stale, another Knowledge
 *  health issue, or none. */
export function wikiHealth(path: string, heldPages: Set<string>, report: LintReport | null | undefined): "held" | "stale" | "issue" | "ok" {
  if (heldPages.has(path)) return "held";
  const on = (c: LintCheck) => c.items.some((i) => i.page === path || i.pages?.includes(path));
  const checks = report?.checks ?? [];
  if (checks.some((c) => (c.id === "stale-pages" || c.id === "stale-dates" || c.id === "changed-sources") && on(c))) return "stale";
  if (checks.some((c) => c.id !== "unlogged-writes" && on(c))) return "issue";
  return "ok";
}

const HEALTH_CHIP = {
  held: ["Held", "acc", "Held: a change to this page waits in Changes for you to accept or reject"],
  stale: ["Stale", "amber", "Stale: a source changed after this page was written, or Knowledge health flags it as out of date"],
  issue: [
    "Check",
    "amber",
    "Check: Knowledge health lists something to look at on this page, such as a link to a page that doesn't exist, or a source that has gone",
  ],
  ok: ["Healthy", "green", "Healthy: Knowledge health finds nothing to fix on this page"],
} as const;

export function WikiScreen() {
  const rows = useFiles(() => api.filesList("wiki"));
  const [tag, setTag] = useViewState<string | null>("wiki:tag", null);
  const [type, setType] = useViewState<string | null>("wiki:type", null);
  const [creating, setCreating] = useState<DOMRect | null>(null);
  const v = useVaultVersion();
  const [meta, setMeta] = useState<Map<string, WikiMeta>>(new Map());
  useEffect(() => {
    void api
      .wikiMeta()
      .then((m) => setMeta(new Map(m.map((x) => [x.path, x]))))
      .catch(() => {});
  }, [v]);
  useEffect(startKnowledge, []);
  const all = useStore(changes);
  const report = useStore(health)?.report;
  const pendingPages = useMemo(() => new Set(held(all ?? []).map((c) => c.page)), [all]);
  const tree = useMemo(() => tagTree(rows ?? []), [rows]);
  const untagged = (rows ?? []).filter((r) => !r.tags.length).length;
  const byTag = useMemo(
    () => (!rows || tag === null ? rows : tag === "" ? rows.filter((r) => !r.tags.length) : rows.filter((r) => hasTag(r, tag))),
    [rows, tag],
  );
  const chips = useMemo(() => typeChips(byTag ?? [], type), [byTag, type]);
  const shown = useMemo(() => {
    if (!byTag || !type) return byTag;
    const named = new Set(chips.filter(([t]) => t !== "other").map(([t]) => t));
    return byTag.filter((r) => (type === "other" ? !r.type || !named.has(r.type) : r.type === type));
  }, [byTag, type, chips]);
  const columns: Column<FileSummary>[] = [
    {
      key: "title",
      label: "Title",
      width: "3",
      value: (r) => r.title,
      render: (r, hit) => {
        const aka = meta.get(r.path)?.aliases ?? [];
        return (
          <div className="ftitle">
            <span className="ell">{r.title}</span>
            {aka.length > 0 && !hit && <span className="faint small ell">also {aka.join(", ")}</span>}
            <Snippet hit={hit} />
          </div>
        );
      },
    },
    {
      key: "type",
      label: "Type",
      width: "0 0 110px",
      value: (r) => r.type,
      render: (r) => (r.type ? <span className="chip">{r.type}</span> : <span className="faint">—</span>),
    },
    {
      key: "sources",
      label: "Sources",
      width: "0 0 72px",
      align: "right",
      value: (r) => meta.get(r.path)?.sources ?? 0,
      render: (r) => <span className="faint">{meta.get(r.path)?.sources ?? 0}</span>,
    },
    {
      key: "health",
      label: "Health",
      width: "0 0 96px",
      value: (r) => wikiHealth(r.path, pendingPages, report),
      render: (r) => {
        const [label, tone, tip] = HEALTH_CHIP[wikiHealth(r.path, pendingPages, report)];
        return (
          <span className={`chip ${tone}`} title={tip}>
            {label}
          </span>
        );
      },
    },
    {
      key: "tags",
      label: "Tags",
      width: "2",
      value: (r) => r.tags.join(" "),
      render: (r) => <span className="faint ell">{r.tags.map((t) => `#${t}`).join(" ")}</span>,
    },
    modCol(),
  ];
  return (
    <main className="main">
      <TopBar
        title="Wiki"
        sub={tag === "" ? "Untagged" : (tag ?? undefined)}
        ask={
          tag
            ? { about: `the wiki's #${tag} pages`, prompt: `About the wiki's #${tag} pages: ` }
            : { about: "the wiki", prompt: "About the wiki: " }
        }
      >
        <button
          type="button"
          className="btn pri"
          title="Make a new concept or entity page in the wiki"
          onClick={(e) => setCreating(e.currentTarget.getBoundingClientRect())}
        >
          <Icon name="plus" size={14} />
          New wiki page
        </button>
      </TopBar>
      {creating && <NewWikiPage at={creating} onClose={() => setCreating(null)} />}
      <div className="body split">
        <nav className="tagtree" aria-label="Tags">
          <button type="button" className={`tnode ${tag === null ? "on" : ""}`} title="Show every wiki page" onClick={() => setTag(null)}>
            <Icon name="wiki" size={13} />
            <span className="ell">All pages</span>
            <span className="n">{rows?.length ?? ""}</span>
          </button>
          {untagged > 0 && (
            <button
              type="button"
              className={`tnode ${tag === "" ? "on" : ""}`}
              title={tag === "" ? "Show every wiki page" : "Show only the pages with no tags"}
              onClick={() => setTag(tag === "" ? null : "")}
            >
              <Icon name="tag" size={13} />
              <span className="ell">Untagged</span>
              <span className="n">{untagged}</span>
            </button>
          )}
          <div className="sect static">Tags</div>
          <TagTreeView nodes={tree} sel={tag || null} onSel={setTag} />
        </nav>
        <FileList
          rows={shown}
          columns={columns}
          layers={["wiki"]}
          noun={["wiki page", "wiki pages"]}
          defaultSort={["title", 1]}
          placeholder="Search the wiki"
          filters={
            <div className="chips">
              <button
                type="button"
                className={`chip f ${type === null ? "on" : ""}`}
                title="Show pages of every type"
                onClick={() => setType(null)}
              >
                All
              </button>
              {chips.map(([t, n]) => (
                <button
                  key={t}
                  type="button"
                  className={`chip f ${type === t ? "on" : ""}`}
                  title={
                    type === t
                      ? "Show pages of every type again"
                      : t === "other"
                        ? "Show only pages of the less common types, or none"
                        : `Show only pages whose type is ${t}`
                  }
                  onClick={() => setType(type === t ? null : t)}
                >
                  {t === "other" ? "Other" : t} <span className="faint">{fmtCount(n)}</span>
                </button>
              ))}
            </div>
          }
        />
      </div>
    </main>
  );
}

/** New wiki page: a concept or an entity, written at once (the user's own page, not a model's) and
 *  opened in Edit. */
function NewWikiPage({ at, onClose }: { at: DOMRect; onClose: () => void }) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"concept" | "entity">("concept");
  const n = name.trim();
  const ok = !!n && !/[\\/:*?"<>|#^[\]]/.test(n);
  const make = async () => {
    if (!ok) return;
    const path = `wiki/${kind === "concept" ? "concepts" : "entities"}/${n}.md`;
    try {
      await api.docCreate(path, filedPage(n, kind, ""));
      onClose();
      openDoc(path);
    } catch (e) {
      reportEditError(e);
    }
  };
  return (
    <Popover anchor={at} onClose={onClose} width={340} place="below">
      <div className="newwiki">
        <label className="inp">
          <input
            autoFocus
            value={name}
            placeholder="Page name"
            aria-label="Page name"
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void make()}
          />
        </label>
        <Seg<"concept" | "entity">
          label="Kind"
          value={kind}
          onChange={setKind}
          options={[
            ["concept", "Concept", "An idea, a process, a topic"],
            ["entity", "Entity", "A person, a team, a product, a company"],
          ]}
        />
        <div className="row">
          <span className="faint small grow">In wiki/{kind === "concept" ? "concepts" : "entities"}/</span>
          <span title={ok ? "Create the page and open it (↩)" : "Type a name the page can be saved under"}>
            <button type="button" className="btn sm pri" disabled={!ok} onClick={() => void make()}>
              Create
            </button>
          </span>
        </div>
      </div>
    </Popover>
  );
}

/** A source's status: ingested (a wiki page cites it), changed since (it's newer than the pages
 *  citing it), or new. */
export function sourceStatus(r: SourceRow, changed: Set<string>): "new" | "changed" | "ingested" {
  if (changed.has(r.path)) return "changed";
  return r.ingested ? "ingested" : "new";
}

/** Files an ingest can read: markdown and text, PDFs, Office files and images (not SVG: its text
 *  is drawn, not photographed). Matches `kind_of` in src-tauri/core/src/extract.rs. */
export const ingestable = (path: string) => /\.(md|txt|pdf|docx|pptx|xlsx)$/i.test(path) || (IMAGE_EXT.test(path) && !/\.svg$/i.test(path));

/** A file the file menu offers Ingest on (start_run ingest takes the same): one an ingest can read,
 *  and not a wiki page or a template, which are the wiki's and the app's own. */
export const canIngest = (path: string) => ingestable(path) && !path.startsWith("wiki/") && !path.startsWith("Templates/");

/** The folder a source sits in under sources/, or "" for one at the top. */
export const sourceFolder = (path: string) =>
  path
    .replace(/^sources\//, "")
    .split("/")
    .slice(0, -1)
    .join("/");

const STATUS_CHIP = { new: ["New", "amber"], changed: ["Changed", "amber"], ingested: ["Ingested", "green"] } as const;

/** Copies files (full paths) into sources/ and says how it went. */
async function importToSources(paths: string[]) {
  try {
    const r = await api.sourcesImport(paths);
    const ok = r.filter((x) => x.path);
    const bad = r.filter((x) => !x.path);
    if (ok.length === 1) toast(`Added “${ok[0].name}” to Sources`, { label: "Open", run: () => openDoc(ok[0].path!) });
    else if (ok.length > 1) toast(`Added ${ok.length} files to Sources`);
    // Settings › AI assistants: new sources are ingested as they arrive.
    const added = ok.map((x) => x.path!).filter(ingestable);
    const toIngest = added.filter((p) => !isTranscriptPath(p));
    if (settings.get().ingestOnArrival && toIngest.length) ingest(toIngest);
    if (bad.length)
      toast(bad.length === 1 ? (bad[0].error ?? "Not added") : `${bad.length} files not added: ${bad[0].error}`, undefined, "bad");
  } catch (e) {
    reportEditError(e);
  }
}

/** Import…: files picked in a dialog, as a drop would add them. */
async function pickSources() {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: true, directory: false, title: "Add files to Sources" });
  const paths = picked == null ? [] : Array.isArray(picked) ? picked : [picked];
  if (paths.length) await importToSources(paths);
}

/** Files dragged from Finder onto the Sources screen are copied into sources/. Whether something
 *  is being dragged over it now. */
function useDropToSources(): boolean {
  const [over, setOver] = useState(false);
  useEffect(() => {
    let off: (() => void) | undefined;
    let live = true;
    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent(async (ev) => {
          const t = ev.payload.type;
          if (t === "enter" || t === "over") return setOver(true);
          setOver(false);
          if (t !== "drop" || !ev.payload.paths.length) return;
          await importToSources(ev.payload.paths);
        }),
      )
      .then((u) => {
        if (live) off = u;
        else u();
      })
      .catch(() => {});
    return () => {
      live = false;
      off?.();
    };
  }, []);
  return over;
}

export function SourcesScreen() {
  const rows = useFiles<SourceRow>(() => api.sourcesList());
  useEffect(startKnowledge, []);
  const h = useStore(health);
  const changed = useMemo(
    () => new Set((h?.report?.checks.find((c) => c.id === "changed-sources")?.items ?? []).map((i) => i.page ?? "")),
    [h],
  );
  const [status, setStatus] = useViewState<"all" | "new" | "changed" | "ingested">("sources:status", "all");
  const shown = useMemo(
    () => (!rows || status === "all" ? rows : rows.filter((r) => sourceStatus(r, changed) === status)),
    [rows, status, changed],
  );
  // Transcripts wait for their meeting notes (Meeting note on their rows), so they aren't ingested as they are.
  const waiting = (rows ?? [])
    .filter((r) => ingestable(r.path) && sourceStatus(r, changed) !== "ingested" && !isTranscriptPath(r.path))
    .map((r) => r.path);
  const dropping = useDropToSources();
  const columns: Column<SourceRow>[] = [
    {
      key: "kind",
      label: "Kind",
      width: "0 0 64px",
      value: (r) => sourceBadge(r.path),
      render: (r) => <span className="badge">{sourceBadge(r.path)}</span>,
    },
    titleCol(),
    {
      key: "ingested",
      label: "Status",
      width: "0 0 96px",
      value: (r) => sourceStatus(r, changed),
      render: (r) => {
        const [label, tone] = STATUS_CHIP[sourceStatus(r, changed)];
        return <span className={`chip ${tone}`}>{label}</span>;
      },
    },
    {
      key: "act",
      label: "",
      width: "0 0 150px",
      value: () => "",
      render: (r) => <SourceButton path={r.path} ingested={r.ingested} />,
    },
    {
      // Only the folder: the title already names the file.
      key: "path",
      label: "Folder",
      width: "0 0 140px",
      value: (r) => sourceFolder(r.path),
      render: (r) => (
        <span className="faint ell" title={r.path}>
          {sourceFolder(r.path) || "—"}
        </span>
      ),
    },
    modCol(),
    sizeCol(),
  ];
  return (
    <main className="main">
      <TopBar title="Sources" ask={{ about: "your sources", prompt: "About my sources: " }}>
        <button type="button" className="btn" title="Copy files into sources/ (or drop them here)" onClick={() => void pickSources()}>
          <Icon name="plus" size={14} />
          Import
        </button>
        <button type="button" className="btn" title="Make meeting notes from the transcripts in sources/" onClick={() => nav.go("meeting")}>
          <Icon name="note" size={14} />
          Meeting notes
        </button>
        <span
          title={
            waiting.length
              ? "Ingest the new and changed sources into the wiki, one after another (transcripts get meeting notes instead)"
              : "No new or changed sources to ingest"
          }
        >
          <button type="button" className="btn pri" disabled={!waiting.length} onClick={() => ingest(waiting)}>
            <Icon name="wiki" size={14} />
            Ingest {waiting.length || ""}
          </button>
        </span>
      </TopBar>
      <div className="body sources">
        <FileList
          rows={shown}
          columns={columns}
          layers={["source"]}
          noun={["source", "sources"]}
          defaultSort={["mtime", -1]}
          placeholder="Search sources"
          filters={
            <Seg
              label="Status"
              value={status}
              options={[
                ["all", "All"],
                ["new", "New"],
                ["changed", "Changed"],
                ["ingested", "Ingested"],
              ]}
              onChange={setStatus}
            />
          }
        />
        <RunsPane />
        {dropping && (
          <div className="dropzone" aria-hidden>
            <div>
              <Icon name="source" size={22} />
              <b>Drop to add to Sources</b>
              <span className="faint">Copied into the vault’s sources/ folder, ready to ingest</span>
            </div>
          </div>
        )}
      </div>
    </main>
  );
}

/** A source's row button: its leading action (src/sourceActions.ts: a transcript's meeting note,
 *  a thread's reply, else Ingest), with every action, the same as the Inbox offers, on its ▾. */
function SourceButton({ path, ingested }: { path: string; ingested: boolean }) {
  const [at, setAt] = useState<DOMRect | null>(null);
  const acts = sourceActions(path, ingested);
  if (!acts.length) return null;
  const lead = SOURCE_ACTION[acts[0]];
  return (
    <span className="splitbtn" onClick={(e) => e.stopPropagation()}>
      <button type="button" className="btn sm" title={lead.tip(ingested)} onClick={() => runSourceAction(acts[0], path)}>
        {lead.short(ingested)}
      </button>
      <button
        type="button"
        className="btn sm"
        aria-label="More ways to use this source"
        title="More ways to use this source"
        onClick={(e) => setAt(e.currentTarget.getBoundingClientRect())}
      >
        <Icon name="chevdown" size={12} />
      </button>
      {at && (
        <Popover anchor={at} onClose={() => setAt(null)} width={230}>
          <div className="menu" role="menu">
            {acts.map((a) => (
              <button
                key={a}
                type="button"
                role="menuitem"
                title={SOURCE_ACTION[a].tip(ingested)}
                onClick={() => (setAt(null), runSourceAction(a, path))}
              >
                <Icon name={SOURCE_ACTION[a].icon} size={14} />
                {SOURCE_ACTION[a].label(ingested)}
              </button>
            ))}
            <button
              type="button"
              role="menuitem"
              title="Open Compare with this as document A, to pick the one to compare it with"
              onClick={() => (setAt(null), nav.go({ screen: "compare", path }))}
            >
              <Icon name="shield" size={14} />
              Compare with…
            </button>
          </div>
        </Popover>
      )}
    </span>
  );
}

/** A new template's starting text: a title from the note's name, and the caret after it. */
export const TEMPLATE_STARTER = "# <% tp.file.title %>\n\n";

/** New template: `Templates/<name>.md`, opened in Edit to write. */
function NewTemplate({ onClose }: { onClose: () => void }) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const n = name.trim().replace(/\.md$/i, "");
  const ok = !!n && !/[\\/:*?"<>|]/.test(n);
  const make = () => {
    if (!ok || busy) return;
    setBusy(true);
    const path = `Templates/${n}.md`;
    api
      .docCreate(path, TEMPLATE_STARTER)
      .then(() => {
        setNewNoteCursor(path, TEMPLATE_STARTER.length);
        onClose();
        openDoc(path);
      })
      .catch(reportEditError)
      .finally(() => setBusy(false));
  };
  return (
    <Dialog onClose={onClose} width={440} label="New template">
      <div className="confirm">
        <h2 className="h2">New template</h2>
        <p className="muted small">
          A note in the Templates folder. Templater's tags run when a note is made from it; it starts with the note's title.
        </p>
        <label className="nnf">
          <span className="faint">Name</span>
          <span className="inp">
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && make()}
              placeholder="Workshop"
            />
          </span>
        </label>
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" title="Close without making a template" onClick={onClose}>
            Cancel
          </button>
          <span title={ok ? "Create the template and open it to edit (↩)" : "Type a name the template can be saved under"}>
            <button type="button" className="btn lg pri" disabled={!ok || busy} onClick={make}>
              Create
            </button>
          </span>
        </div>
      </div>
    </Dialog>
  );
}

export function TemplatesScreen() {
  const rows = useFiles(() => api.filesList("template"));
  const [making, setMaking] = useState(false);
  // Which templates don't compile, as New note would find: by reading each one.
  const [bad, setBad] = useState<Record<string, string>>({});
  useEffect(() => {
    let live = true;
    void Promise.all(
      (rows ?? [])
        .filter((r) => /\.md$/i.test(r.path))
        .map((r) =>
          api
            .docRead(r.path)
            .then((d) => [r.path, checkTemplate(d.content)] as const)
            .catch(() => [r.path, null] as const),
        ),
    ).then((pairs) => live && setBad(Object.fromEntries(pairs.filter(([, e]) => e).map(([p, e]) => [p, e!]))));
    return () => {
      live = false;
    };
  }, [rows]);
  const columns: Column<FileSummary>[] = [
    titleCol(),
    {
      key: "runs",
      label: "",
      width: "0 0 110px",
      value: (r) => (bad[r.path] ? "0" : "1"),
      render: (r) =>
        bad[r.path] ? (
          <span className="chip amber sm" title={bad[r.path]}>
            <Icon name="info" size={12} />
            Doesn't run
          </span>
        ) : null,
    },
    { key: "path", label: "File", width: "2", value: (r) => r.path, render: (r) => <span className="faint mono ell">{r.path}</span> },
    modCol(),
    sizeCol(),
  ];
  return (
    <main className="main">
      <TopBar
        title="Templates"
        sub="Open one to edit it; Test run shows the note it makes"
        ask={{ about: "your templates", prompt: "About my templates: " }}
      >
        <button type="button" className="btn pri" title="Make a new template in the Templates folder" onClick={() => setMaking(true)}>
          <Icon name="plus" size={14} />
          New template
        </button>
      </TopBar>
      {making && <NewTemplate onClose={() => setMaking(false)} />}
      <div className="body">
        <FileList
          rows={rows}
          columns={columns}
          layers={["template" as Layer]}
          noun={["template", "templates"]}
          defaultSort={["title", 1]}
          placeholder="Search templates"
        />
      </div>
    </main>
  );
}
