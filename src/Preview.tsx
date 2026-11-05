// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A file previewed in the window where a Mac shows Quick Look: on Windows the file's own preview
// handler (what Explorer's preview pane shows: Word, Excel and PowerPoint draw their documents,
// Windows its text, RTF and mail) drawn by Windows in a native window over this panel's body, cut
// to its rounded bottom corners (src-tauri/src/platform/preview_win.rs). The page draws the frame
// (the name, Open in its app, Close) and reports where the body is, in the window's physical
// pixels, whenever it moves within the window; the native window follows the window's own moves.
// A Mac shows the system's panel instead and never opens this one; a type Windows has no handler
// for opens in its own app, as it did before.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api, PreviewFrame } from "./api";
import { Icon } from "./icons";
import { isMac, isWindows } from "./platform";
import { Store, useStore } from "./store";
import { useDismiss } from "./ui";

export interface PreviewRequest {
  /** The file's name, for the title and for the type Windows previews by. */
  name: string;
  /** Shows the file: in the panel's body at `frame`, or (null) as the system does it itself. False
   *  when it opened in its own app instead, and the panel closes. */
  look: (frame: PreviewFrame | null) => Promise<boolean | void>;
  /** Opens the file in its own app, for the panel's button. */
  openInApp: () => Promise<void>;
}

/** The file being previewed in the panel, or none. */
export const previewView = new Store<PreviewRequest | null>(null);

/** Where Space (Quick Look, Preview on Windows) shows a file named like this: "quick look", the
 *  system's panel on a Mac; "preview", the panel here on Windows (the button reads Preview) for a
 *  type whose preview handler runs out of process; else "app", its own app. The MCP server's
 *  list_trash and app_status say the same. */
export type LookIn = "quick look" | "preview" | "app";

export async function lookIn(name: string): Promise<LookIn> {
  if (isMac) return "quick look";
  if (!isWindows) return "app";
  return (await api.previewAvailable(name).catch(() => false)) ? "preview" : "app";
}

/** Previews a file as Quick Look (Space) does, where `lookIn` says. */
export async function quickLook(req: PreviewRequest): Promise<void> {
  if ((await lookIn(req.name)) === "preview") previewView.set(req);
  else await req.look(null);
}

/** What the last pointer press was on, kept until the focus moves anywhere else. WebView2, as
 *  Chromium does, focuses a button on a click (a row's checkbox), where WebKit doesn't, and from the
 *  next key on that button matches :focus-visible as if the keyboard had put it there, so this is
 *  the only way to tell the two apart. */
let pressed: Node | null = null;
if (typeof window !== "undefined") {
  window.addEventListener("pointerdown", (e) => (pressed = e.target as Node), true);
  window.addEventListener("focusin", (e) => {
    if (!(pressed && (e.target as Node).contains?.(pressed))) pressed = null;
  });
}

/** Whether Space belongs to what has the focus: a field, or a button the keyboard put the focus on
 *  (a button just clicked would otherwise take every Space after it). */
function ownsSpace(t: HTMLElement | null): boolean {
  if (t?.closest?.("input, textarea, select, [contenteditable=true], [contenteditable=''], .cm-editor")) return true;
  const button = t?.closest?.("button, [role=button], [role=checkbox]");
  return !!button && !(pressed && button.contains(pressed));
}

const plainSpace = (e: KeyboardEvent) => e.key === " " && !e.repeat && !e.metaKey && !e.ctrlKey && !e.altKey;

/** What each screen's Space left alone on a Mac before the Windows preview came: Quick Look
 *  answers Space there exactly as it always has (WebKit never focuses a clicked button, so
 *  `pressed` has nothing to tell apart). */
const MAC_LEAVES = {
  source: "input, textarea, select, button, [contenteditable=true], .cm-editor",
  trash: "input, textarea, button, [contenteditable]",
};

/** Whether a keydown is Space meant for Quick Look (the panel here on Windows) on `screen`: Space
 *  with no modifier, not for the focused field or button and not for a nearer dialog; on a Mac,
 *  the screen's own rule from before. */
