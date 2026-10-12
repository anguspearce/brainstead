// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Draft reply (the vault's `draft-reply` skill, §4): a captured Outlook or Teams thread (or a pasted
// one), whether it answered what was asked, and a plain-text reply grounded in the vault, in a
// tone you pick, to copy into Outlook or Teams. Brainstead never sends anything.

import { useEffect, useMemo, useState } from "react";
import { api, FileSummary, Reply } from "../api";
import { captureStamp } from "../Capture";
import { Icon } from "../icons";
import { fmtShortDate } from "../md/dates";
import { Markdown } from "../md/Markdown";
import { openDoc, place, useViewState } from "../nav";
import { settings, useStore } from "../store";
import { reportEditError } from "../taskModel";
import { toast } from "../Toast";
import { TopBar } from "../TopBar";
import { Seg } from "../ui";

type Tone = "brief" | "warm" | "formal";

/** Add task's task: the follow-up, linked to the captured thread it came from. */
export const replyTask = (task: string, thread: string | null) =>
  thread ? `${task} ([[${thread.split("/").pop()!.replace(/\.md$/, "")}]])` : task;

/** A captured thread: an Outlook email or a Teams chat in sources/ or a folder in it, by its name. */
export const isThread = (path: string) => /^sources\/(?:.+\/)?(Email\. |Teams\. Chat\. )/.test(path);

/** A thread as the picker names it: its subject and day, not its file name
 *  ("Email. Thread. Re Orbit App launch date - 2026-10-02" → "Re Orbit App launch date · Fri 2 Oct"). */
export function threadLabel(f: Pick<FileSummary, "title" | "date" | "path">): string {
  const subject =
    f.title
      .replace(/^(Email\. (Thread\. )?|Teams\. Chat\. )/, "")
      .replace(/\s+-\s+\d{4}-\d{2}-\d{2}$/, "")
      .trim() || f.title;
  const kind = /^sources\/(?:.+\/)?Teams\. /.test(f.path) ? "Teams: " : "";
  return `${kind}${subject}${f.date ? ` · ${fmtShortDate(f.date)}` : ""}`;
}

/** Whether the draft answered the thread, as the verdict above it says (an assistant's draft_reply too). */
export const ANSWERED: Record<string, [string, string]> = {
  yes: ["It answered the question", "green"],
  partly: ["Partly answered", "amber"],
  no: ["It didn't answer the question", "amber"],
};

