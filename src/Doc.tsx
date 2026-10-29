// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// One document: its properties, the text in the editor (live or source) or rendered (read), and
// what links to it. ⌘S saves, and only if the file hasn't changed on disk since it was read; an
// outside change is merged in when it touches other lines. Unsaved text is kept as a draft in
// Rust, and leaving with unsaved changes asks first. Sources only read.

import { ContradictionBanner } from "./Contradictions";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { api, Doc, Draft, FileSummary, TaskRow } from "./api";
import type { EditorMode } from "./editor/Editor";
import { appHooks } from "./editor/hooks";
import { NoteEditor, NoteEditorHandle } from "./editor/NoteEditor";
import { EditSession } from "./editor/session";
import { FileMenu, useFileMenu } from "./FileMenu";
import { Icon } from "./icons";
import { lineText } from "./md/highlight";
import { headingSlug } from "./md/plugins";
import { Markdown } from "./md/Markdown";
import { nav, openDoc, place, setLeaveGuard } from "./nav";
import { bodyStartsWithH1, DOC_ACCENTS, DOC_STYLES, useDocLook } from "./docLook";
import { DatePicker } from "./DatePicker";
import { LookPicker } from "./LookPicker";
import type { ShorthandKind } from "./md/dates";
import { NoteMenuAction, setNoteMenu } from "./noteMenu";
import { focusMode } from "./focus";
import { SpeechBlock, speechBlocks, spokenIntro } from "./speech/blocks";
import { player, readingScene, skipReading, startReading, stillReading, stopReading, toggleReading, voices } from "./speech/player";
import { PlayerBar, ReadAloudButton, SpeakLayer } from "./speech/ReadAloud";
import { FileAction, fileAction, toggleBookmark, useBookmarked } from "./notes/actions";
import { takeEditAsked, takeNewNoteCursor } from "./notes/dialogs";
import { checkTemplate } from "./notes/templater";
import { templaterSyntax } from "./notes/templaterSyntax";
import { templaterAssist } from "./editor/templaterAssist";
import { TemplaterDocsLink, TestRun } from "./notes/TemplateRun";
import { toggledLine } from "./editor/text";
import { localToday } from "./md/taskQuery";
import { toggleTask } from "./taskModel";
import { noteOpened } from "./recent";
import { SourcePreview } from "./SourcePreview";
import { useVaultVersion, vaultVersion } from "./state";
import { applyReadSize, READ_SIZE, settings, useStore } from "./store";
import { reportEditError } from "./taskModel";
import { toast } from "./Toast";
import { askLink, TopBar } from "./TopBar";
import { openGraph } from "./Graph";
import { ingest } from "./Ingest";
import { isThread } from "./skills/Reply";
import { ProvenanceCard } from "./Provenance";
import { AgentChangesCard } from "./Review";
import { FindBar } from "./notes/FindBar";
import { lineDiff } from "./notes/lineDiff";
import { plainText } from "./md/plainText";
import { LastTimeCard, NoteTasksCard, OutlineCard } from "./notes/SideCards";
import { ago, Dialog, LAYER_LABEL, LayerTag, Popover, Seg, useDebounced } from "./ui";
import { TypePill } from "./typeTone";
import { scriptsAllowed } from "./md/scripts";
import { sceneDocMenu } from "./scene";
import { getList, getScalar } from "./wiki/props";
import { FactsCard, SourcesCard, WikiFields } from "./wiki/WikiFields";
import { iconizeEmoji } from "./md/fieldEmoji";
import { dragRegion, isMod, isOptCmd, keyLabel, strayCtrl } from "./platform";
import { useFeatures } from "./features";

const isMarkdown = (p: string) => /\.(md|txt)$/i.test(p);
const editable = (s: FileSummary) => isMarkdown(s.path) && s.layer !== "source";

export type DocMode = EditorMode | "read";
/** How a note opens: in View, or as a screenshot scene says (src/scene.ts). */
function openingMode(): DocMode {
  const sc = document.documentElement.dataset.docMode;
  return sc === "live" || sc === "source" ? sc : "read";
}

