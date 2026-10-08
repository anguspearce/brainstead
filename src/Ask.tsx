// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Ask screen: chats with an AI CLI about the vault, in tabs. Answers stream in and are drawn
// by the document renderer (so [[links]] open notes); what the model is reading shows above with
// Stop; messages typed meanwhile wait in a queue. The composer has `/` for the vault's skills,
// `[[` and `#` from the index, ↑ for earlier messages, and the model picker. The state is in
// src/askState.ts; History lists every chat, saved to the vault or kept in Brainstead's app data.

import { StillInInbox } from "./StillInInbox";
import { HelpButton } from "./help/HelpDrawer";
import { useEffect, useMemo, useRef, useState } from "react";
import { ChatSummary, CliInfo, Skill, api } from "./api";
import {
  ask,
  askedFromInbox,
  askWith,
  Chat,
  closeChat,
  CLI_LABEL,
  clis,
  cliOf,
  DEFAULT_MODEL,
  fallbackTitle,
  findClis,
  isSaved,
  keepChat,
  modelLabel,
  newChat,
  openSaved,
  pendingPrompt,
  selectChat,
  shown,
  send,
  setModel,
  stop,
  unqueue,
  renameSaved,
  trashSaved,
  pinSaved,
  keepSaved,
} from "./askState";
import { suggest } from "./Capture";
import { Icon } from "./icons";
import { richOf } from "./notes/actions";
import { Markdown } from "./md/Markdown";
import { nav, openDoc, Screen } from "./nav";
import { makePage } from "./knowledge";
import { reportEditError } from "./taskModel";
import { openFixName } from "./FixName";
import { settings, useStore } from "./store";
import { toast } from "./Toast";
import { HistButtons } from "./TopBar";
import { ago, Dialog, Popover, useDebounced } from "./ui";
import { ASSISTANT_FRONTMATTER } from "./md/scripts";
import { isMod, keyLabel, strayCtrl } from "./platform";

