// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Inbox (stage 6): GTD's clarify step over what you've captured, the Scratchpad's thoughts and
// the To Do list's `#### Other` tasks. For each, choose what it becomes; Brainstead writes that
// and the item leaves the Inbox (a thought's block is removed; a task leaves once it has a
// project, a context or a waiting-for or someday tag, is done, or is moved). ⌘Z undoes. A
// suggestion from the model is only shown until you accept it.

import { ask as askChats, askedFromInbox } from "./askState";
import { nav, openDoc, useViewState } from "./nav";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, ClarifySuggestion, InboxItem, ProjectRow } from "./api";
import { prepareCapture } from "./Capture";
import { captureDone, contextName, fmtEffort, effortMinutes, projectLink, projectName, unclarified, useInbox, useProjects } from "./gtd";
import { Icon } from "./icons";
import { fmtShortDate, resolveDateWord } from "./md/dates";
import { localToday } from "./md/taskQuery";
import { ingest, meetingProgress, runs, startRuns } from "./Ingest";
import { changes, startKnowledge } from "./knowledge";
import { NewProject } from "./Projects";
import { askAboutNote } from "./Ask";
import { meetingTip, SOURCE_ACTION } from "./sourceActions";
import { useSearch } from "./Search";
import { reportEditError, undoLast } from "./taskModel";
import { toast } from "./Toast";
import { askQuote, TopBar } from "./TopBar";
import { reviewWeek } from "./Weekly";
import { Dialog, TableBand } from "./ui";
import { Markdown } from "./md/Markdown";
import { settings, useStore } from "./store";
import { useVaultVersion } from "./state";
import { followThrough, isTranscriptPath, makeMeetingNotes } from "./meetingFlow";
import { TaskText } from "./TaskList";
import {
  appendAsReference,
  doneNow,
  eachItem,
  eachSaid,
  inboxChoices,
  inboxItemKind,
  leadChoice,
  newReferenceNote,
  settleInboxItem,
  suggestKind,
  trashCapture,
} from "./inboxActions";
import { keyLabel } from "./platform";

export { unclarified } from "./gtd";

export interface Clarified {
  text: string;
  project?: string | null;
  context?: string | null;
  effort?: string | null;
  due?: string | null;
  tag?: "waiting-for" | "someday-maybe" | null;
}

/** The task line an outcome writes: text, then project link, context, effort, tag and due date. */
export function taskLine(c: Clarified): string {
  const bits = [c.text.trim()];
  if (c.project) bits.push(projectLink(projectName(c.project)));
  if (c.context) bits.push(`#context/${contextName(c.context)}`);
  if (c.tag) bits.push(`#${c.tag}`);
  if (c.effort && effortMinutes(c.effort) !== null) bits.push(`[effort:: ${c.effort}]`);
  if (c.due) bits.push(`📅 ${c.due}`);
  return `- [ ] ${prepareCapture("task", bits.join(" "))}`;
}

const itemText = (i: InboxItem) => (i.kind === "task" ? i.text : i.text).trim();
/** The list's bands, in the order shown (and J / K follow): where each item came from. */
const SOURCES: { kind: InboxItem["kind"]; label: string; icon: string; tip: string }[] = [
  { kind: "capture", label: "Captures", icon: "source", tip: "captured from Outlook or Teams, or added to Sources and not yet ingested" },
  { kind: "thought", label: "Scratchpad", icon: "note", tip: "on the Scratchpad" },
  { kind: "task", label: "To Do list › Other", icon: "tasks", tip: "under Other on the To Do list" },
];
/** The items to clarify in the order the list shows them, band by band (Captures, Scratchpad, To Do
 *  list › Other), so Suggest for all asks about the same first 20 (an assistant's list_inbox too). */
export function inboxRows(raw: InboxItem[]): InboxItem[] {
  const open = raw.filter(unclarified);
  return SOURCES.flatMap((g) => open.filter((i) => i.kind === g.kind));
}

/** The band an item is listed under. */
export const bandOf = (i: InboxItem) => SOURCES.find((g) => g.kind === i.kind)!.label;

/** What a row says under its text; its band already says where it came from. */
const rowNote = (i: InboxItem) => (i.kind === "capture" ? [i.lineText, i.stamp].filter(Boolean).join(" · ") : (i.stamp ?? ""));
const sourceOf = (i: InboxItem) =>
  i.kind === "thought" ? `Scratchpad · ${i.stamp ?? ""}` : i.kind === "capture" ? `${i.lineText} · ${i.stamp ?? ""}` : "To Do list · Other";
/** A Teams meeting transcript, which becomes a meeting note: Suggest needn't ask about it. */
const isTranscript = (i: InboxItem) => i.kind === "capture" && isTranscriptPath(i.path);

