// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Running a template from the window: the Env templater.ts needs (the vault through Rust), the
// questions a template asks as it runs (tp.system.prompt, suggester, multi_suggester), the
// template editor's Test run, and the link to Templater's documentation.

import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { Icon } from "../icons";
import { place } from "../nav";
import { settings } from "../store";
import { Dialog } from "../ui";
import { Ask, Env, PromptRequest, runTemplate, RunResult, StopRun, SuggestRequest, tfile } from "./templater";
import { isMod, keyLabel } from "../platform";

export const TEMPLATER_DOCS = "https://silentvoid13.github.io/Templater/";
export const DEFAULT_SCRIPTS = "Templates/scripts";

/** "Templater docs ↗": opens the documentation in the browser. */
export function TemplaterDocsLink({ className = "btn ghost sm" }: { className?: string }) {
  return (
    <button
      type="button"
      className={className}
      title="Templater's documentation, in your browser"
      onClick={() => void openUrl(TEMPLATER_DOCS).catch(() => {})}
    >
      Templater docs ↗
    </button>
  );
}

type Pending = { q: Ask; resolve: (a: string | number[] | null) => void; reject: (e: unknown) => void };

/** The question a running template is waiting on, and how to answer or stop it. */
export function useAsker() {
  const [pending, setPending] = useState<Pending | null>(null);
  const askAny = useCallback(
    (q: Ask) => new Promise<string | number[] | null>((resolve, reject) => setPending({ q, resolve, reject })),
    [],
  );
  const ask = askAny as Env["ask"];
  const answer = (a: string | number[] | null) => {
    pending?.resolve(a);
    setPending(null);
  };
  const stop = () => {
    pending?.reject(new StopRun());
    setPending(null);
  };
  return { ask, pending, answer, stop };
}

/** Runs a template's tp.hooks.on_all_templates_executed callbacks, once its note is made, as New
 *  note does (and an assistant's create_note): each on its own, and why any failed. */
export async function runHooks(hooks: (() => unknown)[]): Promise<string[]> {
  const ran = await Promise.all(
    hooks.map((h) =>
      Promise.resolve()
        .then(h)
        .then(
          () => null,
          (e: unknown) => `The template's finishing step failed: ${e instanceof Error ? e.message : String(e)}`,
        ),
    ),
  );
  return ran.filter((x): x is string => x !== null);
}

/** An Env over the open vault. `create` defaults to making the file (Test run passes its own). */
export async function vaultEnv(templatePath: string, ask: Env["ask"], create?: Env["create"]): Promise<Env> {
  const [files, scripts] = await Promise.all([
    api.filesList(null).then((fs) => fs.map((f) => f.path)),
    api.userScripts(settings.get().templateScripts || DEFAULT_SCRIPTS).catch(() => []),
  ]);
  const here = place.get().place;
  return {
    root: settings.get().vaultPath ?? "",
    files,
    read: async (p) => (await api.docRead(p)).content,
    create: create ?? (async (p, c) => void (await api.docCreate(p, c))),
    clipboard: () => api.clipboardRead(),
    ask,
    scripts,
    templateFile: tfile(templatePath),
    activeFile: here.screen === "doc" && here.path ? tfile(here.path) : null,
  };
}

/** One question from a running template, drawn in place. Enter answers; Cancel answers "no
 *  answer" (the template carries on, as Templater does); Stop ends the run. */
export function AskPanel({ q, onAnswer, onStop }: { q: Ask; onAnswer: (a: string | number[] | null) => void; onStop: () => void }) {
  return (
    <div className="tpask" role="group" aria-label="The template asks">
      <div className="eyebrow">The template asks</div>
      {q.kind === "prompt" ? <PromptAsk q={q} onAnswer={onAnswer} /> : <SuggestAsk q={q} onAnswer={onAnswer} />}
      <div className="row tpaskfoot">
        <button type="button" className="btn ghost sm" title="End the template run here, without asking anything more" onClick={onStop}>
          Stop
        </button>
        <span className="grow" />
        <button type="button" className="btn sm" title="Give no answer; the template carries on without one" onClick={() => onAnswer(null)}>
          Cancel
        </button>
      </div>
    </div>
  );
}

function PromptAsk({ q, onAnswer }: { q: PromptRequest; onAnswer: (a: string) => void }) {
  const [v, setV] = useState(q.value);
  const ref = useRef<HTMLInputElement & HTMLTextAreaElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    if (q.selectAll) el.select();
    else el.setSelectionRange(el.value.length, el.value.length);
  }, [q]);
  const key = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && (!q.multiline || isMod(e))) {
      e.preventDefault();
      e.stopPropagation();
      onAnswer(v);
    }
  };
  return (
    <>
      {q.text && <label className="tpq">{q.text}</label>}
      <span className="inp">
        {q.multiline ? (
          <textarea ref={ref} rows={5} value={v} onChange={(e) => setV(e.target.value)} onKeyDown={key} aria-label={q.text || "Answer"} />
        ) : (
          <input ref={ref} value={v} onChange={(e) => setV(e.target.value)} onKeyDown={key} aria-label={q.text || "Answer"} />
        )}
      </span>
      <div className="row">
        <span className="faint small">{keyLabel(q.multiline ? "⌘↩ to answer" : "↩ to answer")}</span>
        <span className="grow" />
        <button
          type="button"
          className="btn pri sm"
          title={keyLabel(`Answer with what's typed (${q.multiline ? "⌘↩" : "↩"})`)}
          onClick={() => onAnswer(v)}
        >
          OK
        </button>
      </div>
    </>
  );
}

