// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useState } from "react";
import { api, Bookmark } from "./api";
import { Icon, Mark } from "./icons";
import { nav, openDoc, place as placeStore, Screen } from "./nav";
import { useVaultOpening, useVaultVersion, vaultStatus } from "./state";
import { settings, useStore } from "./store";
import { projectFlag, useInbox, useProjects } from "./gtd";
import { unclarified } from "./Inbox";
import { ask } from "./askState";
import { docChecks, docChecksUnseen, startDocChecks } from "./docChecks";
import { isActive, runs, startRuns } from "./Ingest";
import { localToday } from "./md/taskQuery";
import { reportEditError, tasksFailed, todayRows, undoLast, useAllTasks, viewRows, VIEWS } from "./taskModel";
import { ago, fmtCount } from "./ui";
import { toast } from "./Toast";
import { changes, decisions, health, held, startKnowledge } from "./knowledge";
import type { IndexStats, Layer, VaultStatus } from "./api";
import { keyLabel } from "./platform";

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
  { screen: "review", label: "Changes", icon: "review" },
  { screen: "ask", label: "Ask", icon: "ask" },
];

export const LIBRARY: Item[] = [
  { screen: "search", label: "Search", icon: "search" },
  { screen: "notes", label: "Notes", icon: "note", count: (s) => s.notes },
  { screen: "wiki", label: "Wiki", icon: "wiki", count: (s) => s.wiki },
  { screen: "sources", label: "Sources", icon: "source", count: (s) => s.sources },
  { screen: "templates", label: "Templates", icon: "template", count: (s) => s.templates },
  { screen: "graph", label: "Graph", icon: "graph" },
];

/** The screens that do one job with the AI: the sidebar's Tools section. ⌘K lists them under Go to
 *  with the other screens, found by what they do too (Palette's TOOL_WORDS); they aren't in ALL_ITEMS. */
export const TOOLS: Item[] = [
  { screen: "reply", label: "Draft reply", icon: "send", tip: "draft a reply to an email or Teams thread, captured or pasted" },
  { screen: "meeting", label: "Meeting notes", icon: "note", tip: "make meeting and 1-1 notes from Teams transcripts" },
  {
    screen: "doccheck",
    label: "Compare",
    icon: "shield",
    tip: "A document against the canonical, or any two versions of a document",
  },
];

export const FOOTER: Item[] = [
  { screen: "health", label: "Knowledge health", icon: "health" },
  { screen: "activity", label: "Activity", icon: "activity" },
  { screen: "trash", label: "Trash", icon: "trash" },
  { screen: "settings", label: "Settings", icon: "settings" },
];

export const ALL_ITEMS = [...DAILY, ...LIBRARY, ...FOOTER];

/** A vault path's layer, by its top folder (as the walker decides it). */
export function layerOf(path: string): Layer {
  const top = path.split("/")[0];
  if (top === "wiki") return "wiki";
  if (top === "sources") return "source";
  if (top === "Templates" || top === "templates") return "template";
  return "note";
}

/** ⌥⌘1 to ⌥⌘9, then ⌥⌘0, go to the screens used most (⌘1 to ⌘3 are a note's View, Edit and Source). */
export const HOTKEYS: Screen[] = ["today", "inbox", "tasks", "projects", "review", "search", "ask", "notes", "wiki", "sources"];

/** A screen's hotkey: ⌥⌘1 … ⌥⌘9, and ⌥⌘0 for the tenth. */
export const hotkey = (s: Screen): string | null => {
  const k = HOTKEYS.indexOf(s);
  return k < 0 ? null : `⌥⌘${(k + 1) % 10}`;
};

/** A count beside an item; `err` when it couldn't be counted, shown as a warning rather than 0. */
type Extra = { n: number; hot?: boolean; err?: string; bad?: boolean; busy?: string };

