// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Doc check (the vault's `doc-check` skill, §4): checks a document against the version of a
// governing document in force, taken from the canonical docs register (`Me. Canonical Docs.md`),
// never from a file name, and says which version it used and which it left out. A second mode
// checks whether a revision took in the user's earlier feedback. It reports; it changes nothing
// but a note you choose to save.

import { useEffect, useMemo, useState } from "react";
import { api, CanonicalEntry, CanonicalRegister, CheckResult, CheckRun, FileSummary, Finding, SavedCheck } from "../api";
import { Icon } from "../icons";
import { openDoc, place, useViewState } from "../nav";
import { useStore } from "../store";
import { docChecks, docChecksUnseen, mergeRun, seenDocChecks, startDocChecks } from "../docChecks";
import { reportEditError } from "../taskModel";
import { toast } from "../Toast";
import { TopBar } from "../TopBar";
import { ago, SearchPick, Seg } from "../ui";
import { CompareTabs } from "./CompareTabs";
import { ASSISTANT_FRONTMATTER } from "../md/scripts";

type Mode = "standard" | "callouts";

/** The canonical docs register, and what Start the register makes it with (doc_check too). */
export const REGISTER = "Me. Canonical Docs.md";
export const REGISTER_STUB = `# Canonical docs

Which version of each governing document is in force. Doc check reads this table: one row per version; status is canonical (in force), draft or superseded; path is in the vault, \`~/…\` or a full path.

| key | title | version | status | path | aliases |
|---|---|---|---|---|---|
`;

const KIND: Record<string, [string, string]> = {
  diverges: ["Conflict", "red"],
  "not-covered": ["Not covered", "amber"],
  "superseded-term": ["Superseded term", "amber"],
  "beyond-scope": ["Beyond scope", ""],
  aligned: ["Agrees", "green"],
};

/** A register row's status as the screen names it. */
export const STATUS: Record<string, [string, string]> = {
  canonical: ["In force", "green"],
  draft: ["Draft", "amber"],
  superseded: ["Superseded", ""],
};

/** Where Save as note puts a check of the document `name` (its file name), dated today. */
export function findingsNote(name: string, d = new Date()): string {
  const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  return `Doc check. ${name.replace(/\.[^.]+$/, "").replace(/[\\/:*?"<>|#^[\]]/g, " ")} - ${date}.md`;
}

/** The findings as markdown, for Copy findings and Save as note. */
export function findingsMarkdown(name: string, r: CheckResult): string {
  const lines = [`# Doc check: ${name}`, ""];
  if (r.against) lines.push(`Checked against ${r.against.title} ${r.against.version} (in force).`);
  if (r.excluded.length)
    lines.push(`Not used: ${r.excluded.map((e) => `${e.title} ${e.version} (${STATUS[e.status]?.[0] ?? e.status})`).join(", ")}.`);
  lines.push("", `**${r.verdict}**`, "", r.summary, "");
  for (const f of r.findings.filter((x) => x.kind !== "aligned")) {
    lines.push(`## ${KIND[f.kind]?.[0] ?? f.kind}${f.material ? " (material)" : ""}: ${f.title}`, "");
    if (f.where) lines.push(`*${f.where}*`, "");
    if (f.candidate) lines.push(`> This document: ${f.candidate}`, "");
    if (f.canonical) lines.push(`> In force: ${f.canonical}`, "");
  }
  const ok = r.findings.filter((x) => x.kind === "aligned");
  if (ok.length) lines.push(`## Agrees`, "", ...ok.map((f) => `- ${f.title}`), "");
  return lines.join("\n");
}

/** The governing documents the picker offers, one per key (its version in force where the register
 *  has one), in the register's order: the first is chosen until the user picks another (doc_check
 *  without governing_document takes it too). */
export function governingDocs(reg: CanonicalRegister | null): CanonicalEntry[] {
  const m = new Map<string, CanonicalEntry>();
  for (const e of reg?.entries ?? []) if (!m.has(e.key) || e.status === "canonical") m.set(e.key, e);
  return [...m.values()];
}

