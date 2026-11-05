// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Dataview values drawn in a document: links that open, text with its inline markdown, lists and
// objects, and task text with its emoji fields (dates, priority, repeat, cancelled) drawn as the
// app's line icons — no emoji is ever shown; the file keeps them.

import { openUrl } from "@tauri-apps/plugin-opener";
import { DateTime, Duration } from "luxon";
import { ReactNode } from "react";
import { api } from "../../api";
import { Icon } from "../../icons";
import { nav, openDoc } from "../../nav";
import { taskDateByEmoji } from "../../taskDates";
import { TaskDateChip } from "../Markdown";
import { isExternalHref, tokenize, useRefPatterns } from "../refs";
import { parseWikilink, wikiLabel } from "../wikilinks";
import { durationText, Link, toText, typeOf, Value } from "./values";

export function DvLink({ link }: { link: Link }) {
  return (
    <a
      href="#"
      className={`wl ${link.embed ? "embed" : ""}`}
      title={link.path}
      onClick={(e) => {
        e.preventDefault();
        openDoc(
          link.path.endsWith(".md") || link.path.includes(".") ? link.path : `${link.path}.md`,
          link.subpath ? { anchor: link.subpath } : {},
        );
      }}
    >
      {link.label()}
    </a>
  );
}

const PRIORITY: Record<string, [string, string]> = {
  "🔺": ["highest", "Highest priority"],
  "⏫": ["high", "High priority"],
  "🔼": ["medium", "Medium priority"],
  "🔽": ["low", "Low priority"],
  "⏬": ["lowest", "Lowest priority"],
};

/** Inline markdown in a value: links, bold, italic, code, tags, reference keys (Settings › Notes ›
 *  Reference links), and task fields as icons. */