function SuggestAsk({ q, onAnswer }: { q: SuggestRequest; onAnswer: (a: number[]) => void }) {
  const multi = q.kind === "multi";
  const [filter, setFilter] = useState("");
  const [chosen, setChosen] = useState<number[]>(q.chosen);
  const [active, setActive] = useState(q.chosen[0] ?? 0);
  const shown = useMemo(() => {
    const f = filter.trim().toLowerCase();
    const all = q.labels.map((l, i) => [l, i] as const).filter(([l]) => !f || l.toLowerCase().includes(f));
    return q.limit ? all.slice(0, q.limit) : all;
  }, [q, filter]);
  const at = Math.max(
    0,
    shown.findIndex(([, i]) => i === active),
  );
  const pick = (i: number) => (multi ? setChosen((c) => (c.includes(i) ? c.filter((x) => x !== i) : [...c, i])) : onAnswer([i]));
  const key = (e: React.KeyboardEvent) => {
    const d = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (d && shown.length) {
      e.preventDefault();
      e.stopPropagation();
      setActive(shown[(at + d + shown.length) % shown.length][1]);
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      if (multi && isMod(e)) onAnswer(chosen);
      else if (shown[at]) pick(shown[at][1]);
    }
  };
  return (
    <>
      {q.text && <label className="tpq">{q.text}</label>}
      <span className="inp">
        <Icon name="search" size={14} />
        <input
          autoFocus
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          onKeyDown={key}
          placeholder={multi ? "Filter" : q.text || "Choose"}
          aria-label="Filter"
        />
      </span>
      <div className="tpopts" role="listbox" aria-multiselectable={multi}>
        {shown.map(([label, i]) => (
          <button
            key={i}
            type="button"
            role="option"
            aria-selected={multi ? chosen.includes(i) : i === active}
            className={`tpopt ${i === active ? "on" : ""}`}
            onMouseEnter={() => setActive(i)}
            title={multi ? (chosen.includes(i) ? "Untick this choice" : "Tick this choice") : "Choose this answer"}
            onClick={() => pick(i)}
          >
            {multi && (
              <span className={`cb ${chosen.includes(i) ? "on" : ""}`}>{chosen.includes(i) && <Icon name="check" size={12} />}</span>
            )}
            {label}
          </button>
        ))}
        {!shown.length && <span className="faint small">Nothing matches.</span>}
      </div>
      <div className="row">
        <span className="faint small">{keyLabel(multi ? "↩ to tick · ⌘↩ when done" : "↑↓ and ↩ to choose")}</span>
        <span className="grow" />
        {multi && (
          <button
            type="button"
            className="btn pri sm"
            title={keyLabel("Answer with the ticked choices (⌘↩)")}
            onClick={() => onAnswer(chosen)}
          >
            Done ({chosen.length})
          </button>
        )}
      </div>
    </>
  );
}

/** The template editor's Test run: runs the template as New note would, asking its questions,
 *  and shows the note it would make. Nothing is written: tp.file.create_new is only listed. */
export function TestRun({ path, src, onClose }: { path: string; src: string; onClose: () => void }) {
  const asker = useAsker();
  const [out, setOut] = useState<{ r: RunResult; made: string[] } | { error: string } | null>(null);
  useEffect(() => {
    let live = true;
    const made: string[] = [];
    void vaultEnv(path, asker.ask, async (p) => void made.push(p))
      .then((env) => runTemplate(src, env))
      .then((r) => live && setOut({ r, made }))
      .catch((e) => {
        if (!live) return;
        if (e instanceof StopRun) onClose();
        else setOut({ error: e instanceof Error ? e.message : String(e) });
      });
    return () => {
      live = false;
    };
  }, [path, src]); // eslint-disable-line react-hooks/exhaustive-deps
  const r = out && "r" in out ? out.r : null;
  return (
    <Dialog onClose={asker.pending ? asker.stop : onClose} width={720} label="Test run">
      <div className="tprun">
        <div className="row">
          <h2 className="h2 grow">Test run</h2>
          <TemplaterDocsLink />
          <button
            type="button"
            className="ibtn"
            aria-label="Close"
            title={asker.pending ? "Stop the test run" : "Close"}
            onClick={asker.pending ? asker.stop : onClose}
          >
            <Icon name="x" />
          </button>
        </div>
        {asker.pending ? (
          <AskPanel q={asker.pending.q} onAnswer={asker.answer} onStop={asker.stop} />
        ) : !out ? (
          <span className="faint">Running…</span>
        ) : "error" in out ? (
          <div className="nnerr" role="alert">
            <Icon name="info" size={14} />
            <span>{out.error}</span>
          </div>
        ) : (
          r && (
            <>
              <div className="row nnname">
                <span className="faint">Would make</span>
                <span className="mono fname">{r.named ? r.path : `${r.path} (the template doesn't name it)`}</span>
              </div>
              {r.cursor !== null && <span className="faint small">The caret goes at character {r.cursor}.</span>}
              {out && "made" in out && out.made.length > 0 && <span className="faint small">Would also make: {out.made.join(", ")}</span>}
              <pre className="mono tpout">
                {r.cursor === null ? r.content : `${r.content.slice(0, r.cursor)}▍${r.content.slice(r.cursor)}`}
              </pre>
            </>
          )
        )}
      </div>
    </Dialog>
  );
}
