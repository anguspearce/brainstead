// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useState } from "react";
import { Popover } from "./ui";
import { toggleRail } from "./sidebarRail";
import { api, Bookmark } from "./api";
import { Icon, Mark } from "./icons";
import { nav, openDoc, place as placeStore, Screen } from "./nav";
import { useVaultOpening, useVaultVersion, vaultStatus } from "./state";
import { settings, useStore } from "./store";
import { projectFlag, useInbox, useProjects } from "./gtd";
import { heldCount, toClarify, toDecide } from "./needsYou";
import { ask } from "./askState";
import { docChecks, docChecksUnseen, startDocChecks } from "./docChecks";
import { isActive, runs, startRuns } from "./Ingest";
import { localToday } from "./md/taskQuery";
import { reportEditError, tasksFailed, todayRows, undoLast, useAllTasks, viewRows, VIEWS } from "./taskModel";
import { ago, fmtCount } from "./ui";
import { toast } from "./Toast";
import { changes, health, startKnowledge } from "./knowledge";
import type { IndexStats, Layer, VaultStatus } from "./api";
import { dragRegion, keyLabel } from "./platform";

interface Item {
  screen: Screen;
  label: string;
  icon: string;
  count?: (s: IndexStats) => number;
  /** Shown as a filled badge: something needs doing. */
  hot?: boolean;
  /** What it's for, after "Go to …" in its tooltip. */
  tip?: string;
}

export const DAILY: Item[] = [
  { screen: "today", label: "Today", icon: "today" },
  { screen: "inbox", label: "Inbox", icon: "inbox" },
  { screen: "tasks", label: "Tasks", icon: "tasks" },
  { screen: "projects", label: "Projects", icon: "project" },
  { screen: "weekly", label: "Weekly review", icon: "calendar" },
];

export const LIBRARY: Item[] = [
  { screen: "search", label: "Search", icon: "search" },
  { screen: "notes", label: "Notes", icon: "note", count: (s) => s.notes },
  { screen: "wiki", label: "Wiki", icon: "wiki", count: (s) => s.wiki },
  { screen: "sources", label: "Sources", icon: "source", count: (s) => s.sources },
];

/** The screens that do one job with the AI, under Ask in the sidebar's Assistant section. ⌘K lists
 *  them under Go to with the other screens, found by what they do too (Palette's TOOL_WORDS). */
export const TOOLS: Item[] = [
  { screen: "reply", label: "Draft reply", icon: "send", tip: "draft a reply to an email or Teams thread, captured or pasted" },
  { screen: "meeting", label: "Meeting notes", icon: "note", tip: "make meeting and 1-1 notes from Teams transcripts" },
  {
    screen: "doccheck",
    label: "Compare",
    icon: "shield",
    tip: "a document against the canonical, or any two versions of a document",
  },
];

/** The Assistant section: Ask, then the tools (D-20261119-05). */
export const ASSISTANT: Item[] = [{ screen: "ask", label: "Ask", icon: "ask" }, ...TOOLS];

/** The Vault fold at the foot: the vault's upkeep, folded away until wanted (D-20261119-05). */
export const VAULT: Item[] = [
  { screen: "health", label: "Wiki health", icon: "health" },
  { screen: "activity", label: "Activity", icon: "activity" },
  { screen: "trash", label: "Trash", icon: "trash" },
  { screen: "templates", label: "Templates", icon: "template", count: (s) => s.templates },
  { screen: "graph", label: "Graph", icon: "graph" },
];

export const FOOTER: Item[] = [
  { screen: "review", label: "Changes", icon: "review" },
  { screen: "settings", label: "Settings", icon: "settings" },
];

export const ALL_ITEMS = [...DAILY, ...LIBRARY, ...ASSISTANT, ...VAULT, ...FOOTER];

/** A vault path's layer, by its top folder (as the walker decides it). */
export function layerOf(path: string): Layer {
  const top = path.split("/")[0];
  if (top === "wiki") return "wiki";
  if (top === "sources") return "source";
  if (top === "Templates" || top === "templates") return "template";
  return "note";
}

/** ⌘1 to ⌘9, then ⌘0: the ten screens used most. Today to Weekly review, then Ask, then Search to
 *  Sources: the keys they had before Ask moved down to the Assistant section, so hands that know them
 *  keep them (a note's View, Edit and Source are ⌥⌘1 to ⌥⌘3). */
