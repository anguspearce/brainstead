// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// New note (⌘N): a blank note named from type, title and date, or one made from a template in
// `Templates/`. A template runs when you create the note, here in the window (templater.ts), and
// asks its questions in this dialog as it reaches them; Rust only creates the file.

import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { Icon } from "../icons";
import { localToday } from "../md/taskQuery";
import { openDoc } from "../nav";
import { reportEditError } from "../taskModel";
import { toast } from "../Toast";
import { DateField } from "../DatePicker";
import { Dialog } from "../ui";
import { closeNoteDialog, setNewNoteCursor } from "./dialogs";
import { composeFilename } from "./filename";
import { checkTemplate, freeName, runTemplate, StopRun } from "./templater";
import { AskPanel, runHooks, TemplaterDocsLink, useAsker, vaultEnv } from "./TemplateRun";
import { isMod, keyLabel } from "../platform";

export interface LoadedTemplate {
  path: string;
  name: string;
  src: string;
  /** Why the template doesn't compile; null when it does. */
  error: string | null;
}

/** Reads a template and checks that it compiles (it only runs when a note is made). One that
 *  can't be read is listed with the reason, not left out. */
export async function loadTemplate(path: string, name: string): Promise<LoadedTemplate> {
  try {
    const src = (await api.docRead(path)).content;
    return { path, name, src, error: checkTemplate(src) };
  } catch (e) {
    return { path, name, src: "", error: `it couldn't be read (${e instanceof Error ? e.message : String(e)})` };
  }
}

/** What a template will do, in plain words, read from its code without running it: the questions
 *  its `tp.system.prompt("…")` calls ask, how many lists it offers, and whether it names the note. */
