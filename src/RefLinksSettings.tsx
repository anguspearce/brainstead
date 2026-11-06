// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Settings › Notes › Reference links: the prefixes whose keys (ORBIT-12) open a page in the
// browser, and the address each one opens (src/md/refs.ts).

import { useState } from "react";
import { Icon } from "./icons";
import { rowProblem, sampleRef } from "./md/refs";
import { settings, useStore } from "./store";

export function RefLinksSettings() {
  const rows = useStore(settings).referenceLinks ?? [];
  // A row keeps its key as others are added and removed above it, so what is typed in it stays.
  const [kept, setKept] = useState<{ ids: number[]; next: number }>({ ids: [], next: 0 });
  if (kept.ids.length !== rows.length) {
    // Rows came or went somewhere else (the settings tool): number them again, in step.
    const ids = kept.ids.slice(0, rows.length);
    let next = kept.next;
    while (ids.length < rows.length) ids.push(next++);
    setKept({ ids, next });
  }
  const ids = kept.ids.length === rows.length ? kept.ids : rows.map((_, i) => -1 - i);
  const set = (i: number, patch: Partial<{ prefix: string; url: string }>) =>
    settings.update({ referenceLinks: rows.map((r, j) => (j === i ? { ...r, ...patch } : r)) });
  return (
    <section className="sgroup">
      <h2 className="h3">Reference links</h2>
      <p className="muted">
        A ticket or issue key such as <span className="mono">ORBIT-12</span> becomes a link that opens in your browser: in tasks, notes,
        properties and Dataview tables, and gathered in the References card on a note and Tickets and links on a project. A key is the
        prefix and then digits. In the address, <span className="mono">{"{key}"}</span> is the whole key and{" "}
        <span className="mono">{"{num}"}</span> its digits. None are set by default.
      </p>
      <div className="card reflinks">
        {rows.map((r, i) => {
          // The same rule as the settings tool and what is used: a row that doesn't work says why.
          const problem = rowProblem(rows, i);
          const sample = problem ? null : sampleRef(r);
          return (
            <div key={ids[i]} className="srow">
              <Icon name="globe" style={{ color: "var(--ink3)" }} />
              <label className="inp sm mono">
                <input
                  value={r.prefix}
                  onChange={(e) => set(i, { prefix: e.target.value })}
                  placeholder="ORBIT-"
                  aria-label={`Prefix, row ${i + 1}`}
                  spellCheck={false}
                  autoCorrect="off"
                  autoCapitalize="off"
                />
              </label>
              <div className="t">
                <label className="inp mono">
                  <input
                    value={r.url}
                    onChange={(e) => set(i, { url: e.target.value })}
                    placeholder="https://tickets.example.com/browse/{key}"
                    aria-label={`Address, row ${i + 1}`}
                    spellCheck={false}
                    autoCorrect="off"
                    autoCapitalize="off"
                  />
                </label>
                <div className="faint reflink-sample">
                  {sample ? (
                    <>
                      <span className="mono">{sample.key}</span> opens <span className="mono">{sample.url}</span>
                    </>
                  ) : r.prefix.trim() || r.url.trim() ? (
                    problem
                  ) : (
                    ""
                  )}
                </div>
              </div>
              <button
                type="button"
                className="btn sm"
                title={`Stop turning ${r.prefix.trim() || "this prefix"} keys into links`}
                aria-label={`Remove ${r.prefix.trim() || "the empty row"}`}
                onClick={() => {
                  setKept({ ...kept, ids: kept.ids.filter((_, j) => j !== i) });
                  settings.update({ referenceLinks: rows.filter((_, j) => j !== i) });
                }}
              >
                Remove
              </button>
            </div>
          );
        })}
        <div className="srow">
          <button
            type="button"
            className="btn sm"
            title="Add a prefix whose keys open a page in your browser"
            onClick={() => settings.update({ referenceLinks: [...rows, { prefix: "", url: "" }] })}
          >
            <Icon name="plus" size={12} />
            Add a prefix
          </button>
        </div>
      </div>
    </section>
  );
}