export const HOTKEYS: Screen[] = ["today", "inbox", "tasks", "projects", "weekly", "ask", "search", "notes", "wiki", "sources"];

/** A screen's hotkey: ⌘1 … ⌘9, and ⌘0 for the tenth. */
export const hotkey = (s: Screen): string | null => {
  const k = HOTKEYS.indexOf(s);
  return k < 0 ? null : `⌘${(k + 1) % 10}`;
};

/** A count beside an item; `err` when it couldn't be counted, shown as a warning rather than 0. */
type Extra = { n: number; hot?: boolean; err?: string; bad?: boolean; busy?: string };

function NavItem({ item, on, stats, extra, rail }: { item: Item; on: boolean; stats: IndexStats; extra?: Extra; rail?: boolean }) {
  const n = extra?.n ?? item.count?.(stats) ?? 0;
  // On the rail the name is only in the tooltip, so its count is there too.
  const go = `Go to ${item.label}${hotkey(item.screen) ? ` (${keyLabel(hotkey(item.screen)!)})` : ""}${rail && n > 0 ? ` · ${fmtCount(n)}` : ""}`;
  // The rail badges what's going on (the counts worked out for it), not the index's totals of
  // notes, wiki pages and sources, which stay in the tooltip.
  const badge = rail && extra?.n === undefined ? 0 : n;
  return (
    <button
      type="button"
      className={`navi ${on ? "on" : ""}`}
      aria-current={on ? "page" : undefined}
      title={extra?.err ? `${go}. ${extra.err}` : extra?.busy ? `${go}. ${extra.busy}` : item.tip ? `${go}: ${item.tip}` : go}
      aria-label={rail ? item.label : undefined}
      onClick={() => nav.go(item.screen)}
    >
      <Icon name={item.icon} />
      {!rail && <span className="lbl">{item.label}</span>}
      {extra?.err ? (
        <span className="n err" aria-label={extra.err}>
          <Icon name="alert" size={12} />
        </span>
      ) : extra?.busy && !n ? (
        <span className="n busy" aria-label={extra.busy}>
          <span className="spin" />
        </span>
      ) : (
        badge > 0 && <span className={`n ${extra?.hot ? "hot" : ""} ${extra?.bad ? "bad" : ""}`}>{fmtCount(badge)}</span>
      )}
    </button>
  );
}

/** A section heading that folds its items away; remembered in settings. On the rail it's a thin
 *  rule between the groups and its items always show: there's no heading to unfold them with. */
function Section({
  id,
  title,
  what,
  rail,
  children,
}: {
  id: string;
  title: string;
  what?: string;
  rail?: boolean;
  children: React.ReactNode;
}) {
  const s = useStore(settings);
  const folded = !!s.folded?.[id];
  what ??= `the ${title.toLowerCase()}`;
  if (rail)
    return (
      <>
        <div className="railsep" role="separator" aria-label={title} />
        {children}
      </>
    );
  return (
    <>
      <button
        type="button"
        className="sect"
        aria-expanded={!folded}
        title={folded ? `Show ${what}` : `Fold ${what} away`}
        onClick={() => settings.update({ folded: { ...s.folded, [id]: !folded } })}
      >
        {title}
        <Icon name="chevdown" size={12} className={`foldchev ${folded ? "shut" : ""}`} />
      </button>
      {!folded && children}
    </>
  );
}

/** Adds up the badges of the rows a closed fold hides: their count, whether any wants attention, a
 *  warning or a spinner, so nothing that needs you is folded out of sight. */
export function summed(xs: (Extra | undefined)[]): Extra | undefined {
  const mine = xs.filter((x): x is Extra => !!x);
  if (!mine.length) return undefined;
  return {
    n: mine.reduce((a, x) => a + x.n, 0),
    hot: mine.some((x) => x.hot),
    bad: mine.some((x) => x.bad),
    err: mine.find((x) => x.err)?.err,
    busy: mine.find((x) => x.busy)?.busy,
  };
}

/** The foot's Vault fold: a row like the others that opens to Wiki health, Activity, Trash, Templates
 *  and Graph. Closed (as it starts), it carries their badges and is lit while one of them is open. */