function NavItem({ item, on, stats, extra }: { item: Item; on: boolean; stats: IndexStats; extra?: Extra }) {
  const n = extra?.n ?? item.count?.(stats) ?? 0;
  const go = `Go to ${item.label}${hotkey(item.screen) ? ` (${keyLabel(hotkey(item.screen)!)})` : ""}`;
  return (
    <button
      type="button"
      className={`navi ${on ? "on" : ""}`}
      aria-current={on ? "page" : undefined}
      title={extra?.err ? `${go}. ${extra.err}` : extra?.busy ? `${go}. ${extra.busy}` : item.tip ? `${go}: ${item.tip}` : go}
      onClick={() => nav.go(item.screen)}
    >
      <Icon name={item.icon} />
      <span className="lbl">{item.label}</span>
      {extra?.err ? (
        <span className="n err" aria-label={extra.err}>
          <Icon name="alert" size={12} />
        </span>
      ) : extra?.busy && !n ? (
        <span className="n busy" aria-label={extra.busy}>
          <span className="spin" />
        </span>
      ) : (
        n > 0 && <span className={`n ${extra?.hot ? "hot" : ""} ${extra?.bad ? "bad" : ""}`}>{fmtCount(n)}</span>
      )}
    </button>
  );
}

/** A section heading that folds its items away; remembered in settings. */
function Section({ id, title, children }: { id: string; title: string; children: React.ReactNode }) {
  const s = useStore(settings);
  const folded = !!s.folded?.[id];
  return (
    <>
      <button
        type="button"
        className="sect"
        aria-expanded={!folded}
        title={folded ? `Show the ${title.toLowerCase()}` : `Fold the ${title.toLowerCase()} away`}
        onClick={() => settings.update({ folded: { ...s.folded, [id]: !folded } })}
      >
        {title}
        <Icon name="chevdown" size={12} style={{ transform: folded ? "rotate(-90deg)" : undefined, transition: "transform .15s" }} />
      </button>
      {!folded && children}
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
export function ReadOnlyPill({ onOpen }: { onOpen?: () => void }) {
  return (
    <button
      type="button"
      className="ropill"
      title="The vault is read-only: Brainstead won't change any file. Click to open Settings › Vault, where it's turned off."
      onClick={onOpen ?? (() => nav.go({ screen: "settings", pane: "vault" }))}
    >
      Read-only
    </button>
  );
}

/** The list a document belongs to, lit in the sidebar while it's open. */
const LAYER_SCREEN: Record<Layer, Screen> = { note: "notes", wiki: "wiki", source: "sources", template: "templates" };

/** The index line at the bottom: green when current, amber while indexing, red when it failed. */
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

export function Sidebar({ onSearch }: { onSearch: () => void }) {
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
  // Compare's two tabs are one entry.
  const lit = docLayer ? LAYER_SCREEN[docLayer] : place.screen === "compare" ? "doccheck" : place.screen;
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
  if (inbox) extra.inbox = { n: inbox.filter(unclarified).length, hot: inbox.some(unclarified) };
  const projects = useProjects();
  if (projects)
    extra.projects = { n: projects.filter((p) => p.status === "active").length, hot: projects.some((p) => projectFlag(p) === "none") };
  useEffect(startKnowledge, []);
  // Only held changes are counted: the rest of the feed badges nothing.
  const all = useStore(changes);
  const h = useStore(health);
  const waiting = held(all ?? []).length;
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
  const need = decisions(h?.report);
  if (need) extra.health = { n: need };
  const readOnly = !!useStore(settings).readOnly;
  const item = (i: Item) => <NavItem key={i.screen} item={i} on={lit === i.screen} stats={st.stats} extra={extra[i.screen]} />;
  return (
    <aside className="side" aria-label="Sidebar">
      <div className="drag tlspace" data-tauri-drag-region />
      <div className="brand drag" data-tauri-drag-region>
        <Mark />
        <b>Brainstead</b>
      </div>
      <button
        type="button"
        className="search-pill"
        title={keyLabel("Find a note, go to a screen or run a command (⌘K)")}
        onClick={onSearch}
      >
        <Icon name="search" size={14} />
        Search or jump to…
        <span className="kbd">{keyLabel("⌘K")}</span>
      </button>
      <nav className="sidescroll">
        {DAILY.map(item)}
        <Section id="library" title="Library">
          {LIBRARY.map(item)}
        </Section>
        <Section id="tools" title="Tools">
          {TOOLS.map(item)}
        </Section>
        <Section id="bookmarks" title="Bookmarks">
          <Bookmarks current={place.screen === "doc" ? place.path : undefined} />
        </Section>
      </nav>
      <div className="foot">
        {FOOTER.map(item)}
        <div className="statusrow">
          <div className={`status ${line.tone}`} title={line.tip} role="status">
            <i />
            {line.text}
          </div>
          {readOnly && <ReadOnlyPill />}
        </div>
      </div>
    </aside>
  );
}