export function isLookKey(e: KeyboardEvent, screen: "source" | "trash"): boolean {
  const t = e.target as HTMLElement | null;
  if (isMac) {
    if (e.key !== " " || t?.closest?.(MAC_LEAVES[screen])) return false;
    return screen === "trash" || (!e.repeat && !e.metaKey && !e.ctrlKey && !e.altKey && !document.querySelector(".scrim, [role=dialog]"));
  }
  if (!plainSpace(e) || e.defaultPrevented || ownsSpace(t)) return false;
  return !document.querySelector(".scrim, [role=dialog]");
}

/** Closes the panel; one function for the life of the page, so the panel's effects don't re-run. */
const closePreview = () => previewView.set(null);

/** A press outside the panel that leaves it open: the window's own minimise and close (.winctl,
 *  page markup outside the panel; the preview hides and comes back with the window), and the empty
 *  part of the top bar, which drags or maximises the window (Tauri drags only from the element
 *  that carries the mark itself, never from a button in it). */
export const keepOpen = (t: Element): boolean => !!t.closest?.(".winctl") || !!t.hasAttribute?.("data-tauri-drag-region");

export function PreviewHost() {
  const v = useStore(previewView);
  return v ? <PreviewPanel key={v.name} req={v} onClose={closePreview} /> : null;
}

const errText = (e: unknown) => (typeof e === "object" && e && "message" in e ? String((e as { message: unknown }).message) : String(e));

/** `rgb(r, g, b)` (what getComputedStyle gives) as `#rrggbb`. */
function hex(css: string): string {
  const m = css.match(/\d+/g);
  if (!m || m.length < 3) return "#ffffff";
  return (
    "#" +
    m
      .slice(0, 3)
      .map((n) => Number(n).toString(16).padStart(2, "0"))
      .join("")
  );
}

let lastId = 0;

/** A number for a panel, unique for the life of the app even across a reload of the page (the
 *  native side keeps the last few closed). */
export function nextPreviewId(): number {
  lastId = Math.max(Date.now(), lastId + 1);
  return lastId;
}

/** Where the body is, in the window's physical pixels (the page's zoom is in devicePixelRatio), and
 *  the radius of the panel's bottom corners inside its border, which the native window is cut to,
 *  for panel `id`. */
export function frameOf(el: HTMLElement | null, id = 0, hidden = false, holes: number[][] = []): PreviewFrame | null {
  if (!el) return null;
  const b = el.getBoundingClientRect();
  if (!b.width || !b.height) return null;
  const r = window.devicePixelRatio || 1;
  const cs = getComputedStyle(el);
  const panel = el.parentElement ? getComputedStyle(el.parentElement) : null;
  const corner = (parseFloat(panel?.borderBottomRightRadius ?? "") || 0) - (parseFloat(panel?.borderBottomWidth ?? "") || 0);
  return {
    x: Math.round(b.left * r),
    y: Math.round(b.top * r),
    w: Math.round(b.width * r),
    h: Math.round(b.height * r),
    bg: hex(cs.backgroundColor),
    fg: hex(cs.color),
    radius: Math.round(Math.max(0, corner) * r),
    id,
    hidden,
    holes,
  };
}

/** Whether the page shows a dialog of its own over the panel (whose scrim is `own`): the Quit
 *  question, unsaved edits, an update. The native window would cover it, so it hides meanwhile,
 *  and the panel drops below it. */
export function coveredBy(own: Element | null): boolean {
  return [...document.querySelectorAll(".scrim, [role=dialog], [role=alertdialog]")].some((el) => !own?.contains(el));
}

/** Where the page's messages (toasts) and tooltips (the bar's buttons' among them) sit over the body `el`, in
 *  its physical pixels from its top left: the native window is cut away there, so they show through. */
export function holesIn(el: HTMLElement | null, own: Element | null): number[][] {
  if (!el) return [];
  const b = el.getBoundingClientRect();
  const r = window.devicePixelRatio || 1;
  return [...document.querySelectorAll(".toast, .tip")]
    .filter((t) => !own?.contains(t))
    .map((t) => t.getBoundingClientRect())
    .filter((t) => t.width && t.height && t.right > b.left && t.left < b.right && t.bottom > b.top && t.top < b.bottom)
    .map((t) => [Math.floor((t.left - b.left) * r), Math.floor((t.top - b.top) * r), Math.ceil(t.width * r), Math.ceil(t.height * r)]);
}

