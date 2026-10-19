// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Compare: any two documents, A and B, neither taking precedence (Doc check is the one that knows
// which version is in force). Differences: where they disagree, what's only in one, what agrees;
// What changed: added, removed and changed from an earlier version (B) to a newer one (A). It runs
// as Doc check does (in the background, kept until either document changes; D-20261012-04), and
// first looks at their openings, stopping there when they aren't about the same thing.

import { useEffect, useMemo, useState } from "react";
import { api, CheckResult, FileSummary, Finding, SavedCheck } from "../api";
import { Icon } from "../icons";
import { openDoc, place, useViewState } from "../nav";
import { useStore } from "../store";
import { reportEditError } from "../taskModel";
import { toast } from "../Toast";
import { TopBar } from "../TopBar";
import { ago, SearchPick, Seg } from "../ui";
import { ASSISTANT_FRONTMATTER } from "../md/scripts";
import { docChecks, docChecksUnseen, mergeRun, seenDocChecks, startDocChecks } from "../docChecks";
import { CheckProgress } from "./DocCheck";
import { CompareTabs } from "./CompareTabs";

export type CompareMode = "differences" | "changes";

/** Each finding's name and colour, as the screen and its markdown say them. */
export const COMPARE_KIND: Record<string, [string, string]> = {
  differs: ["Differs", "red"],
  "only-a": ["Only in A", "amber"],
  "only-b": ["Only in B", "amber"],
  aligned: ["Agrees", "green"],
  changed: ["Changed", "red"],
  added: ["Added", "green"],
  removed: ["Removed", "amber"],
};

const fileName = (p: string) => p.split("/").pop() ?? p;