export function DvText({ text, done = false }: { text: string; done?: boolean }) {
  const patterns = useRefPatterns();
  const re =
    /(`[^`\n]+`|!?\[\[[^[\]\n]+?\]\]|\[[^\]\n]+\]\([^)\s]+\)|\*\*[^*]+\*\*|\*[^*\s][^*]*\*|(?:^|(?<=\s))#[\p{L}\p{N}_/-]*[\p{L}_/-][\p{L}\p{N}_/-]*|[📅⏳🛫➕✅❌]️?\s*\d{4}-\d{2}-\d{2}|🗓️?\s*\d{4}-\d{2}-\d{2}|[🔺⏫🔼🔽⏬]️?|🔁️?\s*[^📅⏳🛫➕✅❌🆔⛔🏁🔺⏫🔼🔽⏬^]+|🆔\s*[\w-]+|⛔\s*[\w,-]+|🏁\s*\w+|\s*\^rank-\d+\s*$)/gu;
  const out: ReactNode[] = [];
  let last = 0;
  let k = 0;
  // Words between the pieces above, with any reference key in them as a link.
  const words = (s: string) => {
    for (const seg of tokenize(s, { patterns, links: false })) {
      if (!("href" in seg)) out.push(seg.text);
      else {
        const href = seg.href;
        out.push(
          <a
            key={k++}
            href={href}
            title={href}
            className="mdref"
            onClick={(e) => {
              e.preventDefault();
              void openUrl(href);
            }}
          >
            {seg.text}
          </a>,
        );
      }
    }
  };
  for (const m of text.matchAll(re)) {
    if (m.index! > last) words(text.slice(last, m.index));
    last = m.index! + m[0].length;
    out.push(<Piece key={k++} p={m[0]} done={done} />);
  }
  if (last < text.length) words(text.slice(last));
  return <>{out}</>;
}

function Piece({ p, done }: { p: string; done: boolean }) {
  if (p.startsWith("`")) return <code>{p.slice(1, -1)}</code>;
  if (/^!?\[\[/.test(p)) {
    const t = parseWikilink(p.startsWith("!"), p.replace(/^!?\[\[|\]\]$/g, ""));
    if (!t) return <>{p}</>;
    return (
      <a
        href="#"
        className="wl"
        onClick={(e) => {
          e.preventDefault();
          void api.linksResolve([t.target]).then(([to]) => to && openDoc(to, t.heading ? { anchor: t.heading } : {}));
        }}
      >
        {wikiLabel(t)}
      </a>
    );
  }
  if (p.startsWith("[")) {
    const m = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(p)!;
    return (
      <a
        href={m[2]}
        onClick={(e) => {
          e.preventDefault();
          if (isExternalHref(m[2])) void openUrl(m[2]);
        }}
      >
        {m[1]}
      </a>
    );
  }
  if (p.startsWith("**")) return <b>{p.slice(2, -2)}</b>;
  if (p.startsWith("*")) return <i>{p.slice(1, -1)}</i>;
  if (p.startsWith("#"))
    return (
      <span className="mdtag" role="link" tabIndex={0} onClick={() => nav.go({ screen: "search", q: `tag:${p.slice(1)}` })}>
        {p}
      </span>
    );
  if (/^\s*\^rank-/.test(p)) return null;
  const date = /^([📅⏳🛫➕✅❌🗓])️?\s*(\d{4}-\d{2}-\d{2})/u.exec(p);
  if (date) {
    if (date[1] === "❌")
      return (
        <span className="tdate cancelled" title={`Cancelled ${date[2]}`}>
          <Icon name="cancelled" size={12} />
          {date[2]}
        </span>
      );
    const d = taskDateByEmoji(date[1] === "🗓" ? "📅" : date[1]);
    return d ? <TaskDateChip id={d.id} day={date[2]} done={done} /> : <>{date[2]}</>;
  }
  const pr = PRIORITY[p.replace(/️/g, "")];
  if (pr)
    return (
      <span className={`tdate prio ${pr[0]}`} title={pr[1]} aria-label={pr[1]}>
        <Icon name="priority" size={12} />
      </span>
    );
  if (p.startsWith("🔁")) {
    const rule = p.replace(/^🔁️?\s*/, "").trim();
    return (
      <span className="tdate repeat" title={`Repeats ${rule}`}>
        <Icon name="repeat" size={12} />
        {rule}
      </span>
    );
  }
  if (p.startsWith("🆔"))
    return (
      <span className="tdate dvmeta" title="Task id">
        id {p.replace(/^🆔\s*/, "")}
      </span>
    );
  if (p.startsWith("⛔"))
    return (
      <span className="tdate dvmeta" title="Waits for these tasks">
        after {p.replace(/^⛔\s*/, "")}
      </span>
    );
  if (p.startsWith("🏁"))
    return (
      <span className="tdate dvmeta" title="When done">
        when done: {p.replace(/^🏁\s*/, "")}
      </span>
    );
  return <>{p}</>;
}

/** Any Dataview value, as Dataview draws it in a table cell or list item. */
export function DvValue({ v, nested = false }: { v: Value; nested?: boolean }): ReactNode {
  switch (typeOf(v)) {
    case "null":
      return <span className="faint">{nested ? "null" : "-"}</span>;
    case "string":
      return <DvText text={v as string} />;
    case "number":
    case "boolean":
      return <>{String(v)}</>;
    case "date":
      return <>{toText(v as DateTime)}</>;
    case "duration":
      return <>{durationText(v as Duration)}</>;
    case "link":
      return <DvLink link={v as Link} />;
    case "array": {
      const xs = v as Value[];
      if (!xs.length) return <span className="faint">-</span>;
      if (xs.length === 1 && !nested) return <DvValue v={xs[0]} nested />;
      return (
        <ul className="dvcell">
          {xs.map((x, i) => (
            <li key={i}>
              <DvValue v={x} nested />
            </li>
          ))}
        </ul>
      );
    }
    case "function":
      return <span className="faint">&lt;function&gt;</span>;
    case "object":
    default: {
      const o = v as Record<string, Value>;
      return (
        <ul className="dvcell">
          {Object.entries(o).map(([k, x]) => (
            <li key={k}>
              <b>{k}</b>: <DvValue v={x} nested />
            </li>
          ))}
        </ul>
      );
    }
  }
}
