// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The vault at a glance: pie charts of notes and wiki pages by type and by tag, and the files the
// most others link to. Activity shows the notes' and the most linked, in a column right of the log;
// Knowledge health the wiki's, in its side column.
// A slice, or its row in the legend, opens the list it counts: Notes or the Wiki filtered to that
// type, the Wiki at that tag, or a search for a note tag. Five slices at most, then Other; each in
// its type's or tag's own colour (src/tones.ts), as a type pill's dot and the Graph show it.

import type { Count, Glance } from "./api";
import { tonesOf, useGlance } from "./typeTone";
import { nav, openDoc } from "./nav";
import { fmtCount, LayerTag } from "./ui";

export interface Slice {
  key: string;
  label: string;
  n: number;
  /** What a click on it does; none for Other when it isn't a list of its own. */
  open?: () => void;
  tip: string;
  /** Its colour (`p1`…`p6`, src/tones.ts): the type's or tag's own, as everywhere else. */
  tone?: string;
}

const SLICES = 5;

/** The top five, then the rest as one Other slice (`other`). The named ones are what's counted:
 *  `""` (no type) always goes into Other, as on the Notes list's type chips. */
export function fold(counts: Count[], other: (n: number, rest: number) => Omit<Slice, "n" | "key"> | null) {
  const named = counts.filter((c) => c.name);
  const top = named.slice(0, SLICES);
  const restN = counts.reduce((n, c) => n + c.n, 0) - top.reduce((n, c) => n + c.n, 0);
  const rest = counts.length - top.length;
  const o = restN > 0 ? other(restN, rest) : null;
  return { top, other: o ? { ...o, key: "other", n: restN } : null };
}

export { useGlance };

const pct = (n: number, total: number) => (total ? `${Math.round((n / total) * 100)}%` : "");

/** A donut with a gap between slices, the total in the middle, and a legend that names every
 *  slice with its count, so a colour never carries the meaning alone. */