export function DocScreen() {
  const { place: here } = useStore(place);
  const path = here.path ?? "";
  const bookmarked = useBookmarked(path);
  const sideFolded = !!useStore(settings).docSideFolded;
  const version = useStore(vaultVersion);
  const [doc, setDoc] = useState<Doc | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [mode, setMode] = useState<DocMode>(openingMode);
  const menu = useFileMenu();
  const moreBtn = useRef<HTMLButtonElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const readEl = useRef<HTMLDivElement>(null);
  /** Read aloud: the box the word highlight is drawn in, the blocks being read, and the screen's main. */
  const speakWrap = useRef<HTMLDivElement>(null);
  const speechRef = useRef<SpeechBlock[]>([]);
  const mainEl = useRef<HTMLElement>(null);
  /** The Note menu's Read Aloud, set once readAloud is defined below. */
  const readAloudRef = useRef<() => void>(() => {});
  /** The paragraph right-clicked, for the menu's Read aloud from here. */
  const [readFromEl, setReadFromEl] = useState<HTMLElement | null>(null);
  const voiceList = useStore(voices);
  const editor = useRef<NoteEditorHandle>(null);
  const [session, setSession] = useState<EditSession | null>(null);
  /** The editor's text, for the rendered view. */
  const [liveText, setLiveText] = useState<string | null>(null);
  /** Where the caret starts in a note just made from a template. */
  const [caret, setCaret] = useState<number | null>(null);

  // Reload on any change in the vault: this file, or one that now links to it. A different file starts clean.
  const reloadKey = version.n;
  const [shownPath, setShownPath] = useState(path);
  useEffect(() => {
    let live = true;
    if (shownPath !== path) {
      setDoc(null);
      setShownPath(path);
    }
    setErr(null);
    api
      .docRead(path)
      .then((d) => {
        if (!live) return;
        setDoc(d);
        noteOpened(d.meta.summary.path, d.meta.summary.title);
      })
      .catch((e) => live && setErr(String(e)));
    return () => {
      live = false;
    };
  }, [path, reloadKey]); // eslint-disable-line react-hooks/exhaustive-deps

  // One editing session per file, made when it's first read.
  const docPath = doc?.meta.summary.path ?? null;
  const canEdit = !!doc && editable(doc.meta.summary) && !doc.readOnly;
  useEffect(() => {
    // A note that can't be edited opens in View, and isn't left to open in Edit later.
    if (doc && !canEdit) takeEditAsked(doc.meta.summary.path);
    if (!doc || !canEdit) return setSession(null);
    const s = new EditSession(
      doc.meta.summary.path,
      { content: doc.content, version: doc.version },
      { save: api.docSave, draftWrite: api.draftWrite, draftRead: api.draftRead, draftDiscard: api.draftDiscard },
      (t) => editor.current?.replaceText(t),
    );
    setSession(s);
    setLiveText(null);
    void s.checkDraft();
    // Each note opens in View; one just made from a template opens in Edit, the caret where
    // tp.file.cursor() was, and so does one opened to edit (a project's Edit).
    const at = takeNewNoteCursor(s.path);
    const edit = takeEditAsked(s.path);
    setCaret(at);
    setMode(at !== null || edit ? "live" : openingMode());
    return () => s.close();
  }, [docPath, canEdit]); // eslint-disable-line react-hooks/exhaustive-deps
  // Later reads of the same file: an outside change, or our own save coming back.
  useEffect(() => {
    if (doc && session && session.path === doc.meta.summary.path) session.diskChanged({ content: doc.content, version: doc.version });
  }, [doc, session]);

  const view = useSyncExternalStore(
    useCallback((f: () => void) => (session ? session.subscribe(f) : () => {}), [session]),
    () => session?.state ?? null,
  );
  const dirty = !!view?.dirty;

  const save = useCallback(async () => {
    if (!session) return false;
    try {
      return await session.save();
    } catch (e) {
      reportEditError(e);
      return false;
    }
  }, [session]);

  // ⌘F: the find bar (a count, so pressing it again focuses the bar again); 0 is closed.
  const [finding, setFinding] = useState(document.documentElement.dataset.find ? 1 : 0);
  // ⌘S.
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      // ⌥⌘1 View, ⌥⌘2 Edit, ⌥⌘3 Source (also in the Note menu, which usually takes them first); by the
      // key's place, as ⌥ changes what's typed.
      const m = ({ Digit1: "read", Digit2: "live", Digit3: "source" } as Record<string, DocMode>)[e.code];
      if (isOptCmd(e) && m && canEdit) {
        e.preventDefault();
        setMode(m);
        return;
      }
      if (!isMod(e) || e.altKey || strayCtrl(e) || e.shiftKey) return;
      if (e.key === "s") {
        e.preventDefault();
        void save();
      }
      if (e.key === "f") {
        e.preventDefault();
        setFinding((n) => n + 1);
        return;
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [save, canEdit]);

  // Leaving with unsaved changes asks first.
  const [leaving, setLeaving] = useState<(() => void) | null>(null);
  useEffect(() => {
    if (!session || !dirty) return setLeaveGuard(null);
    setLeaveGuard((proceed) => {
      // Renamed or trashed: the edits are in its draft already.
      if (session.detached) return false;
      setLeaving(() => proceed);
      return true;
    });
    return () => setLeaveGuard(null);
  }, [session, dirty]);
  // The window going away (hidden, or the app quitting): write the draft now.
  useEffect(() => {
    if (!session) return;
    const f = () => void session.writeDraft();
    window.addEventListener("blur", f);
    window.addEventListener("pagehide", f);
    return () => {
      window.removeEventListener("blur", f);
      window.removeEventListener("pagehide", f);
    };
  }, [session]);

  useEffect(() => {
    if (view?.notice?.kind === "merged") {
      toast("A change made outside Brainstead was merged in. Save to keep both.");
      session?.dismiss();
    } else if (view?.notice?.kind === "saved") session?.dismiss();
  }, [view?.notice, session]);

  // The date picker that `due:`, `defer:`, `start:` and `created:` open.
  const [picking, setPicking] = useState<{ kind: ShorthandKind; at: DOMRect; insert: (d: string) => void } | null>(null);
  const hooks = useCallback(
    (p: string, root: string) =>
      appHooks(p, root, (kind, at, insert) => setPicking({ kind, at: new DOMRect(at.left, at.top, 1, at.bottom - at.top), insert })),
    [],
  );

  // To the heading or block a link pointed at, once it's drawn.
  useLayoutEffect(() => {
    if (!doc || !here.anchor || !body.current) return;
    const ed = editor.current?.editor;
    if (ed && editable(doc.meta.summary) && mode !== "read") {
      if (!here.anchor.startsWith("^")) ed.revealHeading(here.anchor);
      return;
    }
    const id = here.anchor.startsWith("^") ? here.anchor : headingSlug(here.anchor);
    const el = body.current.querySelector(`[id="${CSS.escape(id)}"]`);
    el?.scrollIntoView({ block: "start" });
    el?.classList.add("flash");
  }, [doc, here.anchor, session]); // eslint-disable-line react-hooks/exhaustive-deps
  // To lines (a page's uncited claims in Knowledge health): highlighted in the editor, or the
  // blocks that hold them marked in View, scrolled to the first.
  const wantLines = (here.lines ?? []).join("\n");
  useEffect(() => {
    const want = wantLines ? wantLines.split("\n") : [];
    if (!doc || !want.length || here.anchor) return;
    const ed = editor.current?.editor;
    if (ed && editable(doc.meta.summary) && mode !== "read") {
      ed.highlightLines(want);
      return;
    }
    const plain = want.map((w) => lineText(w).slice(0, 60)).filter(Boolean);
    if (!plain.length) return;
    // Marks each line's innermost block (a list item before the list holding it).
    const mark = () => {
      const blocks = [...(body.current?.querySelectorAll("p, li, td, th, h1, h2, h3, h4, h5, h6, blockquote") ?? [])].reverse();
      const texts = blocks.map((b) => lineText(b.textContent ?? ""));
      const hits = plain.map((p) => blocks[texts.findIndex((t) => t.includes(p))]).filter((b): b is Element => !!b);
      for (const h of hits) if (!h.classList.contains("line-hit")) h.classList.add("line-hit");
      return hits;
    };
    // The page can be drawn again after it opens (its links resolved, say), which drops the marks:
    // they're put back on every redraw while the page is open.
    const redrawn = new MutationObserver(() => mark());
    if (body.current) redrawn.observe(body.current, { childList: true, subtree: true });
    let tries = 0;
    const t = window.setInterval(() => {
      const hits = mark();
      if (hits.length === plain.length || ++tries > 20) window.clearInterval(t);
      if (!hits.length) return;
      window.clearInterval(t);
      const first = hits.reduce((a, b) => (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_PRECEDING ? b : a));
      first.scrollIntoView({ block: "center" });
    }, 50);
    return () => {
      window.clearInterval(t);
      redrawn.disconnect();
    };
  }, [path, wantLines, doc]); // eslint-disable-line react-hooks/exhaustive-deps
  // Back at the top for a new document; opened from a search (⌘K, Search) without a heading to go
  // to, down to the first match once it's drawn.
  useEffect(() => {
    if (here.anchor || here.lines?.length) return;
    body.current?.scrollTo({ top: 0 });
    if (!here.q || !doc) return;
    let tries = 0;
    const t = window.setInterval(() => {
      const m = readEl.current?.querySelector("mark.search-hit");
      if (m || ++tries > 20) window.clearInterval(t);
      // Only in View: in Edit and Source the rendered copy is hidden (kept for printing).
      if (m && !readEl.current?.classList.contains("doc-print")) {
        m.scrollIntoView({ block: "center" });
        m.classList.add("flash");
      }
    }, 50);
    return () => window.clearInterval(t);
  }, [path, here.anchor, here.lines, here.q, doc]);

  const s = doc?.meta.summary;
  const look = useDocLook(path);
  const focus = useStore(focusMode) && !!s;
  // Esc leaves focus mode, unless something else took it (a dialog, the find bar, a suggestion list).
  useEffect(() => {
    if (!focus) return;
    const k = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented) focusMode.set(false);
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [focus]);
  const richEl = () => (s && isMarkdown(s.path) ? readEl.current : null);
  // Screenshot mode: the scene's ⋯ menu open once the note has loaded.
  useEffect(() => {
    if (s && moreBtn.current && sceneDocMenu.get() === "more") menu.open(moreBtn.current.getBoundingClientRect(), s);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s]);

  // Ticking a task in View: the one-line write the task screens make (⌘Z undoes it), or, with
  // unsaved edits, an edit in the editor's text that's saved with them.
  const tick = (line: number, done: boolean) => {
    const e = editor.current?.editor;
    if (dirty && e) {
      if (line + 1 > e.view.state.doc.lines) return;
      const l = e.view.state.doc.line(line + 1);
      const next = toggledLine(l.text, done, localToday());
      if (next !== l.text) e.view.dispatch({ changes: { from: l.from, to: l.to, insert: next }, userEvent: "input.tick" });
      return;
    }
    if (!doc || !s) return;
    const lineText = doc.content.split(/\r?\n/)[line];
    if (lineText === undefined) return;
    void toggleTask({ path: s.path, line, lineText } as TaskRow, done);
  };

  // The menu bar's Note menu, while this note is open.
  const act = (a: NoteMenuAction) => {
    if (!s) return;
    if (a === "save") return void save();
    if (a === "read-aloud") return readAloudRef.current();
    if (a === "focus") return focusMode.set(!focusMode.get());
    if (a === "view" || a === "edit" || a === "source") return canEdit && setMode(a === "view" ? "read" : a === "edit" ? "live" : "source");
    if (a === "discard-draft") {
      if (session) {
        session.reload();
        void session.discardDraft();
      } else void api.draftDiscard(s.path);
      return toast("Draft discarded");
    }
    void fileAction(a as FileAction, s, richEl());
  };
  const actRef = useRef(act);
  useEffect(() => {
    actRef.current = act;
  });
  const open = !!s;
  useEffect(() => {
    if (!open) return;
    setNoteMenu((a) => actRef.current(a));
    return () => setNoteMenu(null);
  }, [open]);
  const features = useFeatures();

  // Sources (and anything that isn't markdown) only read.
  const shown: DocMode = !s || !canEdit ? "read" : mode;

  // Read aloud (src/speech/): the note as View shows it, a paragraph at a time. Starting in Edit or
  // Source switches to View first; the blocks are read from the rendered page once it's drawn.
  const reading = useStore(player);
  const readingHere = reading.on && !!s && reading.path === s.path;
  const readAloud = (from?: HTMLElement) => {
    if (!s || !isMarkdown(s.path)) return;
    if (readingHere && !from) return toggleReading();
    if (shown !== "read") setMode("read");
    // After View has drawn.
    window.setTimeout(
      () => {
        const el = readEl.current;
        if (!el) return;
        const blocks = speechBlocks(el);
        if (!blocks.length) return toast("There's nothing to read in this note.");
        speechRef.current = blocks;
        const texts = blocks.map((b) => b.text);
        // From a paragraph (⌥-click, Read aloud from here); anywhere else is from the top.
        const at = from ? blocks.findIndex((b) => b.el === from || b.el.contains(from)) : -1;
        if (at >= 0) return startReading(s.path, s.title, texts, at);
        // From the top: the title and date first, and a first heading that only repeats the
        // title isn't said again.
        const repeats = texts[0].trim().toLowerCase() === s.title.trim().toLowerCase();
        startReading(s.path, s.title, texts, repeats && texts.length > 1 ? 1 : 0, spokenIntro(s.title, s.date));
      },
      shown === "read" ? 0 : 80,
    );
  };
  // Screenshot mode: the note shown being read at a block, saying nothing.
  const sceneReading = useStore(readingScene);
  useEffect(() => {
    if (!sceneReading || !s || !doc) return;
    const t = window.setTimeout(() => {
      const el = readEl.current;
      if (!el) return;
      speechRef.current = speechBlocks(el);
      stillReading(s.path, s.title, speechRef.current.length, sceneReading.block, sceneReading.char);
    }, 600);
    return () => window.clearTimeout(t);
  }, [sceneReading, s?.path, doc]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    readAloudRef.current = () => readAloud();
  });
  // Reading belongs to this note: going to another note or screen, or into Edit or Source, stops it.
  const readPath = s?.path;
  useEffect(() => {
    return () => {
      if (player.get().on && player.get().path === readPath) stopReading();
    };
  }, [readPath]);
  useEffect(() => {
    if (readingHere && shown !== "read") stopReading();
  }, [shown, readingHere]);
  // The note's text changed under it (saved here or elsewhere): the paragraphs no longer match.
  const readText = doc?.content;
  useEffect(() => {
    return () => {
      if (player.get().on && player.get().path === readPath) stopReading();
    };
  }, [readText]); // eslint-disable-line react-hooks/exhaustive-deps
  // Keys while reading: Space plays or pauses, F8 too (F7 and F9 skip), Esc stops, unless something
  // nearer wants the key (a field, a menu, a dialog, the help drawer, the find bar).
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      const on = player.get().on && player.get().path === readPath;
      if (e.metaKey || e.ctrlKey || e.altKey || e.defaultPrevented) return;
      const target = e.target as HTMLElement;
      const typing = !!target?.closest?.("input, textarea, select, [contenteditable]");
      const nearer = !!document.querySelector(".dialog, .popover, .menu, .help-drawer, .findbar");
      if (e.key === "F8") {
        e.preventDefault();
        if (on) toggleReading();
        else readAloud();
        return;
      }
      if (!on) return;
      if (e.key === "F7" || e.key === "F9") {
        e.preventDefault();
        skipReading(e.key === "F9" ? 1 : -1);
      } else if (e.key === " " && !typing && !nearer) {
        e.preventDefault();
        toggleReading();
      } else if (e.key === "Escape" && !typing && !nearer) {
        e.preventDefault();
        stopReading();
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  });

  // The rendered copy follows the editor, a moment behind while typing.
  const rendered = useDebounced(liveText ?? doc?.content ?? "", shown === "read" ? 0 : 600);
  // A template (Templates/): Templater's docs, a test run, and a banner when it doesn't compile.
  const isTemplate = s?.layer === "template" && isMarkdown(s.path);
  const templateError = useMemo(() => (isTemplate ? checkTemplate(rendered) : null), [isTemplate, rendered]);
  const [testing, setTesting] = useState(false);
  // A wiki page: its sources, numbered as citation markers in the text and listed beside it.
  const isWiki = s?.layer === "wiki" && isMarkdown(s.path);
  const pageText = liveText ?? doc?.content ?? "";
  const [cites, setCites] = useState<{ listed: string[]; order: string[] }>({ listed: [], order: [] });
  const sourceNames = isWiki ? getList(rendered, "sources").map(linkName).join("\n") : "";
  // Again when the vault changes: a source may have been added or renamed.
  const vv = useVaultVersion();
  useEffect(() => {
    let live = true;
    const names = sourceNames ? sourceNames.split("\n") : [];
    if (!names.length) setCites({ listed: [], order: [] });
    else
      void api.linksResolve(names).then((ps) => {
        const listed = ps.filter((p): p is string => !!p).filter((p, i, a) => a.indexOf(p) === i);
        if (live) setCites((c) => ({ listed, order: c.order }));
      });
    return () => {
      live = false;
    };
  }, [sourceNames, path, vv]);
  const readAloudButton = s ? (
    isMarkdown(s.path) && voiceList.length ? (
      <ReadAloudButton path={s.path} onStart={() => readAloud()} />
    ) : (
      <span
        title={
          isMarkdown(s.path)
            ? "Read aloud needs a system voice, and none was found"
            : "Read aloud reads notes and pages, not this kind of file"
        }
      >
        <button type="button" className="ibtn" aria-label="Read aloud" disabled>
          <Icon name="speaker" />
        </button>
      </span>
    )
  ) : null;
  const textSizeButton = s ? (
    <TextSizeButton off={isMarkdown(s.path) ? undefined : "Text size applies to notes and pages, not this kind of file"} />
  ) : null;
  useTextSizeKeys(!!s && isMarkdown(s.path));
  const [sizeAt, setSizeAt] = useState<DOMRect | null>(null);
  // The note's view actions, at the top of its More menu (the bar keeps View/Edit, Bookmark and More).
  const viewItems = (close: () => void) =>
    s && (
      <>
        <button
          type="button"
          role="menuitem"
          disabled={s.layer === "template"}
          title={s.layer === "template" ? "Templates aren't in the graph" : "Open the graph around this page"}
          onClick={() => (close(), openGraph(s.path))}
        >
          <Icon name="graph" size={14} />
          Graph around this page
        </button>
        {features.readAloud && (
          <button
            type="button"
            role="menuitem"
            disabled={!isMarkdown(s.path) || !voiceList.length}
            title={
              !isMarkdown(s.path)
                ? "Read aloud reads notes and pages, not this kind of file"
                : !voiceList.length
                  ? "Read aloud needs a system voice, and none was found"
                  : keyLabel("Read the note aloud from the top (⇧⌘P)")
            }
            onClick={() => (close(), readAloud())}
          >
            <Icon name="speaker" size={14} />
            Read aloud
            <span className="kbd">{keyLabel("⇧⌘P")}</span>
          </button>
        )}
        <button
          type="button"
          role="menuitem"
          disabled={!isMarkdown(s.path)}
          title={
            isMarkdown(s.path)
              ? keyLabel("Make the text larger or smaller (⌘+ ⌘− ⌘0)")
              : "Text size applies to notes and pages, not this kind of file"
          }
          onClick={() => (close(), setSizeAt(menu.at))}
        >
          <Icon name="textsize" size={14} />
          Text size…
        </button>
        <button
          type="button"
          role="menuitem"
          title={keyLabel("Focus mode: just the text, full window (⌘.)")}
          onClick={() => (close(), focusMode.set(true))}
        >
          <Icon name="expand" size={14} />
          Focus mode
          <span className="kbd">{keyLabel("⌘.")}</span>
        </button>
        <div className="sep" />
      </>
    );

  return (
    <main className={`main${focus ? " focused" : ""}`} ref={mainEl}>
      {focus && s ? (
        <div className="top drag focusbar" {...dragRegion()}>
          <div className="crumbs focustitle">
            <b>{s.title}</b>
          </div>
          {features.readAloud && readAloudButton}
          {textSizeButton}
          <button type="button" className="btn sm" title={keyLabel("Leave focus mode (Esc or ⌘.)")} onClick={() => focusMode.set(false)}>
            Exit focus<span className="kbd">esc</span>
          </button>
        </div>
      ) : (
        <TopBar
          title={s?.title ?? path.split("/").pop() ?? ""}
          sub={s ? LAYER_LABEL[s.layer] : undefined}
          ask={
            s && (isMarkdown(s.path) || s.layer === "source")
              ? { about: `“${s.title}”`, prompt: `About ${askLink(s.path)}: `, sm: true }
              : undefined
          }
        >
          {dirty && (
            <button type="button" className="btn sm pri" onClick={() => void save()} disabled={view?.saving} title={keyLabel("Save (⌘S)")}>
              {view?.saving ? "Saving…" : "Save"}
            </button>
          )}
          {isWiki && s && (
            <>
              <button
                type="button"
                className="btn sm"
                disabled={!cites.listed.length}
                title={
                  cites.listed.length
                    ? "Ingest this page's sources again; their changes are listed in Changes"
                    : "This page lists no sources"
                }
                onClick={() => ingest(cites.listed.filter((p) => !p.startsWith("wiki/")))}
              >
                <Icon name="refresh" size={13} />
                Refresh from sources
              </button>
            </>
          )}
          {s?.layer === "source" && (
            <>
              {isThread(s.path) && (
                <button
                  type="button"
                  className="btn sm"
                  title="Have the AI draft a reply to this thread"
                  onClick={() => nav.go({ screen: "reply", path: s.path })}
                >
                  <Icon name="send" size={13} />
                  Draft a reply
                </button>
              )}
              {/\.(pdf|docx|pptx|xlsx|md|txt)$/i.test(s.path) && !isThread(s.path) && (
                <button
                  type="button"
                  className="btn sm"
                  title="Check it against the version of a governing document in force"
                  onClick={() => nav.go({ screen: "doccheck", path: s.path })}
                >
                  <Icon name="shield" size={13} />
                  Doc check
                </button>
              )}
            </>
          )}
          {isTemplate && (
            <>
              <button
                type="button"
                className="btn sm"
                title="Run this template as New note would, without making anything"
                onClick={() => setTesting(true)}
              >
                Test run
              </button>
              <TemplaterDocsLink />
            </>
          )}
          {s && canEdit && (
            <Seg<DocMode>
              label="View"
              value={mode}
              onChange={setMode}
              options={[
                ["read", "View", keyLabel("View (⌥⌘1)")],
                ["live", "Edit", keyLabel("Edit (⌥⌘2)")],
                ["source", "Source", keyLabel("Source markdown (⌥⌘3)")],
              ]}
            />
          )}
          {s && (
            <button
              type="button"
              className={`ibtn ${bookmarked ? "on" : ""}`}
              aria-pressed={bookmarked}
              aria-label={bookmarked ? "Remove the bookmark" : "Bookmark"}
              title={bookmarked ? "Bookmarked: click to remove" : "Bookmark (in the sidebar)"}
              onClick={() => void toggleBookmark(s.path)}
            >
              <Icon name="pin" />
            </button>
          )}
          {/* While it's being read: the reading's pause and carry on, beside More. */}
          {s && reading.on && reading.path === s.path && readAloudButton}
          {s && (
            <button
              type="button"
              className="ibtn"
              ref={moreBtn}
              aria-label="More"
              title="Graph, read aloud, text size, focus mode, copy, rename and more for this file"
              onClick={(e) => menu.open(e.currentTarget.getBoundingClientRect(), s)}
            >
              <Icon name="more" />
            </button>
          )}
        </TopBar>
      )}
      {finding > 0 && doc && (
        <FindBar
          key={`${path}:${finding}`}
          mode={`${shown}:${session ? "ready" : ""}`}
          getView={() => (shown !== "read" ? (editor.current?.editor?.view ?? null) : null)}
          getRead={() => readEl.current}
          onClose={() => setFinding(0)}
        />
      )}
      <div
        className="body docbody"
        ref={body}
        onContextMenu={(e) => {
          if (!s || shown !== "read") return;
          // On a paragraph of the note, the menu can read aloud from it.
          const t = e.target as HTMLElement;
          setReadFromEl(isMarkdown(s.path) && voiceList.length && readEl.current?.contains(t) ? t : null);
          menu.openAt(e, s);
        }}
      >
        {err && <div className="docerr err">{err}</div>}
        {doc && s && (
          <div className={`docgrid ${sideFolded || focus ? "wide" : ""} ${focus ? "focus" : ""}`}>
            <article
              className={`doc docsurf ${look.style === "brainstead" ? "" : "styled"} ${look.ownTheme ? "owntheme" : ""}`}
              data-doc-theme={look.theme}
              data-doc-style={look.style}
              data-doc-accent={look.accent}
            >
              <header className="dochead">
                {!focus && (
                  <div className="docmeta" data-no-print>
                    <LayerTag layer={s.layer} />
                    {s.type && <TypePill type={s.type} wiki={s.layer === "wiki"} />}
                    {s.date && <span className="faint">{s.date}</span>}
                    <span className="faint mono ell docpath" title={s.path}>
                      {s.path}
                    </span>
                    {dirty && (
                      <button type="button" className="unsaved" title={keyLabel("Save (⌘S)")} onClick={() => void save()}>
                        Unsaved · {keyLabel("⌘S")}
                      </button>
                    )}
                    {isMarkdown(s.path) && <DocLookControls look={look} />}
                    {sideFolded && (
                      <button
                        type="button"
                        className="btn sm ghost"
                        title="Show what links here"
                        onClick={() => settings.update({ docSideFolded: false })}
                      >
                        Linked from · {doc.meta.backlinks.length}
                        <Icon name="chevdown" size={12} />
                      </button>
                    )}
                  </div>
                )}
                <h1 className={`h1 doc-page-title ${bodyStartsWithH1(liveText ?? doc.content) ? "dup" : ""}`}>{s.title}</h1>
                {isWiki && <WikiSummary text={pageText} sources={cites.listed.length} />}
              </header>
              {session && view?.notice && <Notice session={session} notice={view.notice} />}
              <ContradictionBanner path={s.path} />
              {doc.readOnly && (
                <div className="doc-banner warn" role="status" data-no-print>
                  <Icon name="info" size={14} />
                  <span>{doc.readOnly}</span>
                </div>
              )}
              {templateError && (
                <div className="doc-banner warn" role="alert" data-no-print>
                  <Icon name="info" size={14} />
                  <span>This template doesn't run: {templateError}</span>
                </div>
              )}
              {isWiki && dirty && shown !== "read" && (
                <div className="doc-banner warn" role="status" data-no-print>
                  <Icon name="info" size={14} />
                  <span className="sp">Unsaved changes to this page.</span>
                  <button
                    type="button"
                    className="btn sm"
                    title="Throw away the changes and go back to the saved page"
                    onClick={() => session?.reload()}
                  >
                    Discard
                  </button>
                  <span title={view?.saving ? "Saving…" : keyLabel("Save the changes to the page (⌘S)")}>
                    <button type="button" className="btn sm pri" onClick={() => void save()} disabled={view?.saving}>
                      Save · {keyLabel("⌘S")}
                    </button>
                  </span>
                </div>
              )}
              {isWiki && session && shown !== "read" && (
                <WikiFields text={pageText} onChange={(t) => editor.current?.replaceText(t, true)} />
              )}
              {shown === "read" && (
                <div data-no-print>
                  <Properties doc={doc} />
                </div>
              )}
              {session && (
                <div className={shown === "read" ? "editor-hidden" : undefined} data-no-print>
                  <NoteEditor
                    key={session.path}
                    ref={editor}
                    initial={session.text}
                    caret={caret}
                    mode={shown === "read" ? "live" : shown}
                    hooks={
                      isTemplate
                        ? { ...hooks(session.path, doc.root), extensions: [templaterSyntax, templaterAssist] }
                        : hooks(session.path, doc.root)
                    }
                    onChange={(t) => {
                      session.edited(t);
                      setLiveText(t);
                    }}
                  />
                </div>
              )}
              {/* The rendered text: shown in Read, and kept (hidden) for Copy rich text and Export PDF. */}
              {isMarkdown(s.path) && (
                <div
                  ref={speakWrap}
                  className={shown === "read" ? "speakwrap" : undefined}
                  onClick={(e) => {
                    // ⌥-click a paragraph to read aloud from it; while reading, a plain click does.
                    const t = e.target as HTMLElement;
                    if (t.closest("a, button, input, summary")) return;
                    if (e.altKey && voiceList.length && shown === "read") {
                      e.preventDefault();
                      return readAloud(t);
                    }
                    if (!readingHere) return;
                    const b = speechRef.current.find((x) => x.el.contains(t));
                    if (b) skipReading(0, speechRef.current.indexOf(b));
                  }}
                >
                  {readingHere && shown === "read" && <SpeakLayer wrap={speakWrap} blocks={speechRef} />}
                  <div
                    ref={readEl}
                    className={shown !== "read" ? "doc-print" : h1RepeatsTitle(liveText ?? doc.content, s.title) ? "hide-h1" : undefined}
                  >
                    <Markdown
                      content={shown === "read" && liveText !== null ? liveText : rendered}
                      path={s.path}
                      root={doc.root}
                      q={here.q}
                      onTick={canEdit ? tick : undefined}
                      cites={isWiki ? cites.listed : undefined}
                      onCites={(order) => setCites((c) => ({ listed: c.listed, order }))}
                      scripts={s.layer === "note" && scriptsAllowed(s.path, doc.meta.frontmatter)}
                      templater={isTemplate}
                    />
                  </div>
                </div>
              )}
              {s.layer === "source" && !isMarkdown(s.path) && <SourcePreview file={s} root={doc.root} content={doc.content} />}
            </article>
            {!sideFolded && !focus && (
              <aside className="docside" data-no-print>
                {s.layer === "source" && <ProvenanceCard file={s} />}
                {s.layer === "note" && <LastTimeCard path={s.path} />}
                <Backlinks doc={doc} onFold={() => settings.update({ docSideFolded: true })} />
                {isWiki && <FactsCard path={s.path} stamp={doc.content} />}
                <AgentChangesCard path={s.path} />
                {isMarkdown(s.path) && s.layer !== "source" && <NoteTasksCard path={s.path} />}
                {isMarkdown(s.path) && <OutlineCard path={s.path} md={liveText ?? doc.content} />}
                {isWiki && <SourcesCard cites={cites.order.length ? cites.order : cites.listed} />}
              </aside>
            )}
          </div>
        )}
      </div>
      {readingHere && (
        <PlayerBar
          main={mainEl}
          onShow={() => speechRef.current[player.get().index]?.el.scrollIntoView({ block: "center", behavior: "smooth" })}
        />
      )}
      <FileMenu menu={menu} doc={richEl} readFrom={readFromEl ? () => readAloud(readFromEl) : undefined} extra={viewItems} />
      {sizeAt && <TextSizePanel anchor={sizeAt} onClose={() => setSizeAt(null)} />}
      {testing && s && <TestRun path={s.path} src={liveText ?? doc?.content ?? ""} onClose={() => setTesting(false)} />}
      {picking && (
        <Popover anchor={picking.at} onClose={() => setPicking(null)} width={252}>
          <DatePicker
            value={null}
            label={{ due: "Due date", defer: "Defer until", start: "Start date", created: "Created date" }[picking.kind]}
            onPick={(d) => {
              picking.insert(d);
              setPicking(null);
            }}
            onCancel={() => setPicking(null)}
          />
        </Popover>
      )}
      {leaving && session && (
        <Dialog label="Unsaved changes" width={460} onClose={() => setLeaving(null)}>
          <div className="confirm">
            <h2 className="h2">Save changes to “{s?.title}”?</h2>
            <p className="muted">Your changes are kept as a draft until you save or discard them.</p>
            <div className="row">
              <span className="grow" />
              <button type="button" className="btn lg" title="Stay on this note with the changes unsaved" onClick={() => setLeaving(null)}>
                Stay
              </button>
              <button
                type="button"
                className="btn lg"
                title="Keep the changes as a draft, unsaved, and leave; the note offers it when you open it again"
                onClick={async () => {
                  const go = leaving;
                  setLeaving(null);
                  await session.writeDraft();
                  setLeaveGuard(null);
                  go();
                }}
              >
                Keep as draft
              </button>
              <button
                type="button"
                className="btn lg danger"
                title="Throw away the changes and leave"
                onClick={() => {
                  const go = leaving;
                  session.reload();
                  setLeaving(null);
                  setLeaveGuard(null);
                  go();
                }}
              >
                Discard changes
              </button>
              <button
                type="button"
                className="btn lg pri"
                autoFocus
                title="Save the changes, then leave"
                onClick={async () => {
                  const go = leaving;
                  setLeaving(null);
                  if (await save()) {
                    setLeaveGuard(null);
                    go();
                  }
                }}
              >
                Save
              </button>
            </div>
          </div>
        </Dialog>
      )}
    </main>
  );
}

