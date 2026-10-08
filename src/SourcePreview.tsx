// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Sources that aren't markdown: PDFs through pdf.js (Tauri can't show them reliably in a frame),
// Word, PowerPoint and Excel files as the text Brainstead reads from them, with Quick Look for how
// they look (no web renderer draws Office files well), images inline with the text read from them,
// other text as text.

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "./icons";
import { SearchBox } from "./ui";
import { api, FileSummary } from "./api";
import { features, useFeatures } from "./features";
import { isMac } from "./platform";

const ext = (p: string) => p.slice(p.lastIndexOf(".") + 1).toLowerCase();
const IMAGE = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif", "heic"]);
const TEXT = new Set(["txt", "md", "html", "htm", "json", "csv", "log", "yaml", "yml", "xml", "eml", "vtt", "srt"]);

export function sourceKind(path: string): "pdf" | "office" | "image" | "text" | "other" {
  const e = ext(path);
  if (e === "pdf") return "pdf";
  if (e === "docx" || e === "pptx" || e === "xlsx") return "office";
  if (IMAGE.has(e)) return "image";
  if (TEXT.has(e)) return "text";
  return "other";
}

/** The badge in the sources list: PDF, DOCX, PNG, MD… */
export function sourceBadge(path: string): string {
  const e = ext(path);
  return e && e.length <= 5 ? e.toUpperCase() : "FILE";
}

const urlOf = (root: string, path: string) => convertFileSrc(`${root.replace(/\/$/, "")}/${path}`);

export function SourcePreview({ file, root, content }: { file: FileSummary; root: string; content: string }) {
  const kind = sourceKind(file.path);
  const url = urlOf(root, file.path);
  if (kind === "image") return <ImageView path={file.path} url={url} alt={file.title} />;
  if (kind === "pdf") return <PdfView url={url} />;
  if (kind === "office") return <OfficeView path={file.path} />;
  if (kind === "text") return <pre className="srctext">{content}</pre>;
  return (
    <div className="srcnone">
      <p className="muted">Brainstead can't show this kind of file{features.get().quickLook ? "; Quick Look may" : ""}.</p>
      <div className="officebar">
        <QuickLook path={file.path} />
        <OpenInApp path={file.path} />
      </div>
    </div>
  );
}

/** An image, with a folding panel of the text read from it (the same text ingest and search use).
 *  An SVG has none to read. */
function ImageView({ path, url, alt }: { path: string; url: string; alt: string }) {
  const [text, setText] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const readable = ext(path) !== "svg";
  useEffect(() => {
    if (!open || !readable) return;
    let live = true;
    setText(null);
    void api
      .sourceParts(path)
      .then((x) => live && setText(x.parts.map((p) => p.text).join("\n\n")))
      .catch((e) => live && setText(`Couldn't read it: ${String(e)}`));
    return () => {
      live = false;
    };
  }, [path, open, readable]);
  return (
    <div className="srcimgview">
      <img className="srcimg" src={url} alt={alt} />
      {readable && (
        <details className="srcocr" onToggle={(e) => setOpen(e.currentTarget.open)}>
          <summary title="Show the text Brainstead read from the image, which ingest and search use">Text in the image</summary>
          {text === null ? (
            <p className="faint">Reading…</p>
          ) : text.trim() ? (
            <pre className="srctext">{text}</pre>
          ) : (
            <p className="faint">No text found in it.</p>
          )}
        </details>
      )}
    </div>
  );
}

/** Opens a vault file in its own app, through the app (`file_open`), which opens only vault files. */
function OpenInApp({ path }: { path: string }) {
  const [err, setErr] = useState<string | null>(null);
  return (
    <>
      <button
        type="button"
        className="btn"
        title={`Open this file in the app ${isMac ? "macOS" : "the system"} uses for it`}
        onClick={() => {
          setErr(null);
          invoke("file_open", { path }).catch((e) => setErr(String(e?.message ?? e)));
        }}
      >
        Open in its app
      </button>
      {err && <p className="muted">{err}</p>}
    </>
  );
}

