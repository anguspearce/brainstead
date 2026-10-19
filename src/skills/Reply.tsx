// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Draft reply (the vault's `draft-reply` skill, §4): a captured Outlook or Teams thread (or a pasted
// one), whether it answered what was asked, and a plain-text reply grounded in the vault, in a
// tone you pick, to copy into Outlook or Teams. Brainstead never sends anything.

import { keepInSources, StillInInbox, useInInbox } from "../StillInInbox";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, FileSummary, Reply } from "../api";
import { captureStamp } from "../Capture";
import { Icon } from "../icons";
import { fmtShortDate } from "../md/dates";
import { Markdown } from "../md/Markdown";
import { nav, openDoc, place, useViewState } from "../nav";
import { useVaultVersion, vaultVersion } from "../state";
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
  // Changes asked for since the draft: each revises the reply as it stands (its edits in it), and
  // the one before is kept so Undo puts it back. A fresh draft starts them over.
  const [asked, setAsked] = useState<string[]>([]);
  const [before, setBefore] = useState<{ r: Reply; draft: string }[]>([]);
  const [change, setChange] = useState("");
  const chosen = here.path ?? thread;
  const root = useStore(settings).vaultPath ?? "";

  const vv = useVaultVersion();
  useEffect(() => {
    void api
      .filesList("source")
      .then((f) => setThreads(f.filter((x) => isThread(x.path)).sort((a, b) => b.mtime - a.mtime)))
      .catch(() => {});
  }, [vv]);
  // Each draft or change asked for is numbered; an answer that comes back after another thread was
  // chosen (or a newer ask) is dropped rather than shown against the wrong thread.
  const ask = useRef(0);
  // The thread is read, and the draft, asked changes and Undo start over, when another thread is
  // chosen or the thread's own file changes, not when anything else in the vault does.
  const [read, setRead] = useState(0);
  const seen = useRef(vv);
  useEffect(() => {
    if (seen.current === vv) return;
    seen.current = vv;
    const c = vaultVersion.get().changes;
    if (chosen && (!c || c.changed.includes(chosen) || c.removed.includes(chosen))) setRead((n) => n + 1);
  }, [vv]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    ask.current++;
    setBusy(false);
    setR(null);
    setAsked([]);
    setBefore([]);
    if (!chosen) return setText("");
    void api
      .docRead(chosen)
      .then((d) => setText(d.content))
      .catch(() => {
        // Gone (trashed or moved while it was remembered): back to choosing a thread.
        setText("");
        setThread(null);
        const at = place.get().place;
        if (at.screen === "reply" && at.path === chosen) nav.replace({ ...at, path: undefined });
        toast(`${chosen.split("/").pop()} isn't there any more: choose another thread or paste one`, undefined, "bad");
      });
  }, [chosen, read]); // eslint-disable-line react-hooks/exhaustive-deps

  const run = async (t: Tone = tone) => {
    const n = ++ask.current;
    setBusy(true);
    setError("");
    try {
      const out = await api.draftReply(chosen, chosen ? null : pasted, t);
      if (ask.current !== n) return;
      setR(out);
      setDraft(out.draft);
      setAsked([]);
      setBefore([]);
    } catch (e) {
      if (ask.current === n) setError(String(e));
    } finally {
      if (ask.current === n) setBusy(false);
    }
  };
  const revise = async () => {
    const want = change.trim();
    if (!want || !r || busy) return;
    const n = ++ask.current;
    setBusy(true);
    setError("");
    try {
      const out = await api.draftReply(chosen, chosen ? null : pasted, tone, draft, want);
      if (ask.current !== n) return;
      setBefore((b) => [...b, { r, draft }]);
      setAsked((a) => [...a, want]);
      setR(out);
      setDraft(out.draft);
      setChange("");
    } catch (e) {
      if (ask.current === n) setError(String(e));
    } finally {
      if (ask.current === n) setBusy(false);
    }
  };
  const undo = () => {
    const last = before[before.length - 1];
    if (!last) return;
    setR(last.r);
    setDraft(last.draft);
    setBefore((b) => b.slice(0, -1));
    setAsked((a) => a.slice(0, -1));
  };
  const title = useMemo(() => {
    const f = threads.find((t) => t.path === chosen);
    return f ? threadLabel(f) : chosen ? chosen.split("/").pop() : null;
  }, [threads, chosen]);
  // Still in the Inbox, the toast after copying offers to take it out (as the strip below does).
  const inInbox = useInInbox(chosen);
  const offerOut = (msg: string) =>
    toast(msg, inInbox && chosen ? { label: "Take it out of the Inbox", run: () => void keepInSources(chosen) } : undefined, "ok");
  const copy = async () => {
    await navigator.clipboard.writeText(draft);
    offerOut("Reply copied: paste it into Outlook or Teams");
  };
  const copyRich = async () => {
    const html = draft
      .split(/\n{2,}/)
      .map((p) => `<p>${p.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/\n/g, "<br>")}</p>`)
      .join("");
    await navigator.clipboard.write([
      new ClipboardItem({ "text/html": new Blob([html], { type: "text/html" }), "text/plain": new Blob([draft], { type: "text/plain" }) }),
    ]);
    offerOut("Reply copied as rich text");
  };
  const addTask = async () => {
    if (!r?.task) return;
    try {
      toast(await api.capture("task", replyTask(r.task, chosen), captureStamp()), undefined, "ok");
    } catch (e) {
      reportEditError(e);
    }
  };

  // The tone and Draft: in the empty reply's card until there's a draft, then above it.
  const controls = (
    <>
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
    </>
  );

  return (
    <main className="main">
      <TopBar title="Draft reply" sub={title ?? undefined} />
      <div className="body reply">
        <section className="rthread">
          <div className="row">
            <span className="eyebrow grow">Thread</span>
            <select
              className="sel"
              aria-label="Captured thread"
              title="The thread to reply to: an email or Teams chat captured into Sources, or Paste a thread… to paste one in"
              value={chosen ?? ""}
              onChange={(e) => setThread(e.target.value || null)}
            >
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
        <section className="rdraft">
          <div className="row">
            <span className="eyebrow grow">Drafted reply</span>
            {r && controls}
          </div>
          {error && <p className="err">{error}</p>}
          {/* The reply's card, level with the thread's, both as tall as the window lets them be. */}
          {r ? (
            <textarea className="card rbody" value={draft} onChange={(e) => setDraft(e.target.value)} aria-label="The reply" />
          ) : (
            <div className="card rbody rempty">
              {busy ? (
                <p className="faint">
                  <span className="spin" /> Drafting the reply…
                </p>
              ) : (
                <div className="rstart">
                  <p className="muted">
                    The draft reads the thread, checks what it asked, and looks up the people and topics in your vault. Choose a tone:
                  </p>
                  <div className="row">{controls}</div>
                </div>
              )}
            </div>
          )}
          {r && (
            <>
              {/* Ask for a change, as in Ask: it revises the reply above. */}
              {asked.length > 0 && (
                <div className="rasked">
                  {asked.map((a, i) => (
                    <span key={i} className="chip">
                      {a}
                    </span>
                  ))}
                  <button type="button" className="btn sm ghost" title="Put back the reply as it was before the last change" onClick={undo}>
                    <Icon name="undo" size={12} />
                    Undo
                  </button>
                </div>
              )}
              <div className="card askbox">
                <textarea
                  rows={1}
                  value={change}
                  placeholder="Ask for a change: shorter, mention the date, less formal…"
                  aria-label="Ask for a change to the reply"
                  spellCheck
                  disabled={busy}
                  onChange={(e) => setChange(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      void revise();
                    }
                  }}
                />
                <div className="askbar">
                  <span className="faint small">
                    {busy ? "Revising the reply…" : "It revises the reply above, keeping your edits · ↩ send · ⇧↩ new line"}
                  </span>
                  <span className="sp" />
                  <button
                    type="button"
                    className="ibtn send"
                    aria-label="Send"
                    title="Revise the reply as asked (↩)"
                    disabled={!change.trim() || busy}
                    onClick={() => void revise()}
                  >
                    <Icon name="send" size={14} />
                  </button>
                </div>
              </div>
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
              {chosen && <StillInInbox key={chosen} path={chosen} />}
            </>
          )}
        </section>
      </div>
    </main>
  );
}
