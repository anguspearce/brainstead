// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Projects (stage 6): every `Project. <name>.md` note, by status and area, with what moves each
// forward. A project with nothing to do next is flagged red, one that's gone quiet for two weeks
// amber. The open one shows its outcome, next actions, waiting-fors and the notes that link it.

import { FoundProjects } from "./Found";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, ProjectRow, TaskRow } from "./api";
import { askAboutNote } from "./Ask";
import { prepareCapture } from "./Capture";
import {
  areaProjects,
  moveInProjectOrder,
  orderedProjects,
  projectFlag,
  projectTasks,
  ProjectStatus,
  renameInProjectOrder,
  STATUS_LABEL,
  useProjects,
} from "./gtd";
import { Icon } from "./icons";
import { editDoc, nav, openDoc, place, useViewState } from "./nav";
import { settings, Store, useStore } from "./store";
import { trashFile } from "./notes/actions";
import { parseName } from "./notes/filename";
import { commitRename, renamedPath } from "./notes/Rename";
import { DraftNudge, GtdChips, TaskList } from "./TaskList";
import { useVaultVersion } from "./state";
import { reportEditError, undoLast, useAllTasks } from "./taskModel";
import { toast } from "./Toast";
import { askLink, askQuote, MoreMenu, TopBar } from "./TopBar";
import { iconizeEmoji } from "./md/fieldEmoji";
import { ago, Dialog, fmtCount, Seg } from "./ui";

/** ⌘K's New project: the Projects screen opens with New project showing. */
export const newProjectAsked = new Store<boolean>(false);
export function startNewProject() {
  newProjectAsked.set(true);
  nav.go("projects");
}

const TABS: ProjectStatus[] = ["active", "on-hold", "someday", "done"];