export function DocCheckScreen() {
  const here = useStore(place).place;
  const [reg, setReg] = useState<CanonicalRegister | null>(null);
  const [sources, setSources] = useState<FileSummary[]>([]);
  const [cand, setCand] = useViewState<string | null>("doccheck.candidate", null);
  const [doc, setDoc] = useViewState<string>("doccheck.doc", "");
  const [mode, setMode] = useViewState<Mode>("doccheck.mode", "standard");
  const [error, setError] = useState("");
  const [showOk, setShowOk] = useState(false);
  const candidate = here.path ?? cand;
  // The governing document matters only against the version in force: feedback mode keys without it.
  const docKey = mode === "callouts" ? "" : doc;
  // Checks this session (going on in the background, so the screen can be left), and the kept result.
  useEffect(startDocChecks, []);
  const runs = useStore(docChecks);
  // Showing: whatever ended is seen (the sidebar's badge goes), now and as more end.
  const unseen = useStore(docChecksUnseen);
  useEffect(() => {
    if (unseen.n) seenDocChecks();
  }, [unseen.n]);
  const setRuns = (f: (l: CheckRun[]) => CheckRun[]) => docChecks.set(f(docChecks.get()));
  const [saved, setSaved] = useState<SavedCheck | null>(null);
  const run = runs.find((x) => x.candidate === candidate && x.mode === mode && x.doc === docKey) ?? null;
  const busy = run?.status === "running";
  useEffect(() => {
    setSaved(null);
    if (!candidate || (mode === "standard" && !doc)) return;
    let live = true;
    api
      .docCheckSaved(candidate, docKey, mode)
      .then((x) => live && setSaved(x))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [candidate, docKey, mode, run?.status]); // eslint-disable-line react-hooks/exhaustive-deps
  // What's shown: this session's finished check, else the kept one.
  const r: CheckResult | null = run?.status === "done" ? run.result : busy ? null : (saved?.result ?? null);
  const fromSaved = !!saved && r === saved.result;

  const load = () =>
    void api
      .canonicalRegister()
      .then(setReg)
      .catch((e) => setError(String(e)));
  useEffect(load, []);
  useEffect(() => {
    void api
      .filesList("source")
      .then((f) => setSources(f.sort((a, b) => b.mtime - a.mtime)))
      .catch(() => {});
  }, []);
  const docs = useMemo(() => governingDocs(reg), [reg]);
  useEffect(() => {
    if (!doc && docs.length) setDoc(docs[0].key);
  }, [docs]); // eslint-disable-line react-hooks/exhaustive-deps

  const pick = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const f = await open({
      multiple: false,
      directory: false,
      title: "A document to check",
      filters: [{ name: "Documents", extensions: ["pdf", "docx", "pptx", "xlsx", "md", "txt"] }],
    });
    if (typeof f === "string") setCand(f);
  };
  // Why Check can't run yet, shown beside it.
  const why = !candidate
    ? "Choose a document to check first."
    : mode === "standard" && !doc
      ? docs.length
        ? "Choose the governing document to check against."
        : "Add a governing document to the register first."
      : null;
  const start = async () => {
    if (!candidate) return;
    setError("");
    try {
      const x = await api.docCheckStart(candidate, docKey, mode);
      setRuns((l) => mergeRun(l, x));
    } catch (e) {
      setError(String(e));
    }
  };
  const name = candidate ? (candidate.split("/").pop() ?? candidate) : "";
  const count = (k: string) => r?.findings.filter((f) => f.kind === k).length ?? 0;
  const save = async () => {
    if (!r) return;
    const path = findingsNote(name);
    try {
      await api.docCreate(path, ASSISTANT_FRONTMATTER + findingsMarkdown(name, r));
      toast("Saved as a note", { label: "Open", run: () => openDoc(path) }, "ok");
    } catch (e) {
      reportEditError(e);
    }
  };
  const makeRegister = async () => {
    try {
      await api.docCreate(REGISTER, REGISTER_STUB);
      openDoc(REGISTER);
    } catch (e) {
      reportEditError(e);
    }
  };

  return (
    <main className="main">
      <TopBar title="Doc check" sub={name || undefined}>
        <CompareTabs on="doccheck" />
        {r && (
          <>
            <button
              type="button"
              className="btn"
              title="Copy the findings to the clipboard as markdown"
              onClick={() =>
                void navigator.clipboard.writeText(findingsMarkdown(name, r)).then(() => toast("Findings copied", undefined, "ok"))
              }
            >
              <Icon name="copy" size={14} />
              Copy findings
            </button>
            <button
              type="button"
              className="btn pri"
              title="Save the findings as a new dated note in the vault"
              onClick={() => void save()}
            >
              <Icon name="plus" size={14} />
              Save as note
            </button>
          </>
        )}
      </TopBar>
      <div className="body doccheck">
        <section>
          <div className="row">
            <span className="eyebrow">Canonical docs register</span>
            <span className="faint small grow">which version of each governing document is in force · {REGISTER}</span>
            {reg?.exists ? (
              <button type="button" className="btn sm ghost" title="Open the register note to change it" onClick={() => openDoc(REGISTER)}>
                Edit register
              </button>
            ) : (
              <button
                type="button"
                className="btn sm"
                title="Create the register note from a starter table and open it"
                onClick={() => void makeRegister()}
              >
                Start the register
              </button>
            )}
          </div>
          {reg && reg.entries.length > 0 && (
            <div className="card dcreg">
              <div className="dcr th">
                <span>Document</span>
                <span>Version</span>
                <span>Status</span>
                <span>File</span>
              </div>
              {reg.entries.map((e) => (
                <div key={e.line} className={`dcr ${e.status}`}>
                  <span className="ell">{e.title}</span>
                  <span className="mono">{e.version}</span>
                  <span>
                    <span className={`chip ${STATUS[e.status][1]}`}>{STATUS[e.status][0]}</span>
                  </span>
                  <span className={`ell ${e.exists ? "muted" : "red"}`} title={e.resolvedPath}>
                    {e.path.split("/").pop()}
                    {!e.exists && " · not there"}
                  </span>
                </div>
              ))}
            </div>
          )}
          {reg?.problems.map((p) => (
            <p key={p} className="err small">
              {REGISTER}: {p}
            </p>
          ))}
        </section>
        <section className="card dcform">
          <div className="row">
            <span className="eyebrow">Check</span>
            {/* A source by typing its name; a file picked from disk shows as chosen too. */}
            <SearchPick
              className="grow"
              label="Document to check"
              placeholder="Choose a source… (type to find it)"
              value={candidate}
              onChange={(v) => setCand(v || null)}
              items={[
                ...(candidate && !sources.some((s) => s.path === candidate) ? [{ value: candidate, label: name ?? candidate }] : []),
                ...sources.map((s) => ({
                  value: s.path,
                  label: s.path.split("/").pop() ?? s.path,
                  hint: s.path.split("/").slice(1, -1).join("/") || undefined,
                })),
              ]}
            />
            <button type="button" className="btn" title="Pick a document from anywhere on disk" onClick={() => void pick()}>
              Choose a file…
            </button>
          </div>
          <div className="row">
            <Seg<Mode>
              label="Mode"
              value={mode}
              onChange={setMode}
              options={[
                ["standard", "Against the version in force", "Compare the document with the version of a governing document in force"],
                [
                  "callouts",
                  "Against my earlier feedback",
                  "For a revised document: check each point of the feedback you gave on the last version against this one",
                ],
              ]}
            />
            {mode === "standard" && (
              <select
                title="The governing document to check against: its version in force, from the canonical docs register"
                className="sel"
                aria-label="Governing document"
                value={doc}
                onChange={(e) => setDoc(e.target.value)}
              >
                {docs.map((d) => (
                  <option key={d.key} value={d.key}>
                    {d.title}
                  </option>
                ))}
              </select>
            )}
            <span className="grow" />
            {why && <span className="faint small">{why}</span>}
            <span
              title={
                why ??
                (busy
                  ? "Checking the document…"
                  : mode === "standard"
                    ? "Have the AI compare the document with the version in force"
                    : "Have the AI check whether the document took your feedback")
              }
            >
              <button type="button" className="btn pri" disabled={busy || !!why} onClick={() => void start()}>
                <Icon name="ask" size={14} />
                {busy ? "Checking…" : saved || run?.status === "done" ? "Check again" : "Check"}
              </button>
            </span>
          </div>
          {error && <p className="err">{error}</p>}
          {run && run.status !== "done" && <CheckProgress run={run} />}
          {fromSaved && saved && (
            <div className={`doc-banner ${saved.stale ? "warn" : ""}`} role="status">
              <Icon name={saved.stale ? "info" : "check"} size={14} />
              <span>
                Checked {ago(new Date(saved.at).getTime())}
                {saved.result.against ? ` against ${saved.result.against.title} ${saved.result.against.version}` : ""}.{" "}
                {saved.stale
                  ? "The document or the version in force has changed since: check again for findings that match them."
                  : "Neither document has changed since, so this still stands."}
              </span>
            </div>
          )}
        </section>
        {r && r.unrelated && (
          <div className="doc-banner warn" role="status">
            <Icon name="info" size={14} />
            <span>
              <b>{r.verdict}.</b> {r.summary} Nothing was checked.
            </span>
          </div>
        )}
        {r && !r.unrelated && (
          <div className="dcres">
            <section className="card dcverdict">
              <span className="eyebrow">Verdict</span>
              <div className="dcv">{r.verdict}</div>
              <p className="muted small">{r.summary}</p>
              {(
                [
                  ["diverges", "Conflicts", "red"],
                  ["not-covered", "Not covered", "amber"],
                  ["superseded-term", "Superseded terms", "amber"],
                  ["aligned", "Agrees", "green"],
                ] as const
              )
                .filter(([k]) => count(k))
                .map(([k, l, t]) => (
                  <div key={k} className="row small">
                    <span className="muted grow">{l}</span>
                    <span className={`chip ${t}`}>{count(k)}</span>
                  </div>
                ))}
              {r.against && (
                <p className="faint small">
                  Checked against {r.against.title} {r.against.version}.
                  {r.excluded.length > 0 && ` Not used: ${r.excluded.map((e) => `${e.version} (${e.status})`).join(", ")}.`}
                </p>
              )}
            </section>
            <section className="card dcfind">
              {r.findings
                .filter((f) => f.kind !== "aligned")
                .map((f, i) => (
                  <FindingRow key={i} f={f} against={r.against} />
                ))}
              {count("aligned") > 0 && (
                <div className="dcf">
                  <div className="row">
                    <span className="chip green">
                      <Icon name="check" size={11} />
                      Agrees
                    </span>
                    <span className="grow ell">
                      {r.findings
                        .filter((f) => f.kind === "aligned")
                        .map((f) => f.title)
                        .join(", ")}
                    </span>
                    <button
                      type="button"
                      className="btn sm ghost"
                      title={showOk ? "Hide the points where the document agrees" : "Show the points where the document agrees"}
                      onClick={() => setShowOk((x) => !x)}
                    >
                      {showOk ? "Hide" : `Show ${count("aligned")}`}
                    </button>
                  </div>
                  {showOk && r.findings.filter((f) => f.kind === "aligned").map((f, i) => <FindingRow key={i} f={f} against={r.against} />)}
                </div>
              )}
              {!r.findings.length && <p className="faint">No findings.</p>}
            </section>
          </div>
        )}
      </div>
    </main>
  );
}