function PreviewPanel({ req, onClose }: { req: PreviewRequest; onClose: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const scrim = useRef<HTMLDivElement>(null);
  // A dialog of the page's own is over the panel: it has the keys meanwhile.
  const [covered, setCovered] = useState(false);
  const [state, setState] = useState<"opening" | "shown" | "failed">("opening");
  const [err, setErr] = useState("");
  // Escape and a click on the scrim close it. A click in the preview itself never reaches the page,
  // and gives the keys to the document, in the handler's own process, until the page is clicked.
  useDismiss(box, onClose, !covered, keepOpen);
  // Space closes it again, as it closes Quick Look, before the screen's own Space would reopen it.
  useEffect(() => {
    if (covered) return;
    const k = (e: KeyboardEvent) => {
      if (!plainSpace(e) || ownsSpace(e.target as HTMLElement | null)) return;
      e.preventDefault();
      e.stopPropagation();
      onClose();
    };
    window.addEventListener("keydown", k, true);
    return () => window.removeEventListener("keydown", k, true);
  }, [onClose, covered]);
  useEffect(() => {
    let live = true;
    // One number per open, not per panel: React may run this twice for one panel (StrictMode).
    const id = nextPreviewId();
    const frame = () => {
      const c = coveredBy(scrim.current);
      setCovered(c);
      return frameOf(body.current, id, c, holesIn(body.current, scrim.current));
    };
    const f = frame();
    let sent = JSON.stringify(f);
    (f ? req.look(f) : Promise.reject(new Error("The panel has no room to draw in."))).then(
      (here) => {
        if (!live) return;
        if (here === false) onClose();
        else setState("shown");
      },
      (e) => {
        if (!live) return;
        setState("failed");
        setErr(errText(e));
      },
    );
    // Sent only when it changed: the observers below fire for much that doesn't move it.
    const move = () => {
      const f = frame();
      const now = JSON.stringify(f);
      if (!f || now === sent) return;
      sent = now;
      void api.previewMove(f).catch(() => {});
    };
    const sized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(move);
    if (body.current) sized?.observe(body.current);
    // A dialog or a message of the page's own coming and going over the panel.
    const shown = new MutationObserver(move);
    shown.observe(document.body, { childList: true, subtree: true });
    // Light and dark: the body's colours change, which the native window and the handler take.
    const themed = new MutationObserver(move);
    themed.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    const scheme = window.matchMedia?.("(prefers-color-scheme: dark)");
    scheme?.addEventListener("change", move);
    // The physical size changes with the screen's scale without the body's CSS size doing so.
    const scaled = getCurrentWindow()
      .onScaleChanged(move)
      .catch(() => () => {});
    return () => {
      live = false;
      sized?.disconnect();
      shown.disconnect();
      themed.disconnect();
      scheme?.removeEventListener("change", move);
      void scaled.then((off) => off());
      void api.previewClose(id).catch(() => {});
    };
  }, [req, onClose]);
  return createPortal(
    <div ref={scrim} className={`scrim mfull winprevscrim${covered ? " under" : ""}`}>
      <div ref={box} role="dialog" aria-label={`Preview of ${req.name}`} className="dialog winprev">
        <div className="winprevbar">
          <Icon name="eye" size={14} />
          <span className="ell grow" title={req.name}>
            {req.name}
          </span>
          {/* In the bar: the body is under the native window once the preview is up. */}
          {err && state === "shown" && (
            <span className="muted ell winpreverr" role="alert" title={err}>
              {err}
            </span>
          )}
          <button
            type="button"
            className="btn sm"
            title="Open this file in the app the system uses for it"
            onClick={() => void req.openInApp().catch((e) => setErr(errText(e)))}
          >
            Open in its app
          </button>
          <button type="button" className="ibtn" aria-label="Close the preview" title="Close the preview (Escape)" onClick={onClose}>
            <Icon name="x" size={14} />
          </button>
        </div>
        <div ref={body} className="winprevbody">
          {state === "opening" && <p className="faint">Opening the preview…</p>}
          {state !== "shown" && err && <p className="muted">{err}</p>}
        </div>
      </div>
    </div>,
    document.body,
  );
}
