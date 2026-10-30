// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Ask's state: the open chats (tabs), each with its transcript, the answer streaming in, what the
// model is doing, messages queued while it works, and whether it has news you haven't seen.
// Rust runs the CLIs (src-tauri/src/ask.rs) and saves the chats; this keeps the tabs in settings
// (askTabs) so they come back after a restart, and writes a chat after every turn: to the app
// data folder until Save puts it in the vault.

import { api, AskDone, AskEvent, ChatItem, ChatSummary, CliInfo, CliName, SavedChat } from "./api";
import { nav, place } from "./nav";
import { settings, Store } from "./store";

export interface Chat extends SavedChat {
  /** The answer so far, while one is coming. */
  streaming: string | null;
  /** What it's doing now ("Reading Orbit App"), and everything it looked at this turn. */
  status: string | null;
  looked: string[];
  busy: boolean;
  /** Messages typed while it was answering, sent in order. */
  queue: string[];
  unread: boolean;
  /** A scheduled review's run, rather than a chat you started. */
  review?: boolean;
  /** A message you might send next, offered in the box after an answer. */
  suggestion?: string | null;
}

export const CLI_LABEL: Record<CliName, string> = {
  claude: "Claude Code",
  codex: "ChatGPT (Codex)",
  antigravity: "Antigravity (Google)",
  copilot: "GitHub Copilot",
};

/** Which CLI a model id runs on, as Rust routes it (brainstead_core::ask::Cli::route). */
export function cliOf(model: string): CliName {
  if (model.startsWith("agy:")) return "antigravity";
  if (model.startsWith("copilot:")) return "copilot";
  if (model.startsWith("claude")) return "claude";
  return "codex";
}

export const DEFAULT_MODEL = "claude:sonnet";

/** An effort level's name, as Settings › AI assistants shows it ("xhigh" is "Extra high"). */
export function effortLabel(level: string): string {
  return level === "xhigh" ? "Extra high" : level.charAt(0).toUpperCase() + level.slice(1);
}

/** The background jobs that can each have a model (Settings › AI assistants › Models by job): id, label, icon. */
export const MODEL_JOBS: [job: string, label: string, icon: string][] = [
  ["summaries", "Daily and weekly summaries", "calendar"],
  ["weekprep", "Weekly review preparation", "review"],
  ["find", "Find tasks and projects", "zap"],
  ["ingest", "Ingest", "wiki"],
  ["meeting", "Meeting notes from transcripts", "note"],
  ["clarify", "Clarify suggestions", "inbox"],
  ["contradictions", "Contradiction checks", "shield"],
  ["skills", "Triage, Draft reply and Doc check", "send"],
  ["suggest", "Next-message suggestions in Ask", "ask"],
];

/** "Claude Code · Sonnet" for a model id, from the CLIs found. */
export function modelLabel(model: string, clis: CliInfo[] | null): string {
  const cli = cliOf(model);
  const m = clis?.find((c) => c.cli === cli)?.models.find((x) => x.id === model);
  const name = m?.name ?? model.replace(/^(claude:|agy:|copilot:)/, "");
  return `${CLI_LABEL[cli].replace(/ \(.*\)$/, "")} · ${name}`;
}

export const NEW_TITLE = "New chat";