function VaultFold({
  lit,
  extra,
  rail,
  children,
}: {
  lit: Screen;
  extra: Partial<Record<Screen, Extra>>;
  rail?: boolean;
  children: React.ReactNode;
}) {
  const s = useStore(settings);
  const open = s.folded?.vault === false;
  const on = !open && VAULT.some((v) => v.screen === lit);
  const x = open ? undefined : summed(VAULT.map((v) => extra[v.screen]));
  const what = "Wiki health, Activity, Trash, Templates and Graph";
  return (
    <>
      <button
        type="button"
        className={`navi vaultfold ${on ? "on" : ""}`}
        aria-expanded={open}
        aria-label={rail ? "Vault" : undefined}
        title={
          open
            ? `Fold ${what} away`
            : `${rail ? "Vault: show " : "Show "}${what}${rail && x?.n ? ` · ${fmtCount(x.n)}` : ""}${x?.err ? `. ${x.err}` : x?.busy ? `. ${x.busy}` : ""}`
        }
        onClick={() => settings.update({ folded: { ...s.folded, vault: open } })}
      >
        <Icon name="folder" />
        {!rail && <span className="lbl">Vault</span>}
        {x?.err ? (
          <span className="n err" aria-label={x.err}>
            <Icon name="alert" size={12} />
          </span>
        ) : x?.busy && !x.n ? (
          <span className="n busy" aria-label={x.busy}>
            <span className="spin" />
          </span>
        ) : (
          !!x?.n && <span className={`n ${x.hot ? "hot" : ""} ${x.bad ? "bad" : ""}`}>{fmtCount(x.n)}</span>
        )}
        <Icon name="chevdown" size={12} className={`navchev foldchev ${open ? "" : "shut"}`} />
      </button>
      {open && <div className="foldkids">{children}</div>}
    </>
  );
}

/** The rail's Bookmarks: one pin that opens the list beside it, as the full sidebar shows it. */
function RailBookmarks({ current }: { current?: string }) {
  const [at, setAt] = useState<DOMRect | null>(null);
  return (
    <>
      <button
        type="button"
        className={`navi ${at ? "open" : ""}`}
        aria-label="Bookmarks"
        aria-expanded={!!at}
        title="Bookmarks: the notes in Me. Bookmarks.md"
        onClick={(e) => setAt(at ? null : e.currentTarget.getBoundingClientRect())}
      >
        <Icon name="pin" />
      </button>
      {at && (
        <Popover anchor={at} onClose={() => setAt(null)} width={260} place="right">
          {/* A bookmark opened closes the list; its × leaves it open for the next. */}
          <div className="railbm" onClick={(e) => (e.target as Element).closest?.(".navi.bm") && setAt(null)}>
            <div className="eyebrow">Bookmarks</div>
            <Bookmarks current={current} />
          </div>
        </Popover>
      )}
    </>
  );
}