export function ProjectsScreen() {
  const projects = useProjects();
  const all = useAllTasks();
  const here = useStore(place).place;
  const [tab, setTab] = useViewState<ProjectStatus>("projects:tab", "active");
  // The areas folded shut, by name ("" is No area); kept across visits and restarts.
  const [folded, setFolded] = useViewState<string[]>("projects:folded", []);
  const fold = (area: string) => setFolded((f) => (f.includes(area) ? f.filter((a) => a !== area) : [...f, area]));
  // A renamed area keeps its place, folded or not and in the order dragged.
  const renamed = (from: string, to: string) => {
    renameInProjectOrder("areas", from, to, [...new Set((projects ?? []).map((p) => p.area ?? ""))]);
    setFolded((f) => (f.includes(from) ? [...f.filter((a) => a !== from && a !== to), to] : f));
  };
  const asked = useStore(newProjectAsked);
  const [creating, setCreatingState] = useState(false);
  const setCreating = (v: boolean) => {
    setCreatingState(v);
    if (!v) newProjectAsked.set(false);
  };
  const showNew = creating || asked;
  const sel = here.path ?? null;
  const counts = useMemo(() => new Map(TABS.map((s) => [s, (projects ?? []).filter((p) => p.status === s).length])), [projects]);
  const shown = (projects ?? []).filter((p) => p.status === tab);
  // Areas and their projects in the order dragged (kept in settings, D-20261012-01), the rest A–Z.
  const order = useStore(settings).projectOrder;
  const [drag, setDrag] = useState<{ kind: "areas" | "projects"; key: string; keys: string[] } | null>(null);
  // The drag under way's listeners, let go if the screen closes mid-drag (nothing saved).
  const dragging = useRef<(() => void) | null>(null);
  useEffect(() => () => dragging.current?.(), []);
  const areas = useMemo(() => {
    const m = new Map<string, ProjectRow[]>();
    for (const p of orderedProjects(shown, order)) m.set(p.area ?? "", [...(m.get(p.area ?? "") ?? []), p]);
    return [...m];
  }, [shown, order]);
  // While dragging, the list in its new order; it's saved on letting go.
  const asDragged = <T,>(kind: "areas" | "projects", items: T[], key: (t: T) => string): T[] => {
    if (drag?.kind !== kind || !items.some((t) => key(t) === drag.key)) return items;
    const at = new Map(drag.keys.map((k, i) => [k, i]));
    return [...items].sort((a, b) => (at.get(key(a)) ?? 1e9) - (at.get(key(b)) ?? 1e9));
  };
  // Drag by the grip: an area among the areas, a project among its area's; No area stays last.
  const startDrag = (e: React.PointerEvent, kind: "areas" | "projects", group: string, key: string, keys: string[]) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const startY = e.clientY;
    let moved = false;
    let cur = keys;
    const move = (m: PointerEvent) => {
      if (!moved && Math.abs(m.clientY - startY) < 4) return;
      moved = true;
      const others = keys.filter((k) => k !== key);
      const els = [...document.querySelectorAll<HTMLElement>(`[data-drag="${CSS.escape(group)}"]`)].filter((el) => el.dataset.key !== key);
      let at = others.length;
      for (let i = 0; i < els.length; i++) {
        const r = els[i].getBoundingClientRect();
        if (m.clientY < r.top + r.height / 2) {
          at = others.indexOf(els[i].dataset.key ?? "");
          break;
        }
      }
      if (at < 0) at = others.length;
      cur = [...others.slice(0, at), key, ...others.slice(at)];
      setDrag({ kind, key, keys: cur });
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      dragging.current = null;
    };
    const up = () => {
      stop();
      const i = cur.indexOf(key);
      if (moved && i !== keys.indexOf(key)) moveInProjectOrder(projects ?? [], kind, key, cur[i - 1] ?? null, cur[i + 1] ?? null);
      setDrag(null);
    };
    // Cancelled (the system took the pointer, say): back as it was, nothing saved.
    const cancel = () => {
      stop();
      setDrag(null);
    };
    dragging.current?.();
    dragging.current = stop;
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
  };
  const areaKeys = areas.map(([a]) => a).filter(Boolean);
  const open = projects?.find((p) => p.path === sel) ?? null;
  // A second click on the selected project lets it go.
  const pick = (p: ProjectRow) => nav.replace(p.path === sel ? { screen: "projects" } : { screen: "projects", path: p.path });

  return (
    <main className="main">
      <TopBar
        title="Projects"
        sub={projects ? `${counts.get("active")} active · ${counts.get("on-hold")} on hold · ${counts.get("someday")} someday` : undefined}
        ask={
          open
            ? { about: `the project ${askQuote(open.name)}`, prompt: `About ${askLink(open.path)}: ` }
            : { about: "your projects", prompt: "About my projects: " }
        }
      >
        <Seg<ProjectStatus>
          label="Show"
          value={tab}
          options={TABS.map((s) => [s, projects ? `${STATUS_LABEL[s]} ${counts.get(s) ?? 0}` : STATUS_LABEL[s]])}
          onChange={setTab}
        />
        <button
          type="button"
          className="btn pri"
          title="Make a new Project. note with a name, what done looks like and an area"
          onClick={() => setCreating(true)}
        >
          <Icon name="plus" size={14} />
          New project
        </button>
      </TopBar>
      <div className="body projects">
        <div className="plistcol">
          {tab === "active" && <FoundProjects />}
          {projects === null && <p className="faint pad">Loading…</p>}
          {projects && !shown.length && (
            <p className="faint pad">
              {tab === "active"
                ? "No active projects. A project is a note named “Project. …”; New project makes one."
                : `No ${STATUS_LABEL[tab].toLowerCase()} projects.`}
              {TABS.filter((t) => t !== tab && counts.get(t)).map((t) => (
                <button
                  key={t}
                  type="button"
                  className="blink"
                  title={`Show the ${STATUS_LABEL[t].toLowerCase()} projects`}
                  onClick={() => setTab(t)}
                >
                  {counts.get(t)} {STATUS_LABEL[t].toLowerCase()}
                </button>
              ))}
            </p>
          )}
          {!!shown.length && (
            <div className="ptable" role="table" aria-label={`${STATUS_LABEL[tab]} projects`}>
              {asDragged("areas", areas, ([a]) => a).map(([area, ps]) => (
                <div
                  key={area}
                  className={`tb-group ${drag?.kind === "areas" && drag.key === area ? "dragging" : ""}`}
                  role="rowgroup"
                  data-drag={area ? "areas" : undefined}
                  data-key={area}
                >
                  <AreaBand
                    area={area}
                    onGrip={area ? (e) => startDrag(e, "areas", "areas", area, areaKeys) : undefined}
                    n={ps.length}
                    folded={folded.includes(area)}
                    onFold={() => fold(area)}
                    paths={areaProjects(projects ?? [], area).map((p) => p.path)}
                    onRenamed={(to) => renamed(area, to)}
                  />
                  {!folded.includes(area) &&
                    asDragged("projects", ps, (p) => p.path).map((p) => {
                      const flag = projectFlag(p);
                      return (
                        <div
                          key={p.path}
                          role="row"
                          tabIndex={0}
                          className={`prow tb-row ${p.path === sel ? "sel" : ""} ${drag?.key === p.path ? "dragging" : ""}`}
                          data-drag={`projects:${area}`}
                          data-key={p.path}
                          title={`Show ${p.name}: its tasks, notes and activity`}
                          onClick={() => pick(p)}
                          onKeyDown={(e) => {
                            if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
                            e.preventDefault();
                            pick(p);
                          }}
                        >
                          {/* As Today's task rows: the name, then its counts as one grey line. */}
                          <span
                            className="pgrip"
                            title="Drag to put it in order"
                            aria-hidden="true"
                            onPointerDown={(e) =>
                              startDrag(
                                e,
                                "projects",
                                `projects:${area}`,
                                p.path,
                                ps.map((x) => x.path),
                              )
                            }
                            onClick={(e) => e.stopPropagation()}
                          >
                            <Icon name="grip" size={12} />
                          </span>
                          <span className="nm" role="cell">
                            <span className="ell">{p.name}</span>
                            <button
                              type="button"
                              className="ibtn sm pedit"
                              title={`Edit ${p.name}: open its note in Edit`}
                              aria-label={`Edit ${p.name}`}
                              onClick={(e) => {
                                e.stopPropagation();
                                editDoc(p.path);
                              }}
                            >
                              <Icon name="pencil" size={13} />
                            </button>
                          </span>
                          <span className="pmeta" role="cell">
                            {p.status === "active" && !p.next ? (
                              <span className="red" title="Stuck: it has no next action">
                                No next action
                              </span>
                            ) : (
                              <span>{p.next ? `${p.next} next` : "Nothing next"}</span>
                            )}
                            {p.waiting > 0 && <span>{p.waiting} waiting</span>}
                            {flag === "quiet" && (
                              <span className="amber" title="Quiet: nothing has happened for two weeks">
                                Quiet
                              </span>
                            )}
                            <span>Changed {ago(p.lastTouched)}</span>
                          </span>
                        </div>
                      );
                    })}
                </div>
              ))}
            </div>
          )}
        </div>
        <div className="pdetail">
          {open ? (
            <ProjectDetail p={open} all={all} />
          ) : (
            <p className="faint">{shown.length ? "Choose a project to see what moves it forward." : ""}</p>
          )}
        </div>
      </div>
      {showNew && <NewProject onClose={() => setCreating(false)} onMade={(path) => nav.replace({ screen: "projects", path })} />}
    </main>
  );
}

