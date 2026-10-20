// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Contradictions (stage 7b): where wiki pages disagree about the same thing at the same time.
// Code finds the clashes among the pages' claims; a model judges each new one once; a
// contradiction with a known fix is made (held in Changes when a check fails). Here you see each clash's claims
// side by side with the judge's view, and mark it resolved or ignored.

import { useEffect, useState } from "react";
import { api, ContradictionItem, ContradictionsReport } from "./api";
import { Icon } from "./icons";
import { pageName } from "./knowledge";
import { nav, openDoc, useViewState } from "./nav";
import { reportEditError } from "./taskModel";
import { localToday } from "./md/taskQuery";
import { toast } from "./Toast";
import { TopBar } from "./TopBar";
import { DocButton, useGone } from "./docButton";
import { ASSISTANT_FRONTMATTER } from "./md/scripts";
import { LayerTag } from "./ui";

/** How a clash's verdict reads, and its tone. */
export function verdictLabel(it: ContradictionItem): [string, string] {
  const v = it.verdict?.verdict;
  switch (v) {
    case "contradiction":
      return [`Real · ${it.verdict?.severity || "?"}`, "red"];
    case "evolution":
      return ["Newer supersedes older", ""];
    case "compatible":
      return ["Not a conflict", "green"];
    case "unclear":
      return ["Unclear", "amber"];
    case "resolved":
      return ["Resolved", "green"];
    case "ignored":
      return ["Ignored", ""];
    default:
      return ["Not judged yet", ""];
  }
}

/** Open first: real contradictions by severity, then unclear, then the rest. */
export function order(items: ContradictionItem[]): ContradictionItem[] {
  const rank = (it: ContradictionItem) => {
    const v = it.verdict?.verdict;
    if (v === "contradiction") return { high: 0, medium: 1, low: 2 }[it.verdict?.severity ?? ""] ?? 2;
    if (v === "unclear" || !v) return 3;
    return 4;
  };
  return [...items].sort((a, b) => rank(a) - rank(b) || a.subject.localeCompare(b.subject));
}

/** The report as a note: each contradiction with its claims, the verdict and the fix. */
export function reportMarkdown(r: ContradictionsReport, today: string): string {
  const l = r.last;
  const lines = [`# Contradictions - ${today}`, ""];
  if (l.finished)
    lines.push(
      `Checked ${l.finished.slice(0, 16).replace("T", " ")}: ${l.pages} pages, ${l.claims} claims, ${l.clashes} clashes, ${l.contradictions} contradictions.`,
      "",
    );
  const real = r.items.filter((i) => !i.verdict || i.verdict.verdict !== "compatible");
  if (!real.length) lines.push("No contradictions.");
  for (const i of real) {
    lines.push(`## ${i.subject} · ${i.attribute}`, "");
    for (const c of i.claims)
      lines.push(`- [[${pageName(c.page)}]]${c.asOf ? ` (as of ${c.asOf})` : ""}: ${c.value}${c.quote ? ` · “${c.quote}”` : ""}`);
    if (i.verdict) {
      lines.push("", `**${i.verdict.verdict}${i.verdict.severity ? `, ${i.verdict.severity}` : ""}.** ${i.verdict.summary}`);
      if (i.verdict.correct) lines.push("", `Correct: ${i.verdict.correct}`);
      if (i.verdict.fix) lines.push("", `Fix: ${i.verdict.fix}`);
    }
    lines.push("");
  }
  return lines.join("\n");
}

async function saveReport(r: ContradictionsReport) {
  const today = localToday();
  const base = `Contradictions - ${today}`;
  for (let n = 1; n < 20; n++) {
    const path = `${n === 1 ? base : `${base}-${n}`}.md`;
    try {
      await api.docCreate(path, ASSISTANT_FRONTMATTER + reportMarkdown(r, today));
      toast("Report saved as a note", { label: "Open", run: () => openDoc(path) }, "ok");
      return;
    } catch (e) {
      if (!String((e as { message?: string })?.message ?? e).match(/exists|already/i)) return reportEditError(e);
    }
  }
}