export function Pie({ title, slices, unit }: { title: string; slices: Slice[]; unit: [string, string] }) {
  const total = slices.reduce((n, s) => n + s.n, 0);
  const R = 44;
  const W = 14;
  const C = 2 * Math.PI * R;
  // A 2px gap each side of a slice, unless there's only one.
  const gap = slices.length > 1 ? 2 : 0;
  // Where each slice starts along the ring.
  const starts = slices.map((_, i) => slices.slice(0, i).reduce((n, s) => n + (s.n / total) * C, 0));
  return (
    <figure className="pie">
      <figcaption className="eyebrow">{title}</figcaption>
      {!total ? (
        <p className="faint small">None yet.</p>
      ) : (
        <div className="piebody">
          <svg
            viewBox="0 0 120 120"
            width="120"
            height="120"
            role="img"
            aria-label={`${title}: ${slices.map((s) => `${s.label} ${s.n}`).join(", ")}`}
          >
            <g transform="rotate(-90 60 60)">
              {slices.map((s, i) => {
                const len = (s.n / total) * C;
                const dash = Math.max(0, len - gap);
                return (
                  <circle
                    key={s.key}
                    className={`pieslice ${s.key === "other" ? "other" : (s.tone ?? `p${i + 1}`)} ${s.open ? "go" : ""}`}
                    cx="60"
                    cy="60"
                    r={R}
                    fill="none"
                    strokeWidth={W}
                    strokeDasharray={`${dash} ${C - dash}`}
                    strokeDashoffset={-(starts[i] + gap / 2)}
                    data-tip={s.tip}
                    onClick={s.open}
                  />
                );
              })}
            </g>
            <text x="60" y="58" textAnchor="middle" className="pietotal">
              {fmtCount(total)}
            </text>
            <text x="60" y="74" textAnchor="middle" className="pieunit">
              {total === 1 ? unit[0] : unit[1]}
            </text>
          </svg>
          <ul className="pielegend">
            {slices.map((s, i) => (
              <li key={s.key}>
                <button type="button" className="pierow" aria-disabled={!s.open} title={s.tip} onClick={s.open}>
                  <i className={`pieswatch ${s.key === "other" ? "other" : (s.tone ?? `p${i + 1}`)}`} />
                  <span className="grow ell">{s.label}</span>
                  <span className="faint">{fmtCount(s.n)}</span>
                  <span className="faint pct">{pct(s.n, total)}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </figure>
  );
}

const notesOfType = (t: string) => nav.go({ screen: "notes", view: { "notes:type": t } });
const wikiOf = (tag: string | null, type: string | null) => nav.go({ screen: "wiki", view: { "wiki:tag": tag, "wiki:type": type } });
const searchTag = (tag: string) => nav.go({ screen: "search", view: { "search:q": `tag:${tag}` } });

/** Activity's card: notes by type and by tag, and the most linked notes and wiki pages. */
export function VaultGlance({ g }: { g: Glance }) {
  const types = fold(g.noteTypes, (n) => ({
    label: "Other",
    tip: `${fmtCount(n)} ${n === 1 ? "note" : "notes"} of another type or none: show them`,
    open: () => notesOfType("other"),
  }));
  const tags = fold(g.noteTags, (n, rest) => ({
    label: `${fmtCount(rest)} other tags`,
    tip: `${fmtCount(n)} uses of ${fmtCount(rest)} other tags`,
  }));
  return (
    <div className="card glance stacked">
      <Pie
        title="Notes by type"
        unit={["note", "notes"]}
        slices={[
          ...types.top.map((c) => ({
            key: c.name,
            tone: tonesOf(g).noteType.get(c.name.toLowerCase()),
            label: c.name,
            n: c.n,
            tip: `${fmtCount(c.n)} ${c.n === 1 ? "note" : "notes"} of type ${c.name}: show them`,
            open: () => notesOfType(c.name),
          })),
          ...(types.other ? [types.other] : []),
        ]}
      />
      <Pie
        title="Notes by tag"
        unit={["tag use", "tag uses"]}
        slices={[
          ...tags.top.map((c) => ({
            key: c.name,
            tone: tonesOf(g).tag.get(c.name.toLowerCase()),
            label: `#${c.name}`,
            n: c.n,
            tip: `${fmtCount(c.n)} ${c.n === 1 ? "note" : "notes"} tagged #${c.name}: search for them`,
            open: () => searchTag(c.name),
          })),
          ...(tags.other ? [tags.other] : []),
        ]}
      />
      <MostLinked rows={g.mostLinked} />
    </div>
  );
}

function MostLinked({ rows }: { rows: Glance["mostLinked"] }) {
  return (
    <div className="mostlinked">
      <div className="eyebrow">Most linked</div>
      {!rows.length && <p className="faint small">Nothing links to anything yet.</p>}
      <ol>
        {rows.map((r) => (
          <li key={r.path}>
            <LayerTag layer={r.layer} />
            <button type="button" className="blink grow ell" title={`Open ${r.title}`} onClick={() => openDoc(r.path)}>
              {r.title}
            </button>
            <span className="faint small" title={`${fmtCount(r.links)} ${r.links === 1 ? "file links" : "files link"} here`}>
              {fmtCount(r.links)}
            </span>
          </li>
        ))}
      </ol>
    </div>
  );
}

/** Knowledge health's card: wiki pages by type and by tag. */
export function WikiGlance({ g }: { g: Glance }) {
  const types = fold(g.wikiTypes, (n) => ({
    label: "Other",
    tip: `${fmtCount(n)} ${n === 1 ? "page" : "pages"} of another type or none: show them`,
    open: () => wikiOf(null, "other"),
  }));
  const tags = fold(g.wikiTags, (n, rest) => ({
    label: `${fmtCount(rest)} other tags`,
    tip: `${fmtCount(n)} uses of ${fmtCount(rest)} other tags`,
  }));
  return (
    <div className="card hq glance stacked">
      <Pie
        title="Wiki pages by type"
        unit={["page", "pages"]}
        slices={[
          ...types.top.map((c) => ({
            key: c.name,
            tone: tonesOf(g).wikiType.get(c.name.toLowerCase()),
            label: c.name,
            n: c.n,
            tip: `${fmtCount(c.n)} ${c.n === 1 ? "page" : "pages"} of type ${c.name}: show them`,
            open: () => wikiOf(null, c.name),
          })),
          ...(types.other ? [types.other] : []),
        ]}
      />
      <Pie
        title="Wiki pages by tag"
        unit={["tag use", "tag uses"]}
        slices={[
          ...tags.top.map((c) => ({
            key: c.name,
            tone: tonesOf(g).tag.get(c.name.toLowerCase()),
            label: `#${c.name}`,
            n: c.n,
            tip: `${fmtCount(c.n)} ${c.n === 1 ? "page" : "pages"} tagged #${c.name}: show them`,
            open: () => wikiOf(c.name, null),
          })),
          ...(tags.other ? [tags.other] : []),
        ]}
      />
    </div>
  );
}