/** A draft from before, or an outside change that clashes with unsaved edits. */
function Notice({ session, notice }: { session: EditSession; notice: NonNullable<EditSession["state"]["notice"]> }) {
  const [compare, setCompare] = useState(false);
  const [reloading, setReloading] = useState(false);
  if (notice.kind === "draft") {
    const d: Draft = notice.draft;
    return (
      <div className="doc-banner" role="status" data-no-print>
        <Icon name="text" size={14} />
        <span>
          Unsaved changes from {ago(d.at)}
          {notice.stale ? ". The file has changed since; restoring puts your draft over those changes." : "."}
        </span>
        <div className="sp" />
        <button
          type="button"
          className="btn sm"
          title="Throw the draft away; the file stays as it is"
          onClick={() => void session.discardDraft()}
        >
          Discard
        </button>
        <button
          type="button"
          className="btn sm pri"
          title="Put the draft back in the editor, unsaved"
          onClick={() => session.restoreDraft(d)}
        >
          Restore
        </button>
      </div>
    );
  }
  if (notice.kind !== "conflict") return null;
  return (
    <div className="doc-banner warn" role="alert" data-no-print>
      <Icon name="text" size={14} />
      <span>This file was changed outside Brainstead, on the same lines you’ve changed. Nothing has been overwritten.</span>
      <div className="sp" />
      <button
        type="button"
        className="btn sm"
        title="Load the file as it is now; asks first, as your unsaved changes are dropped"
        onClick={() => setReloading(true)}
      >
        Reload
      </button>
      <button
        type="button"
        className="btn sm"
        onClick={() => session.keepMine()}
        title="Keeps your text as a draft; the file stays as it is"
      >
        Keep mine
      </button>
      <button
        type="button"
        className="btn sm pri"
        title="See the file as it is now beside your text, the lines that differ marked"
        onClick={() => setCompare(true)}
      >
        Compare
      </button>
      {reloading && (
        <Dialog label="Reload" width={420} onClose={() => setReloading(false)}>
          <div className="confirm">
            <h2 className="h2">Drop your unsaved changes and load the file as it is now?</h2>
            <p className="muted">Compare first to copy anything you want to keep.</p>
            <div className="row">
              <span className="grow" />
              <button type="button" className="btn lg" autoFocus title="Keep your unsaved changes" onClick={() => setReloading(false)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn lg danger"
                title="Drop your unsaved changes and load the file as it is now"
                onClick={() => {
                  setReloading(false);
                  session.reload(notice.theirs);
                }}
              >
                Reload
              </button>
            </div>
          </div>
        </Dialog>
      )}
      {compare && <Compare theirs={notice.theirs.content} mine={session.text} onClose={() => setCompare(false)} />}
    </div>
  );
}

/** The file on disk beside the unsaved text, the lines that differ marked on each side. */
function Compare({ theirs, mine, onClose }: { theirs: string; mine: string; onClose: () => void }) {
  const d = useMemo(() => lineDiff(theirs, mine), [theirs, mine]);
  const lines = (text: string, changed: boolean[]) =>
    text.split(/\r?\n/).map((l, i) => (
      <div key={i} className={changed[i] ? "chg" : undefined}>
        {l || "\u00a0"}
      </div>
    ));
  return (
    <Dialog label="Compare" width={860} onClose={onClose}>
      <div className="confirm">
        <h2 className="h2">The file now, and your text</h2>
        <p className="muted">Lines that differ are marked.</p>
        <div className="compare">
          <div>
            <div className="eyebrow">On disk</div>
            <pre>{lines(theirs, d.a)}</pre>
          </div>
          <div>
            <div className="eyebrow">Yours (unsaved)</div>
            <pre>{lines(mine, d.b)}</pre>
          </div>
        </div>
        <div className="row">
          <span className="grow" />
          <button
            type="button"
            className="btn lg"
            title="Copy your unsaved text to the clipboard"
            onClick={() => void api.copyText(mine).then(() => toast("Your text is copied"))}
          >
            Copy mine
          </button>
          <button type="button" className="btn lg pri" title="Close the comparison" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </Dialog>
  );
}

const setReadSize = (px: number) => settings.update({ readSize: applyReadSize(px) });

/** ⌘+ / ⌘− / ⌘0 change the text size while `on` (a note or page is open). With Shift they're the
 *  Interface size's (src/interfaceSize.ts), so a shifted = (+ on a US keyboard) isn't taken here. */
function useTextSizeKeys(on: boolean) {
  useEffect(() => {
    if (!on) return;
    const set = setReadSize;
    const k = (e: KeyboardEvent) => {
      if (!isMod(e) || e.altKey || e.shiftKey || strayCtrl(e)) return;
      const cur = settings.get().readSize ?? READ_SIZE.default;
      if (e.key === "=" || e.key === "+") set(cur + 1);
      else if (e.key === "-") set(cur - 1);
      else if (e.key === "0") set(READ_SIZE.default);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [on]);
}

/** The text size slider, in a popover under `anchor`: saved in settings. */
function TextSizePanel({ anchor, onClose }: { anchor: DOMRect; onClose: () => void }) {
  const size = useStore(settings).readSize ?? READ_SIZE.default;
  const set = setReadSize;
  return (
    <Popover anchor={anchor} onClose={onClose} width={260}>
      <div className="textsize">
        <div className="eyebrow">Text size · {size}px</div>
        <div className="row">
          <span style={{ fontSize: 12 }}>A</span>
          <input
            type="range"
            min={READ_SIZE.min}
            max={READ_SIZE.max}
            value={size}
            onChange={(e) => set(+e.target.value)}
            aria-label="Text size"
            style={{ flex: 1 }}
          />
          <span style={{ fontSize: 18 }}>A</span>
        </div>
        <span
          title={
            size === READ_SIZE.default ? "The text is already the default size" : keyLabel("Set the text back to the default size (⌘0)")
          }
        >
          <button type="button" className="btn sm" disabled={size === READ_SIZE.default} onClick={() => set(READ_SIZE.default)}>
            Reset to {READ_SIZE.default}px
          </button>
        </span>
      </div>
    </Popover>
  );
}

/** A sibling app's text size control: a button opening the slider (focus mode's bar has it; the
 *  note's own bar has it in More). With `off`, greyed, and `off` says why. */
export function TextSizeButton({ off }: { off?: string } = {}) {
  const [a, setA] = useState<DOMRect | null>(null);
  if (off)
    return (
      <span title={off}>
        <button type="button" className="ibtn" aria-label="Text size" disabled>
          <Icon name="textsize" />
        </button>
      </span>
    );
  return (
    <>
      <button
        type="button"
        className="ibtn"
        aria-label="Text size"
        title={keyLabel("Text size (⌘+ ⌘− ⌘0)")}
        onClick={(e) => setA(e.currentTarget.getBoundingClientRect())}
      >
        <Icon name="textsize" />
      </button>
      {a && <TextSizePanel anchor={a} onClose={() => setA(null)} />}
    </>
  );
}

const linkName = (v: string) => v.replace(/^\[\[|\]\]$/g, "").split(/[|#]/)[0];

/** Under a wiki page's title: what else it's called, when it was updated, how many sources. */
function WikiSummary({ text, sources }: { text: string; sources: number }) {
  const aliases = getList(text, "aliases");
  const updated = getScalar(text, "updated");
  const bits = [
    aliases.length ? `Also ${aliases.join(", ")}` : null,
    updated ? `Updated ${updated}` : null,
    `${sources} source${sources === 1 ? "" : "s"}`,
  ].filter(Boolean);
  return <p className="wikisum faint">{bits.join(" · ")}</p>;
}

function Backlinks({ doc, onFold }: { doc: Doc; onFold: () => void }) {
  const b = doc.meta.backlinks;
  return (
    <section className="card side-card">
      <div className="row">
        <div className="eyebrow grow">Linked from · {b.length}</div>
        <button
          type="button"
          className="ibtn"
          title="Collapse, for the note's full width"
          aria-label="Collapse Linked from"
          onClick={onFold}
        >
          <Icon name="chevup" size={12} />
        </button>
      </div>
      {b.length === 0 && <p className="faint">Nothing links here yet.</p>}
      {b.map((l) => (
        <button key={l.path} type="button" className="blink" title={`Open ${l.title}`} onClick={() => openDoc(l.path)}>
          <span className="lbt">
            <LayerTag layer={l.layer} />
            <span className="ell">{l.title}</span>
          </span>
          {l.context && <span className="faint">{iconizeEmoji(plainText(l.context))}</span>}
        </button>
      ))}
    </section>
  );
}

function propValue(v: unknown): string {
  if (v === null || v === undefined || v === "") return "—";
  if (Array.isArray(v)) return v.map(propValue).join(", ");
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

function propIcon(v: unknown): string {
  const s = Array.isArray(v) ? "" : String(v ?? "");
  if (/^\d{4}-\d{2}-\d{2}/.test(s)) return "calendar";
  if (/^-?\d+(\.\d+)?$/.test(s)) return "hash";
  if (s === "true" || s === "false") return "check";
  if (Array.isArray(v)) return "tag";
  return "text";
}

/** Every frontmatter property, in the file's order. Values with wikilinks link. */
function Properties({ doc }: { doc: Doc }) {
  const fm = doc.meta.frontmatter;
  const err = doc.meta.frontmatterError;
  if (err)
    return (
      <div className="props err" title={err}>
        The properties at the top of this file aren't valid YAML, so they're shown as text below.
      </div>
    );
  if (!fm || !Object.keys(fm).length) return null;
  return (
    <dl className="props">
      {Object.entries(fm).map(([k, v]) => (
        <div key={k} className="prop">
          <dt>
            <Icon name={propIcon(v)} size={13} />
            {k}
          </dt>
          <dd>
            <PropValue v={v} tags={k === "tags"} />
          </dd>
        </div>
      ))}
    </dl>
  );
}

/** `tags`: the tags property, each shown with its #, as tags are everywhere else. */
function PropValue({ v, tags }: { v: unknown; tags?: boolean }) {
  const items = Array.isArray(v) ? v : [v];
  if (items.every((x) => typeof x === "string" && /^\[\[[^\]]+\]\]$/.test(x))) {
    return (
      <span className="chips">
        {(items as string[]).map((x) => (
          <PropLink key={x} target={x.slice(2, -2).split("|")[0]} />
        ))}
      </span>
    );
  }
  if (Array.isArray(v) && v.every((x) => typeof x !== "object"))
    return (
      <span className="chips">
        {v.map((x, i) => (
          <span key={i} className="chip">
            {tags ? `#${String(x).replace(/^#/, "")}` : String(x)}
          </span>
        ))}
      </span>
    );
  return <>{propValue(v)}</>;
}

function PropLink({ target }: { target: string }) {
  const [to, setTo] = useState<string | null | undefined>(undefined);
  useEffect(() => {
    let live = true;
    api
      .linksResolve([target])
      .then(([p]) => live && setTo(p))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [target]);
  return (
    <span title={to ? `Open ${target}` : to === null ? `There's no page called ${target} yet` : "Looking for the page…"}>
      <button type="button" className={`chip link ${to === null ? "ghost" : ""}`} disabled={!to} onClick={() => to && openDoc(to)}>
        {target}
      </button>
    </span>
  );
}

/** Whether the note opens with an H1 that only repeats its title, which View then leaves out. */
export function h1RepeatsTitle(src: string, title: string): boolean {
  if (!bodyStartsWithH1(src)) return false;
  const h = /^\s*#\s+(.+)$/m.exec(src.replace(/^\uFEFF?---\n[\s\S]*?\n---/, ""));
  const norm = (t: string) => plainText(t).toLowerCase();
  return !!h && norm(h[1]) === norm(title);
}

/** The document's light/dark switch, and its theme and colour (defaults in Settings › Notes). */
function DocLookControls({ look }: { look: ReturnType<typeof useDocLook> }) {
  const [at, setAt] = useState<DOMRect | null>(null);
  const btn = useRef<HTMLButtonElement>(null);
  // Screenshot mode: the scene's Look picker open.
  useEffect(() => {
    if (btn.current && sceneDocMenu.get() === "look") setAt(btn.current.getBoundingClientRect());
  }, []);
  const style = DOC_STYLES.find(([id]) => id === look.style)?.[1] ?? "Brainstead";
  const colour = DOC_ACCENTS.find(([id]) => id === look.accent);
  const label = `${style}, ${colour?.[1] ?? "Brainstead blue"}${look.overridden ? " (this note)" : ""}`;
  return (
    <span className="doclook">
      <button
        type="button"
        className="ibtn sm"
        aria-label={look.theme === "dark" ? "Document in light" : "Document in dark"}
        title={look.theme === "dark" ? "Show documents light" : "Show documents dark"}
        onClick={look.toggleTheme}
      >
        <Icon name={look.theme === "dark" ? "sun" : "moon"} size={14} />
      </button>
      <button
        ref={btn}
        type="button"
        className={`btn ghost sm lookbtn ${look.overridden ? "own" : ""}`}
        aria-label={`Document look: ${label}`}
        title={`How this document looks: ${label}. Click to change the theme and colour`}
        onClick={(e) => setAt(e.currentTarget.getBoundingClientRect())}
      >
        <span className="faint">Look:</span>
        <span className="swatch" style={{ background: colour?.[2] }} />
        {style}
      </button>
      {at && (
        <Popover anchor={at} onClose={() => setAt(null)} width={260}>
          <LookPicker style={look.style} accent={look.accent} onStyle={look.setStyle} onAccent={look.setAccent} />
          <div className="lookfoot">
            {look.overridden ? (
              <button
                type="button"
                className="btn ghost sm"
                title="Drop this note's own theme and colour and use the defaults"
                onClick={look.useDefaults}
              >
                Use the defaults
              </button>
            ) : (
              <span className="faint">Changes apply to this note.</span>
            )}
            <button
              type="button"
              className="btn ghost sm"
              title="Change the default theme and colour in Settings › Notes"
              onClick={() => {
                setAt(null);
                nav.go({ screen: "settings", pane: "notes" });
              }}
            >
              Defaults…
            </button>
          </div>
        </Popover>
      )}
    </span>
  );
}