/** Each choice's key, label and tip for one item (what `inboxChoices` gives it, in that order). */
const CHOICE: Record<string, { key: string; label: (i: InboxItem) => string; tip: (i: InboxItem) => string }> = {
  // A capture's source actions, named as Sources names them (src/sourceActions.ts).
  meeting: { key: SOURCE_ACTION.meeting.key, label: () => SOURCE_ACTION.meeting.label(false), tip: () => SOURCE_ACTION.meeting.tip(false) },
  reply: { key: SOURCE_ACTION.reply.key, label: () => SOURCE_ACTION.reply.label(false), tip: () => SOURCE_ACTION.reply.tip(false) },
  ingest: {
    key: SOURCE_ACTION.ingest.key,
    label: () => SOURCE_ACTION.ingest.label(false),
    tip: () => `${SOURCE_ACTION.ingest.tip(false)}, and take it out of the Inbox`,
  },
  ask: {
    key: SOURCE_ACTION.ask.key,
    label: () => SOURCE_ACTION.ask.label(false),
    tip: () => `${SOURCE_ACTION.ask.tip(false)}; it stays in the Inbox`,
  },
  next: { key: "N", label: () => "Next action", tip: () => "Make it a next action; you set its project, context and due date first" },
  project: { key: "P", label: () => "New project", tip: () => "Start a new project from it" },
  waiting: { key: "W", label: () => "Waiting for", tip: () => "Make it a task tagged waiting-for, to chase later" },
  done: { key: "2", label: () => "Done now (under 2 min)", tip: () => "It takes two minutes: do it now, and this marks it done" },
  someday: { key: "S", label: () => "Someday / maybe", tip: () => "Park it as a task tagged someday-maybe" },
  reference: {
    key: "R",
    label: (i) => (i.kind === "capture" ? "Keep in Sources" : "File as reference"),
    tip: (i) =>
      i.kind === "capture"
        ? "Keep the capture in Sources and take it out of the Inbox"
        : "Add it as a bullet to the end of a note you pick",
  },
  delete: {
    key: "⌫",
    label: (i) => (i.kind === "capture" ? "Move to the Trash" : "Delete"),
    tip: (i) => (i.kind === "capture" ? "Move the captured file to the vault’s Trash" : keyLabel("Delete it from the Inbox; ⌘Z undoes")),
  },
};

/** The choices that can be made for several ticked items at once, in order. */
const BULK = ["meeting", "ingest", "done", "someday", "reference", "delete"];
const BULK_LABEL: Record<string, (n: number, caps: boolean) => [string, string, string]> = {
  meeting: (n) => ["M", followThrough().ingest ? `Make and ingest ${n} meeting notes` : `Make ${n} meeting notes`, meetingTip(n)],
  ingest: (n) => ["I", `Ingest ${n} into the wiki`, "Have the AI read each into the wiki and take it out of the Inbox"],
  done: (n) => ["2", `Mark ${n} done`, "Each took two minutes: done, and out of the Inbox"],
  someday: (n) => ["S", `Move ${n} to Someday / maybe`, "Park each as a task tagged someday-maybe"],
  reference: (n) => ["R", `Keep ${n} in Sources`, "Keep the captures in Sources and take them out of the Inbox"],
  delete: (n, caps) => [
    "⌫",
    caps ? `Move ${n} to the Trash` : `Delete ${n}`,
    caps
      ? "Move the captured files to the vault’s Trash"
      : keyLabel("Delete them from the Inbox (captures go to the Trash); ⌘Z undoes one at a time"),
  ],
};
/** Choices for several ticked items at once, as [key, choice, label, tip]: those every one of them
 *  has (inboxChoices) and that need no form each (Next action, Waiting for and New project are one
 *  at a time); Keep in Sources only when all are captures (filing a thought needs a note picked). */
export function bulkChoices(ps: InboxItem[]) {
  const caps = ps.every((i) => i.kind === "capture");
  const common = BULK.filter((b) => ps.every((i) => inboxChoices(i).includes(b)) && (b !== "reference" || caps));
  return common.map((b) => {
    const [k, label, tip] = BULK_LABEL[b](ps.length, caps);
    return [k, b, label, tip] as const;
  });
}
/** What a choice made for several items says, once done. */
const DONE_ALL: Record<string, string> = {
  ingest: "Ingesting: their changes are listed in Changes",
  done: "Done, and out of the Inbox",
  someday: "Moved to Someday / maybe",
  reference: "Kept in Sources",
  delete: "Deleted",
};
/** What a capture becomes as a task, by default: a follow-up linking the file. */
const startText = (i: InboxItem) => (i.kind === "capture" ? `Follow up on [[${i.text}]]` : itemText(i));

