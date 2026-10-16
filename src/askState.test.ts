// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (c: string, a?: unknown) => invoke(c, a), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { groupTurns } from "./Ask";
import {
  applyDone,
  applyEvent,
  ask,
  blankChat,
  type Chat,
  cliOf,
  exchanges,
  closeChat,
  fallbackTitle,
  keepChat,
  modelLabel,
  newChat,
  recover,
  save,
  send,
  sessionOf,
  toSaved,
  unqueue,
} from "./askState";

describe("Ask state", () => {
  beforeEach(() => {
    ask.set({ chats: [], active: null });
    invoke.mockClear();
  });

  it("routes model ids to their CLI as Rust does", () => {
    expect(cliOf("claude:sonnet")).toBe("claude");
    expect(cliOf("claude-sonnet-5")).toBe("claude");
    expect(cliOf("agy:gemini-3")).toBe("antigravity");
    expect(cliOf("copilot:auto")).toBe("copilot");
    expect(cliOf("gpt-5.5")).toBe("codex");
    const found = [{ cli: "claude" as const, path: "/x", version: "2", models: [{ id: "claude:sonnet", name: "Sonnet" }] }];
    expect(modelLabel("claude:sonnet", found)).toBe("Claude Code · Sonnet");
    expect(modelLabel("gpt-5.5", null)).toBe("ChatGPT · gpt-5.5");
  });

  it("finishes a turn whose ask-done the window missed, with Rust's answer, once", async () => {
    const answer = { chatId: "", text: "Fixed it in two files.", session: "s1", error: null };
    let running = true;
    invoke.mockImplementation(async (cmd: string, a?: unknown) => {
      if (cmd === "ask_outcome")
        return running ? { running: true, done: null } : { running: false, done: { ...answer, chatId: (a as { chatId: string }).chatId } };
      if (cmd === "chat_save") return { filename: "local:Chat. Fix.md", title: "Fix", createdAt: "", updatedAt: "" };
      return undefined;
    });
    const id = newChat("claude:sonnet");
    send(id, "Fix the name");
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("ask_send", expect.anything()));
    await new Promise((r) => setTimeout(r));
    // Still answering in Rust: nothing changes.
    await recover(id);
    expect(ask.get().chats[0].busy).toBe(true);
    // The CLI has finished but its ask-done never came.
    running = false;
    await recover(id);
    const c = ask.get().chats[0];
    expect(c.busy).toBe(false);
    expect(c.transcript.filter((i) => i.role === "assistant").map((i) => i.text)).toEqual(["Fixed it in two files."]);
    expect(invoke).toHaveBeenCalledWith("ask_log", expect.anything());
    // A late check finds nothing to do.
    await recover(id);
    expect(ask.get().chats[0].transcript.filter((i) => i.role === "assistant")).toHaveLength(1);
    invoke.mockImplementation(async () => undefined);
  });

  it("titles a chat from its question until a better title comes", () => {
    expect(fallbackTitle("What's due on [[Orbit App|the app]] this week?")).toBe("What's due on Orbit App this week");
    expect(fallbackTitle("one two three four five six seven eight")).toBe("one two three four five six seven…");
    expect(fallbackTitle("  ")).toBe("New chat");
  });

  it("streams, notes what it read, and ends the turn into the transcript", () => {
    let c: Chat = { ...blankChat("claude:sonnet"), busy: true, streaming: "" };
    c = applyEvent(c, { kind: "status", text: "Reading Orbit App" });
    c = applyEvent(c, { kind: "status", text: "Reading Orbit App" });
    c = applyEvent(c, { kind: "text", text: "Two " });
    c = applyEvent(c, { kind: "text", text: "things." });
    c = applyEvent(c, { kind: "usage", tokens: 1200, window: null });
    c = applyEvent(c, { kind: "usage", tokens: 0, window: 200000 });
    c = applyEvent(c, { kind: "compacted" });
    expect(c.streaming).toBe("Two things.");
    expect(c.looked).toEqual(["Reading Orbit App"]);
    expect([c.contextTokens, c.contextWindow, c.compactions]).toEqual([1200, 200000, 1]);
    c = applyDone(c, { chatId: c.id, text: "Two things.", session: "s1", error: null });
    expect(c.transcript.map((i) => [i.role, i.text])).toEqual([
      ["tool", "Reading Orbit App"],
      ["assistant", "Two things."],
    ]);
    expect(c.busy).toBe(false);
    expect(c.claudeSessionId).toBe("s1");
    expect(c.sessionId).toBeNull();
    // Another CLI's session goes in sessionId, so the previous app never resumes it with Claude.
    const x = applyDone({ ...blankChat("gpt-5"), busy: true }, { chatId: "x", text: "", session: "t1", error: "quota" });
    expect([x.claudeSessionId, x.sessionId, sessionOf(x)]).toEqual([null, "t1", "t1"]);
    expect(x.transcript).toEqual([expect.objectContaining({ role: "error", text: "quota" })]);
  });

  it("saves one chat at a time, so Save during the first answer makes one note", async () => {
    const c: Chat = { ...blankChat("claude:sonnet"), title: "Orbit App", transcript: [{ id: "1", role: "user", text: "Hi" }] };
    ask.set({ chats: [c], active: c.id });
    const sent: string[] = [];
    invoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd !== "chat_save") return undefined;
      const a = args as { chat: { filename: string }; keep: boolean };
      sent.push(a.chat.filename);
      await new Promise((r) => setTimeout(r, a.keep ? 5 : 1));
      const filename = a.keep ? "Chat. Orbit App.md" : a.chat.filename || "local:Chat. Orbit App.md";
      return { filename, title: "Orbit App", createdAt: "t", updatedAt: "t" };
    });
    await Promise.all([keepChat(c.id), save(c.id)]);
    // The second write starts from the note the first made, and doesn't take it back to local.
    expect(sent).toEqual(["", "Chat. Orbit App.md"]);
    expect(ask.get().chats[0].filename).toBe("Chat. Orbit App.md");
    // A late reply naming a local copy never downgrades a saved chat.
    invoke.mockImplementation(async () => ({ filename: "local:Chat. Orbit App.md", title: "Orbit App", createdAt: "t", updatedAt: "t" }));
    await save(c.id);
    expect(ask.get().chats[0].filename).toBe("Chat. Orbit App.md");
    invoke.mockImplementation(async () => undefined);
  });

  it("saves only what belongs in the file", () => {
    const saved = toSaved({ ...blankChat("claude:opus"), busy: true, queue: ["x"], review: true });
    expect(Object.keys(saved).sort()).not.toContain("busy");
    expect(saved).not.toHaveProperty("queue");
    expect(saved).not.toHaveProperty("review");
    expect(saved.model).toBe("claude:opus");
    expect(toSaved({ ...blankChat("claude:opus"), suggestion: "And Lena?" })).not.toHaveProperty("suggestion");
  });

  it("pairs each question with its answer for the next-message suggestion", () => {
    const t = (role: "user" | "assistant" | "tool", text: string) => ({ id: text, role, text });
    expect(
      exchanges([t("user", "q1"), t("tool", "read"), t("assistant", "a1"), t("user", "q2"), t("assistant", "a2"), t("user", "q3")]),
    ).toEqual([
      ["q1", "a1"],
      ["q2", "a2"],
    ]);
  });

  it("queues messages while it answers and sends them in order", () => {
    const id = newChat("claude:sonnet");
    send(id, "first");
    expect(invoke).toHaveBeenCalledWith("ask_send", { chatId: id, prompt: "first", model: "claude:sonnet", session: null });
    send(id, "second");
    send(id, "third");
    expect(ask.get().chats[0].queue).toEqual(["second", "third"]);
    unqueue(id, 0);
    expect(ask.get().chats[0].queue).toEqual(["third"]);
    expect(ask.get().chats[0].title).toBe("first");
    // Closing the last tab opens an empty one, which a new chat reuses rather than stacking.
    closeChat(id);
    expect(ask.get().chats.map((c) => c.transcript.length)).toEqual([0]);
    const a = ask.get().chats[0].id;
    expect(newChat()).toBe(a);
    expect(newChat()).toBe(a);
  });

  it("groups what it looked at with the answer after it", () => {
    const t = groupTurns({
      transcript: [
        { id: "1", role: "user", text: "q" },
        { id: "2", role: "tool", text: "Searching the vault" },
        { id: "3", role: "assistant", text: "a" },
        { id: "4", role: "user", text: "q2" },
        { id: "5", role: "tool", text: "Reading X" },
        { id: "6", role: "error", text: "stopped" },
      ],
    });
    expect(t).toEqual([
      { role: "user", text: "q" },
      { role: "assistant", text: "a", looked: ["Searching the vault"] },
      { role: "user", text: "q2" },
      { role: "assistant", text: "", looked: ["Reading X"] },
      { role: "error", text: "stopped" },
    ]);
  });
});
