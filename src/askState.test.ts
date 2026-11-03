// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

const invoke = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (c: string, a?: unknown) => invoke(c, a), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { listen } from "@tauri-apps/api/event";
import cases from "../tests/fixtures/job-settings.json";
import type { CliInfo, CliName, Settings } from "./api";
import { groupTurns } from "./Ask";
import {
  applyDone,
  applyEvent,
  ask,
  blankChat,
  type Chat,
  cliOf,
  clis,
  effortLabel,
  effortFor,
  exchanges,
  followEffortSettings,
  jobModelOf,
  jobOwnEffort,
  MODEL_JOBS,
  resolveEffort,
  closeChat,
  fallbackTitle,
  keepChat,
  modelLabel,
  newChat,
  recover,
  save,
  send,
  sessionOf,
  setEffort,
  setJobModel,
  setModel,
  startAsk,
  toSaved,
  unqueue,
} from "./askState";
import { settings } from "./store";

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
    const found = [{ cli: "claude" as const, path: "/x", version: "2", models: [{ id: "claude:sonnet", name: "Sonnet" }], efforts: [] }];
    expect(modelLabel("claude:sonnet", found)).toBe("Claude Code · Sonnet");
    expect(modelLabel("gpt-5.5", null)).toBe("ChatGPT · gpt-5.5");
    expect(["low", "xhigh", "max"].map(effortLabel)).toEqual(["Low", "Extra high", "Max"]);
  });

  it("resolves a run's effort: the job's, else the assistant's, else none", () => {
    // The same cases as brainstead_core::ask's effort_resolves_job_then_assistant.
    const claude = ["low", "medium", "high", "xhigh", "max"];
    const codex = ["low", "medium", "high", "xhigh"];
    expect(resolveEffort(claude, "high", "low")).toBe("high");
    expect(resolveEffort(claude, null, "low")).toBe("low");
    expect(resolveEffort(claude, null, null)).toBeNull();
    expect(resolveEffort(codex, "max", "medium")).toBe("medium");
    expect(resolveEffort(codex, "max", null)).toBeNull();
  });

  it("never starts a new chat at a job's effort", () => {
    settings.update({ effort: { claude: "low" }, jobEffort: { weekprep: "max", summaries: "high" }, askModel: "claude:sonnet" });
    expect(jobOwnEffort("weekprep")).toBe("max");
    expect(effortFor("claude:sonnet")).toBe("low");
    expect(blankChat("claude:sonnet").effort).toBe("low");
    settings.update({ effort: {} });
    expect(blankChat("claude:sonnet").effort).toBeNull();
    settings.update({ jobEffort: undefined, askModel: undefined });
  });

  /** The assistants found, each installed with the shared cases' levels. */
  const found = (): CliInfo[] =>
    (Object.keys(cases.levels) as CliName[]).map((cli) => ({
      cli,
      path: `/bin/${cli}`,
      version: "1",
      models: [],
      efforts: cases.levels[cli],
    }));

  it("works out a job's model and effort as Rust does (tests/fixtures/job-settings.json)", () => {
    expect(MODEL_JOBS.map(([job]) => job)).toEqual(cases.jobs);
    clis.set(found());
    for (const c of cases.cases) {
      const s = c.settings as unknown as Settings;
      const model = jobModelOf(c.job, s);
      expect(model, c.name).toBe(c.model);
      expect(jobOwnEffort(c.job, s), c.name).toBe(c.own);
      // What brainstead_core::ask::job_effort gives the run.
      const cli = cliOf(model);
      expect(resolveEffort(cases.levels[cli], s.jobEffort?.[c.job], s.effort?.[cli]), c.name).toBe(c.effort);
    }
    clis.set(null);
  });

  it("drops a job's own effort only when its own model moves to an assistant without it", () => {
    clis.set(found());
    const levels = { weekprep: "max", find: "max", clarify: "high" };
    settings.update({ askModel: "claude:sonnet", jobModels: {}, jobEffort: levels });
    // A model picked in a chat becomes New chats use; the jobs that follow it keep their levels set.
    const id = newChat("claude:sonnet");
    setModel(id, "gpt-5.5");
    expect(ask.get().chats.find((c) => c.id === id)!.model).toBe("gpt-5.5");
    expect(settings.get().askModel).toBe("gpt-5.5");
    expect(settings.get().jobEffort).toEqual(levels);
    settings.update({ askModel: "claude:sonnet" });
    // A model a job inherits (the summaries', for weekprep) sets its level aside until it comes back.
    expect(setJobModel("summaries", "gpt-5")).toBeNull();
    expect(jobOwnEffort("weekprep")).toBeNull();
    expect(settings.get().jobEffort).toEqual(levels);
    setJobModel("summaries", null);
    expect(jobOwnEffort("weekprep")).toBe("max");
    // Its own model on an assistant without the level drops it; one with the level keeps it.
    expect(setJobModel("find", "gpt-5")).toBe("max");
    expect(setJobModel("clarify", "gpt-5")).toBeNull();
    expect(settings.get().jobEffort).toEqual({ weekprep: "max", clarify: "high" });
    setJobModel("find", null);
    expect(jobOwnEffort("find")).toBeNull();
    settings.update({ askModel: undefined, jobModels: undefined, jobEffort: undefined });
    clis.set(null);
  });

  it("keeps an empty tab on the effort set for its assistant until one is picked in it", async () => {
    clis.set(found());
    settings.update({ effort: {} });
    const stop = followEffortSettings();
    const id = newChat("claude:sonnet");
    const effort = () => ask.get().chats.find((c) => c.id === id)!.effort;
    expect(effort()).toBeNull();
    settings.update({ effort: { claude: "high" } });
    expect(effort()).toBe("high");
    await setEffort(id, "low");
    settings.update({ effort: { claude: "max" } });
    expect(effort()).toBe("low");
    stop();
    settings.update({ effort: undefined });
    clis.set(null);
  });

  it("runs a scheduled run's tab at its job's effort, and keeps it until the run ends", async () => {
    const handlers = new Map<string, (e: { payload: unknown }) => void>();
    vi.mocked(listen).mockImplementation(async (name, h) => {
      handlers.set(name, h as (e: { payload: unknown }) => void);
      return () => {};
    });
    settings.update({ effort: { claude: "low" } });
    vi.useFakeTimers();
    const stop = await startAsk();
    // Its listeners, its timer and its following of the effort settings end with this test.
    onTestFinished(() => {
      stop();
      vi.useRealTimers();
      vi.mocked(listen).mockImplementation(async () => () => {});
    });
    handlers.get("review-started")!({
      payload: { chatId: "summary-1", title: "Daily summary", model: "claude:sonnet", effort: "high", prompt: "Daily summary for today" },
    });
    const run = () => ask.get().chats.find((c) => c.id === "summary-1")!;
    expect(run().effort).toBe("high");
    await expect(setEffort("summary-1", "max")).rejects.toThrow(/still running/);
    expect(run().effort).toBe("high");
    expect(invoke).not.toHaveBeenCalledWith("chat_save", expect.anything());
    settings.update({ effort: undefined });
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

  it("starts a chat at its assistant's effort from Settings, and changes the chat's own only", async () => {
    settings.update({ effort: { claude: "high" } });
    const id = newChat("claude:sonnet");
    expect(ask.get().chats.find((c) => c.id === id)!.effort).toBe("high");
    // Another model on the same assistant keeps the chat's level; another assistant starts from its own.
    await setEffort(id, "low");
    setModel(id, "claude:opus");
    expect(ask.get().chats.find((c) => c.id === id)!.effort).toBe("low");
    setModel(id, "gpt-5.5");
    expect(ask.get().chats.find((c) => c.id === id)!.effort).toBeNull();
    // The model goes to New chats use; the assistants' effort settings stay as they are.
    expect(settings.get().askModel).toBe("gpt-5.5");
    expect(settings.get().effort).toEqual({ claude: "high" });
    setModel(id, "claude:sonnet");
    await setEffort(id, "max");
    send(id, "first");
    expect(invoke).toHaveBeenCalledWith("ask_send", expect.objectContaining({ chatId: id, effort: "max" }));
    expect(settings.get().effort).toEqual({ claude: "high" });
    settings.update({ effort: undefined, askModel: undefined });
  });

  it("queues messages while it answers and sends them in order", () => {
    const id = newChat("claude:sonnet");
    send(id, "first");
    expect(invoke).toHaveBeenCalledWith("ask_send", { chatId: id, prompt: "first", model: "claude:sonnet", session: null, effort: null });
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