/** Me. Bookmarks.md, in its order. A bookmark whose note has gone is shown greyed. */
function Bookmarks({ current }: { current?: string }) {
  const [list, setList] = useState<Bookmark[] | null>(null);
  const v = useVaultVersion();
  useEffect(() => {
    let live = true;
    api
      .bookmarks()
      .then((b) => live && setList(b))
      .catch(() => live && setList([]));
    return () => {
      live = false;
    };
  }, [v]);
  if (list && !list.length) return <div className="sideempty">None yet: add wikilinks to Me. Bookmarks.md.</div>;
  return (
    <>
      {(list ?? []).map((b) => (
        <div key={b.target} className="bmrow">
          <button
            type="button"
            className={`navi bm ${b.path && b.path === current ? "on" : ""} ${b.path ? "" : "gone"}`}
            title={b.path ? b.path : "No matching note (renamed or deleted?)"}
            disabled={!b.path}
            onClick={() => b.path && openDoc(b.path)}
          >
            <Icon name="pin" />
            <span className="lbl">{b.title}</span>
          </button>
          {/* On hover: takes the bookmark away (the note stays), with Undo. */}
          <button
            type="button"
            className="ibtn xs bmx"
            aria-label={`Remove the bookmark for ${b.title}`}
            title={keyLabel(`Remove the bookmark for ${b.title}; the note stays (⌘Z puts it back)`)}
            onClick={() =>
              void api
                .bookmarkRemove(b.target)
                .then(() => toast("Bookmark removed", { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok"))
                .catch(reportEditError)
            }
          >
            <Icon name="x" size={11} />
          </button>
        </div>
      ))}
    </>
  );
}

/** Shown while the vault is read-only; opens Settings › Vault, where it's turned off (the tray
 *  window passes its own way there). It doesn't turn read-only off itself. */
export function ReadOnlyPill({ onOpen, rail }: { onOpen?: () => void; rail?: boolean }) {
  return (
    <button
      type="button"
      className={`ropill${rail ? " rail" : ""}`}
      aria-label={rail ? "Read-only" : undefined}
      title="The vault is read-only: Brainstead won't change any file. Click to open Settings › Vault, where it's turned off."
      onClick={onOpen ?? (() => nav.go({ screen: "settings", pane: "vault" }))}
    >
      {rail ? <Icon name="lock" size={12} /> : "Read-only"}
    </button>
  );
}

/** The list a document belongs to, lit in the sidebar while it's open. */
const LAYER_SCREEN: Record<Layer, Screen> = { note: "notes", wiki: "wiki", source: "sources", template: "templates" };

/** Screens with no row of their own, lit as the row they belong to. */
const PART_OF: Partial<Record<Screen, Screen>> = { compare: "doccheck", triage: "inbox", contradictions: "health" };

/** The index line at the bottom: no dot when current, amber while indexing, red when it failed. */
export function statusLine(st: VaultStatus, now: number): { tone: "ok" | "busy" | "bad" | "none"; text: string; tip?: string } {
  switch (st.state) {
    case "indexing":
      return { tone: "busy", text: "Indexing the vault…" };
    case "error":
      return { tone: "bad", text: "Index failed", tip: st.error ?? undefined };
    case "none":
      return { tone: "none", text: "No vault chosen" };
    case "ready":
      return {
        tone: "ok",
        text: `${fmtCount(st.stats.files)} files · ${st.stats.updatedAt ? ago(st.stats.updatedAt, now) : "indexed"}`,
        tip: st.vaultPath ?? undefined,
      };
  }
}

/** The sidebar's own toggle: an icon rail or in full, for the width the window is at (⌘\). */
function RailToggle({ rail }: { rail: boolean }) {
  return (
    <button
      type="button"
      className="ibtn railtoggle"
      aria-label={rail ? "Show the sidebar in full" : "Collapse the sidebar to icons"}
      title={keyLabel(rail ? "Show the sidebar in full, with its names (⌘\\)" : "Collapse the sidebar to icons (⌘\\)")}
      onClick={toggleRail}
    >
      <Icon name="sidebar" size={15} />
    </button>
  );
}

export function Sidebar({ onSearch, rail = false }: { onSearch: () => void; rail?: boolean }) {
  const { place } = useStore(placeStore);
  const st = useStore(vaultStatus);
  // Re-render each half minute so "2 min ago" stays true.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(t);
  }, []);
  const line = statusLine(st, now);
  const docLayer = place.screen === "doc" && place.path ? layerOf(place.path) : null;
  // Compare's two tabs are one entry; Triage is the Inbox's, Contradictions a tab of Wiki health.
  const lit = docLayer ? LAYER_SCREEN[docLayer] : (PART_OF[place.screen] ?? place.screen);
  // Today: overdue and due today (a badge when any is overdue); Tasks: next actions.
  const tasks = useAllTasks();
  const inbox = useInbox();
  const today = localToday();
  // While the vault is still opening, a failed read is only "not ready yet": no error, no count.
  const opening = useVaultOpening();
  const failed = useStore(tasksFailed) && !opening;
  const extra: Partial<Record<Screen, Extra>> = {};
  if (failed) {
    const err = "Couldn't load the tasks, so there's no count";
    extra.today = { n: 0, err };
    extra.tasks = { n: 0, err };
  } else if (tasks) {
    const t = todayRows(tasks, today);
    extra.today = { n: t.overdue.length + t.due.length, hot: t.overdue.length > 0 };
    extra.tasks = { n: viewRows(tasks, VIEWS[0], today, inbox).length };
  }
  // The counts Today's To process and the menu-bar window show too (src/needsYou.ts).
  if (inbox) extra.inbox = { n: toClarify(inbox), hot: toClarify(inbox) > 0 };
  const projects = useProjects();
  if (projects)
    extra.projects = { n: projects.filter((p) => p.status === "active").length, hot: projects.some((p) => projectFlag(p) === "none") };
  useEffect(startKnowledge, []);
  // Only held changes are counted: the rest of the feed badges nothing.
  const all = useStore(changes);
  const h = useStore(health);
  const waiting = heldCount(all ?? []);
  if (waiting) extra.review = { n: waiting, hot: true };
  // Ask: open chats with an answer you haven't seen.
  const chats = useStore(ask).chats;
  const unread = chats.filter((c) => c.unread).length;
  const answering = chats.filter((c) => c.busy).length;
  if (unread || answering)
    extra.ask = {
      n: unread,
      hot: true,
      busy: answering ? `${answering === 1 ? "A chat is" : `${answering} chats are`} being answered` : undefined,
    };
  // Tools at work: a check or meeting notes going (a spinner), checks that ended unseen (a badge).
  useEffect(startDocChecks, []);
  useEffect(startRuns, []);
  const checks = useStore(docChecks);
  const checking = checks.filter((c) => c.status === "running");
  const unseenChecks = useStore(docChecksUnseen);
  if (checking.length || unseenChecks.n)
    extra.doccheck = {
      n: unseenChecks.n,
      hot: true,
      bad: unseenChecks.bad,
      busy: checking.length
        ? `${["differences", "changes"].includes(checking[0].mode) ? "Comparing" : "Checking"} ${checking[0].candidate.split("/").pop()}${checking.length > 1 ? ` and ${checking.length - 1} more` : ""}…`
        : undefined,
    };
  const writing = (useStore(runs) ?? []).filter((r) => r.kind === "meeting" && isActive(r)).length;
  if (writing) extra.meeting = { n: 0, busy: `${writing === 1 ? "A meeting note is" : `${writing} meeting notes are`} being made` };
  const need = toDecide(h?.report);
  if (need) extra.health = { n: need };
  const readOnly = !!useStore(settings).readOnly;
  const item = (i: Item) => <NavItem key={i.screen} item={i} on={lit === i.screen} stats={st.stats} extra={extra[i.screen]} rail={rail} />;
  const current = place.screen === "doc" ? place.path : undefined;
  return (
    <aside className={`side${rail ? " rail" : ""}`} aria-label="Sidebar">
      <div className="drag tlspace" {...dragRegion()} />
      <div className="brand drag" {...dragRegion()}>
        {!rail && (
          <>
            <Mark />
            <b>Brainstead</b>
          </>
        )}
        <RailToggle rail={rail} />
      </div>
      <button
        type="button"
        className="search-pill"
        aria-label={rail ? "Search or jump to…" : undefined}
        title={keyLabel("Find a note, go to a screen or run a command (⌘K)")}
        onClick={onSearch}
      >
        <Icon name="search" size={14} />
        {!rail && (
          <>
            Search or jump to…
            <span className="kbd">{keyLabel("⌘K")}</span>
          </>
        )}
      </button>
      <nav className="sidescroll">
        {DAILY.map(item)}
        <Section id="library" title="Library" rail={rail}>
          {LIBRARY.map(item)}
        </Section>
        <Section id="assistant" title="Assistant" what="Ask and the assistant's tools" rail={rail}>
          {ASSISTANT.map(item)}
        </Section>
        <Section id="bookmarks" title="Bookmarks" rail={rail}>
          {rail ? <RailBookmarks current={current} /> : <Bookmarks current={current} />}
        </Section>
      </nav>
      <div className="foot">
        <VaultFold lit={lit} extra={extra} rail={rail}>
          {VAULT.map(item)}
        </VaultFold>
        {FOOTER.map(item)}
        <div className="statusrow">
          <div className={`status ${line.tone}`} title={rail ? [line.text, line.tip].filter(Boolean).join(" · ") : line.tip} role="status">
            <i />
            {rail ? <span className="vh">{line.text}</span> : line.text}
          </div>
          {readOnly && <ReadOnlyPill rail={rail} />}
        </div>
      </div>
    </aside>
  );
}