/** A check under way (or stopped, or failed): its steps ticked off with their times, a clock on the
 *  one going, and Stop. */
export function CheckProgress({ run }: { run: CheckRun }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (run.status !== "running") return;
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [run.status]);
  const secs = (ms: number) => (ms < 1000 ? `${ms} ms` : `${Math.round(ms / 1000)} s`);
  const since = Math.max(0, now - new Date(run.started).getTime());
  return (
    <div className="dcprog">
      <div className="runsteps">
        {run.steps.map((s, i) => (
          <div key={i} className={`runstep ${s.status}`}>
            <span className="sd">
              {s.status === "running" && run.status === "running" ? (
                <span className="spin" />
              ) : s.status === "done" ? (
                <Icon name="check" size={11} />
              ) : (
                <Icon name="x" size={11} />
              )}
            </span>
            <span className="grow nm">{s.name}</span>
            {s.ms > 0 && <span className="faint mono small">{secs(s.ms)}</span>}
          </div>
        ))}
      </div>
      <div className="row">
        <span className="faint small grow">
          {run.status === "running"
            ? `Checking for ${secs(since)}. A long document can take minutes; you can leave this page and come back.`
            : run.status === "stopped"
              ? "Stopped: nothing was kept from it."
              : `Failed: ${run.error ?? "the check didn't finish"}`}
        </span>
        {run.status === "running" && (
          <button
            type="button"
            className="btn sm"
            title="Stop the check; nothing is kept from it"
            onClick={() => void api.docCheckStop(run.id)}
          >
            <Icon name="stop" size={12} />
            Stop
          </button>
        )}
      </div>
    </div>
  );
}

function FindingRow({ f, against }: { f: Finding; against: CanonicalEntry | null }) {
  const [label, tone] = KIND[f.kind] ?? [f.kind, ""];
  return (
    <div className="dcf">
      <div className="row">
        <span className={`chip ${tone}`}>{label}</span>
        <b className="grow">{f.title}</b>
        {f.material && <span className="chip red">Material</span>}
        {f.where && <span className="faint small">{f.where}</span>}
      </div>
      {(f.candidate || f.canonical) && (
        <div className="dcq">
          {f.candidate && (
            <div className="q">
              <div className="faint small">This document</div>“{f.candidate}”
            </div>
          )}
          {f.canonical && (
            <div className="q">
              <div className="faint small">{against ? `In force · ${against.title} ${against.version}` : "Asked for"}</div>“{f.canonical}”
            </div>
          )}
        </div>
      )}
    </div>
  );
}