export function ContradictionsScreen() {
  const [r, setR] = useState<ContradictionsReport | null>(null);
  const [sel, setSel] = useViewState<string | null>("contradictions:sel", null);
  const load = () =>
    void api
      .contradictionsReport()
      .then(setR)
      .catch(() => {});
  useEffect(() => {
    load();
    const off = api.onContradictionsChanged(() => load()).catch(() => () => {});
    return () => void off.then((f) => f());
  }, []);
  const items = order(r?.items ?? []);
  const cur = items.find((i) => i.id === sel) ?? items[0] ?? null;
  const last = r?.last;
  const run = () =>
    void api.contradictionsRun().then((ok) => {
      if (!ok) toast("A check is running already.");
      load();
    });
  const mark = (v: string) => cur && api.contradictionsMark(cur.id, v).then(load).catch(reportEditError);
  const real = items.filter((i) => i.verdict?.verdict === "contradiction").length;
  const gone = useGone(cur?.claims.map((c) => c.page) ?? []);

  return (
    <main className="main">
      <TopBar
        title="Contradictions"
        sub={last?.finished ? `checked ${last.finished.slice(0, 16).replace("T", " ")}` : "not checked yet"}
        ask={
          cur
            ? {
                about: `the clash over ${cur.subject}'s ${cur.attribute}`,
                prompt: `About the clash over ${cur.subject}'s ${cur.attribute}: `,
              }
            : { about: "the wiki's contradictions", prompt: "About the contradictions in my wiki: " }
        }
      >
        {r && last?.finished && !last.running && (
          <button
            type="button"
            className="btn"
            title="Save this check's findings as a new note, Contradictions - <date>"
            onClick={() => void saveReport(r)}
          >
            <Icon name="note" size={14} />
            Save report as note
          </button>
        )}
        <span
          title={
            last?.running
              ? "A check is running; Stop ends it"
              : "Read the wiki pages changed since the last check and look for claims that disagree"
          }
        >
          <button type="button" className="btn pri" disabled={last?.running} aria-busy={last?.running} onClick={run}>
            {last?.running ? <span className="spin" /> : <Icon name="refresh" size={14} />}
            {last?.running ? last.doing || "Checking…" : "Check · changed pages only"}
          </button>
        </span>
        {last?.running && (
          <button
            type="button"
            className="btn"
            title="Stop the check; what it has read so far is kept for next time"
            onClick={() => void api.contradictionsStop()}
          >
            <Icon name="stop" size={14} />
            Stop
          </button>
        )}
      </TopBar>
      <div className="body contra">
        <div className="cstats">
          {[
            [last?.pages ?? 0, "pages"],
            [last?.claims ?? 0, "claims"],
            [last?.extracted ?? 0, "pages read this time"],
            [items.length, "clashes found by code"],
            [real, "judged real"],
            [last?.proposed ?? 0, "fixes made"],
          ].map(([n, l]) => (
            <div key={l} className="cst">
              <b>{n}</b>
              <span>{l}</span>
            </div>
          ))}
        </div>
        {last?.error && (
          <p className="doc-banner warn">
            <Icon name="info" size={14} /> {last.error}
          </p>
        )}
        {!!last?.failed && (
          <p className="doc-banner warn">
            <Icon name="info" size={14} /> {last.failed === 1 ? "1 fix" : `${last.failed} fixes`} not made: {last.failures.join("; ")}
          </p>
        )}
        <div className="cgrid">
          <div className="ilist tb">
            {!items.length && (
              <p className="faint pad">{last?.finished ? "No pages disagree." : "Run a check to read the wiki's claims."}</p>
            )}
            {items.map((it) => {
              const [label, tone] = verdictLabel(it);
              return (
                <button
                  key={it.id}
                  type="button"
                  className={`qrow tb-row ${cur?.id === it.id ? "sel" : ""}`}
                  title="Show the pages' claims side by side"
                  onClick={() => setSel(it.id)}
                >
                  <span className="qtop">
                    <span className="lbt">
                      <LayerTag layer="wiki" />
                      <span className="ell">{it.subject}</span>
                    </span>
                    <span className={`chip sm ${tone}`}>{label}</span>
                  </span>
                  <span className="tt">{it.attribute.replace(/^other:/, "").replace(/_/g, " ")}</span>
                  <span className="faint">{it.claims.map((c) => c.value).join(" vs ")}</span>
                </button>
              );
            })}
          </div>
          {cur && (
            <div className="iopen qopen selectable">
              <div>
                <div className="eyebrow">
                  {cur.subject} · {cur.attribute.replace(/_/g, " ")}
                </div>
                <h1 className="h2">{cur.verdict?.summary || `${cur.claims.length} pages give different values`}</h1>
              </div>
              <div className="cclaims">
                {cur.claims.map((c, i) => (
                  <div key={i} className="card cclaim">
                    <div className="row">
                      <span className="chip sm">{String.fromCharCode(65 + i)}</span>
                      <b>{c.value}</b>
                    </div>
                    <blockquote>“{c.quote}”</blockquote>
                    <span className="faint small">
                      <DocButton path={c.page} gone={gone(c.page)} className="blink">
                        {pageName(c.page)}
                      </DocButton>
                      {c.asOf ? ` · as of ${c.asOf}` : ""}
                    </span>
                  </div>
                ))}
              </div>
              {cur.verdict && (
                <div className="card cjudge">
                  <span className="eyebrow">Judge's view</span>
                  {cur.verdict.fix && <p className="muted">{cur.verdict.fix}</p>}
                  {cur.verdict.correct && <p className="faint small">Right: {pageName(cur.verdict.correct)}</p>}
                </div>
              )}
              <div className="qfoot">
                {cur.verdict?.patch && (
                  <button
                    type="button"
                    className="btn pri lg"
                    title="Open Changes, where the fix is listed with Revert"
                    onClick={() => nav.go("review")}
                  >
                    <Icon name="review" size={14} />
                    The fix is in Changes
                  </button>
                )}
                <button type="button" className="btn lg" title="Mark this contradiction as fixed" onClick={() => void mark("resolved")}>
                  <Icon name="check" size={14} />
                  Mark resolved
                </button>
                <button
                  type="button"
                  className="btn lg ghost"
                  title="Not a real contradiction; stop flagging it"
                  onClick={() => void mark("ignored")}
                >
                  Ignore
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </main>
  );
}

/** On a wiki page: the contradictions it's part of. */
export function ContradictionBanner({ path }: { path: string }) {
  const [items, setItems] = useState<ContradictionItem[]>([]);
  useEffect(() => {
    if (!path.startsWith("wiki/")) return;
    let live = true;
    api
      .pageContradictions(path)
      .then((r) => live && setItems(r))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [path]);
  if (!items.length) return null;
  const it = items[0];
  const other = it.claims.find((c) => c.page !== path);
  return (
    <div className="doc-banner warn" role="status" data-no-print>
      <Icon name="info" size={14} />
      <span className="grow">
        {items.length === 1 ? "Another page disagrees" : `${items.length} contradictions`}: {it.subject} {it.attribute.replace(/_/g, " ")}{" "}
        is “{it.claims.find((c) => c.page === path)?.value}” here{other ? ` but “${other.value}” in ${pageName(other.page)}` : ""}.
      </span>
      <button
        type="button"
        className="btn sm"
        title="Open Contradictions to see the pages side by side"
        onClick={() => nav.go("contradictions")}
      >
        See it
      </button>
    </div>
  );
}