export function InboxScreen() {
  // Drawn again when settings change: Make a meeting note's label follows the Meeting notes switches.
  useStore(settings);
  const raw = useInbox();
  const projects = useProjects();
  const items = useMemo(() => inboxRows(raw ?? []), [raw]);
  const [sel, setSel] = useViewState<string | null>("inbox:sel", null);
  const [form, setForm] = useState<{ mode: "next" | "waiting"; c: Clarified } | null>(null);
  const [filing, setFiling] = useState(false);
  const [making, setMaking] = useState<string | null>(null);
  const [sugg, setSugg] = useState<Record<string, ClarifySuggestion>>({});
  const [thinking, setThinking] = useState(false);
  const [done, setDone] = useState(0);
  // A capture by its file: two captures can share a line and the start of their text (two Teams
  // transcripts did), and then both rows were chosen at once.
  const key = (i: InboxItem) => (i.kind === "capture" ? `capture:${i.path}` : `${i.kind}:${i.line}:${i.lineText.slice(0, 40)}`);
  // Ticked to act on together; clicking a row chooses it alone.
  const [checked, setChecked] = useState<string[]>([]);
  const toggle = (k: string) =>
    setChecked((c) => {
      const next = c.includes(k) ? c.filter((x) => x !== k) : [...c, k];
      if (next.length === 1) setSel(next[0]);
      return next;
    });
  // What the weekly review's preparation suggested for these items, until Suggest asks again;
  // read again when a preparation finishes.
  const [prepped, setPrepped] = useState(0);
  useEffect(() => {
    const off = api.onWeekprepChanged(() => setPrepped((n) => n + 1));
    return () => void off.then((f) => f());
  }, []);
  useEffect(() => {
    void api
      .weekprepStatus(reviewWeek(localToday()))
      .then((p) => {
        const ready: Record<string, ClarifySuggestion> = {};
        for (const x of p.prep?.suggestions ?? []) {
          const a = x.action;
          if (a?.do !== "clarify") continue;
          const id = key({ ...a.item, stamp: null });
          ready[id] = {
            id,
            becomes: a.becomes,
            text: a.text ?? "",
            project: a.project,
            context: a.context,
            effort: null,
            due: a.due,
            why: x.text,
          };
        }
        setSugg((m) => ({ ...ready, ...m }));
      })
      .catch(() => {});
  }, [prepped]);
  const idx = Math.max(
    0,
    items.findIndex((i) => key(i) === sel),
  );
  const cur = items[idx] ?? null;
  // A transcript whose meeting note is under way (or failed): its row says so, and while it's
  // under way it can't be ticked or chosen, so it isn't made twice.
  useEffect(startRuns, []);
  useEffect(startKnowledge, []);
  const runList = useStore(runs);
  const changeRows = useStore(changes);
  const progress = (i: InboxItem) => (isTranscript(i) ? meetingProgress(runList ?? [], i.path, changeRows) : null);
  const busy = (i: InboxItem) => !!progress(i)?.busy;
  // What's ticked and still in the Inbox; with more than one, the pane acts on them together.
  const picked = items.filter((i) => checked.includes(key(i)) && !busy(i));
  const many = picked.length > 1;
  const active = (projects ?? []).filter((p) => p.status === "active");

  const after = (msg: string) => {
    setDone((n) => n + 1);
    setForm(null);
    setFiling(false);
    const next = items[idx + 1] ?? items[idx - 1] ?? null;
    setSel(next ? key(next) : null);
    toast(msg, { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok");
  };
  const settle = settleInboxItem;
  // One action at a time: a second key while the first is still writing would act on the same item again.
  const inFlight = useRef(false);
  const act = (f: () => Promise<unknown>, msg: string) => {
    if (inFlight.current) return;
    inFlight.current = true;
    return f()
      .then(() => after(msg))
      .catch(reportEditError)
      .finally(() => {
        inFlight.current = false;
      });
  };

  const outcome = (i: InboxItem, s: ClarifySuggestion | { becomes: string; text?: string; project?: string | null }) => {
    const proj = s.project ? (active.find((p) => p.name.toLowerCase() === s.project!.toLowerCase())?.path ?? null) : null;
    const base: Clarified = { text: s.text || startText(i), project: proj };
    const full = "context" in s ? { ...base, context: s.context, effort: s.effort, due: s.due } : base;
    switch (s.becomes) {
      case "next":
        return setForm({ mode: "next", c: full });
      case "waiting":
        return setForm({ mode: "waiting", c: { ...full, tag: "waiting-for" } });
      case "project":
        return setMaking(s.text || itemText(i));
      case "meeting":
        // In one step when Brainstead is sure of the note; otherwise the meeting screen asks.
        return void makeMeetingNotes([i.path]);
      case "reply":
        return nav.go({ screen: "reply", path: i.path });
      case "ask":
        // Its strip shows on the chat about it, and goes with a new chat (that clears it).
        askAboutNote(i.path);
        return askedFromInbox.set({ path: i.path, chat: askChats.get().active ?? "" });
      case "ingest":
        if (inFlight.current) return;
        ingest([i.path]);
        return act(() => captureDone(i.path), "Ingesting: its changes are listed in Changes");
      case "done":
        return act(() => doneNow(i), "Done, and out of the Inbox");
      case "someday":
        return act(() => settle(i, taskLine({ ...full, tag: "someday-maybe" }), proj, "Next actions"), "Moved to Someday / maybe");
      case "reference":
        if (i.kind === "capture") return act(() => captureDone(i.path), "Kept in Sources");
        return setFiling(true);
      case "delete":
        if (i.kind === "capture")
          // Not marked clarified: the file leaving sources/ takes it out, and restored (from the
          // Trash, ⌘Z or Changes) it's back in the Inbox. A transcript's links point at its note first.
          return act(() => trashCapture(i.path), "Moved to the Trash");
        return act(() => settle(i, null), "Deleted");
    }
  };

  // Several ticked: what can be done to all of them at once (bulkChoices).
  const n = picked.length;
  const actAll = (ps: InboxItem[], b: string) => {
    if (inFlight.current || !ps.length) return;
    if (b === "meeting") {
      // Each transcript stays (its row says how its note is getting on) until the note is made and
      // ingested; pressed again meanwhile, nothing more starts (makeMeetingNotes).
      setChecked([]);
      return void makeMeetingNotes(ps.map((i) => i.path));
    }
    if (b === "ingest") ingest(ps.map((i) => i.path));
    const one = (i: InboxItem): Promise<unknown> => {
      if (b === "ingest" || b === "reference") return captureDone(i.path);
      if (b === "done") return doneNow(i);
      if (b === "someday") return settle(i, taskLine({ text: startText(i), project: null, tag: "someday-maybe" }), null, "Next actions");
      if (i.kind === "capture") return trashCapture(i.path);
      return settle(i, null);
    };
    // Bottom up within a note, so taking a line out doesn't move the lines still to do. One that
    // fails doesn't stop the rest: it stays ticked, and the toast says why.
    const order = [...ps].sort((x, y) => x.path.localeCompare(y.path) || y.line - x.line);
    inFlight.current = true;
    eachItem(order, one)
      .then(({ ok, failed }) => {
        const gone = new Set(ok.map(key));
        setDone((d) => d + ok.length);
        setChecked((c) => c.filter((k) => !gone.has(k)));
        toast(
          eachSaid(
            DONE_ALL[b],
            ok.length,
            failed.map(([i, why]) => `${i.kind === "capture" ? captureTitle(i.text) : itemText(i).split("\n")[0]}: ${why}`),
          ),
          undefined,
          failed.length ? "bad" : "ok",
        );
      })
      .finally(() => {
        inFlight.current = false;
      });
  };

  // Keys: the choices, J / K between items, ↩ the suggestion; with several ticked, their choices.
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (!cur || form || filing || making || e.metaKey || e.ctrlKey || e.altKey) return;
      // Its meeting note under way: J / K still move, nothing acts on it.
      if (busy(cur) && !many && !["j", "k", "ArrowDown", "ArrowUp"].includes(e.key)) return;
      if ((e.target as HTMLElement)?.closest?.("input, textarea, [contenteditable]")) return;
      if (many) {
        const c = bulkChoices(picked).find(([k]) => (k === "⌫" ? "Backspace" : k.toLowerCase()) === e.key);
        if (!c) return;
        e.preventDefault();
        if (!e.repeat) actAll(picked, c[1]);
        return;
      }
      const s = isTranscript(cur) ? undefined : sugg[key(cur)];
      const map: Record<string, string> = Object.fromEntries(
        inboxChoices(cur).map((b) => [CHOICE[b].key === "⌫" ? "Backspace" : CHOICE[b].key.toLowerCase(), b]),
      );
      if (e.key === "j" || e.key === "ArrowDown") setSel(key(items[Math.min(items.length - 1, idx + 1)]));
      else if (e.key === "k" || e.key === "ArrowUp") setSel(key(items[Math.max(0, idx - 1)]));
      else if ((e.key === "Enter" && s) || map[e.key]) {
        // A held key repeats: it acts once, and not again while that action is still under way.
        if (!e.repeat && !inFlight.current) outcome(cur, e.key === "Enter" && s ? s : { becomes: map[e.key] });
      } else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  });

  const suggest = (which: InboxItem[]) => {
    setThinking(true);
    api
      .clarifySuggest(
        which.map((i) => ({ id: key(i), kind: suggestKind(i), text: itemText(i) })),
        active.map((p) => p.name),
      )
      .then((r) => {
        setSugg((m) => ({ ...m, ...Object.fromEntries(r.map((x) => [x.id, x])) }));
        // Said either way, so a Suggest that found nothing isn't silent.
        if (!r.length) toast("The AI gave no suggestions this time; try Suggest again", undefined, "bad");
        else if (which.length > 1)
          toast(
            `${r.length === 1 ? "1 suggestion" : `${r.length} suggestions`}: “suggests …” under each item, and on the item when it's open`,
            undefined,
            "ok",
          );
      })
      .catch((e) => toast(String(e), undefined, "bad"))
      .finally(() => setThinking(false));
  };

  const total = items.length + done;
  // What Suggest asks about: not transcripts, whose choice is the meeting note.
  const askable = items.filter((i) => !isTranscript(i));
  return (
    <main className="main">
      <TopBar
        title="Inbox"
        sub={raw ? `${items.length} to clarify` : undefined}
        ask={
          cur
            ? { about: `the Inbox item ${askQuote(cur.text)}`, prompt: `About the Inbox item “${cur.text.replace(/\s+/g, " ").trim()}”: ` }
            : { about: "your Inbox", prompt: "About my Inbox: " }
        }
      >
        {total > 0 && (
          <>
            <span className="faint small">{items.length ? `Item ${idx + 1} of ${items.length}` : "All clear"}</span>
            <span className="pbar">
              <i style={{ width: `${(done / total) * 100}%` }} />
            </span>
          </>
        )}
        <span
          title={
            thinking
              ? "The AI is working on its suggestions"
              : askable.length
                ? `Ask the AI what ${askable.length > 20 ? "the first 20 items" : "each item"} should become (a transcript is a meeting note, so it isn't asked about); nothing changes until you choose`
                : "Nothing in the Inbox to ask about"
          }
        >
          <button type="button" className="btn" disabled={!askable.length || thinking} onClick={() => suggest(askable.slice(0, 20))}>
            <Icon name="cpu" size={14} />
            {thinking ? "Thinking…" : askable.length > 20 ? "Suggest for all (first 20)" : "Suggest for all"}
          </button>
        </span>
      </TopBar>
      <div className="body inbox">
        <div className="ilist tb" role="table" aria-label="Inbox">
          {raw === null && <p className="faint pad">Loading…</p>}
          {raw && !items.length && (
            <p className="faint pad">
              Nothing to clarify. Thoughts captured to the Scratchpad, tasks under Other on the To Do list, and emails and Teams chats
              captured from the browser land here.
            </p>
          )}
          {SOURCES.map((g) => {
            const these = items.filter((i) => i.kind === g.kind);
            if (!these.length) return null;
            return (
              <div key={g.kind} className="tb-group" role="rowgroup">
                <TableBand
                  lead={
                    <CheckAll
                      keys={these.filter((i) => !busy(i)).map(key)}
                      checked={checked}
                      label={g.label}
                      onChange={(on) =>
                        setChecked((c) =>
                          on
                            ? [...new Set([...c, ...these.filter((i) => !busy(i)).map(key)])]
                            : c.filter((k) => !these.some((i) => key(i) === k)),
                        )
                      }
                    />
                  }
                  icon={g.icon}
                  label={g.label}
                  n={these.length}
                  tip={`${these.length} to clarify ${g.tip}`}
                />
                {these.map((i) => {
                  const note = [rowNote(i), sugg[key(i)] && `suggests ${LABEL[sugg[key(i)].becomes]}`].filter(Boolean).join(" · ");
                  const p = progress(i);
                  const on = checked.includes(key(i)) && !p?.busy;
                  return (
                    <div
                      key={key(i)}
                      role="row"
                      className={`irowc tb-row ${p?.busy ? "busy" : ""} ${(many ? on : cur && key(cur) === key(i)) ? "sel" : ""}`}
                    >
                      <input
                        type="checkbox"
                        className="icheck"
                        checked={on}
                        disabled={p?.busy}
                        aria-label="Tick to act on it with others"
                        title={p?.busy ? "Its meeting note is under way" : "Tick to act on several items together"}
                        onChange={() => toggle(key(i))}
                      />
                      <button
                        type="button"
                        className="irow"
                        disabled={p?.busy}
                        title={p?.busy ? `${p.label} It leaves the Inbox once that's done.` : "Show this item to clarify it (J / K)"}
                        onClick={() => {
                          setChecked([]);
                          setSel(key(i));
                        }}
                      >
                        <span className="tt">
                          <TaskText text={i.kind === "capture" ? captureTitle(i.text) : itemText(i).split("\n")[0]} />
                        </span>
                        {p ? (
                          <span className={`faint iprog ${p.busy ? "" : "bad"}`} title={p.why}>
                            {p.busy && <span className="spin" />}
                            {p.label}
                          </span>
                        ) : (
                          note && <span className="faint">{note}</span>
                        )}
                      </button>
                    </div>
                  );
                })}
              </div>
            );
          })}
        </div>
        <div className="iopen">
          {many ? (
            <>
              <span className="eyebrow">{n} items ticked</span>
              <ul className="imany">
                {picked.map((i) => (
                  <li key={key(i)}>
                    <span className="ikindsm" title={inboxItemKind(i).label}>
                      <Icon name={inboxItemKind(i).icon} size={13} />
                    </span>
                    <TaskText text={i.kind === "capture" ? captureTitle(i.text) : itemText(i).split("\n")[0]} />
                    <span className="faint small"> · {sourceOf(i)}</span>
                  </li>
                ))}
              </ul>
              <div className="ikeys">
                {bulkChoices(picked).map(([k, b, l, tip]) => (
                  <button key={b} type="button" className="key" title={`${tip} (${k})`} onClick={() => actAll(picked, b)}>
                    <span className="kbd">{k}</span>
                    {l}
                  </button>
                ))}
              </div>
              <p className="faint small">
                Next action, Waiting for and New project are one item at a time: click an item to clarify it alone.{" "}
                <button type="button" className="blink" title="Untick every item" onClick={() => setChecked([])}>
                  Untick all
                </button>
              </p>
            </>
          ) : cur ? (
            <>
              <span className="eyebrow ikind" title={inboxItemKind(cur).label}>
                <Icon name={inboxItemKind(cur).icon} size={16} />
                {sourceOf(cur)}
              </span>
              {cur.kind === "capture" ? <CaptureBody item={cur} /> : <ItemBody text={itemText(cur)} />}
              {progress(cur) && (
                <div className={`doc-banner ${progress(cur)!.busy ? "" : "warn"}`} role="status">
                  {progress(cur)!.busy ? <span className="spin" /> : <Icon name="info" size={14} />}
                  <span>
                    {progress(cur)!.label}
                    {progress(cur)!.busy
                      ? " It leaves the Inbox once its note is made and ingested."
                      : progress(cur)!.why
                        ? ` ${progress(cur)!.why}`
                        : ""}
                  </span>
                </div>
              )}
              {isTranscript(cur) ? null : sugg[key(cur)] ? (
                <SuggestionCard s={sugg[key(cur)]} />
              ) : (
                <span
                  title={
                    thinking
                      ? "The AI is working on its suggestions"
                      : "Ask the AI what this item should become; nothing changes until you choose"
                  }
                >
                  <button type="button" className="btn sm" disabled={thinking} onClick={() => suggest([cur])}>
                    <Icon name="cpu" size={12} />
                    {thinking ? "Thinking…" : "Suggest"}
                  </button>
                </span>
              )}
              {!busy(cur) && (
                <div className="ikeys">
                  {sugg[key(cur)] && !isTranscript(cur) && (
                    <button
                      type="button"
                      className="key rec"
                      title={`Do what the AI suggests: ${LABEL[sugg[key(cur)].becomes]} (↩)`}
                      onClick={() => outcome(cur, sugg[key(cur)])}
                    >
                      <span className="kbd">↩</span>Accept suggestion
                    </button>
                  )}
                  {inboxChoices(cur).map((b) => {
                    const c = CHOICE[b];
                    const k = c.key;
                    return (
                      <button
                        key={b}
                        type="button"
                        className={`key ${b === leadChoice(cur) ? "rec" : ""}`}
                        title={`${c.tip(cur)} (${k})`}
                        onClick={() => outcome(cur, { becomes: b })}
                      >
                        <span className="kbd">{k}</span>
                        {c.label(cur)}
                      </button>
                    );
                  })}
                </div>
              )}
              <p className="faint small">Nothing changes until you choose. J / K move between items; {keyLabel("⌘Z")} undoes.</p>
            </>
          ) : (
            raw && (
              <div className="soon">
                <Icon name="inbox" size={28} />
                <h1 className="h2">Inbox zero</h1>
                <p className="muted">{done ? `${done} clarified this time.` : "Nothing waiting to be clarified."}</p>
              </div>
            )
          )}
        </div>
      </div>
      {form && cur && (
        <ClarifyForm
          mode={form.mode}
          start={form.c}
          projects={active}
          onClose={() => setForm(null)}
          onSave={(c) =>
            act(
              () => settle(cur, taskLine(c), c.project ?? null, form.mode === "waiting" ? "Waiting for" : "Next actions"),
              form.mode === "waiting"
                ? "Added to Waiting for"
                : c.project
                  ? `Next action on ${projectName(c.project)}`
                  : "Next action added",
            )
          }
        />
      )}
      {filing && cur && (
        <FileAsReference item={cur} onClose={() => setFiling(false)} onDone={(to) => act(() => settle(cur, null), `Filed in ${to}`)} />
      )}
      {making !== null && cur && (
        <NewProject
          name={making}
          onClose={() => setMaking(null)}
          onMade={(path) => {
            setMaking(null);
            void act(
              () => settle(cur, cur.kind === "task" ? taskLine({ text: itemText(cur) }) : null, path),
              `Project ${projectName(path)} made`,
            );
          }}
        />
      )}
    </main>
  );
}

