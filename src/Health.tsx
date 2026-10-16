// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Knowledge health (stage 7a): the previous app's eight wiki checks, ported to Rust, and
// "More checks", Brainstead's own (possible duplicates, stale pages others rely on, sources changed since
// they were cited, page shape, system notes' headers and the rest). They rerun in the background whenever the index changes. Safe fixes
// (cross-links, `updated:` dates, log lines, system notes' headers) apply straight away and can be undone; anything that
// needs judgement goes to Ask, whose changes are listed in Changes.

import { useEffect, useMemo, useRef, useState } from "react";
import { api, CurrentStateRun, FileSummary, LintCheck, LintItem, LintReport, TrashEntry } from "./api";
import { fmtShortDate } from "./md/dates";
import { linkMatches } from "./Capture";
import { askWith } from "./askState";
import { ingest } from "./Ingest";
import { Icon } from "./icons";
import { ingestable } from "./Lists";
import { useGlance, WikiGlance } from "./Glance";
import {
  ADVISORY,
  claimsByPage,
  decisions,
  OWN_ACTIONS,
  fixOf,
  fixWithAskPrompt,
  fileSetChanged,
  GhostHelp,
  ghostHelps,
  ghostPaths,
  health,
  pageName,
  IGNORE_ALL,
  reloadHealth,
  safeFixes,
  startKnowledge,
  trendLabels,
  trendPoints,
  trendShown,
  trashByName,
} from "./knowledge";
import { nav, openDoc, useViewState } from "./nav";
import { reportEditError, undoLast } from "./taskModel";
import { toast } from "./Toast";
import { TopBar } from "./TopBar";
import { Dialog } from "./ui";
import { Store, useStore } from "./store";
import { keyLabel } from "./platform";
import { vaultVersion } from "./state";

const undo = { label: "Undo", kbd: "⌘Z", run: () => void undoLast() };

/** Actions under way, by key: their buttons show a spinner until the work and the recheck after it are done. */
const busy = new Store<ReadonlySet<string>>(new Set());
const setBusy = (key: string, on: boolean) => {
  const n = new Set(busy.get());
  if (on) n.add(key);
  else n.delete(key);
  busy.set(n);
};

/** Resolves when the checks next report (they rerun in the background after a fix), or after 15 s. */
function nextReport(): Promise<void> {
  return new Promise((done) => {
    const t = setTimeout(finish, 15_000);
    const off = health.subscribe(finish);
    function finish() {
      clearTimeout(t);
      off();
      done();
    }
  });
}

/** Runs an action with its button busy; with `recheck`, until the checks have run again. */
export function act<T>(key: string, run: () => Promise<T>, then: (r: T) => void, recheck = true): void {
  if (busy.get().has(key)) return;
  setBusy(key, true);
  const report = recheck ? nextReport() : Promise.resolve();
  run()
    .then(async (r) => {
      then(r);
      await report;
    })
    .catch(reportEditError)
    .finally(() => setBusy(key, false));
}