/** A chat's title until a better one is made: the start of its first question. */
export function fallbackTitle(question: string): string {
  const words = question
    .replace(/\[\[([^\]|]*)(\|[^\]]*)?\]\]/g, "$1")
    .replace(/[/\\:*?"<>|#`]/g, " ")
    .split(/\s+/)
    .filter(Boolean);
  const t = words.slice(0, 7).join(" ");
  return t ? (words.length > 7 ? `${t}…` : t) : NEW_TITLE;
}

const uid = () => (typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`);

/** The effort a new chat on `model` starts with: the one set for its assistant in Settings › AI
 *  assistants, or null for the assistant's own default. */
export function effortFor(model: string): string | null {
  return settings.get().effort?.[cliOf(model)] ?? null;
}

export function blankChat(model: string): Chat {
  return {
    filename: "",
    id: uid(),
    title: NEW_TITLE,
    claudeSessionId: null,
    sessionId: null,
    model,
    effort: effortFor(model),
    createdAt: "",
    updatedAt: "",
    contextTokens: null,
    contextWindow: null,
    compactions: 0,
    state: "archived",
    transcript: [],
    streaming: null,
    status: null,
    looked: [],
    busy: false,
    queue: [],
    unread: false,
  };
}

export function fromSaved(c: SavedChat): Chat {
  return { ...c, streaming: null, status: null, looked: [], busy: false, queue: [], unread: false, review: c.kind === "review" };
}

/** What Rust saves: the chat without what only lives while it's open. */
export function toSaved(c: Chat): SavedChat {
  const { streaming: _s, status: _st, looked: _l, busy: _b, queue: _q, unread: _u, review: _r, suggestion: _n, ...saved } = c;
  return saved;
}

/** The session the chat's CLI resumes. */
export const sessionOf = (c: Chat) => (cliOf(c.model ?? DEFAULT_MODEL) === "claude" ? c.claudeSessionId : c.sessionId);

/** A stream event applied to a chat. */
export function applyEvent(c: Chat, e: AskEvent): Chat {
  switch (e.kind) {
    case "text":
      return { ...c, streaming: (c.streaming ?? "") + e.text, status: null };
    case "status":
      return { ...c, status: e.text, looked: c.looked.includes(e.text) ? c.looked : [...c.looked, e.text] };
    case "usage":
      return { ...c, contextTokens: e.tokens > 0 ? e.tokens : c.contextTokens, contextWindow: e.window ?? c.contextWindow };
    case "compacted":
      return { ...c, compactions: c.compactions + 1 };
  }
}

/** The end of a turn: what it looked at and its answer (or the error) join the transcript. */
export function applyDone(c: Chat, d: AskDone): Chat {
  const items: ChatItem[] = c.looked.map((t) => ({ id: uid(), role: "tool", text: t }));
  const text = d.text || c.streaming || "";
  if (text.trim()) items.push({ id: uid(), role: "assistant", text, meta: c.model ?? undefined });
  if (d.error) items.push({ id: uid(), role: "error", text: d.error });
  const claude = cliOf(c.model ?? DEFAULT_MODEL) === "claude";
  return {
    ...c,
    transcript: [...c.transcript, ...items],
    claudeSessionId: claude ? (d.session ?? c.claudeSessionId) : c.claudeSessionId,
    sessionId: claude ? c.sessionId : (d.session ?? c.sessionId),
    streaming: null,
    status: null,
    looked: [],
    busy: false,
  };
}

// --- the store -----------------------------------------------------------------------------

export interface AskState {
  chats: Chat[];
  active: string | null;
}

export const ask = new Store<AskState>({ chats: [], active: null });

/** The review run whose tab should be selected when it opens (Run now). */
let wanted: string | null = null;

function setChats(f: (cs: Chat[]) => Chat[], active?: string | null) {
  const s = ask.get();
  const chats = f(s.chats);
  const a = active === undefined ? s.active : active;
  ask.set({ chats, active: chats.some((c) => c.id === a) ? a : (chats[chats.length - 1]?.id ?? null) });
  remember();
}

function update(id: string, f: (c: Chat) => Chat) {
  setChats((cs) => cs.map((c) => (c.id === id ? f(c) : c)));
}

/** The tabs, kept in settings so they reopen after a restart. */
function remember() {
  const s = ask.get();
  const tabs = s.chats.filter((c) => c.filename || c.transcript.length).map((c) => ({ id: c.id, filename: c.filename }));
  const cur = settings.get();
  if (JSON.stringify(cur.askTabs ?? []) !== JSON.stringify(tabs) || cur.askActive !== (s.active ?? undefined)) {
    settings.update({ askTabs: tabs, askActive: s.active ?? undefined });
  }
}

/** The window is in front of you: closing it only hides it, and a hidden page can still say it's
 *  visible, so focus is asked too. */
export const shown = () => document.visibilityState !== "hidden" && document.hasFocus();

const seeing = (id: string) => place.get().place.screen === "ask" && ask.get().active === id && shown();

/** The capture the Inbox sent to Ask about it, and the chat that's about it, for Ask's Still in the
 *  Inbox strip. Any new chat (⌘T, or asking about something else) clears it. */
export const askedFromInbox = new Store<{ path: string; chat: string } | null>(null);

export function newChat(model = settings.get().askModel ?? DEFAULT_MODEL): string {
  askedFromInbox.set(null);
  // An empty new tab is reused rather than stacked.
  const empty = ask.get().chats.find((c) => !c.filename && !c.transcript.length && !c.busy && !c.review);
  if (empty) {
    setChats((cs) => cs.map((c) => (c.id === empty.id ? { ...c, model, effort: effortFor(model) } : c)), empty.id);
    return empty.id;
  }
  const c = blankChat(model);
  setChats((cs) => [...cs, c], c.id);
  return c.id;
}

export function selectChat(id: string) {
  setChats((cs) => cs.map((c) => (c.id === id ? { ...c, unread: false } : c)), id);
}

export function closeChat(id: string) {
  const c = ask.get().chats.find((x) => x.id === id);
  if (c?.busy) void api.askCancel(id);
  setChats((cs) => cs.filter((x) => x.id !== id));
  // The last tab closed: a new empty one, so Ask is always ready to type in.
  if (!ask.get().chats.length) newChat(c?.model ?? undefined);
}

export function setModel(id: string, model: string) {
  // Another assistant has its own levels, so the chat starts again from the one set for it.
  const same = (c: Chat) => cliOf(c.model ?? DEFAULT_MODEL) === cliOf(model);
  update(id, (c) => (c.transcript.length ? c : { ...c, model, effort: same(c) ? c.effort : effortFor(model) }));
  settings.update({ askModel: model });
}

/** This chat's effort from its next message on (null: the assistant's own default). Settings › AI
 *  assistants, which new chats start from, isn't changed. */
export async function setEffort(id: string, effort: string | null) {
  update(id, (c) => ({ ...c, effort }));
  if (ask.get().chats.find((c) => c.id === id)?.transcript.length) await save(id);
}

/** Opens a saved chat in a tab (or goes to its tab). */
export async function openSaved(filename: string) {
  const open = ask.get().chats.find((c) => c.filename === filename);
  if (open) return selectChat(open.id);
  const c = fromSaved(await api.chatRead(filename));
  setChats((cs) => [...cs.filter((x) => x.id !== c.id), c], c.id);
}

/** Sends a message, or queues it while the chat is answering. */
export function send(id: string, text: string) {
  const t = text.trim();
  if (!t) return;
  const c = ask.get().chats.find((x) => x.id === id);
  if (!c) return;
  if (c.busy) return update(id, (x) => ({ ...x, queue: [...x.queue, t] }));
  start(id, t);
}

export function unqueue(id: string, i: number) {
  update(id, (c) => ({ ...c, queue: c.queue.filter((_, j) => j !== i) }));
}

function start(id: string, text: string) {
  update(id, (c) => ({
    ...c,
    model: c.model ?? DEFAULT_MODEL,
    title: c.transcript.length || c.title !== NEW_TITLE ? c.title : fallbackTitle(text),
    transcript: [...c.transcript, { id: uid(), role: "user", text }],
    busy: true,
    streaming: "",
    status: null,
    looked: [],
    suggestion: null,
  }));
  const c = ask.get().chats.find((x) => x.id === id)!;
  api
    .askSend(id, text, c.model ?? DEFAULT_MODEL, sessionOf(c), c.effort ?? null)
    .then(() => sent.add(id))
    .catch((e) => finish({ chatId: id, text: "", session: null, error: String(e) }));
}

/** The chats whose turn Rust has started, so it knows how the turn stands (recover). */
const sent = new Set<string>();

/** Stop: the CLI is stopped, and a chat whose answer had already ended without the window hearing
 *  of it finishes at once. */
export function stop(id: string) {
  void api.askCancel(id).then(() => recover(id));
}

/** Finishes a chat still answering whose turn Rust says has ended: its `ask-done` was missed (one
 *  stayed on Writing… after the CLI had finished). Checked on Stop, and every few seconds while a
 *  chat answers. */
export async function recover(id: string) {
  if (!sent.has(id) || !ask.get().chats.find((c) => c.id === id)?.busy) return;
  const t = await api.askOutcome(id).catch(() => null);
  if (!t || t.running || !sent.has(id)) return;
  void api.askLog(`chat ${id}: the window missed the answer's end, so took it from Rust`).catch(() => {});
  await finish(
    t.done ?? { chatId: id, text: "", session: null, error: "This answer's end was lost, so Brainstead stopped waiting for it." },
  );
}

async function finish(d: AskDone) {
  const before = ask.get().chats.find((c) => c.id === d.chatId);
  sent.delete(d.chatId);
  // Each turn ends once: an ask-done that arrives after recover has finished it is ignored.
  if (!before?.busy) return;
  try {
    update(d.chatId, (c) => ({ ...applyDone(c, d), unread: !seeing(c.id) }));
  } catch (e) {
    void api.askLog(`chat ${d.chatId}: couldn't finish the answer: ${String(e)}`).catch(() => {});
    update(d.chatId, (c) => ({ ...c, busy: false, streaming: null, status: null }));
    return;
  }
  await save(d.chatId);
  const c = ask.get().chats.find((x) => x.id === d.chatId);
  if (!c) return;
  // After the first exchange, a proper title.
  const users = c.transcript.filter((i) => i.role === "user");
  const answer = c.transcript.find((i) => i.role === "assistant");
  if (users.length === 1 && answer && !c.review) void retitle(c.id, users[0].text, answer.text);
  if (c.queue.length) {
    const [next, ...rest] = c.queue;
    update(c.id, (x) => ({ ...x, queue: rest }));
    start(c.id, next);
  } else if (!c.review && !d.error) void suggest(c.id);
}

/** The pairs of question and answer in a transcript, oldest first. */
export function exchanges(items: ChatItem[]): [string, string][] {
  const out: [string, string][] = [];
  let q: string | null = null;
  for (const i of items) {
    if (i.role === "user") q = i.text;
    else if (i.role === "assistant" && q !== null) {
      out.push([q, i.text]);
      q = null;
    }
  }
  return out;
}

/** Asks for a next message to offer, and keeps it only if the chat hasn't moved on meanwhile. */
async function suggest(id: string) {
  if (settings.get().askSuggest === false) return;
  const c = ask.get().chats.find((x) => x.id === id);
  if (!c) return;
  const at = c.transcript.length;
  const s = await api.askNext(c.title, exchanges(c.transcript)).catch(() => null);
  if (s) update(id, (x) => (x.busy || x.transcript.length !== at ? x : { ...x, suggestion: s }));
}

/** Each chat's writes, one at a time: a write starts from the chat as the one before left it, so
 *  Save while the first answer finishes can't make two files, or put a saved chat back to local. */
const writes = new Map<string, Promise<unknown>>();

function queued<T>(id: string, job: () => Promise<T>): Promise<T> {
  const prev = writes.get(id) ?? Promise.resolve();
  const next = prev.catch(() => undefined).then(job);
  writes.set(id, next);
  void next
    .catch(() => undefined)
    .then(() => {
      if (writes.get(id) === next) writes.delete(id);
    });
  return next;
}

/** The chat with what Rust wrote: a reply never takes a saved chat back to the app data folder. */
function written(id: string, s: ChatSummary) {
  update(id, (x) =>
    isSaved(x) && !isSaved(s) ? x : { ...x, filename: s.filename, title: s.title, createdAt: s.createdAt, updatedAt: s.updatedAt },
  );
}

/** Writes a chat where it is (the app data folder until it's saved); Rust names a new one and says
 *  where it went. */
export function save(id: string): Promise<void> {
  return queued(id, async () => {
    const c = ask.get().chats.find((x) => x.id === id);
    if (!c || !c.transcript.length) return;
    try {
      written(id, await api.chatSave(toSaved(c)));
    } catch (e) {
      console.error("chat save", e);
    }
  });
}

/** A chat is saved once it's a note in the vault. */
export const isSaved = (c: { filename: string }) => !!c.filename && !c.filename.startsWith("local:");

/** Save: the chat goes to the vault as a note, and its later turns keep it up to date there.
 *  Returns the note's file name. */
export function keepChat(id: string): Promise<string> {
  return queued(id, async () => {
    const c = ask.get().chats.find((x) => x.id === id);
    if (!c?.transcript.length) throw new Error("There's nothing in this chat to save yet.");
    const s: ChatSummary = await api.chatSave(toSaved(c), true);
    written(id, s);
    return s.filename;
  });
}

async function retitle(id: string, user: string, assistant: string) {
  const t = await api.chatTitle(user, assistant).catch(() => null);
  const c = ask.get().chats.find((x) => x.id === id);
  if (!t || !c?.filename || t === c.title) return;
  await rename(id, t);
}

export async function rename(id: string, title: string) {
  const c = ask.get().chats.find((x) => x.id === id);
  if (!c) return;
  if (!c.filename) return update(id, (x) => ({ ...x, title }));
  const filename = await api.chatRename(c.filename, title);
  update(id, (x) => ({
    ...x,
    filename,
    title: filename
      .replace(/^local:/, "")
      .replace(/^Chat\. /, "")
      .replace(/\.md$/, ""),
  }));
}

export async function setPinned(id: string, pinned: boolean) {
  update(id, (c) => ({ ...c, state: pinned ? "pinned" : "archived" }));
  await save(id);
}

/** Starts listening for answers and reopens the tabs from last time. */
export async function startAsk() {
  await api.onAskEvent(({ chatId, event }) => update(chatId, (c) => applyEvent(c, event)));
  await api.onAskDone((d) => {
    // A review run's turn is finished and saved by Rust (review-done).
    if (ask.get().chats.find((c) => c.id === d.chatId)?.review) return;
    void finish(d);
  });
  // A missed ask-done doesn't leave a chat answering for ever.
  setInterval(() => {
    for (const c of ask.get().chats) if (c.busy && !c.review) void recover(c.id);
  }, 5000);
  // A scheduled review opens as a tab while it runs, without taking you to Ask.
  await api.onReviewStarted((r) => {
    const c: Chat = {
      ...blankChat(r.model),
      id: r.chatId,
      title: r.title,
      review: true,
      busy: true,
      streaming: "",
      transcript: [{ id: uid(), role: "user", text: r.prompt }],
    };
    // Run now selects its tab; a scheduled run opens beside the one you're in.
    const pick = wanted === c.id || !ask.get().active;
    if (wanted === c.id) wanted = null;
    setChats((cs) => [...cs.filter((x) => x.id !== c.id), c], pick ? c.id : ask.get().active);
  });
  await api.onReviewDone(async (r) => {
    if (!r.filename) return update(r.chatId, (c) => ({ ...c, busy: false, streaming: null, status: null }));
    try {
      const saved = fromSaved(await api.chatRead(r.filename));
      update(r.chatId, () => ({ ...saved, id: r.chatId, review: true, unread: !seeing(r.chatId) }));
    } catch {
      update(r.chatId, (c) => ({ ...c, busy: false }));
    }
  });
  // A screenshot scene sets its own chats.
  if (settings.frozen) return;
  const s = settings.get();
  const chats: Chat[] = [];
  for (const t of s.askTabs ?? []) {
    if (!t.filename) continue;
    try {
      chats.push(fromSaved(await api.chatRead(t.filename)));
    } catch {
      // Gone (renamed or trashed elsewhere): the tab isn't reopened.
    }
  }
  // A review that started while the tabs were being read is already open: it stays.
  const now = ask.get();
  const all = [...chats, ...now.chats.filter((c) => !chats.some((x) => x.id === c.id))];
  const has = (id: string | null | undefined) => !!id && all.some((c) => c.id === id);
  const active = has(s.askActive) ? s.askActive! : has(now.active) ? now.active : (all[all.length - 1]?.id ?? null);
  ask.set({ chats: all, active });
}

/** Selects a review run's tab: now if it's open, else as soon as it opens. */
export function selectReviewRun(chatId: string) {
  if (ask.get().chats.some((c) => c.id === chatId)) selectChat(chatId);
  else wanted = chatId;
}

/** Opens a review run's chat in a tab and goes to Ask. */
export async function openReviewRun(filename: string) {
  await openSaved(filename);
  const c = ask.get().chats.find((x) => x.filename === filename);
  if (c) update(c.id, (x) => ({ ...x, review: true }));
  nav.go("ask");
}

/** Opens Ask on a new chat with `text` typed in (not sent), for "Ask about this note". */
export const pendingPrompt = new Store<{ id: string; text: string } | null>(null);
export function askWith(text: string) {
  const id = newChat();
  pendingPrompt.set({ id, text });
}

/** The CLIs found on this computer, looked up once (it takes a moment). */
export const clis = new Store<CliInfo[] | null>(null);
let looking: Promise<void> | null = null;
export function findClis(again = false) {
  if (looking && !again) return looking;
  looking = api
    .askClis()
    .then((c) => clis.set(c))
    .catch(() => clis.set([]));
  return looking;
}

/** Saved-chat actions from History, keeping an open tab in step. */
export async function renameSaved(c: ChatSummary, title: string) {
  const open = ask.get().chats.find((x) => x.filename === c.filename);
  if (open) return rename(open.id, title);
  await api.chatRename(c.filename, title);
}

export async function pinSaved(c: ChatSummary, pin: boolean) {
  const open = ask.get().chats.find((x) => x.filename === c.filename);
  if (open) return setPinned(open.id, pin);
  const full = await api.chatRead(c.filename);
  await api.chatSave({ ...full, state: pin ? "pinned" : "archived" });
}

export async function effortSaved(c: ChatSummary, effort: string | null) {
  const open = ask.get().chats.find((x) => x.filename === c.filename);
  if (open) return setEffort(open.id, effort);
  const full = await api.chatRead(c.filename);
  await api.chatSave({ ...full, effort });
}

export async function keepSaved(c: ChatSummary) {
  const open = ask.get().chats.find((x) => x.filename === c.filename);
  if (open) return void (await keepChat(open.id));
  await api.chatSave(await api.chatRead(c.filename), true);
}

export async function trashSaved(c: ChatSummary) {
  await api.chatTrash(c.filename);
  const open = ask.get().chats.find((x) => x.filename === c.filename);
  if (open) closeChat(open.id);
}