/** A band's tick-all box: ticked when all its items are, part-ticked when some are. */
function CheckAll({
  keys,
  checked,
  label,
  onChange,
}: {
  keys: string[];
  checked: string[];
  label: string;
  onChange: (on: boolean) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  const some = keys.filter((k) => checked.includes(k)).length;
  const all = some === keys.length && some > 0;
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = some > 0 && !all;
  }, [some, all]);
  return (
    <input
      ref={ref}
      type="checkbox"
      className="icheck"
      checked={all}
      aria-label={`Tick every item in ${label}`}
      title={all ? `Untick every item in ${label}` : `Tick every item in ${label}`}
      onChange={() => onChange(!all)}
    />
  );
}

/** "Email. Thread. Re Launch - 2026-10-02" → "Re Launch". */
const captureTitle = (stem: string) => stem.replace(/^(Email|Teams)\.( [A-Z][a-z]+\.)? /, "").replace(/ - \d{4}-\d{2}-\d{2}(-\d+)?$/, "");

/** A capture: its title, a link to the file, and the start of its text. */
function CaptureBody({ item }: { item: InboxItem }) {
  const [doc, setDoc] = useState<{ content: string; root: string } | null>(null);
  const v = useVaultVersion();
  useEffect(() => {
    let live = true;
    api
      .docRead(item.path)
      .then((d) => live && setDoc({ content: d.content, root: d.root }))
      .catch(() => live && setDoc(null));
    return () => {
      live = false;
    };
  }, [item.path, v]);
  // The heading, link and time above the first `---` are shown above it already.
  const text = doc?.content ?? "";
  const cut = text.indexOf("\n---\n");
  const body = (cut >= 0 ? text.slice(cut + 5) : text.replace(/^# .*\n+/, "")).trim();
  return (
    <div className="ibody selectable">
      <h1 className="h2 itext">{captureTitle(item.text)}</h1>
      <button type="button" className="btn sm icopen" title="Open the captured file in full" onClick={() => openDoc(item.path)}>
        <Icon name="source" size={12} />
        Open the capture
      </button>
      {doc && (
        <div className="icapture">
          <Markdown content={body.length > 4000 ? `${body.slice(0, 4000)}…` : body} path={item.path} root={doc.root} />
        </div>
      )}
    </div>
  );
}

/** The open item: its first line as the title, the rest drawn as markdown at reading size. */
function ItemBody({ text }: { text: string }) {
  const root = useStore(settings).vaultPath ?? "";
  const [first, ...rest] = text.split("\n");
  const body = rest.join("\n").trim();
  return (
    <div className="ibody selectable">
      <h1 className="h2 itext">
        <TaskText text={first.replace(/^[-*+]\s+\[.\]\s+/, "").replace(/\\([\\`*_{}[\]()#+\-.!&])/g, "$1")} />
      </h1>
      {body && <Markdown content={body} path="" root={root} />}
    </div>
  );
}

const LABEL: Record<string, string> = {
  next: "a next action",
  project: "a project",
  waiting: "waiting for",
  done: "done now",
  someday: "someday / maybe",
  reference: "reference",
  delete: "delete",
  ingest: "ingest into the wiki",
  meeting: "a meeting note",
  reply: "a reply",
};

function SuggestionCard({ s }: { s: ClarifySuggestion }) {
  return (
    <div className="card isugg">
      <div className="row">
        <Icon name="cpu" size={14} style={{ color: "var(--accent)" }} />
        <b>Suggested</b>
      </div>
      <dl className="kv">
        <dt>Becomes</dt>
        <dd>
          <b>{LABEL[s.becomes] ?? s.becomes}</b>
          {s.text && s.becomes !== "delete" && <> — {s.text}</>}
        </dd>
        {s.project && (
          <>
            <dt>Project</dt>
            <dd>{s.project}</dd>
          </>
        )}
        {s.context && (
          <>
            <dt>Context</dt>
            <dd>@{s.context}</dd>
          </>
        )}
        {s.effort && effortMinutes(s.effort) !== null && (
          <>
            <dt>Effort</dt>
            <dd>{fmtEffort(effortMinutes(s.effort)!)}</dd>
          </>
        )}
        {s.due && (
          <>
            <dt>Due</dt>
            <dd>{fmtShortDate(s.due)}</dd>
          </>
        )}
        {s.why && (
          <>
            <dt>Why</dt>
            <dd className="muted">{s.why}</dd>
          </>
        )}
      </dl>
    </div>
  );
}

/** The next action or waiting-for, before it's written. */
function ClarifyForm({
  mode,
  start,
  projects,
  onClose,
  onSave,
}: {
  mode: "next" | "waiting";
  start: Clarified;
  projects: ProjectRow[];
  onClose: () => void;
  onSave: (c: Clarified) => void;
}) {
  const [c, setC] = useState<Clarified>(start);
  const [dueWord, setDueWord] = useState(start.due ?? "");
  const due = dueWord ? (resolveDateWord(dueWord, localToday()) ?? null) : null;
  // A next action needs a project or a context, or it would land back in the Inbox.
  const ok = !!c.text.trim() && (mode === "waiting" || !!c.project || !!c.context?.trim());
  const save = () => ok && onSave({ ...c, due });
  return (
    <Dialog onClose={onClose} width={520} label={mode === "next" ? "Next action" : "Waiting for"}>
      <div className="confirm">
        <h2 className="h2">{mode === "next" ? "Next action" : "Waiting for"}</h2>
        <label className="nnf">
          <span className="faint">{mode === "next" ? "What's the next physical step?" : "Who, and what are you waiting for?"}</span>
          <span className="inp">
            <input
              autoFocus
              value={c.text}
              onChange={(e) => setC({ ...c, text: e.target.value })}
              onKeyDown={(e) => e.key === "Enter" && save()}
            />
          </span>
        </label>
        <label className="nnf">
          <span className="faint">Project</span>
          <select
            title={
              mode === "next"
                ? "The active project this next action belongs to; with none, give it a context"
                : "The active project it belongs to, or none for the To Do list"
            }
            className="sel"
            value={c.project ?? ""}
            onChange={(e) => setC({ ...c, project: e.target.value || null })}
          >
            <option value="">{mode === "next" ? "None: give it a context below" : "None (the To Do list)"}</option>
            {projects.map((p) => (
              <option key={p.path} value={p.path}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        <div className="row gap">
          <label className="nnf grow">
            <span className="faint">Context</span>
            <span className="inp">
              <input value={c.context ?? ""} placeholder="calls" onChange={(e) => setC({ ...c, context: e.target.value || null })} />
            </span>
          </label>
          <label className="nnf grow">
            <span className="faint">Effort</span>
            <span className="inp">
              <input value={c.effort ?? ""} placeholder="15m" onChange={(e) => setC({ ...c, effort: e.target.value || null })} />
            </span>
          </label>
          <label className="nnf grow">
            <span className="faint">Due {due ? `· ${fmtShortDate(due)}` : ""}</span>
            <span className="inp">
              <input value={dueWord} placeholder="fri, next week" onChange={(e) => setDueWord(e.target.value)} />
            </span>
          </label>
        </div>
        <p className="faint mono small">{taskLine({ ...c, due })}</p>
        {mode === "next" && !c.project && !c.context?.trim() && (
          <p className="faint small">
            <Icon name="info" size={12} /> With no project, it needs a context (calls, office…) to land on a list; without either it would
            come back to the Inbox.
          </p>
        )}
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" title="Close and leave the item in the Inbox (Esc)" onClick={onClose}>
            Cancel
          </button>
          <span
            title={
              ok
                ? "Save the task and take the item out of the Inbox"
                : c.text.trim()
                  ? "Give it a project or a context first"
                  : "Write the task first"
            }
          >
            <button type="button" className="btn lg pri" disabled={!ok} onClick={save}>
              Save
            </button>
          </span>
        </div>
      </div>
    </Dialog>
  );
}

/** File as reference: append the item's text to a note you pick (or a new one). */
function FileAsReference({ item, onClose, onDone }: { item: InboxItem; onClose: () => void; onDone: (title: string) => void }) {
  const [q, setQ] = useState("");
  const found = useSearch(q, ["note", "wiki"], 8);
  const text = itemText(item).replace(/^[-*+]\s+\[.\]\s+/, "");
  const append = async (path: string, title: string) => {
    try {
      await appendAsReference(path, text);
      onDone(title);
    } catch (e) {
      reportEditError(e);
    }
  };
  const create = async () => {
    const title = q.trim();
    if (!title) return;
    try {
      await newReferenceNote(title, text);
      onDone(title);
    } catch (e) {
      reportEditError(e);
    }
  };
  return (
    <Dialog onClose={onClose} width={520} label="File as reference">
      <div className="confirm">
        <h2 className="h2">File as reference</h2>
        <p className="muted small">Adds it to the end of a note as a bullet.</p>
        <label className="inp">
          <Icon name="search" size={14} />
          <input autoFocus placeholder="Find a note, or type a new note's name" value={q} onChange={(e) => setQ(e.target.value)} />
        </label>
        <div className="ifile">
          {(found?.hits ?? []).map((h) => (
            <button
              key={h.file.path}
              type="button"
              className="blink"
              title={`Add it as a bullet at the end of ${h.file.title}`}
              onClick={() => void append(h.file.path, h.file.title)}
            >
              <span className="lb note">{h.file.title}</span>
              <span className="faint">{h.file.path}</span>
            </button>
          ))}
          {q.trim() && (
            <button type="button" className="blink" title="Create a new note with this name holding the item" onClick={() => void create()}>
              <span className="lb note">New note “{q.trim()}”</span>
            </button>
          )}
        </div>
      </div>
    </Dialog>
  );
}