/** An area's band in the list: folds its projects away, and renames the area on every project in
 *  it, whatever their status (`areaProjects`). */
function AreaBand({
  area,
  n,
  folded,
  onFold,
  paths,
  onRenamed,
  onGrip,
}: {
  area: string;
  n: number;
  folded: boolean;
  onFold: () => void;
  paths: string[];
  onRenamed: (to: string) => void;
  /** Starts dragging the area into a new place; absent for No area, which stays last. */
  onGrip?: (e: React.PointerEvent) => void;
}) {
  const [renaming, setRenaming] = useState(false);
  const label = area || "No area";
  const rename = (to: string) => {
    setRenaming(false);
    if (!to || to === area) return;
    api
      .projectsSetArea(paths, area, to)
      .then((undo) => {
        onRenamed(to);
        toast(undo, undefined, "ok");
      })
      .catch(reportEditError);
  };
  return (
    <div className={`tb-band pband ${area ? "" : "none"}`} role="row">
      <span role="rowheader">
        {renaming ? (
          <input
            className="pin"
            autoFocus
            defaultValue={area}
            aria-label={`Rename the area ${area}`}
            onKeyDown={(e) => {
              if (e.key === "Enter") rename(e.currentTarget.value.trim());
              else if (e.key === "Escape") {
                e.stopPropagation();
                setRenaming(false);
              }
            }}
            onBlur={() => setRenaming(false)}
          />
        ) : (
          <>
            {onGrip && (
              <span className="pgrip" title="Drag to put the area in order" aria-hidden="true" onPointerDown={onGrip}>
                <Icon name="grip" size={12} />
              </span>
            )}
            <button
              type="button"
              className="pfold"
              aria-expanded={!folded}
              title={folded ? `Show the projects in ${label}` : `Fold ${label} away`}
              onClick={onFold}
            >
              <Icon name={folded ? "chevright" : "chevdown"} size={13} />
              <Icon name="folder" size={13} />
              <span className="ell">{label}</span>
            </button>
            {area && (
              <button
                type="button"
                className="ibtn sm pedit"
                title={`Rename ${label} on its ${paths.length === 1 ? "project" : `${paths.length} projects`}`}
                aria-label={`Rename ${label}`}
                onClick={() => setRenaming(true)}
              >
                <Icon name="pencil" size={12} />
              </button>
            )}
          </>
        )}
      </span>
      <span className="n" title={`${n} ${n === 1 ? "project" : "projects"} in this area`}>
        {fmtCount(n)}
      </span>
    </div>
  );
}