export function templateSummary(src: string): { questions: string[]; lists: number; names: boolean } {
  const questions = [...src.matchAll(/tp\.system\.prompt\(\s*(["'`])((?:\\.|(?!\1)[^\\])*)\1/g)].map((m) => m[2].replace(/\\(.)/g, "$1"));
  const lists = (src.match(/tp\.system\.(?:multi_)?suggester\(/g) ?? []).length;
  return { questions, lists, names: /tp\.file\.(?:rename|move)\(/.test(src) };
}

const BLANK = "";

export function NewNoteDialog({ initial }: { initial?: string }) {
  const [templates, setTemplates] = useState<LoadedTemplate[] | null>(null);
  const [existing, setExisting] = useState<Set<string>>(new Set());
  const [sel, setSel] = useState<string>(BLANK);
  const [blank, setBlank] = useState({ type: "", title: "", date: localToday() });
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [showCode, setShowCode] = useState(false);
  const [types, setTypes] = useState<string[]>([]);
  const asker = useAsker();

  useEffect(() => {
    let live = true;
    api
      .filesList(null)
      .then(async (all) => {
        if (!live) return;
        setExisting(new Set(all.map((f) => f.path.toLowerCase())));
        setTypes([...new Set(all.map((f) => f.type).filter((t): t is string => !!t))].sort((a, b) => a.localeCompare(b)));
        const ts = all.filter((f) => f.layer === "template" && /\.md$/i.test(f.path));
        const loaded = await Promise.all(ts.map((t) => loadTemplate(t.path, t.title)));
        loaded.sort((a, b) => a.name.localeCompare(b.name));
        if (!live) return;
        setTemplates(loaded);
        const want = initial && loaded.find((t) => t.name === initial || t.path === initial);
        if (want) setSel(want.path);
      })
      .catch((e) => {
        if (!live) return;
        setTemplates([]);
        setLoadErr(e instanceof Error ? e.message : String(e));
      });
    return () => {
      live = false;
    };
  }, [initial]);

  const tpl = templates?.find((t) => t.path === sel) ?? null;
  useEffect(() => setErr(null), [sel]);

  const filename = tpl ? "" : composeFilename(blank);
  const taken = !!filename && existing.has(filename.toLowerCase());
  const canCreate = !busy && (tpl ? !tpl.error : !!filename && !taken);
  const createTip = busy
    ? "Creating the note…"
    : tpl
      ? tpl.error
        ? "This template has an error, so it can't be run"
        : keyLabel("Run the template and open the note it makes (⌘↩)")
      : taken
        ? "There's already a note with this name"
        : !filename
          ? "Give the note a title first"
          : keyLabel("Create the note and open it (⌘↩)");

  const create = async () => {
    if (!canCreate) return;
    setBusy(true);
    setErr(null);
    try {
      if (!tpl) {
        await api.docCreate(filename, "");
        closeNoteDialog();
        openDoc(filename);
        return;
      }
      const r = await runTemplate(tpl.src, await vaultEnv(tpl.path, asker.ask));
      // A note the template didn't name is Untitled, as Templater makes it; a named one must be free.
      const path = r.named ? r.path : freeName(r.path, (p) => existing.has(p.toLowerCase()));
      if (r.named && existing.has(path.toLowerCase())) {
        setErr(`There's already a note called ${path}.`);
        setBusy(false);
        return;
      }
      await api.docCreate(path, r.content);
      if (r.cursor !== null) setNewNoteCursor(path, r.cursor);
      closeNoteDialog();
      openDoc(path);
      void runHooks(r.hooks).then((failed) => failed.forEach((f) => toast(f, undefined, "bad")));
      if (r.open.length) toast(`Also made ${r.open.join(", ")}`);
    } catch (e) {
      setBusy(false);
      if (e instanceof StopRun) return;
      if (e instanceof Error) setErr(e.message);
      else reportEditError(e);
    }
  };

  const list = useRef<HTMLDivElement>(null);
  const options = [BLANK, ...(templates ?? []).map((t) => t.path)];
  // ↑↓ choose the template from anywhere in the dialog, the fields included (a select keeps its
  // own arrows). From the list, focus follows the choice.
  const onArrows = (e: React.KeyboardEvent) => {
    const d = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (!d || e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
    if ((e.target as HTMLElement).closest("select, textarea")) return;
    e.preventDefault();
    const i = Math.max(0, Math.min(options.length - 1, options.indexOf(sel) + d));
    setSel(options[i]);
    // Keep the keys in the dialog: the field typed in may go with the old template, which would
    // leave focus nowhere. The chosen template takes it, unless a field of the new one did.
    const from = e.target as HTMLElement;
    requestAnimationFrame(() => {
      const a = document.activeElement;
      const inForm = a && a !== document.body && list.current?.closest("form")?.contains(a);
      if (from.closest(".nnside") || !inForm) list.current?.querySelectorAll<HTMLButtonElement>("button")[i]?.focus();
    });
  };
  const running = !!asker.pending || (busy && !!tpl);

  return (
    <Dialog onClose={asker.pending ? asker.stop : closeNoteDialog} width={900} label="New note">
      <form
        className="newnote"
        onSubmit={(e) => {
          e.preventDefault();
          void create();
        }}
        onKeyDown={(e) => {
          if (asker.pending) return;
          if (e.key === "Enter" && isMod(e)) {
            e.preventDefault();
            void create();
          } else onArrows(e);
        }}
      >
        <div className="nnside" ref={list} role="listbox" aria-label="Template">
          <span className="eyebrow">Template</span>
          <button
            type="button"
            role="option"
            aria-selected={sel === BLANK}
            className={`nnt ${sel === BLANK ? "on" : ""}`}
            title="Start from an empty note: pick its type, title and date"
            onClick={() => setSel(BLANK)}
            disabled={running}
          >
            <b>Blank note</b>
            <span className="faint">Type, title and date</span>
          </button>
          {templates === null && <span className="faint nnhint">Reading templates…</span>}
          {loadErr && (
            <span className="nnbad nnhint" role="alert">
              <Icon name="info" size={12} /> The templates couldn't be read: {loadErr}
            </span>
          )}
          {templates?.map((t) => (
            <button
              key={t.path}
              type="button"
              role="option"
              aria-selected={sel === t.path}
              className={`nnt ${sel === t.path ? "on" : ""}`}
              title={`Start from the ${t.name} template`}
              onClick={() => setSel(t.path)}
              disabled={running}
            >
              <b>{t.name}</b>
              {t.error ? (
                <span className="nnbad">
                  <Icon name="info" size={12} /> Doesn't run
                </span>
              ) : (
                <span className="faint">Templater</span>
              )}
            </button>
          ))}
          <span className="faint nnhint">↑↓ to choose · Templates live in the vault’s Templates folder</span>
        </div>
        <div className="nnmain">
          <div className="row">
            <h2 className="h2 grow">{tpl ? `New ${tpl.name} note` : "New note"}</h2>
            <button
              type="button"
              className="ibtn"
              aria-label="Close"
              title={asker.pending ? "Stop the template and close" : "Close without creating a note"}
              onClick={asker.pending ? asker.stop : closeNoteDialog}
            >
              <Icon name="x" />
            </button>
          </div>
          {asker.pending ? (
            <AskPanel q={asker.pending.q} onAnswer={asker.answer} onStop={asker.stop} />
          ) : tpl ? (
            <div className="nnprev">
              {tpl.error ? (
                <div className="nnerr" role="alert">
                  <Icon name="info" size={14} />
                  <span>This template doesn't run: {tpl.error}</span>
                </div>
              ) : (
                <>
                  <TemplateSummary src={tpl.src} />
                  <button
                    type="button"
                    className="btn ghost sm nncode"
                    aria-expanded={showCode}
                    title={showCode ? "Hide the template's code" : "Show the template's code"}
                    onClick={() => setShowCode(!showCode)}
                  >
                    <Icon name={showCode ? "chevup" : "chevdown"} size={12} />
                    {showCode ? "Hide code" : "Show code"}
                  </button>
                  {showCode && <pre className="mono">{tpl.src}</pre>}
                </>
              )}
            </div>
          ) : (
            <>
              <div className="nnfields">
                <label className="nnf">
                  <span>Type</span>
                  <span className="inp">
                    <input
                      value={blank.type}
                      placeholder="Meeting"
                      list="nn-types"
                      onChange={(e) => setBlank({ ...blank, type: e.target.value })}
                    />
                  </span>
                  <datalist id="nn-types">
                    {types.map((t) => (
                      <option key={t} value={t} />
                    ))}
                  </datalist>
                </label>
                <label className="nnf">
                  <span>Title</span>
                  <span className="inp">
                    <input autoFocus value={blank.title} onChange={(e) => setBlank({ ...blank, title: e.target.value })} />
                  </span>
                </label>
                <div className="nnf">
                  <span>Date</span>
                  <DateField value={blank.date || null} label="Date" onChange={(d) => setBlank({ ...blank, date: d ?? "" })} />
                </div>
              </div>
              <div className="row nnname">
                <span className="faint">Filename</span>
                {filename ? <span className="mono fname">{filename}</span> : <span className="faint">—</span>}
                {filename && (taken ? <span className="chip red">Already exists</span> : <span className="chip green">Available</span>)}
              </div>
            </>
          )}
          {err && (
            <div className="nnerr" role="alert">
              <Icon name="info" size={14} />
              <span>{err}</span>
            </div>
          )}
          {!asker.pending && (
            <div className="row nnfoot">
              <TemplaterDocsLink />
              <span className="grow" />
              <button type="button" className="btn lg" title="Close without creating a note" onClick={closeNoteDialog}>
                Cancel
              </button>
              <span title={createTip}>
                <button type="submit" className="btn pri lg" disabled={!canCreate}>
                  {busy ? "Creating…" : "Create"} <span className="kbd">{keyLabel("⌘↩")}</span>
                </button>
              </span>
            </div>
          )}
        </div>
      </form>
    </Dialog>
  );
}

/** A template's questions and naming, said plainly above its code. */
function TemplateSummary({ src }: { src: string }) {
  const { questions, lists, names } = templateSummary(src);
  return (
    <div className="nnsum small">
      {questions.length > 0 ? (
        <>
          <span className="faint">When you create the note, it asks:</span>
          <ul>
            {questions.map((q, i) => (
              <li key={i}>{q}</li>
            ))}
          </ul>
        </>
      ) : (
        !lists && <span className="faint">It asks no questions.</span>
      )}
      {lists > 0 && (
        <span className="faint">
          {questions.length ? "It also offers" : "It offers"} {lists === 1 ? "a list" : `${lists} lists`} to choose from.{" "}
        </span>
      )}
      <span className="faint">{names ? "It names the note itself." : "The note starts as Untitled; rename it afterwards."}</span>
    </div>
  );
}
