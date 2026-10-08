// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Read view of a markdown document, with the previous app's plugins (GFM without single-tilde
// strike-through, line breaks, $$ maths, raw HTML) plus what it lacked: links to headings and
// blocks, ghost links, note embeds as links, syntax highlighting, sanitised HTML, and `$…$`
// maths that leaves money alone (src/md/math.ts).

import { templaterHtml } from "../notes/templaterSyntax";
import { explainInText } from "../editor/templaterAssist";
import { convertFileSrc } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Element } from "hast";
import { toString } from "hast-util-to-string";
import { Children, createContext, isValidElement, ReactElement, ReactNode, useContext, useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown, { Components } from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import { api } from "../api";
import { imageView } from "../ImageViewer";
import { nav, openDoc } from "../nav";
import { rehypeSearchHighlight } from "./highlight";
import { Mermaid } from "./Mermaid";
import { remarkDisplayMath, remarkPandocMath } from "./math";
import { MathTex } from "./MathTex";
import { rehypeAnchors, sanitizeSchema, splitFrontmatter, stripRanks } from "./plugins";
import { DataviewBlock, DataviewInline } from "./Dataview";
import { TaskBlock } from "./TaskBlock";
import { DateChipData, remarkFieldEmoji, remarkTaskDates } from "./dateChips";
import { FieldEmoji, iconizeEmoji } from "./fieldEmoji";
import { fmtShortDate } from "./dates";
import { daysUntil } from "./taskQuery";
import { chipOf } from "../taskFields";
import { Icon } from "../icons";
import { remarkHighlights } from "./marks";
import { remarkTags } from "./tags";
import { calloutLook, defaultTitle, remarkCallouts } from "./callouts";
import { imageSize, parseWikilink, remarkWikilinks, WikiTarget } from "./wikilinks";
import { ErrorBoundary } from "../ErrorBoundary";
import { useVaultVersion } from "../state";

const WIKI_IN_TEXT = /!?\\?\[\\?\[([^[\]\n]+?)\\?\]\\?\]/g;

/** Every wikilink target in the text, for one lookup. */
export function wikiTargets(md: string): string[] {
  const out = new Set<string>();
  for (const m of md.matchAll(WIKI_IN_TEXT)) {
    const t = parseWikilink(m[0].startsWith("!"), m[1]);
    if (t?.target) out.add(t.target);
  }
  return [...out];
}

/** Where wikilink targets go, looked up in the index: again when the vault changes, as a note
 *  linked to may have been made, renamed or trashed. */
function useResolved(targets: string[]): Map<string, string | null> | null {
  const key = targets.join("\u0000");
  const v = useVaultVersion();
  const [map, setMap] = useState<Map<string, string | null> | null>(null);
  useEffect(() => {
    let live = true;
    if (!targets.length) {
      setMap(new Map());
      return;
    }
    api
      .linksResolve(targets)
      .then((r) => live && setMap(new Map(targets.map((t, i) => [t, r[i]]))))
      .catch(() => live && setMap(new Map()));
    return () => {
      live = false;
    };
  }, [key, v]); // eslint-disable-line react-hooks/exhaustive-deps
  return map;
}

/** `decodeURIComponent`, or the text as it is when a stray `%` makes it malformed. */
export function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

/** JSON put on an element by this module's plugins, or null when raw HTML in the note set it. */
function attrJson<T>(raw: string): T | null {
  try {
    const v = JSON.parse(raw) as unknown;
    return v && typeof v === "object" ? (v as T) : null;
  } catch {
    return null;
  }
}

const join = (root: string, rel: string) => `${root.replace(/[\\/]$/, "")}/${rel.replace(/^\/+/, "")}`;

/** Where an image's src points: a URL to use as it is, a name to look up, or a vault path.
 *  the previous app's own URLs (`/api/vault-assets/<path>`, `/api/vault-assets/by-name/<name>`), which
 *  notes written there contain in raw `<img>` tags, map back to the vault. */
export function imageSource(src: string): { url: string } | { name: string } | { path: string } {
  if (/^(https?:|data:|blob:)/.test(src)) return { url: src };
  if (src.startsWith("wiki:")) return { name: safeDecode(src.slice(5)) };
  const ww = /^\/api\/vault-assets\/(by-name\/)?(.+)$/.exec(src);
  if (ww) return ww[1] ? { name: safeDecode(ww[2]) } : { path: safeDecode(ww[2]) };
  return { path: safeDecode(src.replace(/^\/+/, "")) };
}

/** An image in the vault: by name for `![[pic.png]]`, else relative to the vault (as the previous
 *  app resolves it), then to the note's folder. */
function VaultImage({
  src,
  alt,
  root,
  dir,
  width,
  height,
}: {
  src: string;
  alt: string;
  root: string;
  dir: string;
  width?: string | number;
  height?: string | number;
}) {
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const where = imageSource(src);
  useEffect(() => {
    let live = true;
    setFailed(false);
    const w = imageSource(src);
    if ("url" in w) setUrl(w.url);
    else if ("name" in w)
      api
        .assetFind(w.name)
        .then((p) => live && (p ? setUrl(convertFileSrc(join(root, p))) : setFailed(true)))
        .catch(() => live && setFailed(true));
    else setUrl(convertFileSrc(join(root, w.path)));
    return () => {
      live = false;
    };
  }, [src, root]);
  if (failed)
    return (
      <span className="ghostimg" title={`No file called ${alt || src} in the vault`}>
        🖼 {alt || src}
      </span>
    );
  if (!url) return null;
  return (
    <img
      src={url}
      alt={alt}
      width={width}
      height={height}
      // CSS keeps it in proportion (height: auto); a height alone still sizes it.
      style={height && !width ? { height: `${String(height).replace(/px$/, "")}px` } : undefined}
      loading="lazy"
      className="zoomable"
      title="Show the image full size"
      onClick={(e) => imageView.set({ url: e.currentTarget.currentSrc || url, alt })}
      onError={(e) => {
        // Not under the vault: try beside the note.
        const beside = "path" in where && dir ? convertFileSrc(join(root, `${dir}/${where.path}`)) : null;
        if (beside && e.currentTarget.src !== beside) e.currentTarget.src = beside;
        else setFailed(true);
      }}
    />
  );
}

/** The system-note callout: a quote whose first paragraph starts with bold "This is a system note". */
function isSystemNote(node: Element | undefined): boolean {
  const p = node?.children.find((c): c is Element => c.type === "element" && c.tagName === "p");
  const strong = p?.children.find((c): c is Element => c.type === "element" && c.tagName === "strong");
  return !!strong && toString(strong).trim().startsWith("This is a system note");
}

function SystemNote({ path, children }: { path: string; children: ReactNode }) {
  const key = `system-note:${path}`;
  const [open, setOpen] = useState(() => {
    try {
      return localStorage.getItem(key) !== "closed";
    } catch {
      return true;
    }
  });
  return (
    <details
      className="sysnote"
      open={open}
      onToggle={(e) => {
        const o = (e.currentTarget as HTMLDetailsElement).open;
        setOpen(o);
        try {
          localStorage.setItem(key, o ? "open" : "closed");
        } catch {
          // A remembered fold is a nicety.
        }
      }}
    >
      <summary>System note</summary>
      <blockquote>{children}</blockquote>
    </details>
  );
}

/** A callout (src/md/callouts.ts): a tinted box with its type's icon and a title, folding when
 *  written with `+` (open) or `-` (closed). */
function Callout({ type, fold, children }: { type: string; fold: string; children: ReactNode }) {
  const look = calloutLook(type);
  const items = Children.toArray(children);
  const at = items.findIndex((c) => isValidElement(c) && (c.props as Record<string, unknown>)["data-callout-title"] !== undefined);
  const written = at >= 0 ? (items[at] as ReactElement<{ children?: ReactNode }>).props.children : null;
  const hasTitle = Children.toArray(written).some((c) => typeof c !== "string" || c.trim());
  const body = items.filter((c, i) => i !== at && (typeof c !== "string" || c.trim()));
  const head = (
    <>
      <Icon name={look.icon} size={15} className="callout-icon" />
      <span className="callout-title">{hasTitle ? written : defaultTitle(type)}</span>
    </>
  );
  const inner = body.length ? <div className="callout-body">{body}</div> : null;
  if (fold)
    return (
      <details className="callout" data-tone={look.tone} open={fold === "+"}>
        <summary className="callout-head">
          {head}
          <Icon name="chevright" size={13} className="callout-chev" />
        </summary>
        {inner}
      </details>
    );
  return (
    <div className="callout" data-tone={look.tone}>
      <div className="callout-head">{head}</div>
      {inner}
    </div>
  );
}

export interface MarkdownProps {
  content: string;
  /** The document's vault path: links to the same note, image folders, remembered folds. */
  path: string;
  /** The vault's real path, for image URLs. */
  root: string;
  /** Words to highlight. */
  q?: string;
  /** Makes task checkboxes tickable: `line` is 0-based in `content`. */
  onTick?: (line: number, done: boolean) => void;
  /** A wiki page's sources, by vault path in their order: links to them (and to any other file in
   *  sources/) are drawn as numbered citation markers. */
  cites?: string[];
  /** The citation numbers' order, as drawn: the sources above, then others the page links. */
  onCites?: (order: string[]) => void;
  /** Runs dataviewjs, inline `$=` and Tasks `by function` code: only for a note the user wrote
   *  (see scripts.ts). Off, a quiet note stands where a script would run. */
  scripts?: boolean;
  /** A template: its Templater tags are drawn as coloured code (src/notes/templaterSyntax.ts). */
  templater?: boolean;
}

/** The source line of the task list item being drawn, for its checkbox. */
const TaskLine = createContext<number | null>(null);

/** A document drawn as markdown; a note that fails to draw says so instead of taking the window with it. */
export function Markdown(props: MarkdownProps) {
  return (
    <ErrorBoundary
      resetKey={props.content}
      fallback={(e) => (
        <div className="prose">
          <p className="faint">This couldn’t be drawn: {e.message}</p>
        </div>
      )}
    >
      <MarkdownView {...props} />
    </ErrorBoundary>
  );
}

function MarkdownView({ content, path, root, q, onTick, cites, onCites, scripts = false, templater = false }: MarkdownProps) {
  const { body, offset, raw } = useMemo(() => {
    const b = splitFrontmatter(content).body;
    // Lines before the body (the properties), so a list item's line in the body maps to the file.
    const before = content.replace(/^\uFEFF/, "").slice(0, content.replace(/^\uFEFF/, "").length - b.length);
    const raw = stripRanks(b);
    return { body: templater ? templaterHtml(raw) : raw, offset: before.split("\n").length - 1, raw };
  }, [content, templater]);
  const tip = useTemplateHover(templater ? raw : null);
  const tickRef = useRef(onTick);
  useEffect(() => {
    tickRef.current = onTick;
  });
  const tickable = !!onTick;
  const targets = useMemo(() => wikiTargets(body), [body]);
  const resolved = useResolved(targets);
  const dir = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
  // Citation numbers: the page's sources first, then other sources it links, as they come.
  const citeOrder = useMemo(() => {
    if (!cites) return null;
    const list = [...cites];
    for (const t of targets) {
      const p = resolved?.get(t);
      if (p && p.startsWith("sources/") && !list.includes(p)) list.push(p);
    }
    return list;
  }, [cites, targets, resolved]);
  const citesRef = useRef(onCites);
  useEffect(() => {
    citesRef.current = onCites;
  });
  const citeKey = citeOrder?.join("\n") ?? "";
  useEffect(() => {
    if (citeOrder) citesRef.current?.(citeOrder);
  }, [citeKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const components: Components = useMemo(
    () => ({
      a: ({ href, children, node }) => {
        const wikiRaw = node?.properties?.dataWiki as string | undefined;
        const wiki = wikiRaw ? attrJson<WikiTarget>(wikiRaw) : null;
        if (wiki) {
          const t = wiki;
          const to = t.target ? resolved?.get(t.target) : path;
          const pending = !resolved && !!t.target;
          const ghost = !pending && !to;
          const anchor = t.heading ?? (t.block ? `^${t.block}` : undefined);
          const n = to && !t.embed && citeOrder ? citeOrder.indexOf(to) : -1;
          if (n >= 0)
            return (
              <a
                href="#"
                className="cite"
                title={`${(to ?? "").split("/").pop()?.replace(/\.md$/, "")}${anchor ? ` · ${anchor}` : ""}`}
                onClick={(e) => {
                  e.preventDefault();
                  openDoc(to!, anchor ? { anchor } : {});
                }}
              >
                {n + 1}
              </a>
            );
          return (
            <a
              href="#"
              className={`wl ${ghost ? "ghost" : ""} ${t.embed ? "embed" : ""}`}
              title={ghost ? `No note called “${t.target}” yet` : (to ?? undefined)}
              onClick={(e) => {
                e.preventDefault();
                if (to) openDoc(to, anchor ? { anchor } : {});
              }}
            >
              {t.embed && <span className="embedmark">↪ </span>}
              {children}
            </a>
          );
        }
        if (href && /^(https?:|mailto:)/.test(href)) {
          return (
            <a
              href={href}
              title={href}
              onClick={(e) => {
                e.preventDefault();
                void openUrl(href);
              }}
            >
              {children}
            </a>
          );
        }
        // A markdown link to another file in the vault, relative to the vault or this note.
        if (href && !href.startsWith("#") && !/^[a-z]+:/i.test(href)) {
          const rel = safeDecode(href.split("#")[0]);
          return (
            <a
              href="#"
              className="wl"
              onClick={(e) => {
                e.preventDefault();
                void api.linksResolve([rel]).then(([p]) => openDoc(p ?? (dir ? `${dir}/${rel}` : rel)));
              }}
            >
              {children}
            </a>
          );
        }
        return <a href={href}>{children}</a>;
      },
      img: ({ src, alt, width, height }) => {
        // `![A chart|400](pic.png)`: the size is in the alt text.
        const size = imageSize(alt);
        return typeof src === "string" ? (
          <VaultImage src={src} alt={size.alt} root={root} dir={dir} width={width ?? size.width} height={height ?? size.height} />
        ) : null;
      },
      pre: ({ children, node }) => {
        const code = node?.children[0] as Element | undefined;
        const cls = (code?.properties?.className as string[] | undefined) ?? [];
        const lang = cls.find((c) => c.startsWith("language-"))?.slice(9);
        const src = code ? toString(code).replace(/\n$/, "") : "";
        if (lang === "dataview" || lang === "dataviewjs") return <DataviewBlock lang={lang} src={src} path={path} scripts={scripts} />;
        if (lang === "tasks") return <TaskBlock src={src} path={path} scripts={scripts} />;
        if (lang === "mermaid") return <Mermaid src={src} />;
        if (lang === "math") return <MathTex tex={src} display raw={`$$\n${src}\n$$`} />;
        return <pre>{children}</pre>;
      },
      // Inline Dataview: `= expression` and `$= script`.
      code: ({ node, children, className, ...props }) => {
        const text = node ? toString(node) : "";
        // Maths: `$…$`, and `$$…$$` within a line (src/md/math.ts).
        if (className?.includes("language-math")) {
          const display = node?.properties?.dataMath === "display";
          return <MathTex tex={text} display={display} raw={display ? `$$${text}$$` : `$${text}$`} />;
        }
        if (!className && /^\$?=\s/.test(text) && !text.includes("\n")) return <DataviewInline code={text} path={path} scripts={scripts} />;
        return (
          <code className={className} {...props}>
            {className ? children : iconizeEmoji(children)}
          </code>
        );
      },
      blockquote: ({ children, node }) => {
        const type = node?.properties?.dataCallout;
        if (typeof type === "string")
          return (
            <Callout type={type} fold={String(node?.properties?.dataFold ?? "")}>
              {children}
            </Callout>
          );
        return isSystemNote(node) ? <SystemNote path={path}>{children}</SystemNote> : <blockquote>{children}</blockquote>;
      },
      span: ({ node, children, ...props }) => {
        const tag = node?.properties?.dataTag as string | undefined;
        if (tag)
          return (
            <span
              className="mdtag"
              role="link"
              tabIndex={0}
              title={`Find notes tagged #${tag}`}
              onClick={() => nav.go({ screen: "search", q: `tag:${tag}` })}
              onKeyDown={(e) => e.key === "Enter" && nav.go({ screen: "search", q: `tag:${tag}` })}
            >
              {children}
            </span>
          );
        const emoji = node?.properties?.dataFemoji as string | undefined;
        if (emoji) return <FieldEmoji emoji={emoji} />;
        const raw = node?.properties?.dataTdate as string | undefined;
        const chip = raw ? attrJson<DateChipData>(raw) : null;
        if (!chip || typeof chip.day !== "string") return <span {...props}>{children}</span>;
        return <TaskDateChip {...chip} />;
      },
      li: ({ node, children, ...props }) => {
        const line = node?.position?.start.line;
        const isTask = String(props.className ?? "").includes("task-list-item");
        const task = tickable && line !== undefined && isTask;
        // A done task's own text is struck through, not the tasks nested under it.
        const body = isTask && node && isDone(node) ? strikeOwnText(children) : children;
        return <li {...props}>{task ? <TaskLine.Provider value={line - 1 + offset}>{body}</TaskLine.Provider> : body}</li>;
      },
      input: ({ type, checked }) => (type === "checkbox" ? <TaskCheckbox checked={!!checked} onTick={tickRef} /> : null),
    }),
    [resolved, path, root, dir, tickable, offset, citeOrder, scripts],
  );

  return (
    <div className="prose" onMouseOver={tip.over} onMouseLeave={tip.leave}>
      {tip.node}
      <ReactMarkdown
        remarkPlugins={[
          [remarkGfm, { singleTilde: false }],
          // Before line breaks, so the title is still the first line of the text.
          remarkCallouts,
          remarkBreaks,
          [remarkMath, { singleDollarTextMath: false }],
          remarkPandocMath,
          remarkDisplayMath,
          remarkWikilinks,
          remarkTaskDates,
          remarkFieldEmoji,
          remarkTags,
          remarkHighlights,
        ]}
        rehypePlugins={[
          rehypeRaw,
          [rehypeSanitize, sanitizeSchema],
          [rehypeHighlight, { detect: false, plainText: ["tasks", "dataview", "dataviewjs", "mermaid", "math"] }],
          rehypeAnchors,
          rehypeSearchHighlight(q),
        ]}
        components={components}
        urlTransform={(url) => url}
      >
        {body}
      </ReactMarkdown>
    </div>
  );
}

/** A template in View: hovering a name in a `<% %>` tag explains it, as the editor's hover does
 *  (src/editor/templaterAssist.ts). Each token says where it starts in the template (`data-tp`). */
function useTemplateHover(text: string | null) {
  const [at, setAt] = useState<{ x: number; y: number; dom: Node } | null>(null);
  const seq = useRef(0);
  const closing = useRef<number | undefined>(undefined);
  const stay = () => window.clearTimeout(closing.current);
  // Closed a moment after the pointer leaves the word, so it can cross the gap to the box (and click
  // its Docs link); kept open while it's over the box.
  const close = () => {
    stay();
    closing.current = window.setTimeout(() => {
      seq.current++;
      setAt(null);
    }, 300);
  };
  useEffect(() => stay, []);
  const over = (e: React.MouseEvent) => {
    if (text === null) return;
    const t = e.target as HTMLElement;
    if (t.closest(".tpview")) return stay();
    const el = t.closest<HTMLElement>("[data-tp]");
    if (!el) return close();
    stay();
    const n = ++seq.current;
    const r = el.getBoundingClientRect();
    void explainInText(text, Number(el.dataset.tp) + 1).then((dom) => {
      if (n === seq.current) setAt(dom ? { x: r.left, y: r.top, dom } : null);
    });
  };
  const node = at ? <TipBox x={at.x} y={at.y} dom={at.dom} /> : null;
  return { over, leave: close, node };
}

function TipBox({ x, y, dom }: { x: number; y: number; dom: Node }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.replaceChildren(dom);
  }, [dom]);
  // Above the word, as in the editor; the box sizes itself to its text.
  return <div ref={ref} className="cm-tooltip cm-tpinfo tpview" style={{ left: x, top: y }} role="tooltip" />;
}

/** Whether a task list item's own checkbox is ticked (directly, or in its first paragraph). */
function isDone(li: Element): boolean {
  for (const c of li.children) {
    if (c.type !== "element") continue;
    if (c.tagName === "input") return !!c.properties?.checked;
    if (c.tagName === "p") return isDone(c);
  }
  return false;
}

/** The item's children with everything but nested lists wrapped to be struck through. */
function strikeOwnText(children: ReactNode): ReactNode {
  const out: ReactNode[] = [];
  let run: ReactNode[] = [];
  const flush = () => {
    if (run.some((c) => typeof c !== "string" || c.trim()))
      out.push(
        <span key={`d${out.length}`} className="tdone">
          {run}
        </span>,
      );
    else out.push(...run);
    run = [];
  };
  for (const c of Children.toArray(children)) {
    if (isValidElement(c) && (c.type === "ul" || c.type === "ol")) {
      flush();
      out.push(c);
    } else run.push(c);
  }
  flush();
  return out;
}

/** A task's date in View: its icon and the day; due dates turn amber when close and red when past. */
export function TaskDateChip({ id, day, done }: DateChipData) {
  const c = chipOf(id, day, fmtShortDate);
  if (!c) return <>{day}</>;
  const isDay = /^\d{4}-\d{2}-\d{2}$/.test(day);
  const n = isDay ? daysUntil(day) : 0;
  const tone = id === "due" && isDay && !done ? (n < 0 ? "is-late" : n <= 3 ? "is-soon" : "") : "";
  return (
    <span className={`tdate ${id} ${tone}`} title={c.tip}>
      <Icon name={c.icon} size={12} />
      {c.text}
    </span>
  );
}

function TaskCheckbox({ checked, onTick }: { checked: boolean; onTick: React.RefObject<MarkdownProps["onTick"]> }) {
  const line = useContext(TaskLine);
  if (line === null || !onTick.current) return <input type="checkbox" checked={checked} readOnly disabled />;
  return (
    <input
      type="checkbox"
      className="tick"
      checked={checked}
      aria-label={checked ? "Untick" : "Tick"}
      onChange={(e) => onTick.current?.(line, e.target.checked)}
      onClick={(e) => e.stopPropagation()}
    />
  );
}