/** A PDF, page by page, with page navigation, zoom and find (the text of each page, from pdf.js). */
function PdfView({ url }: { url: string }) {
  const box = useRef<HTMLDivElement>(null);
  const [err, setErr] = useState<string | null>(null);
  const [pages, setPages] = useState(0);
  const [cur, setCur] = useState(1);
  // 1 fits the width; the steps go by quarters.
  const [zoom, setZoom] = useState(1);
  const [texts, setTexts] = useState<string[]>([]);
  const [q, setQ] = useState("");
  const [hit, setHit] = useState(0);
  const doc = useRef<import("pdfjs-dist").PDFDocumentProxy | null>(null);

  // Load once per file: the document and every page's text, for find.
  useEffect(() => {
    let live = true;
    doc.current = null;
    setPages(0);
    setTexts([]);
    (async () => {
      const pdfjs = await import("pdfjs-dist");
      pdfjs.GlobalWorkerOptions.workerSrc = new URL("pdfjs-dist/build/pdf.worker.min.mjs", import.meta.url).toString();
      const data = new Uint8Array(await (await fetch(url)).arrayBuffer());
      const pdf = await pdfjs.getDocument({ data }).promise;
      if (!live) return;
      doc.current = pdf;
      setPages(pdf.numPages);
      const t: string[] = [];
      for (let i = 1; i <= pdf.numPages && live; i++) {
        const c = await (await pdf.getPage(i)).getTextContent();
        t.push(c.items.map((x) => ("str" in x ? x.str : "")).join(" "));
      }
      if (live) setTexts(t);
    })().catch((e) => live && setErr(String(e?.message ?? e)));
    return () => {
      live = false;
    };
  }, [url]);

  // Draw the pages at the zoom.
  useEffect(() => {
    let live = true;
    const el = box.current;
    const pdf = doc.current;
    if (!el || !pdf) return;
    el.replaceChildren();
    (async () => {
      const width = (el.clientWidth || 760) * zoom;
      for (let i = 1; i <= pdf.numPages && live; i++) {
        const page = await pdf.getPage(i);
        const v1 = page.getViewport({ scale: 1 });
        const scale = width / v1.width;
        const vp = page.getViewport({ scale: scale * window.devicePixelRatio });
        const c = document.createElement("canvas");
        c.width = vp.width;
        c.height = vp.height;
        c.style.width = `${width}px`;
        c.className = "pdfpage";
        c.dataset.page = String(i);
        el.appendChild(c);
        await page.render({ canvas: c, canvasContext: c.getContext("2d")!, viewport: vp }).promise;
      }
    })().catch((e) => live && setErr(String(e?.message ?? e)));
    return () => {
      live = false;
    };
  }, [pages, zoom]);

  // The page most in view.
  useEffect(() => {
    const el = box.current;
    if (!el || !pages) return;
    const io = new IntersectionObserver(
      (es) => {
        const v = es.filter((e) => e.isIntersecting).sort((a, b) => b.intersectionRatio - a.intersectionRatio)[0];
        if (v) setCur(Number((v.target as HTMLElement).dataset.page));
      },
      { threshold: [0.25, 0.5, 0.75] },
    );
    const watch = () => el.querySelectorAll("canvas").forEach((c) => io.observe(c));
    watch();
    const mo = new MutationObserver(watch);
    mo.observe(el, { childList: true });
    return () => {
      io.disconnect();
      mo.disconnect();
    };
  }, [pages, zoom]);

  const go = (n: number) => {
    const p = Math.min(pages, Math.max(1, n));
    box.current?.querySelector(`canvas[data-page="${p}"]`)?.scrollIntoView({ block: "start", behavior: "smooth" });
    setCur(p);
  };
  const found = useMemo(() => {
    const w = q.trim().toLowerCase();
    if (w.length < 2) return [];
    return texts.map((t, i) => [i + 1, t.toLowerCase().split(w).length - 1] as const).filter(([, n]) => n > 0);
  }, [q, texts]);
  const jumped = useRef(false);
  useEffect(() => {
    setHit(0);
    jumped.current = false;
  }, [q]);
  const snippet = (page: number) => {
    const t = texts[page - 1] ?? "";
    const at = t.toLowerCase().indexOf(q.trim().toLowerCase());
    return at < 0 ? "" : `…${t.slice(Math.max(0, at - 40), at + q.trim().length + 60).trim()}…`;
  };

  return (
    <div className="pdfview">
      {pages > 0 && (
        <div className="pdfbar">
          <span title="Previous page">
            <button type="button" className="ibtn sm" aria-label="Previous page" disabled={cur <= 1} onClick={() => go(cur - 1)}>
              <Icon name="back" size={13} />
            </button>
          </span>
          <span className="faint small">
            Page {cur} of {pages}
          </span>
          <span title="Next page">
            <button type="button" className="ibtn sm" aria-label="Next page" disabled={cur >= pages} onClick={() => go(cur + 1)}>
              <Icon name="forward" size={13} />
            </button>
          </span>
          <span className="sep" />
          <span title="Zoom out">
            <button
              type="button"
              className="ibtn sm"
              aria-label="Zoom out"
              disabled={zoom <= 0.5}
              onClick={() => setZoom((z) => Math.max(0.5, z - 0.25))}
            >
              <Icon name="minus" size={13} />
            </button>
          </span>
          <button type="button" className="btn sm ghost" title="Fit the width" onClick={() => setZoom(1)}>
            {Math.round(zoom * 100)}%
          </button>
          <span title="Zoom in">
            <button
              type="button"
              className="ibtn sm"
              aria-label="Zoom in"
              disabled={zoom >= 3}
              onClick={() => setZoom((z) => Math.min(3, z + 0.25))}
            >
              <Icon name="plus" size={13} />
            </button>
          </span>
          <span className="grow" />
          <SearchBox
            value={q}
            onChange={setQ}
            placeholder="Find in this file"
            className="pdffind"
            onKeyDown={(e) => {
              if (e.key === "Escape" && q) {
                setQ("");
                e.preventDefault();
                return;
              }
              if (e.key !== "Enter" || !found.length) return;
              // The first Enter shows the first page found; then ↩ the next, ⇧↩ the one before.
              const n = jumped.current ? (hit + (e.shiftKey ? -1 : 1) + found.length) % found.length : 0;
              jumped.current = true;
              setHit(n);
              go(found[n][0]);
            }}
          >
            {q.trim().length >= 2 && (
              <span className="faint small nav">
                {found.length
                  ? `${found.reduce((n, [, c]) => n + c, 0)} on ${found.length} page${found.length === 1 ? "" : "s"}`
                  : texts.length
                    ? "Not found"
                    : "Reading…"}
              </span>
            )}
          </SearchBox>
        </div>
      )}
      {found.length > 0 && (
        <div className="pdfhits">
          {found.slice(0, 12).map(([p]) => (
            <button
              key={p}
              type="button"
              className={`blink ${found[hit]?.[0] === p ? "on" : ""}`}
              title={`Go to page ${p}`}
              onClick={() => go(p)}
            >
              <span className="lb source">Page {p}</span>
              <span className="faint">{snippet(p)}</span>
            </button>
          ))}
        </div>
      )}
      {err && <p className="err">Couldn't show this PDF: {err}</p>}
      <div ref={box} className="pdfpages" />
    </div>
  );
}

/** PowerPoint as its slides' text, Excel as its sheets (tab-separated cells drawn as tables),
 *  from the same extraction ingest uses. */
/** Shows a vault file in the Quick Look panel; Space does the same while the preview is showing. */
function QuickLook({ path }: { path: string }) {
  const available = useFeatures().quickLook;
  const [err, setErr] = useState<string | null>(null);
  const look = useCallback(() => {
    setErr(null);
    api.fileQuickLook(path).catch((e) => setErr(String(e?.message ?? e)));
  }, [path]);
  useEffect(() => {
    if (!available) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== " " || e.repeat || e.metaKey || e.ctrlKey || e.altKey) return;
      const t = e.target as HTMLElement | null;
      if (t?.closest("input, textarea, select, button, [contenteditable=true], .cm-editor")) return;
      if (document.querySelector(".scrim, [role=dialog]")) return;
      e.preventDefault();
      look();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [look, available]);
  if (!available) return null;
  return (
    <>
      <button type="button" className="btn pri" title="See the file as it looks in its app, in Quick Look (Space)" onClick={look}>
        <Icon name="eye" size={14} />
        Quick Look
      </button>
      {err && <p className="muted">{err}</p>}
    </>
  );
}

/** A Word, PowerPoint or Excel file: the text Brainstead reads from it (what search, ingest and
 *  quotes use), by slide or sheet, with Quick Look and Open in its app for how it looks. */
function OfficeView({ path }: { path: string }) {
  const [parts, setParts] = useState<{ label: string; text: string }[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setParts(null);
    setErr(null);
    void api
      .sourceParts(path)
      .then((x) => live && setParts(x.parts))
      .catch((e) => live && setErr(String(e)));
    return () => {
      live = false;
    };
  }, [path]);
  const kind = ext(path);
  const sheet = kind === "xlsx";
  const label = (p: { label: string }, i: number) => p.label || (sheet ? `Sheet ${i + 1}` : kind === "pptx" ? `Slide ${i + 1}` : "");
  return (
    <div className="officeview">
      <div className="officebar">
        <QuickLook path={path} />
        <OpenInApp path={path} />
        <span className="faint small grow">The text Brainstead reads from it, which search, ingest and quotes use.</span>
      </div>
      {err && <p className="err">Couldn't read this file: {err}</p>}
      {!err && !parts && <p className="faint">Reading…</p>}
      {parts?.map((p, i) => (
        <section key={i} className="card officepart">
          {label(p, i) && <div className="eyebrow">{label(p, i)}</div>}
          {sheet ? (
            <div className="officetable">
              <table>
                <tbody>
                  {p.text
                    .split("\n")
                    .filter((l) => l.trim())
                    .slice(0, 500)
                    .map((l, r) => (
                      <tr key={r}>
                        {l.split("\t").map((c, k) => (
                          <td key={k}>{c}</td>
                        ))}
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
          ) : (
            <pre>{p.text}</pre>
          )}
        </section>
      ))}
    </div>
  );
}
