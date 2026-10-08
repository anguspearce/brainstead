// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Inbox (stage 6): GTD's clarify step over what you've captured, the Scratchpad's thoughts and
// the To Do list's `#### Other` tasks. For each, choose what it becomes; Brainstead writes that
// and the item leaves the Inbox (a thought's block is removed; a task leaves once it has a
// project, a context or a waiting-for or someday tag, is done, or is moved). ⌘Z undoes. A
// suggestion from the model is only shown until you accept it.

import { nav, openDoc, useViewState } from "./nav";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, ClarifySuggestion, InboxItem, ProjectRow } from "./api";
import { prepareCapture } from "./Capture";
import { contextName, fmtEffort, effortMinutes, projectLink, projectName, unclarified, useInbox, useProjects } from "./gtd";
import { Icon } from "./icons";
import { fmtShortDate, resolveDateWord } from "./md/dates";
import { localToday } from "./md/taskQuery";
import { ingest } from "./Ingest";
import { NewProject } from "./Projects";
import { useSearch } from "./Search";
import { reportEditError, undoLast } from "./taskModel";
import { toast } from "./Toast";
import { askQuote, TopBar } from "./TopBar";
import { isThread } from "./skills/Reply";
import { reviewWeek } from "./Weekly";
import { Dialog, TableBand } from "./ui";
import { Markdown } from "./md/Markdown";
import { settings, useStore } from "./store";
import { useVaultVersion } from "./state";
import { makeMeetingNotes } from "./meetingFlow";
import { TaskText } from "./TaskList";
import { appendAsReference, doneNow, newReferenceNote, settleInboxItem } from "./inboxActions";
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
  { kind: "capture", label: "Captures", icon: "source", tip: "captured from Outlook or Teams" },
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
/** A Teams meeting transcript, which can become a meeting note. */
const isTranscript = (i: InboxItem) => i.kind === "capture" && /^sources\/Teams\. Transcript\./.test(i.path);
/** What a capture becomes as a task, by default: a follow-up linking the file. */
const startText = (i: InboxItem) => (i.kind === "capture" ? `Follow up on [[${i.text}]]` : itemText(i));

