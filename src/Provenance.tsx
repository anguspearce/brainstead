// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A source's provenance (§4 "source preview with provenance"): when it was ingested and by which
// run, what the index holds of it, the wiki pages that cite it and the passages they cite, and
// Re-ingest, Reveal and Trash.

import { useEffect, useState } from "react";
import { api, FileSummary, Provenance } from "./api";
import { fmtDay } from "./FileList";
import { Icon } from "./icons";
import { ingest } from "./Ingest";
import { ingestable } from "./Lists";
import { openDoc } from "./nav";
import { trashFile } from "./notes/actions";
import { useVaultVersion } from "./state";
import { fmtBytes } from "./ui";
import { plainText } from "./md/plainText";
import { fileManager } from "./platform";

/** A citing passage as the card quotes it: plain text, without its links to the source or its
 *  block id (the provenance tool quotes it the same way). */
export const passageText = (context: string) =>
  plainText(context.replace(/\[\[([^\]|#]+)(#[^\]|]*)?(\|[^\]]*)?\]\]/g, "").replace(/\s*\^[\w-]+$/, "")).replace(/\s+([.,;:])/g, "$1");

/** How many of a page's passages the card quotes. */
export const PASSAGES_SHOWN = 3;

export function ProvenanceCard({ file }: { file: FileSummary }) {
  const [p, setP] = useState<Provenance | null>(null);
  const v = useVaultVersion();
  useEffect(() => {
    let live = true;
    api
      .sourceProvenance(file.path)
      .then((x) => live && setP(x))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [file.path, v]);
  if (!p) return null;
  const run = p.lastRun;
  const passages = p.citers.reduce((n, c) => n + c.passages.length, 0);
  return (
    <section className="card side-card prov">
      <div className="row">
        {p.citers.length ? (
          <span className="chip green">
            <Icon name="check" size={11} />
            {run?.finished ? `Ingested ${fmtDay(Date.parse(run.finished))}` : "Cited in the wiki"}
          </span>
        ) : (
          <span className="chip amber">Not ingested yet</span>
        )}
        {run && (
          <span className="faint small" title={run.id}>
            {run.status === "done" ? "" : `${run.status} · `}
            {run.model.replace(/^[a-z]+:/, "")}
          </span>
        )}
      </div>
      <dl className="kv small">
        <dt>Size</dt>
        <dd>{fmtBytes(p.size)}</dd>
        <dt>Changed</dt>
        <dd>{fmtDay(p.mtime)}</dd>
      </dl>
      <details className="provmore small">
        <summary title="What the search index holds of this file, and its fingerprint">Details</summary>
        <dl className="kv">
          <dt>Text</dt>
          <dd>{p.chunks ? `${p.chunks} passage${p.chunks === 1 ? "" : "s"} indexed · searchable` : "None read yet"}</dd>
          <dt>Hash</dt>
          <dd className="mono" title={`The file's SHA-256 fingerprint: ${p.sha256}`}>
            {p.sha256.slice(0, 6)}…{p.sha256.slice(-4)}
          </dd>
        </dl>
      </details>
      <div className="eyebrow">
        Cited by {p.citers.length} page{p.citers.length === 1 ? "" : "s"}
        {passages ? ` · ${passages} passage${passages === 1 ? "" : "s"}` : ""}
      </div>
      {!p.citers.length && <p className="faint">No wiki page cites it yet. Ingest it to have the wiki take it in.</p>}
      {p.citers.map((c) => (
        <div key={c.path} className="provciter">
          <button type="button" className="blink" title={`Open ${c.title}`} onClick={() => openDoc(c.path)}>
            <span className="lb wiki">{c.title}</span>
            <span className="faint">
              {c.passages.length ? `${c.passages.length} passage${c.passages.length === 1 ? "" : "s"}` : "in its sources"}
            </span>
          </button>
          {c.passages.slice(0, PASSAGES_SHOWN).map((x) => (
            <button
              key={x.line}
              type="button"
              className="provquote"
              title={`Open ${c.title}, the page that quotes this`}
              onClick={() => openDoc(c.path)}
            >
              <span>{passageText(x.context)}</span>
              {x.anchor && <span className="faint small">at “{x.anchor}”</span>}
            </button>
          ))}
        </div>
      ))}
      <div className="row provacts">
        {ingestable(file.path) && (
          <button
            type="button"
            className="btn sm"
            title={
              p.citers.length
                ? "Have the AI read this source again and update the wiki from it"
                : "Have the AI read this source and add what it says to the wiki"
            }
            onClick={() => ingest([file.path])}
          >
            <Icon name="refresh" size={12} />
            {p.citers.length ? "Re-ingest" : "Ingest"}
          </button>
        )}
        <button
          type="button"
          className="btn sm"
          title={`Show the file in ${fileManager}`}
          onClick={() => void api.reveal(file.path).catch(() => {})}
        >
          <Icon name="finder" size={12} />
          Reveal
        </button>
        <span className="grow" />
        <button type="button" className="btn sm ghost" title="Move the file to the vault’s Trash" onClick={() => void trashFile(file.path)}>
          <Icon name="trash" size={12} />
          Move to the Trash
        </button>
      </div>
    </section>
  );
}