function ProjectDetail({ p, all }: { p: ProjectRow; all: ReturnType<typeof useAllTasks> }) {
  const t = useMemo(() => projectTasks(all ?? [], p.path), [all, p.path]);
  const [adding, setAdding] = useState("");
  const [editing, setEditing] = useState<"name" | "outcome" | "area" | null>(null);
  const [deleting, setDeleting] = useState(false);
  const set = (key: "status" | "area" | "outcome", value: string | null, done?: string) =>
    api
      .projectSet(p.path, key, value)
      .then(() => done && toast(done, undefined, "ok"))
      .catch(reportEditError);
  const add = () => {
    const text = adding.trim();
    if (!text) return;
    setAdding("");
    api
      .projectAddTask(p.path, prepareCapture("task", text))
      .then((r) => r.undo && toast(r.undo, undefined, "ok"))
      .catch(reportEditError);
  };
  // The name is the note's title: renamed as the file menu's Rename does, links to it rewritten.
  const rename = (to: string) => {
    const n = parseName(p.path);
    const dest = to ? renamedPath(p.path, { type: n.type ?? "", title: to, date: n.date ?? "" }) : "";
    if (!dest || dest === p.path) return;
    api
      .renamePreview(p.path, dest)
      .then((plan) => commitRename(p.path, plan))
      .catch(reportEditError);
  };
  const field = (key: "outcome" | "area", label: string, value: string | null) =>
    editing === key ? (
      <input
        className="pin"
        autoFocus
        defaultValue={value ?? ""}
        aria-label={label}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            setEditing(null);
            void set(key, e.currentTarget.value.trim() || null);
          } else if (e.key === "Escape") {
            e.stopPropagation();
            setEditing(null);
          }
        }}
        onBlur={() => setEditing(null)}
      />
    ) : (
      <button type="button" className="pval" onClick={() => setEditing(key)} title={`Change the ${label.toLowerCase()}`}>
        {value || <span className="faint">Add {label.toLowerCase()}…</span>}
        <Icon name="pencil" size={12} className="pedit" />
      </button>
    );

  return (
    <div className="pgrid">
      <div className="pmain">
        <div className="phead">
          <div className="row">
            <span className={`chip ${p.status === "active" ? "green" : ""}`}>{STATUS_LABEL[p.status]}</span>
            <span className="faint small">{p.path}</span>
          </div>
          {editing === "name" ? (
            <input
              className="h1 pname-in"
              autoFocus
              defaultValue={p.name}
              aria-label="Project name"
              onFocus={(e) => e.currentTarget.select()}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  setEditing(null);
                  rename(e.currentTarget.value.trim());
                } else if (e.key === "Escape") {
                  e.stopPropagation();
                  setEditing(null);
                }
              }}
              onBlur={() => setEditing(null)}
            />
          ) : (
            <h1 className="h1">
              <button
                type="button"
                className="pname"
                onClick={() => setEditing("name")}
                title="Rename the project; links to its note follow"
              >
                {p.name}
                <Icon name="pencil" size={16} className="pedit" />
              </button>
            </h1>
          )}
          <div className="pkv">
            <span className="faint">Done looks like</span>
            {field("outcome", "Outcome", p.outcome)}
            <span className="faint">Area</span>
            {field("area", "Area", p.area)}
          </div>
        </div>
        <section>
          <div className="ghead">
            <h2 className="h3">Next actions</h2>
            <span className="faint">{t.next.length}</span>
          </div>
          <div className="card tcard">
            <TaskList rows={t.next} manual keys hideProject showNote={false} empty="Nothing to do next: what's the next physical step?" />
            <div className="padd">
              <Icon name="plus" size={14} />
              <input
                value={adding}
                onChange={(e) => setAdding(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && add()}
                placeholder="Add a next action — @context, effort:15m, due:fri"
                aria-label="Add a next action"
              />
            </div>
          </div>
        </section>
        {!!t.waiting.length && (
          <section>
            <div className="ghead">
              <h2 className="h3">Waiting for</h2>
              <span className="faint">{t.waiting.length}</span>
            </div>
            <div className="card tcard">
              <TaskList rows={t.waiting} manual={false} keys hideProject showNote={false} rowAction={(w) => <DraftNudge t={w} />} />
            </div>
          </section>
        )}
        {!!t.someday.length && (
          <section>
            <div className="ghead">
              <h2 className="h3">Someday / maybe</h2>
              <span className="faint">{t.someday.length}</span>
            </div>
            <div className="card tcard">
              <TaskList rows={t.someday} manual={false} keys hideProject showNote={false} />
            </div>
          </section>
        )}
        <div className="row pacts">
          {p.status !== "done" && (
            <button
              type="button"
              className="btn"
              title="Mark the project done; it moves to the Completed tab"
              onClick={() => void set("status", "done", `${p.name} completed`)}
            >
              <Icon name="check" size={14} />
              Mark complete
            </button>
          )}
          {p.status !== "active" && (
            <button
              type="button"
              className="btn"
              title="Move the project back to the active list"
              onClick={() => void set("status", "active", `${p.name} is active`)}
            >
              <Icon name="refresh" size={14} />
              Make active
            </button>
          )}
          <button
            type="button"
            className="btn ghost"
            title="Open the project note in Edit, to change any of it"
            onClick={() => editDoc(p.path)}
          >
            <Icon name="pencil" size={14} />
            Edit
          </button>
          {/* The project's other moves under More, so it has one main action. */}
          <MoreMenu
            title="Put on hold, Someday, Ask about this project, Move to the Trash"
            items={[
              ...(p.status === "active"
                ? [
                    {
                      label: "Put on hold",
                      icon: "waiting",
                      title: "Pause the project; it moves to the On hold tab",
                      run: () => void set("status", "on-hold", `${p.name} on hold`),
                    },
                  ]
                : []),
              ...(p.status !== "someday"
                ? [
                    {
                      label: "Someday",
                      icon: "someday",
                      title: "Park the project for some day; it moves to the Someday tab",
                      run: () => void set("status", "someday", `${p.name} moved to someday`),
                    },
                  ]
                : []),
              {
                label: "Ask about this project",
                icon: "ask",
                title: "Open Ask with the project note attached, to ask the model about it",
                run: () => askAboutNote(p.path),
              },
              {
                label: "Move to the Trash",
                icon: "trash",
                title: "Move the project note to the Trash, after asking first",
                run: () => setDeleting(true),
                danger: true,
              },
            ]}
          />
        </div>
      </div>
      <aside className="pside">
        <div className="card side-card">
          <div className="eyebrow">The project note</div>
          <button type="button" className="blink" title={`Open ${p.path}`} onClick={() => openDoc(p.path)}>
            <span className="lb note">{p.name}</span>
            <span className="faint">Changed {ago(p.lastTouched)}</span>
          </button>
        </div>
        <div className="card side-card">
          <div className="eyebrow">Notes that link it</div>
          {!p.links.length && <p className="faint small">Any note that links [[Project. {p.name}]] shows here.</p>}
          {p.links.slice(0, 12).map((l) => (
            <button key={l} type="button" className="blink" title={`Open ${l}`} onClick={() => openDoc(l)}>
              <span className="lb note">{l.replace(/\.md$/, "").split("/").pop()}</span>
            </button>
          ))}
        </div>
        <ProjectWiki path={p.path} />
        <RecentActivity links={p.links} done={t.done} />
        {!!t.done.length && (
          <div className="card side-card">
            <div className="eyebrow">Done</div>
            {t.done.slice(0, 8).map((d) => (
              <div key={`${d.path}:${d.line}`} className={`pdone ${d.status === "-" ? "cancelled" : ""}`}>
                {d.status === "-" ? (
                  <Icon name="cancelled" size={12} style={{ color: "var(--ink3)" }} />
                ) : (
                  <Icon name="check" size={12} style={{ color: "var(--green)" }} />
                )}
                <span className="ell">{doneText(d)}</span>
                {d.status === "-" && <span className="faint small">Cancelled</span>}
                <GtdChips t={d} project={false} />
              </div>
            ))}
          </div>
        )}
      </aside>
      {deleting && <DeleteProject p={p} tasks={t} onClose={() => setDeleting(false)} />}
    </div>
  );
}