export function InboxScreen() {
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
  const key = (i: InboxItem) => `${i.kind}:${i.line}:${i.lineText.slice(0, 40)}`;
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
      case "ingest":
        if (inFlight.current) return;
        ingest([i.path]);
        return act(() => api.inboxCaptureDone(i.path), "Ingesting: its changes are listed in Changes");
      case "done":
        return act(() => doneNow(i), "Done, and out of the Inbox");
      case "someday":
        return act(() => settle(i, taskLine({ ...full, tag: "someday-maybe" }), proj, "Next actions"), "Moved to Someday / maybe");
      case "reference":
        if (i.kind === "capture") return act(() => api.inboxCaptureDone(i.path), "Kept in Sources");
        return setFiling(true);
      case "delete":
        if (i.kind === "capture")
          return act(async () => {
            await api.trashMove(i.path);
            await api.inboxCaptureDone(i.path);
          }, "Moved to the Trash");
        return act(() => settle(i, null), "Deleted");
    }
  };

  // Keys: the choices, J / K between items, ↩ the suggestion.
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (!cur || form || filing || making || e.metaKey || e.ctrlKey || e.altKey) return;
      if ((e.target as HTMLElement)?.closest?.("input, textarea, [contenteditable]")) return;
      const s = sugg[key(cur)];
      const map: Record<string, string> = {
        n: "next",
        p: "project",
        w: "waiting",
        "2": "done",
        s: "someday",
        r: "reference",
        Backspace: "delete",
        ...(cur.kind === "capture" ? { i: "ingest" } : {}),
        ...(isTranscript(cur) ? { m: "meeting" } : {}),
        ...(cur.kind === "capture" && isThread(cur.path) ? { d: "reply" } : {}),
      };
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
        which.map((i) => ({ id: key(i), kind: i.kind, text: itemText(i) })),
        active.map((p) => p.name),
      )
      .then((r) => setSugg((m) => ({ ...m, ...Object.fromEntries(r.map((x) => [x.id, x])) })))
      .catch((e) => toast(String(e), undefined, "bad"))
      .finally(() => setThinking(false));
  };

  const total = items.length + done;
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
              : items.length
                ? `Ask the AI what ${items.length > 20 ? "the first 20 items" : "each item"} should become; nothing changes until you choose`
                : "Nothing in the Inbox to clarify"
          }
        >
          <button type="button" className="btn" disabled={!items.length || thinking} onClick={() => suggest(items.slice(0, 20))}>
            <Icon name="cpu" size={14} />
            {thinking ? "Thinking…" : items.length > 20 ? "Suggest for all (first 20)" : "Suggest for all"}
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
                <TableBand icon={g.icon} label={g.label} n={these.length} tip={`${these.length} to clarify ${g.tip}`} />
                {these.map((i) => {
                  const note = [rowNote(i), sugg[key(i)] && `suggests ${LABEL[sugg[key(i)].becomes]}`].filter(Boolean).join(" · ");
                  return (
                    <button
                      key={key(i)}
                      type="button"
                      role="row"
                      className={`irow tb-row ${cur && key(cur) === key(i) ? "sel" : ""}`}
                      title="Show this item to clarify it (J / K)"
                      onClick={() => setSel(key(i))}
                    >
                      <span className="tt">
                        <TaskText text={i.kind === "capture" ? captureTitle(i.text) : itemText(i).split("\n")[0]} />
                      </span>
                      {note && <span className="faint">{note}</span>}
                    </button>
                  );
                })}
              </div>
            );
          })}
        </div>
        <div className="iopen">
          {cur ? (
            <>
              <span className="eyebrow">{sourceOf(cur)}</span>
              {cur.kind === "capture" ? <CaptureBody item={cur} /> : <ItemBody text={itemText(cur)} />}
              {sugg[key(cur)] ? (
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
              <div className="ikeys">
                {sugg[key(cur)] && (
                  <button
                    type="button"
                    className="key rec"
                    title={`Do what the AI suggests: ${LABEL[sugg[key(cur)].becomes]} (↩)`}
                    onClick={() => outcome(cur, sugg[key(cur)])}
                  >
                    <span className="kbd">↩</span>Accept suggestion
                  </button>
                )}
                {(
                  [
                    ...(isTranscript(cur)
                      ? ([["M", "meeting", "Make a meeting note", "Turn this transcript into a meeting note"]] as const)
                      : []),
                    ...(cur.kind === "capture" && isThread(cur.path)
                      ? ([["D", "reply", "Draft a reply", "Draft a reply to this thread"]] as const)
                      : []),
                    ...(cur.kind === "capture"
                      ? ([
                          ["I", "ingest", "Ingest into the wiki", "Have the AI read it into the wiki and take it out of the Inbox"],
                        ] as const)
                      : []),
                    ["N", "next", "Next action", "Make it a next action; you set its project, context and due date first"],
                    ["P", "project", "New project", "Start a new project from it"],
                    ["W", "waiting", "Waiting for", "Make it a task tagged waiting-for, to chase later"],
                    ["2", "done", "Done now (under 2 min)", "It takes two minutes: do it now, and this marks it done"],
                    ["S", "someday", "Someday / maybe", "Park it as a task tagged someday-maybe"],
                    [
                      "R",
                      "reference",
                      cur.kind === "capture" ? "Keep in Sources" : "File as reference",
                      cur.kind === "capture"
                        ? "Keep the capture in Sources and take it out of the Inbox"
                        : "Add it as a bullet to the end of a note you pick",
                    ],
                    [
                      "⌫",
                      "delete",
                      cur.kind === "capture" ? "Move to the Trash" : "Delete",
                      cur.kind === "capture"
                        ? "Move the captured file to the vault’s Trash"
                        : keyLabel("Delete it from the Inbox; ⌘Z undoes"),
                    ],
                  ] as const
                ).map(([k, b, l, tip]) => (
                  <button key={b} type="button" className="key" title={`${tip} (${k})`} onClick={() => outcome(cur, { becomes: b })}>
                    <span className="kbd">{k}</span>
                    {l}
                  </button>
                ))}
              </div>
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
  ingest: "ingest",
  meeting: "a meeting note",
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
          <select className="sel" value={c.project ?? ""} onChange={(e) => setC({ ...c, project: e.target.value || null })}>
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