/** A button that shows a spinner while its action runs. */
function BusyButton({
  id,
  icon,
  className = "btn sm",
  disabled,
  title,
  onClick,
  children,
}: {
  id: string;
  icon?: string;
  className?: string;
  disabled?: boolean;
  title?: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  const on = useStore(busy).has(id);
  return (
    <button type="button" className={className} disabled={disabled || on} aria-busy={on} title={title} onClick={onClick}>
      {on ? <span className="spin" /> : icon ? <Icon name={icon} size={className.includes("sm") ? 12 : 14} /> : null}
      {children}
    </button>
  );
}

const fixed = (label: string) => toast(label, undo, "ok");

/** After Reshape pages: how many, and the way to Changes, where they can be reverted. */
const reshaped = (r: { applied: number; failed: string[] }) =>
  toast(
    `Reshaped ${r.applied === 1 ? "1 page" : `${r.applied} pages`}${r.failed.length ? `; ${r.failed.length} couldn't be: ${r.failed[0]}` : ""}`,
    { label: "Changes", run: () => nav.go("review") },
    r.failed.length ? "bad" : "ok",
  );

export function HealthScreen() {
  useEffect(startKnowledge, []);
  const h = useStore(health);
  const r = h?.report ?? null;
  const [open, setOpen] = useViewState<string | null>("health:open", "missing-pages", false);
  const [trash, setTrash] = useState<string | null>(null);
  const [linking, setLinking] = useState<LintItem | null>(null);
  const [reshaping, setReshaping] = useState<number | null>(null);
  const fixes = safeFixes(r);
  // The vault's files and the Trash, for a missing link's Did you mean and In the Trash: read again
  // when a file is added, removed or renamed (or the vault reindexed), not on every autosave.
  const [paths, setPaths] = useState<string[]>([]);
  const [trashed, setTrashed] = useState<Map<string, TrashEntry>>(new Map());
  const known = useRef<Set<string> | null>(null);
  const vv = useStore(vaultVersion);
  useEffect(() => {
    if (!fileSetChanged(known.current, vv.changes)) return;
    void api
      .filesList(null)
      .then((f) => {
        known.current = new Set(f.map((x) => x.path));
        setPaths(ghostPaths(f));
      })
      .catch(() => {});
    void api
      .trashList()
      .then((t) => setTrashed(trashByName(t)))
      .catch(() => {});
  }, [vv]);
  const ghosts = useMemo(() => ghostHelps(r, paths, trashed), [r, paths, trashed]);
  const need = decisions(r);

  const recheck = () =>
    act(
      "recheck",
      () => reloadHealth(true),
      () => {},
      false,
    );
  const fixWithAsk = () => {
    if (!r) return;
    askWith(fixWithAskPrompt(r));
    nav.go("ask");
  };
  const classic = r?.checks.filter((c) => c.classic) ?? [];
  const ours = r?.checks.filter((c) => !c.classic) ?? [];
  const history = h?.history ?? [];
  const pts = trendPoints(history, 280, 52);
  const labels = trendShown(history) ? trendLabels(history) : { from: "", to: "", tip: "" };

  return (
    <main className="main">
      <TopBar
        title="Knowledge health"
        sub={r ? `${r.wikiPages} wiki pages · ${r.sources} sources · rechecks when the vault changes` : "Checking…"}
        ask={{ about: "the wiki's health", prompt: "About my wiki's health: " }}
      >
        <button
          type="button"
          className="btn"
          title="Show places where wiki pages disagree with each other"
          onClick={() => nav.go("contradictions")}
        >
          <Icon name="info" size={14} />
          Contradictions
        </button>
        <BusyButton id="recheck" icon="refresh" className="btn" title="Run the checks again now" onClick={recheck}>
          Check now
        </BusyButton>
        <BusyButton
          id="fix-all"
          icon="check"
          className="btn"
          title={keyLabel("Fix every issue that's safe to fix; ⌘Z undoes it")}
          disabled={!fixes.length}
          onClick={() => act("fix-all", () => api.healthFix(fixes), fixed)}
        >
          Fix safe issues{fixes.length ? ` (${fixes.length})` : ""}
        </BusyButton>
        <button
          type="button"
          className="btn pri"
          disabled={!need}
          onClick={fixWithAsk}
          title="Asks the assistant to fix them; its changes are listed in Changes, where you can revert them"
        >
          <Icon name="ask" size={14} />
          Fix with Ask
        </button>
      </TopBar>
      <div className="body health">
        <div className="hmain">
          <div className="card hsum">
            <div className="hnum">
              <span className="eyebrow">Need a decision</span>
              <b>{r ? need : "…"}</b>
              <span className="faint small">{fixes.length ? `${fixes.length} more are safe to fix` : "nothing safe to fix"}</span>
            </div>
            {trendShown(history) ? (
              <div className="htrend" title={labels.tip}>
                <svg width="280" height="52" viewBox="0 0 280 52" role="img" aria-label={labels.tip}>
                  <polyline points={pts} className="trend" />
                </svg>
                <div className="htrend-ends faint small">
                  <span>{labels.from}</span>
                  <span>{labels.to}</span>
                </div>
              </div>
            ) : history.length < 2 ? (
              <span className="faint small">The trend shows after a second day.</span>
            ) : null}
          </div>
          {r && (
            <Group title="Wiki checks" checks={classic} open={open} setOpen={setOpen} on={{ setTrash, setLinking, setReshaping, ghosts }} />
          )}
          {r && (
            <Group title="More checks" checks={ours} open={open} setOpen={setOpen} on={{ setTrash, setLinking, setReshaping, ghosts }} />
          )}
        </div>
        <aside className="hside">{r && <SideCards r={r} />}</aside>
      </div>
      {trash && (
        <Dialog onClose={() => setTrash(null)} width={440} label="Move image to Trash">
          <div className="confirm">
            <h2 className="h2">Move {pageName(trash)} to the Trash?</h2>
            <p className="muted">No note embeds or links it. You can restore it from the Trash.</p>
            <div className="row">
              <span className="grow" />
              <button type="button" className="btn lg" title="Keep the image" onClick={() => setTrash(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn lg pri"
                title="Move the image to the Trash; you can restore it from there"
                onClick={() => {
                  const p = trash;
                  setTrash(null);
                  act(
                    `trash:${p}`,
                    () => api.healthTrashImage(p),
                    () => toast("Moved to the Trash", { label: "Open Trash", run: () => nav.go("trash") }, "ok"),
                  );
                }}
              >
                Move to the Trash
              </button>
            </div>
          </div>
        </Dialog>
      )}
      {linking && <LinkGhost item={linking} onClose={() => setLinking(null)} />}
      {reshaping !== null && (
        <Dialog onClose={() => setReshaping(null)} width={460} label="Reshape pages">
          <div className="confirm">
            <h2 className="h2">Reshape {reshaping === 1 ? "1 page" : `${reshaping} pages`}?</h2>
            <p className="muted">
              Each page gets the page shape: opening text, Current state, its topics, a Timeline newest first, See also. Sections move whole
              and dated headings are rewritten; no text is lost. Each page is one change in Changes, revertable alone or all together. Pages
              that need you are left as they are.
            </p>
            <div className="row">
              <span className="grow" />
              <button type="button" className="btn lg" title="Leave the pages as they are" onClick={() => setReshaping(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn lg pri"
                title="Reshape them now; revertable in Changes"
                onClick={() => {
                  setReshaping(null);
                  act("reshape-all", () => api.healthReshape(null), reshaped);
                }}
              >
                Reshape pages
              </button>
            </div>
          </div>
        </Dialog>
      )}
    </main>
  );
}

/** What each check looks for, in a line: the title alone doesn't say. */
export const CHECK_TIPS: Record<string, string> = {
  "missing-pages": "Links to pages that don't exist yet",
  "broken-sources": "A page's sources: list names a note or file that isn't in the vault",
  orphans: "Wiki pages no other page links to, so they're hard to come across",
  "missing-crossrefs": "A page mentions another page by name without linking to it",
  "stale-dates": "A page's “updated:” date is older than its last change; Fix sets it to the day it changed",
  "unlogged-writes": "Wiki pages changed after the last entry in log.md, so the log doesn't record the change; Fix adds the log lines",
  "uningested-sources": "Files in sources/ that no wiki page cites yet",
  "unreferenced-images": "Images that no note embeds or links to",
  duplicates: "Pages so alike in name or text that they may be about the same thing",
  "stale-pages": "Pages many others link to that haven't changed in 90 days or more",
  "changed-sources": "Sources that changed after the pages citing them were written",
  "no-current-state":
    "Wiki pages with a Timeline but no Current state section; Write Current state has the model write one from the page's newest entries",
  "page-shape":
    "Entity and concept pages not laid out as opening text, Current state, topics, Timeline (newest first) and See also; Reshape pages moves their sections",
  "system-callouts": 'System notes whose "This is a system note" header has gone; Fix puts it back as Brainstead last saw it',
};

type Handlers = {
  setTrash: (p: string) => void;
  setLinking: (i: LintItem) => void;
  setReshaping: (n: number) => void;
  /** Each missing link's In the Trash and Did you mean, by check and item text (ghostHelps). */
  ghosts: Map<string, GhostHelp>;
};

function Group({
  title,
  checks,
  open,
  setOpen,
  on,
}: {
  title: string;
  checks: LintCheck[];
  open: string | null;
  setOpen: (id: string | null) => void;
  on: Handlers;
}) {
  return (
    <div className="card hchecks">
      <div className="hgroup eyebrow">{title}</div>
      {checks.map((c) => {
        const n = c.items.length;
        const ignored = c.ignored ?? 0;
        const isOpen = open === c.id && n + ignored > 0;
        return (
          <div key={c.id}>
            <button
              type="button"
              className={`ck ${n + ignored ? "" : "zero"} ${isOpen ? "open" : ""}`}
              aria-expanded={isOpen}
              title={n + ignored ? (isOpen ? "Hide the issues" : "Show the issues") : "Nothing to fix here"}
              onClick={() => setOpen(isOpen ? null : c.id)}
            >
              <Icon
                name={n ? "info" : "check"}
                size={14}
                className={n ? (ADVISORY.has(c.id) ? "advisory" : c.items.some((i) => !i.safe) ? "amber" : "") : "green"}
              />
              <span className="grow" title={CHECK_TIPS[c.id]}>
                {c.title}
              </span>
              <span className="c">{n}</span>
              {n + ignored > 0 && <Icon name="chevdown" size={12} style={{ transform: isOpen ? undefined : "rotate(-90deg)" }} />}
            </button>
            {isOpen && (
              <div className={`ckitems ${c.id === "missing-pages" || c.id === "broken-sources" ? "ghosts" : ""}`}>
                {c.id === "page-shape" && <ReshapeAll items={c.items} on={on} />}
                {c.id === "no-current-state" && <WriteCurrentState n={c.items.length} />}
                {IGNORE_ALL.has(c.id) && n > 0 && <IgnoreAll check={c.id} n={n} />}
                {c.id === "orphans" && n > 0 && <OrphansFix n={n} />}
                {c.id === "uncited-claims"
                  ? claimsByPage(c.items)
                      .slice(0, 200)
                      .map((g) => <ClaimsRow key={g.page} page={g.page} items={g.items} />)
                  : c.items.slice(0, 200).map((i) => <Row key={i.text} check={c.id} i={i} on={on} />)}
                {n > 200 && <p className="faint small">and {n - 200} more</p>}
                {ignored > 0 && <Ignored check={c.id} n={ignored} />}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/** One issue, with what can be done about it. */
function Row({ check, i, on }: { check: string; i: LintItem; on: Handlers }) {
  const page = i.page;
  // A missing page, or a source a page cites that isn't there: the same help either way, In the
  // Trash and Did you mean (ghostHelp).
  const help = on.ghosts.get(`${check}:${i.text}`) ?? null;
  const ghost = help?.name ?? null;
  const linking = check === "broken-sources" && page ? [page] : (i.pages ?? []);
  const inTrash = help?.inTrash ?? null;
  const near = help?.near ?? null;
  const open = page && !page.startsWith("images/") ? () => openDoc(page) : undefined;
  const label = (() => {
    switch (check) {
      case "missing-pages":
        return `[[${i.name}]]`;
      case "broken-sources":
        return `${pageName(page ?? "")} cites [[${i.name}]], which isn't there`;
      case "missing-crossrefs":
        return `${pageName(page ?? "")}: “${i.name}” not linked`;
      case "duplicates":
        return i.text;
      case "stale-dates":
        return `${pageName(page ?? "")}: updated says older than the file (${i.detail})`;
      case "stale-pages":
        return `${pageName(page ?? "")} · ${i.count} backlinks · ${i.detail}`;
      case "page-shape":
        return pageName(page ?? "");
      default:
        return page ? (check === "uningested-sources" || check === "unreferenced-images" ? page : pageName(page)) : i.text;
    }
  })();
  return (
    <div className={`ckitem ${ghost ? "ghost" : ""}`}>
      {open ? (
        <button type="button" className="blink grow" title={`Open ${page}`} onClick={open}>
          {label}
        </button>
      ) : (
        <span className="grow">{label}</span>
      )}
      {check === "duplicates" && i.detail && <span className="faint small">{i.detail}</span>}
      {check === "page-shape" && page && <ReshapeOne page={page} i={i} />}
      {i.safe && page && check !== "page-shape" && (
        <BusyButton
          id={`fix:${i.text}`}
          title={keyLabel("Fix this issue; ⌘Z undoes it")}
          onClick={() => act(`fix:${i.text}`, () => api.healthFix([fixOf(check, i)]), fixed)}
        >
          Fix
        </BusyButton>
      )}
      {/* A missing link (a missing page, or a source that isn't there): three columns shared by
          the check's rows (.ckitems.ghosts) — what's known about it, the suggestion, the buttons. */}
      {ghost && (
        <>
          <div className="gpills">
            {inTrash && (
              <span className="chip amber" title={`${inTrash.originalRel} was moved to the Trash`}>
                In the Trash since {fmtShortDate(inTrash.deletedAt.slice(0, 10))}
              </span>
            )}
            {check === "missing-pages" && (
              <span className="faint small" title={(i.pages ?? []).join("\n")}>
                Linked from {(i.pages ?? []).slice(0, 2).map(pageName).join(", ")}
                {(i.pages?.length ?? 0) > 2 ? ` and ${(i.pages?.length ?? 0) - 2} more` : ""}
              </span>
            )}
          </div>
          <div className="gnear small">
            {near && (
              <>
                Did you mean <b>{pageName(near)}</b>?
              </>
            )}
          </div>
          <div className="gbtns">
            {near && (
              <BusyButton
                id={`near:${i.name}`}
                className="btn sm pri"
                title={`Point the link at ${pageName(near)} on ${linking.length === 1 ? "its page" : "each page"}; revertable in Changes`}
                onClick={() =>
                  act(
                    `near:${i.name}`,
                    () => api.healthLinkGhost(ghost, near, linking),
                    (n) =>
                      toast(
                        `Linked to ${pageName(near)} on ${n} page${n === 1 ? "" : "s"}`,
                        { label: "Changes", run: () => nav.go("review") },
                        "ok",
                      ),
                  )
                }
              >
                Link to it
              </BusyButton>
            )}
            {inTrash && (
              <BusyButton
                id={`restore:${i.name}`}
                className={`btn sm ${near ? "" : "pri"}`}
                title={`Put ${inTrash.basename} back where it was, so the link works again`}
                onClick={() =>
                  act(
                    `restore:${i.name}`,
                    () => api.trashRestore(inTrash.id, null),
                    (p) => toast(`Restored ${pageName(p)}`, undefined, "ok"),
                  )
                }
              >
                Restore from Trash
              </BusyButton>
            )}
            {/* A file in the Trash isn't a page to write: Restore or Link to it instead. */}
            {check === "missing-pages" && !inTrash && (
              <BusyButton
                id={`create:${i.name}`}
                title={keyLabel(`Make an entity page called ${pageName(ghost)} in wiki/entities and open it; ⌘Z undoes it`)}
                onClick={() =>
                  act(
                    `create:${i.name}`,
                    () => api.healthCreatePage(ghost, "entities"),
                    (p) => {
                      toast(`Created ${pageName(p)}`, undo, "ok");
                      openDoc(p);
                    },
                    false,
                  )
                }
              >
                Create
              </BusyButton>
            )}
            <button
              type="button"
              className="btn sm"
              title={
                check === "broken-sources"
                  ? "Point this source at a file that's there instead"
                  : "Point this missing link at a page that already exists"
              }
              onClick={() => on.setLinking({ ...i, name: ghost, pages: linking })}
            >
              Link to…
            </button>
            {check === "broken-sources" && (
              <BusyButton
                id={`ignore:${check}:${i.text}`}
                className="btn sm ghost"
                title="Stop listing this issue until its page changes"
                onClick={() =>
                  act(
                    `ignore:${check}:${i.text}`,
                    () => api.healthIgnore(check, i.text),
                    () => {},
                    false,
                  )
                }
              >
                Ignore
              </BusyButton>
            )}
          </div>
        </>
      )}
      {check === "duplicates" && page && i.pages?.[0] && (
        <>
          <button type="button" className="btn sm" title={`Open ${i.pages![0]}`} onClick={() => openDoc(i.pages![0])}>
            Open the other
          </button>
          <BusyButton
            id={`dismiss:${i.text}`}
            className="btn sm ghost"
            title="They're about different things: stop listing them as possible duplicates"
            onClick={() =>
              act(
                `dismiss:${i.text}`,
                () => api.healthDismiss(page, i.pages![0]),
                () => {},
              )
            }
          >
            Not duplicates
          </BusyButton>
        </>
      )}
      {check === "unreferenced-images" && page && (
        <BusyButton id={`trash:${page}`} title="Move this image to the Trash, after you confirm" onClick={() => on.setTrash(page)}>
          Move to the Trash
        </BusyButton>
      )}
      {check === "uningested-sources" && page && <IngestButton path={page} />}
      {check === "changed-sources" && page && <IngestButton path={page} again />}
      {!i.safe && !OWN_ACTIONS.has(check) && !ghost && (
        <BusyButton
          id={`ignore:${check}:${i.text}`}
          className="btn sm ghost"
          title="Stop listing this issue until its page changes"
          onClick={() =>
            act(
              `ignore:${check}:${i.text}`,
              () => api.healthIgnore(check, i.text),
              () => {},
              false,
            )
          }
        >
          Ignore
        </BusyButton>
      )}
    </div>
  );
}

/** A page's claims with no citation, one row: each line under the page's name, all of them
 *  highlighted when the page opens, and Ignore for them all. */
function ClaimsRow({ page, items }: { page: string; items: LintItem[] }) {
  const lines = items.map((i) => i.detail ?? "").filter(Boolean);
  const id = `ignore:uncited-claims:${page}`;
  return (
    <div className="ckitem">
      <button
        type="button"
        className="blink grow"
        title={`Open ${page} with ${lines.length === 1 ? "this line" : `these ${lines.length} lines`} highlighted`}
        onClick={() => openDoc(page, { lines })}
      >
        {pageName(page)}
        {lines.map((l) => (
          <span key={l} className="ckline faint">
            {l}
          </span>
        ))}
      </button>
      <BusyButton
        id={id}
        className="btn sm ghost"
        title={lines.length === 1 ? "Stop listing this claim until its line changes" : "Stop listing these claims until their lines change"}
        onClick={() =>
          act(
            id,
            async () => {
              for (const i of items) await api.healthIgnore("uncited-claims", i.text);
            },
            () => {},
            false,
          )
        }
      >
        Ignore
      </BusyButton>
    </div>
  );
}

/** Ignore all: every issue the check lists, each until its page (or a claim's line) changes. */
function IgnoreAll({ check, n }: { check: string; n: number }) {
  const id = `ignore-all:${check}`;
  const what = check === "uncited-claims" ? (n === 1 ? "this claim" : `these ${n} claims`) : n === 1 ? "this page" : `these ${n} pages`;
  return (
    <div className="ckitem">
      <span className="grow faint small">
        {check === "uncited-claims" ? "Each comes back when its line changes." : "Each comes back when its page changes."}
      </span>
      <BusyButton
        id={id}
        className="btn sm ghost"
        title={`Stop listing ${what} until ${check === "uncited-claims" ? "their lines" : "they"} change; Show again lists them`}
        onClick={() =>
          act(
            id,
            () => api.healthIgnoreAll(check),
            (k) => toast(`Ignored ${k === 1 ? "1 issue" : `${k} issues`}`, undefined, "ok"),
            false,
          )
        }
      >
        Ignore all
      </BusyButton>
    </div>
  );
}

/** Orphan pages: index.md's catalogue links every wiki page, so bringing it up to date (as the daily
 *  check does) leaves none an orphan. */
function OrphansFix({ n }: { n: number }) {
  return (
    <div className="ckitem">
      <span className="grow faint small">
        {n === 1 ? "It isn't" : "They aren't"} in index.md's catalogue yet, which lists every wiki page; the daily check brings it up to
        date, or do it now.
      </span>
      <BusyButton
        id="orphans-index"
        className="btn sm pri"
        title="Bring index.md's catalogue up to date now, so every wiki page is linked from it"
        onClick={() =>
          act(
            "orphans-index",
            () => api.catalogueUpdate(),
            (m) => toast(m.charAt(0).toUpperCase() + m.slice(1), undefined, "ok"),
          )
        }
      >
        Add {n === 1 ? "it" : "them"} to index.md
      </BusyButton>
    </div>
  );
}

/** A check's ignored issues: how many, and Show again. */
function Ignored({ check, n }: { check: string; n: number }) {
  return (
    <div className="ckitem">
      <span className="grow faint small">
        {n === 1 ? "1 issue ignored until its page changes" : `${n} issues ignored until their pages change`}
      </span>
      <BusyButton
        id={`unignore:${check}`}
        className="btn sm ghost"
        title="List this check's ignored issues again"
        onClick={() =>
          act(
            `unignore:${check}`,
            () => api.healthUnignore(check),
            () => {},
          )
        }
      >
        Show again
      </BusyButton>
    </div>
  );
}

const SAMPLE = 5;

/** Write Current state: a sample first, then the rest, with the run's progress while it goes. */
function WriteCurrentState({ n }: { n: number }) {
  const [run, setRun] = useState<CurrentStateRun | null>(null);
  useEffect(() => {
    void api
      .currentStateStatus()
      .then(setRun)
      .catch(() => {});
    const off = api.onCurrentStateChanged((r) => {
      setRun(r);
      if (!r.running) void reloadHealth(true);
    });
    return () => void off.then((f) => f()).catch(() => {});
  }, []);
  const start = (limit: number | null) =>
    api
      .currentStateStart(null, limit)
      .then((ok) => !ok && toast("Write Current state is running already.", undefined, "bad"))
      .catch(reportEditError);
  if (run?.running)
    return (
      <div className="ckitem">
        <span className="spin" />
        <span className="grow faint small">
          Writing Current state: {run.done} of {run.total}, {run.written} written
        </span>
        <button type="button" className="btn sm ghost" title="Stop after the page it's on" onClick={() => void api.currentStateStop()}>
          Stop
        </button>
      </div>
    );
  const last =
    run && run.total
      ? `Last run: ${run.written} written${run.nothing ? `, ${run.nothing} with nothing current to say` : ""}${run.failed.length ? `, ${run.failed.length} couldn't be: ${run.failed[0]}` : ""}. `
      : "";
  return (
    <div className="ckitem">
      <span className="grow faint small" title={run?.failed.join("\n") || undefined}>
        {last}The model writes each from the page's opening and newest Timeline entries; each is a change in Changes.
      </span>
      <button
        type="button"
        className="btn sm"
        disabled={!n}
        title={`Write ${Math.min(SAMPLE, n)} first, to look at in Changes`}
        onClick={() => void start(SAMPLE)}
      >
        Write {Math.min(SAMPLE, n)}
      </button>
      <button
        type="button"
        className="btn sm"
        disabled={!n}
        title="Write a Current state on every page listed"
        onClick={() => void start(null)}
      >
        Write all{n ? ` (${n})` : ""}
      </button>
    </div>
  );
}

/** The check's own action: every page that reshapes by itself, asked first. */
function ReshapeAll({ items, on }: { items: LintItem[]; on: Handlers }) {
  const n = items.filter((i) => i.safe).length;
  const review = items.length - n;
  return (
    <div className="ckitem">
      <span className="grow faint small">
        {n === 1 ? "1 reshapes by itself" : n ? `${n} reshape by themselves` : "None reshape by themselves"}
        {review ? ` · ${review} need you first: fix what's listed, or reshape them as proposed` : ""}
      </span>
      <BusyButton
        id="reshape-all"
        icon="check"
        disabled={!n}
        title={n ? "Reshape every page that doesn't need you; each is a change in Changes" : "Every page left needs you first"}
        onClick={() => on.setReshaping(n)}
      >
        Reshape pages
      </BusyButton>
    </div>
  );
}

/** One page: why it isn't in the shape, and Reshape (as proposed, when it needs you). */
function ReshapeOne({ page, i }: { page: string; i: LintItem }) {
  const broken = i.detail?.startsWith("Can't be reshaped");
  return (
    <>
      <span className={`small ${i.safe ? "faint" : "why"}`} title={i.detail}>
        {i.safe ? (i.count === 1 ? "1 timeline entry" : `${i.count ?? 0} timeline entries`) : i.detail}
      </span>
      {!broken && (
        <BusyButton
          id={`reshape:${page}`}
          className={i.safe ? "btn sm" : "btn sm ghost"}
          title={
            i.safe
              ? "Reshape this page; a change in Changes you can revert"
              : "Reshape it as proposed anyway; open Changes to see the result and revert it if it's not right"
          }
          onClick={() => act(`reshape:${page}`, () => api.healthReshape([page]), reshaped)}
        >
          {i.safe ? "Reshape" : "Reshape anyway"}
        </BusyButton>
      )}
    </>
  );
}

function IngestButton({ path, again = false }: { path: string; again?: boolean }) {
  // As on Sources: only files an ingest can read get the button.
  if (!ingestable(path))
    return (
      <span className="faint small" title="Ingest reads notes, text, PDFs, Office files and images, not this kind of file">
        Can't ingest
      </span>
    );
  return (
    <button
      type="button"
      className="btn sm"
      title={
        again
          ? "Ingest it again, so the pages citing it catch up with how it reads now; their changes are listed in Changes"
          : "Ingest this source into the wiki; its page changes are listed in Changes"
      }
      onClick={() => ingest([path])}
    >
      {again ? "Ingest again" : "Ingest"}
    </button>
  );
}

function SideCards({ r }: { r: LintReport }) {
  const get = (id: string) => r.checks.find((c) => c.id === id)?.items ?? [];
  const pendingSources = get("uningested-sources");
  const changed = get("changed-sources");
  const glance = useGlance();
  return (
    <>
      {glance && <WikiGlance g={glance} />}
      <div className="card hq">
        <div className="row">
          <span className="eyebrow grow">Pending sources</span>
          <span className="faint small">{pendingSources.length + changed.length}</span>
        </div>
        {!pendingSources.length && !changed.length && <p className="faint small">Every source is cited.</p>}
        {pendingSources.slice(0, 6).map((i) => (
          <div key={i.text} className="hqi">
            <span className="lb source" />
            <span className="grow ell">{pageName(i.page ?? "")}</span>
            <IngestButton path={i.page ?? ""} />
          </div>
        ))}
        {changed.slice(0, 4).map((i) => (
          <div key={i.text} className="hqi">
            <span className="lb source" />
            <span className="grow ell">{pageName(i.page ?? "")}</span>
            <span className="chip amber sm">Changed</span>
          </div>
        ))}
      </div>
      <p className="faint small">
        Safe fixes (cross-links, dates, log lines, system-note headers) apply straight away and can be undone with {keyLabel("⌘Z")}.
        Anything that changes meaning is an agent change, listed in Changes with Revert.
      </p>
    </>
  );
}

/** Points a ghost link's links at a page that exists, one change per page (revertable in Changes). */
export function LinkGhost({ item, onClose }: { item: LintItem; onClose: () => void }) {
  const [name, setName] = useState("");
  const [working, setWorking] = useState(false);
  // The wiki's pages, matched as you type as `[[` matches them in Capture; the one picked is linked.
  const [pages, setPages] = useState<FileSummary[]>([]);
  const [picked, setPicked] = useState<FileSummary | null>(null);
  const [hi, setHi] = useState(0);
  const [listOpen, setListOpen] = useState(true);
  useEffect(() => {
    let live = true;
    api
      .filesList("wiki")
      .then((f) => live && setPages(f.filter((p) => !/^wiki\/(index|log)\.md$/.test(p.path))))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  const matches = useMemo(() => (name.trim() ? linkMatches(pages, name, 8) : []), [pages, name]);
  const showList = listOpen && !picked && matches.length > 0;
  const pick = (f: FileSummary) => {
    setPicked(f);
    setName(f.title);
    setListOpen(false);
  };
  const go = async () => {
    if (working) return;
    setWorking(true);
    try {
      const [to] = picked ? [picked.path] : await api.linksResolve([name.trim()]);
      if (!to) return toast(`There's no page called ${name.trim()}.`, undefined, "bad");
      // The button stays busy until the checks have run again, as the other fixes' do.
      const report = nextReport();
      const n = await api.healthLinkGhost(item.name!, to, item.pages ?? []);
      if (n) await report;
      onClose();
      toast(`Linked on ${n} page${n === 1 ? "" : "s"}`, { label: "Changes", run: () => nav.go("review") }, "ok");
    } catch (e) {
      reportEditError(e);
    } finally {
      setWorking(false);
    }
  };
  return (
    <Dialog onClose={onClose} width={460} label="Link to a page">
      <div className="confirm">
        <h2 className="h2">Link [[{item.name}]] to…</h2>
        <p className="muted small">
          On each page linking it, [[{item.name}]] becomes a link to the page you name, still reading “{item.name}”. Revertable in Changes.
        </p>
        <div className="pagepick">
          <label className="inp">
            <input
              autoFocus
              role="combobox"
              aria-expanded={showList}
              aria-controls="linkghost-pages"
              aria-label="The page to link to"
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                setPicked(null);
                setHi(0);
                setListOpen(true);
              }}
              onKeyDown={(e) => {
                if (showList && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
                  e.preventDefault();
                  setHi((h) => (h + (e.key === "ArrowDown" ? 1 : matches.length - 1)) % matches.length);
                } else if (e.key === "Enter") {
                  e.preventDefault();
                  if (showList) pick(matches[hi]);
                  else void go();
                }
              }}
              placeholder="Search the wiki's pages"
            />
          </label>
          {showList && (
            <ul className="csugg pagelist" id="linkghost-pages" role="listbox" aria-label="Wiki pages">
              {matches.map((f, i) => (
                <li key={f.path}>
                  <button
                    type="button"
                    role="option"
                    aria-selected={i === hi}
                    className={i === hi ? "on" : ""}
                    title={`Link to ${f.path}`}
                    onMouseDown={(e) => e.preventDefault()}
                    onMouseEnter={() => setHi(i)}
                    onClick={() => pick(f)}
                  >
                    <span>{f.title}</span>
                    <span className="faint small mono">{f.path.replace(/^wiki\//, "").replace(/\/[^/]+$/, "")}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" title="Close without changing anything" onClick={onClose}>
            Cancel
          </button>
          <span
            title={
              working ? "Linking…" : name.trim() ? "Change each linking page (↩); revertable in Changes" : "Name the page to link to first"
            }
          >
            <button type="button" className="btn lg pri" disabled={!name.trim() || working} aria-busy={working} onClick={() => void go()}>
              {working && <span className="spin" />}
              Link them
            </button>
          </span>
        </div>
      </div>
    </Dialog>
  );
}