/** A finished task's words, without its ✅ or ❌ date and what follows. */
const doneText = (d: TaskRow) => d.text.replace(/\s*[✅❌].*$/u, "");

/** The wiki pages the project note links to. */
function ProjectWiki({ path }: { path: string }) {
  const [pages, setPages] = useState<string[]>([]);
  const v = useVaultVersion();
  useEffect(() => {
    let live = true;
    void api
      .docRead(path)
      .then(
        (d) => live && setPages([...new Set(d.meta.links.map((l) => l.resolved).filter((r): r is string => !!r && r.startsWith("wiki/")))]),
      )
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [path, v]);
  if (!pages.length) return null;
  return (
    <div className="card side-card">
      <div className="eyebrow">Wiki pages</div>
      {pages.slice(0, 10).map((w) => (
        <button key={w} type="button" className="blink" title={`Open ${w}`} onClick={() => openDoc(w)}>
          <span className="lb wiki">{w.replace(/\.md$/, "").split("/").pop()}</span>
        </button>
      ))}
    </div>
  );
}

/** What happened lately: notes linking the project that changed, and tasks done, newest first. */
function RecentActivity({ links, done }: { links: string[]; done: TaskRow[] }) {
  const [mtimes, setMtimes] = useState<Map<string, number>>(new Map());
  const v = useVaultVersion();
  useEffect(() => {
    let live = true;
    void api
      .filesList(null)
      .then((f) => live && setMtimes(new Map(f.map((x) => [x.path, x.mtime]))))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [v]);
  const items = [
    ...links
      .filter((l) => mtimes.has(l))
      .map((l) => ({ at: mtimes.get(l)!, key: l, text: l.replace(/\.md$/, "").split("/").pop()!, path: l, kind: "note" })),
    ...done
      .filter((d) => d.doneOn)
      .map((d) => ({
        at: Date.parse(`${d.doneOn}T12:00:00`),
        key: `${d.path}:${d.line}`,
        text: doneText(d),
        path: d.path,
        kind: d.status === "-" ? "cancelled" : "done",
      })),
  ]
    .sort((a, b) => b.at - a.at)
    .slice(0, 6);
  if (!items.length) return null;
  return (
    <div className="card side-card">
      <div className="eyebrow">Recent activity</div>
      {items.map((i) => (
        <button key={i.key} type="button" className="blink" title={`Open ${i.path}`} onClick={() => openDoc(i.path)}>
          <span className={i.kind === "note" ? "lb note" : "pdone ell"}>
            {i.kind === "done" && <Icon name="check" size={12} style={{ color: "var(--green)" }} />}
            {i.kind === "cancelled" && <Icon name="cancelled" size={12} style={{ color: "var(--ink3)" }} />} {iconizeEmoji(i.text)}
          </span>
          <span className="faint">
            {i.kind === "done" ? "Done" : i.kind === "cancelled" ? "Cancelled" : "Changed"} {ago(i.at)}
          </span>
        </button>
      ))}
    </div>
  );
}

/** Delete project: asks first, then moves the project's note to the Trash (Undo, or restore from Trash). */
function DeleteProject({ p, tasks, onClose }: { p: ProjectRow; tasks: ReturnType<typeof projectTasks>; onClose: () => void }) {
  const all = [...tasks.next, ...tasks.waiting, ...tasks.someday, ...tasks.done];
  const inNote = all.filter((t) => t.path === p.path).length;
  const elsewhere = all.length - inNote;
  const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;
  const del = () => {
    onClose();
    void trashFile(p.path).then(() => nav.replace({ screen: "projects" }));
  };
  return (
    <Dialog onClose={onClose} width={460} label="Move the project to the Trash">
      <div className="confirm">
        <h2 className="h2">Move “{p.name}” to the Trash?</h2>
        <p className="muted">
          Moves its note, {p.path}, to the Trash
          {inNote ? `, with the ${plural(inNote, "task")} written in it` : ""}. You can undo it or restore it from the Trash.
        </p>
        {elsewhere > 0 && (
          <p className="muted">
            {plural(elsewhere, "task")} in other notes link it; they stay where they are, and their link shows as missing until the project
            comes back.
          </p>
        )}
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" autoFocus title="Keep the project" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="btn lg danger-pri" title={`Move ${p.path} to the Trash; Undo brings it back`} onClick={del}>
            Move to the Trash
          </button>
        </div>
      </div>
    </Dialog>
  );
}

/** New project: a name, its outcome and area. Writes the Project. note. */
export function NewProject({ onClose, onMade, name: start = "" }: { onClose: () => void; onMade?: (path: string) => void; name?: string }) {
  const projects = useProjects();
  const [name, setName] = useState(start);
  const [outcome, setOutcome] = useState("");
  const [area, setArea] = useState("");
  const areas = [...new Set((projects ?? []).map((p) => p.area).filter(Boolean))] as string[];
  const make = () => {
    if (!name.trim()) return;
    api
      .projectCreate(name.trim(), "active", area.trim() || null, outcome.trim() || null)
      .then(({ path, undo }) => {
        toast(undo || `Made ${path.replace(/\.md$/, "")}`, { label: "Undo", kbd: "⌘Z", run: () => void undoLast() }, "ok");
        onMade?.(path);
        onClose();
      })
      .catch(reportEditError);
  };
  return (
    <Dialog onClose={onClose} width={480} label="New project">
      <div className="confirm">
        <h2 className="h2">New project</h2>
        <label className="nnf">
          <span className="faint">Name</span>
          <span className="inp">
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && make()}
              placeholder="Orbit App launch"
            />
          </span>
        </label>
        <label className="nnf">
          <span className="faint">Done looks like</span>
          <span className="inp">
            <input
              value={outcome}
              onChange={(e) => setOutcome(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && make()}
              placeholder="What's true when it's finished"
            />
          </span>
        </label>
        <label className="nnf">
          <span className="faint">Area</span>
          <span className="inp">
            <input
              list="project-areas"
              value={area}
              onChange={(e) => setArea(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && make()}
              placeholder="Work, Personal…"
            />
          </span>
          <datalist id="project-areas">
            {areas.map((a) => (
              <option key={a} value={a} />
            ))}
          </datalist>
        </label>
        <p className="faint small">Makes “Project. {name.trim() || "…"}.md” with Next actions, Waiting for and Notes headings.</p>
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" title="Close without making a project" onClick={onClose}>
            Cancel
          </button>
          <span title={name.trim() ? `Make “Project. ${name.trim()}.md”` : "Give the project a name first"}>
            <button type="button" className="btn lg pri" disabled={!name.trim()} onClick={make}>
              Make project
            </button>
          </span>
        </div>
      </div>
    </Dialog>
  );
}