export function ReplyScreen() {
  const here = useStore(place).place;
  const [threads, setThreads] = useState<FileSummary[]>([]);
  const [thread, setThread] = useViewState<string | null>("reply.thread", null);
  const [pasted, setPasted] = useState("");
  const [tone, setTone] = useViewState<Tone>("reply.tone", "brief");
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [r, setR] = useState<Reply | null>(null);
  const [draft, setDraft] = useState("");
  const chosen = here.path ?? thread;
  const root = useStore(settings).vaultPath ?? "";

  useEffect(() => {
    void api
      .filesList("source")
      .then((f) => setThreads(f.filter((x) => isThread(x.path)).sort((a, b) => b.mtime - a.mtime)))
      .catch(() => {});
  }, []);
  useEffect(() => {
    setR(null);
    if (!chosen) return setText("");
    void api
      .docRead(chosen)
      .then((d) => setText(d.content))
      .catch(() => setText(""));
  }, [chosen]);

  const run = async (t: Tone = tone) => {
    setBusy(true);
    setError("");
    try {
      const out = await api.draftReply(chosen, chosen ? null : pasted, t);
      setR(out);
      setDraft(out.draft);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const title = useMemo(() => {
    const f = threads.find((t) => t.path === chosen);
    return f ? threadLabel(f) : chosen ? chosen.split("/").pop() : null;
  }, [threads, chosen]);
  const copy = async () => {
    await navigator.clipboard.writeText(draft);
    toast("Reply copied: paste it into Outlook or Teams", undefined, "ok");
  };
  const copyRich = async () => {
    const html = draft
      .split(/\n{2,}/)
      .map((p) => `<p>${p.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/\n/g, "<br>")}</p>`)
      .join("");
    await navigator.clipboard.write([
      new ClipboardItem({ "text/html": new Blob([html], { type: "text/html" }), "text/plain": new Blob([draft], { type: "text/plain" }) }),
    ]);
    toast("Reply copied as rich text", undefined, "ok");
  };
  const addTask = async () => {
    if (!r?.task) return;
    try {
      toast(await api.capture("task", replyTask(r.task, chosen), captureStamp()), undefined, "ok");
    } catch (e) {
      reportEditError(e);
    }
  };

  return (
    <main className="main">
      <TopBar title="Draft reply" sub={title ?? undefined} />
      <div className="body reply">
        <section className="rthread">
          <div className="row">
            <span className="eyebrow grow">Thread</span>
            <select className="sel" aria-label="Captured thread" value={chosen ?? ""} onChange={(e) => setThread(e.target.value || null)}>
              <option value="">Paste a thread…</option>
              {threads.map((t) => (
                <option key={t.path} value={t.path}>
                  {threadLabel(t)}
                </option>
              ))}
            </select>
          </div>
          {chosen ? (
            <div className="card rtext">
              <div className="rthreadmd">
                <Markdown content={text} path={chosen} root={root} />
              </div>
              <button type="button" className="btn sm ghost" title={`Open the captured thread, ${chosen}`} onClick={() => openDoc(chosen)}>
                Open the capture
              </button>
            </div>
          ) : (
            <textarea
              className="card rpaste"
              placeholder="Paste the thread here: the messages, newest last, with who said what."
              value={pasted}
              onChange={(e) => setPasted(e.target.value)}
            />
          )}
          {r && r.answered !== "n/a" && ANSWERED[r.answered] && (
            <div className={`rverdict ${ANSWERED[r.answered][1]}`}>
              <b>{ANSWERED[r.answered][0]}</b>
              <div>{r.verdict}</div>
              {r.callouts.length > 0 && (
                <ul>
                  {r.callouts.map((c, i) => (
                    <li key={i}>
                      <span className={`chip ${c.status === "answered" ? "green" : c.status === "partly" ? "amber" : "red"}`}>
                        {c.status}
                      </span>{" "}
                      {c.point}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </section>
        <section className="card rdraft">
          <div className="row">
            <span className="eyebrow grow">Drafted reply</span>
            <Seg<Tone>
              label="Tone"
              value={tone}
              onChange={(t) => {
                setTone(t);
                if (r) void run(t);
              }}
              options={[
                ["brief", "Brief"],
                ["warm", "Warm"],
                ["formal", "Formal"],
              ]}
            />
            <span
              title={
                busy
                  ? "The model is drafting the reply"
                  : !chosen && !pasted.trim()
                    ? "Pick a captured thread or paste one first"
                    : r
                      ? "Draft the reply again, replacing this one"
                      : "Have the model draft a reply to the thread, drawing on your vault"
              }
            >
              <button type="button" className="btn pri" disabled={busy || (!chosen && !pasted.trim())} onClick={() => void run()}>
                <Icon name={r ? "refresh" : "ask"} size={14} />
                {busy ? "Drafting…" : r ? "Redraft" : "Draft"}
              </button>
            </span>
          </div>
          {error && <p className="err">{error}</p>}
          {!r && !busy && (
            <p className="faint">The draft reads the thread, checks what it asked, and looks up the people and topics in your vault.</p>
          )}
          {r && (
            <>
              <textarea className="rbody" value={draft} onChange={(e) => setDraft(e.target.value)} aria-label="The reply" />
              {r.gaps.length > 0 && (
                <div className="rgaps">
                  <span className="eyebrow">To fill in</span>
                  {r.gaps.map((g, i) => (
                    <div key={i} className="small">
                      {g}
                    </div>
                  ))}
                </div>
              )}
              {r.grounded.length > 0 && <p className="faint small">Drew on {r.grounded.join(", ")}.</p>}
              <div className="row rbtns">
                <button
                  type="button"
                  className="btn pri"
                  title="Copy the reply as plain text, to paste into Outlook or Teams"
                  onClick={() => void copy()}
                >
                  <Icon name="copy" size={14} />
                  Copy reply
                </button>
                <button
                  type="button"
                  className="btn"
                  title="Copy the reply with its paragraphs kept, for an email"
                  onClick={() => void copyRich()}
                >
                  Copy as rich text
                </button>
                {r.task && (
                  <button
                    type="button"
                    className="btn"
                    title={chosen ? "Capture this follow-up as a task, linked to the captured thread" : "Capture this follow-up as a task"}
                    onClick={() => void addTask()}
                  >
                    <Icon name="tasks" size={14} />
                    Add task: {r.task}
                  </button>
                )}
              </div>
              <p className="faint small">Paste into Outlook or Teams: Brainstead never sends anything.</p>
            </>
          )}
        </section>
      </div>
    </main>
  );
}