export function AskScreen() {
  const s = useStore(ask);
  const fromInbox = useStore(askedFromInbox);
  const found = useStore(clis);
  const [history, setHistory] = useState(false);
  useEffect(() => {
    void findClis();
    if (!ask.get().chats.length) newChat();
    // ⌘T: a new chat.
    const k = (e: KeyboardEvent) => {
      if (isMod(e) && !e.altKey && !strayCtrl(e) && !e.shiftKey && e.key === "t") {
        newChat();
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, []);
  // Seeing a chat reads it: an answer that lands while the window is closed stays unread until it's back.
  const c = s.chats.find((x) => x.id === s.active) ?? null;
  useEffect(() => {
    if (!c?.unread) return;
    const id = c.id;
    const read = () => {
      if (shown()) selectChat(id);
    };
    read();
    window.addEventListener("focus", read);
    document.addEventListener("visibilitychange", read);
    return () => {
      window.removeEventListener("focus", read);
      document.removeEventListener("visibilitychange", read);
    };
  }, [c?.id, c?.unread]);

  return (
    <main className="main ask">
      <div className="top drag" data-tauri-drag-region>
        <HistButtons />
        <div className="asktabs" role="tablist" aria-label="Chats">
          {s.chats.map((x) => (
            <ChatTab key={x.id} c={x} on={x.id === s.active} />
          ))}
          <button type="button" className="ibtn" aria-label="New chat" title={keyLabel("New chat (⌘T)")} onClick={() => newChat()}>
            <Icon name="plus" size={14} />
          </button>
        </div>
        <div className="sp" />
        {c && <ContextMeter c={c} />}
        {c && <SaveButton c={c} />}
        <button
          type="button"
          className="btn"
          title="Find, reopen, rename or pin your chats, saved or not, or move them to the Trash"
          onClick={() => setHistory(true)}
        >
          <Icon name="history" size={14} />
          History
        </button>
        <HelpButton />
      </div>
      {/* A capture the Inbox sent here to ask about: its Inbox choices, on the chat about it. */}
      {fromInbox && fromInbox.chat === s.active && (
        <StillInInbox key={fromInbox.path} path={fromInbox.path} onDone={() => askedFromInbox.set(null)} />
      )}
      {c ? <ChatView key={c.id} c={c} found={found} /> : <div className="body" />}
      {history && <HistoryDialog onClose={() => setHistory(false)} />}
    </main>
  );
}

function ChatTab({ c, on }: { c: Chat; on: boolean }) {
  return (
    <span className={`asktab ${on ? "on" : ""}`}>
      <button type="button" role="tab" aria-selected={on} onClick={() => selectChat(c.id)} title={c.title}>
        {c.review ? <Icon name="calendar" size={12} /> : c.busy ? <span className="spin" aria-label="Answering" /> : null}
        <span className="t">{c.title}</span>
        {c.unread && <span className="dot" aria-label="New answer" />}
      </button>
      <button
        type="button"
        className="ibtn xs"
        aria-label={`Close ${c.title}`}
        title="Close this chat; it stays in History"
        onClick={() => closeChat(c.id)}
      >
        <Icon name="x" size={11} />
      </button>
    </span>
  );
}

/** Save: the chat goes to the vault as a note and stays up to date there; till then it's only in
 *  Brainstead's app data. Once saved, it opens the note. */
function SaveButton({ c }: { c: Chat }) {
  if (isSaved(c))
    return (
      <button
        type="button"
        className="btn ghost"
        title={`Saved in the vault as ${c.filename.replace(/\.md$/, "")}; it keeps up to date as the chat goes on. Click to open the note.`}
        onClick={() => openDoc(c.filename)}
      >
        <Icon name="check" size={14} />
        Saved
      </button>
    );
  // Disabled buttons get no pointer events in WebKit, so the tooltip goes on a wrapper.
  return (
    <span
      title={
        c.transcript.length
          ? "Save this chat to the vault as a note; it keeps up to date there as the chat goes on. Unsaved chats stay in Brainstead only, and closed ones beyond the latest 20 are deleted."
          : "Nothing to save yet: ask something first"
      }
    >
      <button
        type="button"
        className="btn"
        disabled={!c.transcript.length}
        onClick={() =>
          void keepChat(c.id)
            .then((f) => toast(`Saved ${f.replace(/\.md$/, "")}`, { label: "Open", run: () => openDoc(f) }, "ok"))
            .catch(reportEditError)
        }
      >
        <Icon name="save" size={14} />
        Save
      </button>
    </span>
  );
}

function ContextMeter({ c }: { c: Chat }) {
  if (!c.contextTokens || !c.contextWindow) return null;
  const pct = Math.min(100, (c.contextTokens / c.contextWindow) * 100);
  const k = (n: number) => (n >= 1e6 ? `${+(n / 1e6).toFixed(1)}M` : `${Math.round(n / 1000)}k`);
  const tip = `${k(c.contextTokens)} of ${k(c.contextWindow)} tokens of context used${c.compactions ? `; compacted ${c.compactions}×` : ""}`;
  return (
    <span className="ctxm" title={tip}>
      <span className="faint">
        {k(c.contextTokens)} of {k(c.contextWindow)} context
      </span>
      <span className="bar">
        <i style={{ width: `${pct}%` }} />
      </span>
    </span>
  );
}

function ChatView({ c, found }: { c: Chat; found: CliInfo[] | null }) {
  const end = useRef<HTMLDivElement>(null);
  const root = useStore(settings).vaultPath ?? "";
  // Follow the answer as it comes, unless you've scrolled up to read.
  const body = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  useEffect(() => {
    if (pinned.current) end.current?.scrollIntoView({ block: "end" });
  }, [c.transcript.length, c.streaming, c.status]);
  // Esc stops it.
  useEffect(() => {
    if (!c.busy) return;
    const k = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented) stop(c.id);
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [c.busy, c.id]);

  const turns = useMemo(() => groupTurns(c), [c]);
  const [filing, setFiling] = useState<string | null>(null);
  const [another, setAnother] = useState<{ at: DOMRect; question: string } | null>(null);
  // The question an answer replies to: the nearest user message before it.
  const questionBefore = (i: number) => [...turns.slice(0, i)].reverse().find((t) => t.role === "user")?.text ?? "";
  return (
    <>
      {filing !== null && <FileAnswer text={filing} chat={c.filename || null} onClose={() => setFiling(null)} />}
      {another && (
        <Popover anchor={another.at} onClose={() => setAnother(null)} width={300} place="above">
          <ModelMenu
            found={found}
            value={c.model ?? DEFAULT_MODEL}
            onPick={(m) => {
              const id = newChat(m);
              send(id, another.question);
              setAnother(null);
            }}
          />
        </Popover>
      )}
      <div
        className="body askbody"
        ref={body}
        onScroll={(e) => {
          const el = e.currentTarget;
          pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
      >
        <div className="askcol">
          {!c.transcript.length && !c.busy && <AskEmpty c={c} found={found} />}
          {turns.map((t, i) =>
            t.role === "user" ? (
              <div key={i} className="askq selectable">
                {t.text}
              </div>
            ) : t.role === "error" ? (
              <div key={i} className="askerr selectable">
                <Icon name="info" size={14} />
                <span>{t.text}</span>
              </div>
            ) : (
              <div key={i} className="aska">
                {!!t.looked?.length && <Looked items={t.looked} />}
                {t.text && (
                  <div className="selectable">
                    <Markdown content={t.text} path="" root={root} />
                  </div>
                )}
                {t.text && (
                  <div className="askacts">
                    <button
                      type="button"
                      className="btn sm ghost"
                      title="Copy the answer: rich text where formatting pastes, markdown where it doesn't"
                      onClick={(e) => {
                        const shown = e.currentTarget.closest(".askacts")?.previousElementSibling as HTMLElement | null;
                        void copyAnswer(shown, t.text);
                      }}
                    >
                      <Icon name="copy" size={12} />
                      Copy
                    </button>
                    {t.text.length > 200 && !c.review && (
                      <button
                        type="button"
                        className="btn sm ghost"
                        onClick={() => setFiling(t.text)}
                        title="Make it a new wiki page; you can revert it in Changes"
                      >
                        <Icon name="wiki" size={12} />
                        File this answer
                      </button>
                    )}
                    {!c.busy && (
                      <>
                        <button
                          type="button"
                          className="btn sm ghost"
                          title="The assistant adds the tasks this answer suggests; each is listed in Changes, where you can revert it"
                          onClick={() =>
                            send(c.id, "From your answer above, add the tasks I should take on, using create_task. Say what you added.")
                          }
                        >
                          <Icon name="tasks" size={12} />
                          Add tasks
                        </button>
                        <button
                          type="button"
                          className="btn sm ghost"
                          title="The assistant makes the wiki changes this answer supports; each is listed in Changes, where you can revert it"
                          onClick={() =>
                            send(
                              c.id,
                              "Make the wiki changes your answer above supports, using edit_page, quoting the sources word for word. Say what you changed.",
                            )
                          }
                        >
                          <Icon name="wiki" size={12} />
                          Update the wiki
                        </button>
                        <button
                          type="button"
                          className="btn sm ghost"
                          title="Save the question and answer as a note"
                          onClick={() => void saveAnswer(questionBefore(i), t.text)}
                        >
                          <Icon name="note" size={12} />
                          Save as note
                        </button>
                        {questionBefore(i) && (
                          <button
                            type="button"
                            className="btn sm ghost"
                            title="Ask the same question in a new chat with another model"
                            onClick={(e) => setAnother({ at: e.currentTarget.getBoundingClientRect(), question: questionBefore(i) })}
                          >
                            <Icon name="cpu" size={12} />
                            Ask another model
                          </button>
                        )}
                      </>
                    )}
                  </div>
                )}
              </div>
            ),
          )}
          {c.busy && (
            <div className="aska">
              {!!c.looked.length && <Looked items={c.looked} />}
              {!!c.streaming && (
                <div className="selectable">
                  <Markdown content={c.streaming} path="" root={root} />
                </div>
              )}
              <div className="askworking">
                <span className="spin" />
                <span>{c.status ? `${c.status}…` : c.streaming ? "Writing…" : "Thinking…"}</span>
                <span className="sp" />
                <button type="button" className="btn sm" title="Stop the answer where it is (Esc)" onClick={() => stop(c.id)}>
                  <Icon name="stop" size={12} />
                  Stop<span className="kbd">Esc</span>
                </button>
              </div>
            </div>
          )}
          <div ref={end} />
        </div>
      </div>
      <Composer c={c} found={found} />
    </>
  );
}

/** Questions to start from, in an empty chat: clicking one types it in. */
export const EXAMPLE_QUESTIONS = [
  "What changed in my notes this week?",
  "Which projects have no next action?",
  "What does the wiki say about ",
];

/** An empty chat: where to start, or, with no assistant on this computer, what to install. */
function AskEmpty({ c, found }: { c: Chat; found: CliInfo[] | null }) {
  const [skills, setSkills] = useState<Skill[]>([]);
  useEffect(() => {
    let live = true;
    void api
      .skillsList()
      .then((s) => live && setSkills(groupSkills(s.filter((x) => x.source === "brainstead" && !x.opens))))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  const fill = (text: string) => pendingPrompt.set({ id: c.id, text });
  if (c.review)
    return (
      <div className="askempty">
        <h1 className="h2">Scheduled run</h1>
      </div>
    );
  if (found && !found.some((x) => x.path))
    return (
      <div className="askempty">
        <h1 className="h2">No assistant found</h1>
        <p className="muted">Install Claude Code, Codex, Antigravity or Copilot, sign in, then open Ask again.</p>
      </div>
    );
  return (
    <div className="askempty">
      <h1 className="h2">What can I help with?</h1>
      <p className="muted">Ask about anything in your vault. Try one of these:</p>
      <div className="askstarts">
        {EXAMPLE_QUESTIONS.map((q) => (
          <button key={q} type="button" className="btn" title="Type this question in the box, to edit or send" onClick={() => fill(q)}>
            {q.endsWith(" ") ? `${q}…` : q}
          </button>
        ))}
      </div>
      {skills.length > 0 && (
        <>
          <p className="muted">Or start a skill:</p>
          <div className="askstarts">
            {skills.map((k) => (
              <button
                key={k.name}
                type="button"
                className="btn sm ghost"
                title={`${k.description} Types /${k.name} in the box.`}
                onClick={() => fill(`/${k.name} `)}
              >
                /{k.name}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

/** The page an answer becomes when filed: a concept or entity with the answer as its current state. */
/** The `/` picker's groups, by what a skill is for; anything else goes under Other. */
const SKILL_GROUPS: [string, RegExp][] = [
  ["Reviews", /review/],
  ["Inbox", /inbox|bookmark|clarify/],
  ["Writing", /reply|meeting|doc-check|draft/],
  ["Wiki", /wiki|ingest|contradiction|fix-name|lint|query/],
];
export const skillGroup = (name: string) => SKILL_GROUPS.find(([, re]) => re.test(name))?.[0] ?? "Other";

/** Skills in their groups' order (Reviews, Inbox, Writing, Wiki, Other), names A–Z within. */
export function groupSkills<T extends { name: string }>(items: T[]): T[] {
  const order = [...SKILL_GROUPS.map(([g]) => g), "Other"];
  return [...items].sort((a, b) => order.indexOf(skillGroup(a.name)) - order.indexOf(skillGroup(b.name)) || a.name.localeCompare(b.name));
}

export function filedPage(name: string, kind: "concept" | "entity", answer: string): string {
  return `---\ntype: ${kind}\nname: ${name}\ndescription: \nsources: []\n---\n\n## Current state\n\n${answer.trim()}\n`;
}

/** "File this answer": the answer made into a new wiki page (in Changes, revertable). */
function FileAnswer({ text, chat, onClose }: { text: string; chat: string | null; onClose: () => void }) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"concept" | "entity">("concept");
  const n = name.trim();
  const ok = !!n && !/[\\/:*?"<>|]/.test(n);
  const file = () =>
    makePage(`wiki/${kind === "concept" ? "concepts" : "entities"}/${n}.md`, filedPage(n, kind, text), `New page: ${n}`, {
      kind: "chat",
      chat: chat ?? undefined,
      label: "File this answer",
      run: chat ?? undefined,
    })
      .then((o) => {
        onClose();
        toast(`Made ${n}`, { label: "Open", run: () => openDoc(o.page) }, "ok");
      })
      .catch((e) => toast(String((e as { message?: string }).message ?? e), undefined, "bad"));
  return (
    <Dialog onClose={onClose} width={460} label="File this answer">
      <div className="confirm">
        <h2 className="h2">File this answer</h2>
        <p className="muted small">It becomes a new wiki page, listed in Changes where you can revert it.</p>
        <label className="nnf">
          <span className="faint">Page name</span>
          <span className="inp">
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && ok && void file()}
              placeholder="Launch readiness"
            />
          </span>
        </label>
        <label className="nnf">
          <span className="faint">Kind</span>
          <select
            title="What kind of wiki page it becomes: a concept (an idea, process or topic) or an entity (a person, team, project or product)"
            className="sel"
            value={kind}
            onChange={(e) => setKind(e.target.value as "concept" | "entity")}
          >
            <option value="concept">Concept (an idea, a process, a topic)</option>
            <option value="entity">Entity (a person, team, project, product)</option>
          </select>
        </label>
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" title="Close without filing the answer" onClick={onClose}>
            Cancel
          </button>
          <span
            title={
              ok ? "Make the answer a new wiki page (revertable in Changes)" : "Give the page a name, without characters such as / or :"
            }
          >
            <button type="button" className="btn lg pri" disabled={!ok} onClick={() => void file()}>
              File it
            </button>
          </span>
        </div>
      </div>
    </Dialog>
  );
}

interface Turn {
  role: "user" | "assistant" | "error";
  text: string;
  looked?: string[];
}

/** The transcript as it's drawn: what it looked at goes with the answer after it. */
export function groupTurns(c: { transcript: Chat["transcript"] }): Turn[] {
  const out: Turn[] = [];
  let looked: string[] = [];
  for (const it of c.transcript) {
    if (it.role === "tool") looked.push(it.text);
    else if (it.role === "assistant") {
      out.push({ role: "assistant", text: it.text, looked });
      looked = [];
    } else if (it.role === "user" || it.role === "error") {
      if (looked.length) out.push({ role: "assistant", text: "", looked });
      looked = [];
      out.push({ role: it.role, text: it.text });
    }
  }
  if (looked.length) out.push({ role: "assistant", text: "", looked });
  return out;
}

function Looked({ items }: { items: string[] }) {
  return (
    <div className="looked">
      {items.map((t) => (
        <span key={t} className="chip">
          <Icon name={t.startsWith("Reading") ? "note" : t.startsWith("Running") ? "cpu" : "search"} size={12} />
          {t}
        </span>
      ))}
    </div>
  );
}

type Pick =
  | { kind: "skill"; items: Skill[]; i: number }
  | { kind: "link" | "tag"; items: { label: string; insert: string }[]; start: number; i: number };

/** Save as note: the question and the answer as `Ask. <question> - <date>.md`, opened. */
async function saveAnswer(question: string, answer: string) {
  const d = new Date();
  const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  const name = (fallbackTitle(question || "Answer") || "Answer")
    .replace(/[\\/:*?"<>|#^[\]]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 80);
  const path = `Ask. ${name} - ${date}.md`;
  try {
    await api.docCreate(
      path,
      `${ASSISTANT_FRONTMATTER}# ${name}\n\n${question ? `> ${question.split("\n").join("\n> ")}\n\n` : ""}${answer.trim()}\n`,
    );
    toast("Saved as a note", { label: "Open", run: () => openDoc(path) }, "ok");
  } catch (e) {
    reportEditError(e);
  }
}

/** Copies an answer as both rich text (the drawn answer) and markdown (its text), so it pastes
 *  formatted into Outlook or Word and as markdown into a plain-text box or another note. */
async function copyAnswer(shown: HTMLElement | null, md: string) {
  try {
    const html = shown ? richOf(shown).html : "";
    await api.copyRich(html || md, md.trim());
    toast("Answer copied", undefined, "ok");
  } catch (e) {
    toast(String(e), undefined, "bad");
  }
}

/** Keeps a prompt for ↑ in the message box: the last 200, a repeat moved to the end. */
function rememberPrompt(text: string) {
  const t = text.trim();
  const h = (settings.get().askHistory ?? []).filter((x) => x !== t);
  settings.update({ askHistory: [...h, t].slice(-200) });
}

function Composer({ c, found }: { c: Chat; found: CliInfo[] | null }) {
  const [text, setText] = useState("");
  const [pick, setPick] = useState<Pick | null>(null);
  const [models, setModels] = useState<DOMRect | null>(null);
  const [recall, setRecall] = useState<number | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const skills = useRef<Skill[] | null>(null);
  const pickReq = useRef(0);

  // "Ask about this note" types its prompt here.
  const pending = useStore(pendingPrompt);
  useEffect(() => {
    if (pending?.id === c.id) {
      setText(pending.text);
      pendingPrompt.set(null);
      const at = pending.text.length;
      requestAnimationFrame(() => {
        box.current?.focus();
        box.current?.setSelectionRange(at, at);
      });
    }
  }, [pending, c.id]);
  useEffect(() => box.current?.focus(), [c.id]);
  // Grow with the text, up to a point.
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, [text]);

  const update = async (v: string, caret: number) => {
    setText(v);
    setRecall(null);
    // Only the latest request's answer is shown, and none once the prompt is sent.
    const n = ++pickReq.current;
    const before = v.slice(0, caret);
    const sk = /^\/([\w-]*)$/.exec(before);
    if (sk) {
      skills.current ??= await api.skillsList().catch(() => []);
      if (n !== pickReq.current) return;
      const q = sk[1].toLowerCase();
      const items = groupSkills(skills.current.filter((s) => s.name.toLowerCase().includes(q)));
      return setPick(items.length ? { kind: "skill", items, i: 0 } : null);
    }
    const l = /\[\[([^\]\n]*)$/.exec(before);
    const t = /(^|\s)#([\w/-]*)$/.exec(before);
    const tr = l
      ? { kind: "link" as const, q: l[1], start: caret - l[0].length }
      : t
        ? { kind: "tag" as const, q: t[2], start: caret - t[2].length - 1 }
        : null;
    if (!tr) return setPick(null);
    const items = await suggest(tr.kind, tr.q).catch(() => []);
    if (n !== pickReq.current) return;
    setPick(items.length ? { kind: tr.kind, items, start: tr.start, i: 0 } : null);
  };

  const accept = (i: number) => {
    if (!pick) return;
    pickReq.current++;
    const el = box.current!;
    let v: string;
    let at: number;
    if (pick.kind === "skill" && pick.items[i].opens) {
      // Brainstead does this one on a screen of its own now.
      const to = pick.items[i].opens!;
      setText("");
      setPick(null);
      if (to === "fixname") openFixName();
      else nav.go(to as Screen);
      return;
    }
    if (pick.kind === "skill") {
      v = `/${pick.items[i].name} `;
      at = v.length;
    } else {
      const s = pick.items[i];
      v = text.slice(0, pick.start) + s.insert + text.slice(el.selectionStart);
      at = pick.start + s.insert.length;
    }
    setText(v);
    setPick(null);
    requestAnimationFrame(() => {
      el.setSelectionRange(at, at);
      el.focus();
    });
  };

  const hint = !c.busy && c.suggestion ? c.suggestion : null;
  const submit = () => {
    if (!text.trim()) return;
    send(c.id, text);
    rememberPrompt(text);
    pickReq.current++;
    setText("");
    setPick(null);
    setRecall(null);
  };

  // Every prompt sent, in any chat, oldest first (kept across restarts).
  const mine = settings.get().askHistory ?? [];
  const key = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (pick) {
      const n = pick.items.length;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        setPick({ ...pick, i: (pick.i + (e.key === "ArrowDown" ? 1 : -1) + n) % n });
        return e.preventDefault();
      }
      if (e.key === "Enter" || e.key === "Tab") {
        accept(pick.i);
        return e.preventDefault();
      }
      if (e.key === "Escape") {
        pickReq.current++;
        setPick(null);
        return e.preventDefault();
      }
    }
    // → (or Tab) in an empty box types the suggested next message, to edit or send.
    if ((e.key === "ArrowRight" || e.key === "Tab") && !text && hint) {
      setText(hint);
      return e.preventDefault();
    }
    // ↑ and ↓ step through the prompts sent before, in any chat: from an empty box, while going
    // through them, or ↑ with the caret at the very start.
    const atStart = e.currentTarget.selectionStart === 0 && e.currentTarget.selectionEnd === 0;
    if ((e.key === "ArrowUp" || e.key === "ArrowDown") && mine.length && (!text || recall !== null || (e.key === "ArrowUp" && atStart))) {
      pickReq.current++;
      const cur = recall ?? mine.length;
      const next = e.key === "ArrowUp" ? Math.max(0, cur - 1) : cur + 1;
      if (next >= mine.length) {
        setRecall(null);
        setText("");
      } else {
        setRecall(next);
        setText(mine[next]);
      }
      return e.preventDefault();
    }
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      submit();
      return e.preventDefault();
    }
  };

  const model = c.model ?? DEFAULT_MODEL;
  const locked = c.transcript.length > 0;
  return (
    <div className="askfoot">
      <div className="askcol">
        {!!c.queue.length && (
          <div className="askqueue">
            <span className="faint">Queued</span>
            {c.queue.map((q, i) => (
              <span key={i} className="chip" title={q}>
                <span className="t">{q}</span>
                <button
                  type="button"
                  className="ibtn xs"
                  aria-label="Remove from the queue"
                  title="Remove this message from the queue; it won't be sent"
                  onClick={() => unqueue(c.id, i)}
                >
                  <Icon name="x" size={11} />
                </button>
              </span>
            ))}
          </div>
        )}
        {pick && (
          <ul className="asksugg" role="listbox" aria-label={pick.kind === "skill" ? "Skills" : "Suggestions"}>
            {pick.kind === "skill"
              ? pick.items.map((s, i) => (
                  <li key={s.name}>
                    {(i === 0 || skillGroup(pick.items[i - 1].name) !== skillGroup(s.name)) && (
                      <div className="asgroup">{skillGroup(s.name)}</div>
                    )}
                    <button
                      type="button"
                      role="option"
                      aria-selected={i === pick.i}
                      className={i === pick.i ? "on" : ""}
                      onMouseDown={(e) => e.preventDefault()}
                      title={s.opens ? `Open the screen for /${s.name}` : `Use /${s.name} in this message`}
                      onClick={() => accept(i)}
                    >
                      <span className="nm">/{s.name}</span>
                      <span className="ds">{s.description}</span>
                      {s.source === "brainstead" && <span className="chip sm acc">Brainstead</span>}
                      {s.opens ? <span className="chip sm">Opens screen</span> : <span className="chip sm faint">Runs in chat</span>}
                    </button>
                  </li>
                ))
              : pick.items.map((s, i) => (
                  <li key={s.label}>
                    <button
                      type="button"
                      role="option"
                      aria-selected={i === pick.i}
                      className={i === pick.i ? "on" : ""}
                      onMouseDown={(e) => e.preventDefault()}
                      title={`Insert ${s.label}`}
                      onClick={() => accept(i)}
                    >
                      {s.label}
                    </button>
                  </li>
                ))}
          </ul>
        )}
        <div className="card askbox">
          <textarea
            ref={box}
            rows={1}
            value={text}
            placeholder={hint ? `${hint}   → to use it` : "Ask about your vault"}
            aria-label="Message"
            spellCheck
            onChange={(e) => void update(e.target.value, e.target.selectionStart)}
            onKeyDown={key}
          />
          <div className="askbar">
            <span
              className="chip"
              title="It never writes a file itself: its changes go through Brainstead, which records each in Changes, where you can revert it"
            >
              <Icon name="eye" size={12} />
              No direct edits
            </span>
            <span className="sp" />
            <span title={locked ? "Each chat keeps its model: start a new chat for another." : "Choose the model"}>
              <button
                type="button"
                className="btn sm"
                disabled={locked}
                onClick={(e) => setModels(e.currentTarget.getBoundingClientRect())}
              >
                <Icon name="cpu" size={12} />
                {modelLabel(model, found)}
                {!locked && <Icon name="chevdown" size={12} />}
              </button>
            </span>
            <button type="button" className="ibtn send" aria-label="Send" title="Send (↩)" disabled={!text.trim()} onClick={submit}>
              <Icon name="send" size={14} />
            </button>
          </div>
        </div>
        <div className="faint askhint">
          Runs {CLI_LABEL[cliOf(model)]} on this computer through Brainstead's tools; a chat stays in Brainstead until you Save it · / skill
          · [[ note · # tag · ↑ earlier · {keyLabel("↩ send · ⇧↩ new line")}
        </div>
      </div>
      {models && (
        <Popover anchor={models} onClose={() => setModels(null)} width={300} place="above">
          <ModelMenu
            found={found}
            value={model}
            onPick={(m) => {
              setModel(c.id, m);
              setModels(null);
            }}
          />
        </Popover>
      )}
    </div>
  );
}

export function ModelMenu({ found, value, onPick }: { found: CliInfo[] | null; value: string; onPick: (m: string) => void }) {
  if (!found) return <div className="menu faint mpad">Looking for assistants…</div>;
  const there = found.filter((c) => c.path);
  return (
    <div className="menu modelmenu" role="menu">
      {!there.length && (
        <div className="faint mpad">
          No assistant found. Install Claude Code, Codex, Antigravity or Copilot, sign in, then open Ask again.
        </div>
      )}
      {there.map((c) => (
        <div key={c.cli}>
          <div className="mg">
            {CLI_LABEL[c.cli]} {c.version && <span className="v">· {c.version}</span>}
          </div>
          {!c.models.length && <div className="faint mpad">No models listed: is it signed in?</div>}
          {c.models.map((m) => (
            <button
              key={m.id}
              type="button"
              role="menuitemradio"
              aria-checked={m.id === value}
              title={`Use ${m.name}`}
              onClick={() => onPick(m.id)}
            >
              <span style={{ width: 14 }}>{m.id === value && <Icon name="check" size={14} />}</span>
              {m.name}
            </button>
          ))}
        </div>
      ))}
      <div className="sep" />
      <div className="faint mpad">Each chat keeps its model.</div>
    </div>
  );
}

function HistoryDialog({ onClose }: { onClose: () => void }) {
  const [all, setAll] = useState<ChatSummary[] | null>(null);
  const [q, setQ] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const dq = useDebounced(q, 120);
  const load = () =>
    api
      .chatsList()
      .then(setAll)
      .catch(() => setAll([]));
  useEffect(() => void load(), []);
  const shown = (all ?? []).filter((c) => !dq || c.title.toLowerCase().includes(dq.toLowerCase()));
  const pinned = shown.filter((c) => c.state === "pinned");
  const rest = shown.filter((c) => c.state !== "pinned");

  const open = async (c: ChatSummary) => {
    try {
      await openSaved(c.filename);
      onClose();
    } catch (e) {
      toast(String(e), undefined, "bad");
    }
  };
  const act = (f: () => Promise<unknown>) => () => {
    f()
      .then(load)
      .catch((e) =>
        toast(typeof e === "object" && e && "message" in e ? String((e as { message: string }).message) : String(e), undefined, "bad"),
      );
  };
  const row = (c: ChatSummary) => (
    <li key={c.filename} className="hrow">
      {editing === c.filename ? (
        <input
          className="hedit"
          autoFocus
          defaultValue={c.title}
          aria-label="Chat title"
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              const t = e.currentTarget.value.trim();
              setEditing(null);
              if (t && t !== c.title) act(() => renameSaved(c, t))();
            } else if (e.key === "Escape") {
              e.stopPropagation();
              setEditing(null);
            }
          }}
          onBlur={() => setEditing(null)}
        />
      ) : (
        <button type="button" className="hopen" title="Open this chat" onClick={() => void open(c)}>
          <span className="t">{c.title}</span>
          <span className="faint">
            {c.model ? modelLabel(c.model, clis.get()) : "Claude Code"} · {ago(Date.parse(c.updatedAt) || 0)}
            {isSaved(c) ? " · saved" : " · not saved"}
          </span>
        </button>
      )}
      <button type="button" className="ibtn" aria-label="Rename" title="Rename" onClick={() => setEditing(c.filename)}>
        <Icon name="text" size={14} />
      </button>
      <button
        type="button"
        className="ibtn"
        aria-label={c.state === "pinned" ? "Unpin" : "Pin"}
        title={
          c.state === "pinned"
            ? `Unpin${isSaved(c) ? "" : " (unsaved chats beyond the latest 20 are deleted)"}`
            : "Pin: keep it whatever happens"
        }
        onClick={act(() => pinSaved(c, c.state !== "pinned"))}
      >
        <Icon name="pin" size={14} style={{ color: c.state === "pinned" ? "var(--accent)" : undefined }} />
      </button>
      {!isSaved(c) && (
        <button
          type="button"
          className="ibtn"
          aria-label="Save"
          title="Save this chat to the vault as a note"
          onClick={act(() => keepSaved(c))}
        >
          <Icon name="save" size={14} />
        </button>
      )}
      <button type="button" className="ibtn" aria-label="Move to the Trash" title="Move to the Trash" onClick={act(() => trashSaved(c))}>
        <Icon name="trash" size={14} />
      </button>
    </li>
  );
  return (
    <Dialog onClose={onClose} width={560} label="Chat history">
      <div className="row askhead">
        <h2 className="h2 grow">Chats</h2>
        <button type="button" className="ibtn" aria-label="Close" title="Close" onClick={onClose}>
          <Icon name="x" size={14} />
        </button>
      </div>
      <label className="inp hsearch">
        <Icon name="search" size={14} />
        <input autoFocus placeholder="Find a chat by title" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Find a chat" />
      </label>
      <div className="hlist">
        {!all && <p className="faint">Loading…</p>}
        {all && !shown.length && <p className="faint">{q ? "No chat has that in its title." : "No chats yet."}</p>}
        {!!pinned.length && (
          <>
            <div className="mg">Pinned</div>
            <ul>{pinned.map(row)}</ul>
          </>
        )}
        {!!rest.length && (
          <>
            {!!pinned.length && <div className="mg">Recent</div>}
            <ul>{rest.map(row)}</ul>
          </>
        )}
      </div>
    </Dialog>
  );
}

/** "Ask about this note": Ask, on a new chat whose message names the note, ready to finish. */
export function askAboutNote(path: string) {
  askWith(
    `About [[${path
      .replace(/\.(md|txt)$/i, "")
      .split("/")
      .pop()}]]: `,
  );
  nav.go("ask");
}