/** Where Save as note puts a comparison, dated today. */
export function compareNote(a: string, b: string, d = new Date()): string {
  const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  const clean = (n: string) => n.replace(/\.[^.]+$/, "").replace(/[\\/:*?"<>|#^[\]]/g, " ");
  return `Compare. ${clean(a)} vs ${clean(b)} - ${date}.md`;
}

/** A comparison as markdown, for Copy findings and Save as note (compare_docs too). */
export function compareMarkdown(a: string, b: string, mode: CompareMode, r: CheckResult): string {
  const lines = [
    `# Compare: ${a} and ${b}`,
    "",
    mode === "changes" ? `What changed from ${b} (earlier) to ${a} (newer).` : `A: ${a}. B: ${b}.`,
  ];
  lines.push("", `**${r.verdict}**`, "", r.summary, "");
  for (const f of r.findings.filter((x) => x.kind !== "aligned")) {
    lines.push(`## ${COMPARE_KIND[f.kind]?.[0] ?? f.kind}${f.material ? " (material)" : ""}: ${f.title}`, "");
    if (f.where) lines.push(`*${f.where}*`, "");
    if (f.candidate) lines.push(`> A: ${f.candidate}`, "");
    if (f.canonical) lines.push(`> B: ${f.canonical}`, "");
  }
  const ok = r.findings.filter((x) => x.kind === "aligned");
  if (ok.length) lines.push(`## Agrees`, "", ...ok.map((f) => `- ${f.title}`), "");
  return lines.join("\n");
}

export function CompareScreen() {
  const here = useStore(place).place;
  const [files, setFiles] = useState<FileSummary[]>([]);
  const [aPick, setA] = useViewState<string | null>("compare.a", null);
  const [b, setB] = useViewState<string | null>("compare.b", null);
  const [mode, setMode] = useViewState<CompareMode>("compare.mode", "differences");
  const [error, setError] = useState("");
  // A file sent here (Compare with…) is A.
  const a = here.path ?? aPick;
  useEffect(() => {
    void api
      .filesList(null)
      .then((f) => setFiles(f.filter((x) => x.layer !== "template").sort((p, q) => q.mtime - p.mtime)))
      .catch(() => {});
  }, []);
  useEffect(startDocChecks, []);
  const runs = useStore(docChecks);
  const unseen = useStore(docChecksUnseen);
  useEffect(() => {
    if (unseen.n) seenDocChecks();
  }, [unseen.n]);
  const run = runs.find((x) => x.candidate === a && x.doc === b && x.mode === mode) ?? null;
  const busy = run?.status === "running";
  const [saved, setSaved] = useState<SavedCheck | null>(null);
  useEffect(() => {
    setSaved(null);
    if (!a || !b) return;
    let live = true;
    api
      .docCheckSaved(a, b, mode)
      .then((x) => live && setSaved(x))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [a, b, mode, run?.status]);
  const r: CheckResult | null = run?.status === "done" ? run.result : busy ? null : (saved?.result ?? null);
  const fromSaved = !!saved && r === saved.result;
  const [showOk, setShowOk] = useState(false);

  // Every note, source and wiki page, newest first; a file chosen from disk is offered too.
  const items = useMemo(() => {
    const list = files.map((f) => ({
      value: f.path,
      label: fileName(f.path),
      hint: f.path.split("/").slice(0, -1).join("/") || undefined,
    }));
    const extra = [a, b]
      .filter((p): p is string => !!p && !files.some((f) => f.path === p))
      .map((p) => ({ value: p, label: fileName(p), hint: p }));
    return [...extra, ...list];
  }, [files, a, b]);
  const pickFile = async (set: (p: string) => void) => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const f = await open({
      multiple: false,
      directory: false,
      title: "A document to compare",
      filters: [{ name: "Documents", extensions: ["pdf", "docx", "pptx", "xlsx", "md", "txt"] }],
    });
    if (typeof f === "string") set(f);
  };
  const setAPath = (p: string) => {
    if (here.path) place.set({ ...place.get(), place: { ...here, path: undefined } });
    setA(p);
  };
  const swap = () => {
    const x = a;
    setAPath(b ?? "");
    setB(x);
  };
  const why = !a
    ? "Choose document A first."
    : !b
      ? "Choose document B to compare it with."
      : a === b
        ? "Choose two different documents."
        : null;
  const start = async () => {
    if (!a || !b || why) return;
    setError("");
    try {
      const x = await api.docCheckStart(a, b, mode);
      docChecks.set(mergeRun(docChecks.get(), x));
    } catch (e) {
      setError(String(e));
    }
  };
  const an = a ? fileName(a) : "";
  const bn = b ? fileName(b) : "";
  const save = async () => {
    if (!r || !a || !b) return;
    const path = compareNote(an, bn);
    try {
      await api.docCreate(path, ASSISTANT_FRONTMATTER + compareMarkdown(an, bn, mode, r));
      toast("Saved as a note", { label: "Open", run: () => openDoc(path) }, "ok");
    } catch (e) {
      reportEditError(e);
    }
  };
  const count = (k: string) => r?.findings.filter((f) => f.kind === k).length ?? 0;
  const tally: [string, string][] =
    mode === "changes"
      ? [
          ["changed", "Changed"],
          ["added", "Added"],
          ["removed", "Removed"],
        ]
      : [
          ["differs", "Differs"],
          ["only-a", "Only in A"],
          ["only-b", "Only in B"],
          ["aligned", "Agrees"],
        ];

  return (
    <main className="main">
      <TopBar title="Compare" sub={a && b ? `${an} and ${bn}` : undefined}>
        <CompareTabs on="compare" />
        {r && !r.unrelated && (
          <>
            <button
              type="button"
              className="btn"
              title="Copy the findings to the clipboard as markdown"
              onClick={() =>
                void navigator.clipboard.writeText(compareMarkdown(an, bn, mode, r)).then(() => toast("Findings copied", undefined, "ok"))
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
        <section className="card dcform">
          <div className="row">
            <span className="eyebrow cmpab">{mode === "changes" ? "A · newer" : "A"}</span>
            <SearchPick
              className="grow"
              label="Document A"
              placeholder="Choose a document… (type to find it)"
              value={a}
              onChange={setAPath}
              items={items}
            />
            <button type="button" className="btn" title="Pick document A from anywhere on disk" onClick={() => void pickFile(setAPath)}>
              Choose a file…
            </button>
          </div>
          <div className="row">
            <span className="eyebrow cmpab">{mode === "changes" ? "B · earlier" : "B"}</span>
            <SearchPick
              className="grow"
              label="Document B"
              placeholder="Choose a document… (type to find it)"
              value={b}
              onChange={setB}
              items={items}
            />
            <button type="button" className="btn" title="Pick document B from anywhere on disk" onClick={() => void pickFile(setB)}>
              Choose a file…
            </button>
          </div>
          <div className="row">
            <Seg<CompareMode>
              label="Look for"
              value={mode}
              onChange={setMode}
              options={[
                ["differences", "Differences", "Where they disagree, what's only in one of them, and what agrees"],
                ["changes", "What changed", "From an earlier version (B) to a newer one (A): what was added, removed and changed"],
              ]}
            />
            <button type="button" className="btn" title="Swap A and B" disabled={!a && !b} onClick={swap}>
              <Icon name="refresh" size={14} />
              Swap
            </button>
            <span className="grow" />
            {why && <span className="faint small">{why}</span>}
            <span title={why ?? (busy ? "Comparing…" : "Have the AI compare them; it looks first whether they're about the same thing")}>
              <button type="button" className="btn pri" disabled={busy || !!why} onClick={() => void start()}>
                <Icon name="ask" size={14} />
                {busy ? "Comparing…" : saved || run?.status === "done" ? "Compare again" : "Compare"}
              </button>
            </span>
          </div>
          {error && <p className="err">{error}</p>}
          {run && run.status !== "done" && <CheckProgress run={run} />}
          {fromSaved && saved && (
            <div className={`doc-banner ${saved.stale ? "warn" : ""}`} role="status">
              <Icon name={saved.stale ? "info" : "check"} size={14} />
              <span>
                Compared {ago(new Date(saved.at).getTime())}.{" "}
                {saved.stale
                  ? "One of them has changed since: compare again for findings that match them."
                  : "Neither has changed since, so this still stands."}
              </span>
            </div>
          )}
        </section>
        {/* Before anything's compared: what the two ways do, so the page isn't just a form. */}
        {!r && !run && !error && (
          <div className="dcempty">
            <p>
              <b>Differences</b> shows where two documents disagree, what's only in one of them, and what they agree on.
            </p>
            <p>
              <b>What changed</b> reads B as the earlier version and A as the newer one, and lists what was added, removed and changed.
            </p>
            <p className="faint">
              Choose the documents above, from the vault or a file. Brainstead first checks they're about the same thing.
            </p>
          </div>
        )}
        {r && r.unrelated && (
          <div className="doc-banner warn" role="status">
            <Icon name="info" size={14} />
            <span>
              <b>{r.verdict}.</b> {r.summary} Nothing was compared.
            </span>
          </div>
        )}
        {r && !r.unrelated && (
          <div className="dcres">
            <section className="card dcverdict">
              <span className="eyebrow">Verdict</span>
              <div className="dcv">{r.verdict}</div>
              <p className="muted small">{r.summary}</p>
              {tally
                .filter(([k]) => count(k))
                .map(([k, l]) => (
                  <div key={k} className="row small">
                    <span className="muted grow">{l}</span>
                    <span className={`chip ${COMPARE_KIND[k][1]}`}>{count(k)}</span>
                  </div>
                ))}
            </section>
            <section className="card dcfind">
              {r.findings
                .filter((f) => f.kind !== "aligned")
                .map((f, i) => (
                  <CompareRow key={i} f={f} mode={mode} />
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
                      title={showOk ? "Hide the points where they agree" : "Show the points where they agree"}
                      onClick={() => setShowOk((x) => !x)}
                    >
                      {showOk ? "Hide" : `Show ${count("aligned")}`}
                    </button>
                  </div>
                  {showOk && r.findings.filter((f) => f.kind === "aligned").map((f, i) => <CompareRow key={i} f={f} mode={mode} />)}
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

function CompareRow({ f, mode }: { f: Finding; mode: CompareMode }) {
  const [label, tone] = COMPARE_KIND[f.kind] ?? [f.kind, ""];
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
              <div className="faint small">{mode === "changes" ? "A · newer" : "A"}</div>“{f.candidate}”
            </div>
          )}
          {f.canonical && (
            <div className="q">
              <div className="faint small">{mode === "changes" ? "B · earlier" : "B"}</div>“{f.canonical}”
            </div>
          )}
        </div>
      )}
    </div>
  );
}
