// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Graph (§4: neighbourhood view): a note at the centre with its links and backlinks to depth 1–3,
// or the whole wiki. Drawn on a canvas with d3-force; hover dims to a node's neighbours, click
// centres on it, double-click opens it. The data is the index's links table (core/src/graph.rs).

import { forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, Simulation, SimulationNodeDatum } from "d3-force";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, FileSummary, Graph, GraphNode } from "./api";
import { Icon } from "./icons";
import { nav, openDoc, place, useViewState } from "./nav";
import { useVaultVersion } from "./state";
import { useStore } from "./store";
import { askLink, askQuote, TopBar } from "./TopBar";
import { fmtCount, LayerTag, Popover, SearchBox, Seg } from "./ui";
import { TypeDot } from "./typeTone";
import { KIND_COLOUR, KINDS, Kind, kindLabel, kindOf, kindsIn, labelled, neighbours, pick, radius, visible } from "./graphModel";

type Mode = "around" | "wiki";

type SimNode = SimulationNodeDatum & GraphNode & { r: number };

const isGhost = (id: string) => id.startsWith("ghost:");

/** Centres the graph on a file. */
export const openGraph = (path: string) => nav.go({ screen: "graph", path });

export function GraphScreen() {
  const here = useStore(place).place;
  const version = useVaultVersion();
  const [lastCenter, setLastCenter] = useViewState<string | null>("graph.center", null);
  const [savedMode, setMode] = useViewState<Mode>("graph.mode", "around");
  const [depth, setDepth] = useViewState("graph.depth", 2);
  const [hidden, setHidden] = useViewState<Kind[]>("graph.hidden", []);
  const center = here.path ?? lastCenter;
  // With no page to centre on yet, the whole wiki.
  const mode: Mode = here.path ? "around" : savedMode === "around" && center ? "around" : "wiki";
  const around = mode === "around" && !!center;

  const [g, setG] = useState<Graph | null>(null);
  const [error, setError] = useState("");
  const [picked, setPicked] = useState<string | null>(null);
  const [fit, setFit] = useState(0);

  useEffect(() => {
    if (here.path && here.path !== lastCenter) setLastCenter(here.path);
  }, [here.path]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    let live = true;
    api
      .graph(around ? center : null, depth)
      .then((x) => {
        if (!live) return;
        setG(x);
        setError("");
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [mode, center, depth, version]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => setPicked(null), [center, mode]);

  const shown = useMemo(() => (g ? visible(g, hidden, around ? center : null) : null), [g, hidden, around, center]);
  const kinds = useMemo(() => (g ? kindsIn(g) : []), [g]);

  const centreOn = (id: string) => {
    if (isGhost(id)) return setPicked(id);
    nav.go({ screen: "graph", path: id });
  };
  const switchMode = (m: Mode) => {
    if (m === "wiki") {
      nav.go({ screen: "graph" });
      setMode("wiki");
    } else {
      if (center) nav.go({ screen: "graph", path: center });
      setMode("around");
    }
  };

  const centreNode = g?.nodes.find((n) => n.id === center) ?? null;
  // What the Ask button is about: the page picked, else the one at the centre.
  const gfocus = (picked && g?.nodes.find((n) => n.id === picked)) || (around ? centreNode : null);
  const sub = around
    ? `around ${centreNode?.title ?? center?.split("/").pop()?.replace(/\.md$/, "")}`
    : mode === "wiki"
      ? "whole wiki"
      : undefined;

  return (
    <main className="main">
      <TopBar
        title="Graph"
        sub={sub}
        ask={
          gfocus
            ? {
                about: `${askQuote(gfocus.title)} and what links to it`,
                prompt: `About ${askLink(gfocus.id)} and how it links to other pages: `,
              }
            : { about: "how the wiki links together", prompt: "About how my wiki pages link together: " }
        }
      >
        <CentrePicker onPick={centreOn} />
        <Seg<Mode>
          label="Show"
          value={mode}
          onChange={switchMode}
          options={[
            ["around", "Neighbourhood", "The page at the centre and what links to and from it"],
            ["wiki", "Whole wiki", "Every wiki page and how they link"],
          ]}
        />
        <button type="button" className="btn sm" title="Fit the graph to the window" onClick={() => setFit((n) => n + 1)} disabled={!shown}>
          <Icon name="fit" size={14} />
          Fit
        </button>
      </TopBar>
      <div className="body graph">
        <div className="gstage">
          {shown && shown.nodes.length > 0 ? (
            <GraphCanvas
              g={shown}
              center={around ? center : null}
              fit={fit}
              onCentre={centreOn}
              onOpen={(id) => !isGhost(id) && openDoc(id)}
            />
          ) : (
            <div className="gempty faint">{error || (g ? "Nothing to show." : "Loading…")}</div>
          )}
          <div className="gtools">
            {around && (
              <div className="card gdepth">
                <label htmlFor="gdepth">Depth</label>
                <input id="gdepth" type="range" min={1} max={3} value={depth} onChange={(e) => setDepth(Number(e.target.value))} />
                <b>{depth}</b>
              </div>
            )}
            {kinds.length > 1 &&
              KINDS.filter(([k]) => kinds.includes(k)).map(([k, label]) => {
                const on = !hidden.includes(k);
                return (
                  <button
                    key={k}
                    type="button"
                    className={`chip f ${on ? "on" : ""}`}
                    aria-pressed={on}
                    title={on ? `Hide ${label.toLowerCase()}` : `Show ${label.toLowerCase()}`}
                    onClick={() => setHidden(on ? [...hidden, k] : hidden.filter((x) => x !== k))}
                  >
                    <i className={`gdot ${k}`} style={{ background: k === "ghost" ? "transparent" : `var(${KIND_COLOUR[k]})` }} />
                    {label}
                  </button>
                );
              })}
          </div>
          {shown && (
            <div className="gfoot faint">
              {fmtCount(shown.nodes.length)} {shown.nodes.length === 1 ? "node" : "nodes"} · {fmtCount(shown.edges.length)} links
              {g?.truncated && " · the nearest 400"} · click to centre, double-click to open
            </div>
          )}
        </div>
        <GraphPane g={shown} around={around} center={center} picked={picked} onCentre={centreOn} />
      </div>
    </main>
  );
}

/** The side pane: the centre (or a picked ghost) and what it links to, or the whole wiki's busiest pages. */
function GraphPane({
  g,
  around,
  center,
  picked,
  onCentre,
}: {
  g: Graph | null;
  around: boolean;
  center: string | null;
  picked: string | null;
  onCentre: (id: string) => void;
}) {
  const nb = useMemo(() => (g ? neighbours(g) : new Map<string, Set<string>>()), [g]);
  if (!g) return <aside className="gpane" />;
  const byId = new Map(g.nodes.map((n) => [n.id, n]));
  const focus = (picked && byId.get(picked)) || (around && center ? byId.get(center) : undefined);
  const row = (n: GraphNode) => (
    <button
      key={n.id}
      type="button"
      className={`glink ${n.layer === "ghost" ? "ghost" : ""}`}
      onClick={() => onCentre(n.id)}
      title={n.layer === "ghost" ? "Not written yet" : n.id}
    >
      <i className={`gdot ${kindOf(n)}`} style={{ background: n.layer === "ghost" ? "transparent" : `var(${KIND_COLOUR[kindOf(n)]})` }} />
      <span>{n.title}</span>
      {n.layer === "ghost" && <em>not written yet</em>}
    </button>
  );
  if (!focus) {
    const pages = g.nodes.filter((n) => n.layer !== "ghost");
    const ghosts = g.nodes.length - pages.length;
    const busiest = [...pages].sort((a, b) => b.degree - a.degree).slice(0, 15);
    return (
      <aside className="gpane">
        <div>
          <span className="chip">Whole wiki</span>
          <h2 className="h2">
            {fmtCount(pages.length)} {pages.length === 1 ? "page" : "pages"}
          </h2>
          <div className="faint">
            {fmtCount(g.edges.length)} links{ghosts > 0 && ` · ${fmtCount(ghosts)} not written yet`}
          </div>
        </div>
        <div>
          <div className="eyebrow">Most linked</div>
          {busiest.map(row)}
        </div>
      </aside>
    );
  }
  const linked = [...(nb.get(focus.id) ?? [])]
    .map((id) => byId.get(id))
    .filter((n): n is GraphNode => !!n)
    .sort((a, b) => b.degree - a.degree || a.title.localeCompare(b.title));
  const ghost = focus.layer === "ghost";
  return (
    <aside className="gpane">
      <div>
        <span className="chip">
          <TypeDot colour={ghost ? "transparent" : `var(${KIND_COLOUR[kindOf(focus)]})`} />
          {kindLabel(focus)}
        </span>
        <h2 className="h2">{focus.title}</h2>
        <div className="faint">
          {fmtCount(focus.degree)} {focus.degree === 1 ? "link" : "links"} in the vault{!ghost && ` · ${focus.id}`}
        </div>
      </div>
      {!ghost && (
        <div className="row">
          <button type="button" className="btn pri sm" title="Open this page" onClick={() => openDoc(focus.id)}>
            <Icon name="chevright" size={12} />
            Open
          </button>
        </div>
      )}
      <div>
        <div className="eyebrow">Linked · {fmtCount(linked.length)}</div>
        {linked.length ? linked.map(row) : <p className="faint">No links to or from it.</p>}
      </div>
    </aside>
  );
}

/** A search over the vault's titles; picking one centres the graph on it. */
function CentrePicker({ onPick }: { onPick: (path: string) => void }) {
  const [q, setQ] = useState("");
  const [files, setFiles] = useState<FileSummary[] | null>(null);
  const [rect, setRect] = useState<DOMRect | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const version = useVaultVersion();
  useEffect(() => setFiles(null), [version]);
  useEffect(() => {
    if (q && !files) void api.filesList(null).then((f) => setFiles(f.filter((x) => x.layer !== "template")));
  }, [q, files]);
  const hits = useMemo(() => pick(files ?? [], q), [files, q]);
  const choose = (p: string) => {
    setQ("");
    setRect(null);
    onPick(p);
  };
  return (
    <div ref={box} className="gpick">
      <SearchBox
        value={q}
        onChange={(v) => {
          setQ(v);
          setRect(v ? (box.current?.getBoundingClientRect() ?? null) : null);
        }}
        placeholder="Find a page to centre on"
        onKeyDown={(e) => {
          if (e.key === "Enter" && hits[0]) choose(hits[0].path);
          if (e.key === "Escape") setQ("");
        }}
      />
      {rect && q && hits.length > 0 && (
        <Popover anchor={rect} onClose={() => setRect(null)} width={Math.max(260, rect.width)}>
          <div className="menu">
            {hits.map((f) => (
              <button key={f.path} type="button" title={`Centre the graph on ${f.title}`} onClick={() => choose(f.path)}>
                <LayerTag layer={f.layer} />
                <span className="gpick-t">{f.title}</span>
              </button>
            ))}
          </div>
        </Popover>
      )}
    </div>
  );
}

interface View {
  k: number;
  x: number;
  y: number;
}

function cssVar(name: string) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
}

/** The force-directed drawing. Positions carry over by id when the graph changes, so recentring moves rather than redraws. */
function GraphCanvas({
  g,
  center,
  fit,
  onCentre,
  onOpen,
}: {
  g: Graph;
  center: string | null;
  fit: number;
  onCentre: (id: string) => void;
  onOpen: (id: string) => void;
}) {
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const sim = useRef<Simulation<SimNode, undefined> | null>(null);
  const nodes = useRef<SimNode[]>([]);
  const links = useRef<{ source: SimNode; target: SimNode }[]>([]);
  const view = useRef<View>({ k: 1, x: 0, y: 0 });
  const hover = useRef<string | null>(null);
  const frame = useRef(0);
  const size = useRef({ w: 0, h: 0 });
  const cb = useRef({ onCentre, onOpen });

  const nb = useMemo(() => neighbours(g), [g]);
  const always = useMemo(() => labelled(g, center), [g, center]);

  // Callbacks kept outside the render (a tick or a resize calls the latest paint).
  const paintRef = useRef<() => void>(() => {});
  const draw = () => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => paintRef.current());
  };

  const paint = () => {
    const c = canvas.current;
    if (!c) return;
    const ctx = c.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const { w, h } = size.current;
    const { k, x, y } = view.current;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.translate(x, y);
    ctx.scale(k, k);
    const colour: Record<string, string> = {};
    for (const [kind] of KINDS) colour[kind] = cssVar(KIND_COLOUR[kind]);
    const line = cssVar("--line2");
    const ink = cssVar("--ink");
    const ink3 = cssVar("--ink3");
    const ground = cssVar("--ground");
    const accentSoft = cssVar("--accent-soft");
    const hv = hover.current;
    const lit = hv ? new Set([hv, ...(nb.get(hv) ?? [])]) : null;

    ctx.lineWidth = 1 / Math.max(k, 0.6);
    for (const l of links.current) {
      const on = !lit || (lit.has(l.source.id) && lit.has(l.target.id) && (l.source.id === hv || l.target.id === hv));
      ctx.globalAlpha = on ? (lit ? 0.9 : 0.6) : 0.12;
      ctx.strokeStyle = on && lit ? ink3 : line;
      ctx.setLineDash(isGhost(l.source.id) || isGhost(l.target.id) ? [3, 3] : []);
      ctx.beginPath();
      ctx.moveTo(l.source.x!, l.source.y!);
      ctx.lineTo(l.target.x!, l.target.y!);
      ctx.stroke();
    }
    ctx.setLineDash([]);

    for (const n of nodes.current) {
      const on = !lit || lit.has(n.id);
      ctx.globalAlpha = on ? 1 : 0.18;
      const kind = kindOf(n);
      ctx.beginPath();
      ctx.arc(n.x!, n.y!, n.r, 0, Math.PI * 2);
      if (n.id === center) {
        ctx.fillStyle = accentSoft;
        ctx.beginPath();
        ctx.arc(n.x!, n.y!, n.r + 6, 0, Math.PI * 2);
        ctx.fill();
        ctx.beginPath();
        ctx.arc(n.x!, n.y!, n.r, 0, Math.PI * 2);
        ctx.fillStyle = colour[kind];
        ctx.fill();
      } else if (kind === "ghost") {
        ctx.fillStyle = ground;
        ctx.fill();
        ctx.setLineDash([2, 2]);
        ctx.strokeStyle = ink3;
        ctx.lineWidth = 1.5;
        ctx.stroke();
        ctx.setLineDash([]);
      } else {
        ctx.fillStyle = colour[kind];
        ctx.fill();
      }
    }

    // Labels: in screen pixels so they stay readable at any zoom. Each sits in a small box up and
    // to the right of its bubble, a short line from the bubble's edge to the box, so it's plain
    // which bubble a label names.
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.textBaseline = "middle";
    const surface = cssVar("--surface");
    const ink2 = cssVar("--ink2");
    for (const n of nodes.current) {
      const show = n.id === hv || always.has(n.id);
      if (!show) continue;
      const cx = n.x! * k + x;
      const cy = n.y! * k + y;
      const rr = n.r * k;
      const big = n.id === center;
      ctx.font = `${big ? 600 : 500} ${big ? 12.5 : 11}px "Geist Variable", -apple-system, system-ui, sans-serif`;
      const label = n.title.length > 42 ? `${n.title.slice(0, 40)}…` : n.title;
      // From the edge at the upper right, out along the diagonal, then the box.
      const ex = cx + rr * 0.71;
      const ey = cy - rr * 0.71;
      const lx = ex + 10;
      const ly = ey - 10;
      const w = ctx.measureText(label).width + 12;
      const h = big ? 20 : 18;
      ctx.globalAlpha = 1;
      // Lines and outlines in the secondary ink, which shows on either ground (the line colour
      // was lost in dark mode); the centre and the hovered one stronger.
      ctx.strokeStyle = n.id === hv || big ? ink2 : ink3;
      ctx.lineWidth = n.id === hv || big ? 1.5 : 1.25;
      ctx.beginPath();
      ctx.moveTo(ex, ey);
      ctx.lineTo(lx, ly);
      ctx.stroke();
      ctx.fillStyle = ctx.strokeStyle;
      ctx.beginPath();
      ctx.arc(ex, ey, 2, 0, Math.PI * 2);
      ctx.fill();
      ctx.beginPath();
      ctx.roundRect(lx, ly - h / 2, w, h, 6);
      ctx.fillStyle = surface;
      ctx.fill();
      ctx.strokeStyle = n.id === hv || big ? ink2 : ink3;
      ctx.stroke();
      ctx.fillStyle = isGhost(n.id) ? ink3 : ink;
      ctx.fillText(label, lx + 6, ly + 0.5);
    }
    ctx.globalAlpha = 1;
  };

  useEffect(() => {
    paintRef.current = paint;
    cb.current = { onCentre, onOpen };
    draw();
  });

  /** Whether you've panned or zoomed: until then the graph keeps fitting the window, as the layout
   *  settles and the window changes size. Reset puts it back. */
  const moved = useRef(false);

  /** The whole graph, labels included, as large as fits, centred, with a margin all round (more at
   *  the top for the tools and at the foot for the counts). */
  const zoomToFit = () => {
    const ns = nodes.current;
    const { w, h } = size.current;
    if (!ns.length || !w) return;
    const pad = { l: 32, r: 32, t: 64, b: 44 };
    const aw = Math.max(40, w - pad.l - pad.r);
    const ah = Math.max(40, h - pad.t - pad.b);
    const ctx = canvas.current?.getContext("2d");
    // The labels' boxes, in screen pixels, sit up and to the right of their bubbles.
    const labelW = new Map<string, number>();
    for (const n of ns) {
      if (!always.has(n.id) || !ctx) continue;
      const big = n.id === center;
      ctx.font = `${big ? 600 : 500} ${big ? 12.5 : 11}px "Geist Variable", -apple-system, system-ui, sans-serif`;
      const label = n.title.length > 42 ? `${n.title.slice(0, 40)}…` : n.title;
      labelW.set(n.id, ctx.measureText(label).width + 12);
    }
    const box = (k: number) => {
      let x0 = Infinity,
        y0 = Infinity,
        x1 = -Infinity,
        y1 = -Infinity;
      for (const n of ns) {
        const cx = n.x! * k;
        const cy = n.y! * k;
        const rr = n.r * k;
        x0 = Math.min(x0, cx - rr);
        y0 = Math.min(y0, cy - rr);
        x1 = Math.max(x1, cx + rr);
        y1 = Math.max(y1, cy + rr);
        const lw = labelW.get(n.id);
        if (lw) {
          x1 = Math.max(x1, cx + rr * 0.71 + 10 + lw);
          y0 = Math.min(y0, cy - rr * 0.71 - 10 - 10);
        }
      }
      return { x0, y0, x1, y1 };
    };
    // Labels don't grow with the zoom, so find the scale in a few steps.
    let k = 1;
    for (let i = 0; i < 4; i++) {
      const b = box(k);
      k *= Math.min(aw / (b.x1 - b.x0 || 1), ah / (b.y1 - b.y0 || 1));
      k = Math.min(2.5, Math.max(0.1, k));
    }
    const b = box(k);
    view.current = {
      k,
      x: pad.l + aw / 2 - (b.x0 + b.x1) / 2,
      y: pad.t + ah / 2 - (b.y0 + b.y1) / 2,
    };
    draw();
  };

  // The simulation, rebuilt when the graph changes; nodes already placed keep their place.
  useEffect(() => {
    const old = new Map(nodes.current.map((n) => [n.id, n]));
    const centreOld = center ? old.get(center) : undefined;
    const ns: SimNode[] = g.nodes.map((n, i) => {
      const was = old.get(n.id);
      const a = i * 2.39996; // a sunflower spiral for the newcomers
      const d = 30 * Math.sqrt(i + 1);
      return {
        ...n,
        r: radius(n.degree, n.id === center),
        x: was?.x != null ? was.x - (centreOld?.x ?? 0) : Math.cos(a) * d,
        y: was?.y != null ? was.y - (centreOld?.y ?? 0) : Math.sin(a) * d,
      };
    });
    const byId = new Map(ns.map((n) => [n.id, n]));
    const c = center ? byId.get(center) : undefined;
    if (c) {
      c.fx = 0;
      c.fy = 0;
    }
    const ls = g.edges.flatMap(([a, b]) => {
      const s = byId.get(a);
      const t = byId.get(b);
      return s && t ? [{ source: s, target: t }] : [];
    });
    nodes.current = ns;
    links.current = ls;
    const linked = new Set(ls.flatMap((l) => [l.source.id, l.target.id]));
    const big = ns.length > 150;
    sim.current?.stop();
    const s = forceSimulation<SimNode>(ns)
      .force(
        "link",
        forceLink<SimNode, { source: SimNode; target: SimNode }>(ls)
          .distance((l) => (center ? 40 + 22 * Math.max(l.source.depth, l.target.depth) : 36))
          .strength(big ? 0.25 : 0.5),
      )
      .force("charge", forceManyBody<SimNode>().strength(big ? -45 : -220))
      .force("collide", forceCollide<SimNode>((n) => n.r + 3).iterations(big ? 3 : 1))
      // A page with no links here is pulled in harder, or it drifts off and the fit has to shrink
      // everything else to keep it in view.
      .force(
        "x",
        forceX<SimNode>(0).strength((n) => (linked.has(n.id) ? (center ? 0.02 : 0.06) : 0.3)),
      )
      .force(
        "y",
        forceY<SimNode>(0).strength((n) => (linked.has(n.id) ? (center ? 0.02 : 0.06) : 0.3)),
      )
      .alpha(old.size ? 0.6 : 1)
      .stop();
    // A new graph's layout is mostly worked out before the first frame (within a time budget), so
    // only the last of the settling is seen; then it cools in about a second rather than d3's five.
    if (!old.size) {
      // Big graphs need most of their settling done here, or they open bunched up with bubbles on
      // top of each other, and only look right once something makes them settle again.
      const t0 = performance.now();
      const until = big ? 0.02 : 0.1;
      const budget = big ? 900 : 300;
      while (s.alpha() > until && performance.now() - t0 < budget) s.tick();
    }
    // A new graph fits the window again; while it settles, each step keeps it fitted, until you
    // pan or zoom.
    moved.current = false;
    s.alphaDecay(0.08)
      .on("tick", () => (moved.current ? draw() : zoomToFit()))
      .restart();
    const early = window.setTimeout(zoomToFit, 0);
    sim.current = s;
    return () => {
      window.clearTimeout(early);
      s.stop();
    };
  }, [g, center]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!fit) return;
    moved.current = false;
    zoomToFit();
  }, [fit]); // eslint-disable-line react-hooks/exhaustive-deps

  // The canvas follows its box, at the screen's pixel density.
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const c = canvas.current;
      if (!c) return;
      const { width: w, height: h } = el.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      c.width = Math.round(w * dpr);
      c.height = Math.round(h * dpr);
      c.style.width = `${w}px`;
      c.style.height = `${h}px`;
      size.current = { w, h };
      if (moved.current) draw();
      else zoomToFit();
    });
    ro.observe(el);
    // WebKit sends a trackpad pinch as gesture events.
    let start = 1;
    const gs = (e: Event) => {
      e.preventDefault();
      start = view.current.k;
    };
    const gc = (e: Event) => {
      e.preventDefault();
      const ge = e as Event & { scale: number; clientX: number; clientY: number };
      const r = el.getBoundingClientRect();
      const px = ge.clientX - r.left;
      const py = ge.clientY - r.top;
      const { k, x, y } = view.current;
      const nk = Math.min(4, Math.max(0.1, start * ge.scale));
      moved.current = true;
      view.current = { k: nk, x: px - ((px - x) / k) * nk, y: py - ((py - y) / k) * nk };
      draw();
    };
    el.addEventListener("gesturestart", gs);
    el.addEventListener("gesturechange", gc);
    // A theme change repaints with the new colours.
    const mo = new MutationObserver(draw);
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", draw);
    return () => {
      ro.disconnect();
      el.removeEventListener("gesturestart", gs);
      el.removeEventListener("gesturechange", gc);
      mo.disconnect();
      mq.removeEventListener("change", draw);
      cancelAnimationFrame(frame.current);
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Pointer: drag the background to pan, drag a node to move it, wheel or pinch to zoom.
  const toGraph = (e: { clientX: number; clientY: number }) => {
    const r = canvas.current!.getBoundingClientRect();
    const { k, x, y } = view.current;
    return { gx: (e.clientX - r.left - x) / k, gy: (e.clientY - r.top - y) / k };
  };
  const hit = (e: { clientX: number; clientY: number }): SimNode | null => {
    const { gx, gy } = toGraph(e);
    const slack = 4 / view.current.k;
    let best: SimNode | null = null;
    let bestD = Infinity;
    for (const n of nodes.current) {
      const d = Math.hypot(n.x! - gx, n.y! - gy);
      if (d <= n.r + slack && d < bestD) {
        best = n;
        bestD = d;
      }
    }
    return best;
  };

  const drag = useRef<{ node: SimNode | null; sx: number; sy: number; vx: number; vy: number; moved: boolean } | null>(null);
  const clickTimer = useRef(0);

  const onPointerDown = (e: React.PointerEvent) => {
    const node = hit(e);
    drag.current = { node, sx: e.clientX, sy: e.clientY, vx: view.current.x, vy: view.current.y, moved: false };
    (e.target as Element).setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) {
      const n = hit(e);
      const id = n?.id ?? null;
      if (id !== hover.current) {
        hover.current = id;
        canvas.current!.style.cursor = n ? "pointer" : "grab";
        draw();
      }
      return;
    }
    if (!d.moved && Math.hypot(e.clientX - d.sx, e.clientY - d.sy) < 4) return;
    d.moved = true;
    if (d.node) {
      moved.current = true;
      const { gx, gy } = toGraph(e);
      d.node.fx = gx;
      d.node.fy = gy;
      sim.current?.alphaTarget(0.25).restart();
    } else {
      moved.current = true;
      view.current = { ...view.current, x: d.vx + e.clientX - d.sx, y: d.vy + e.clientY - d.sy };
      canvas.current!.style.cursor = "grabbing";
      draw();
    }
  };
  const onPointerUp = (e: React.PointerEvent) => {
    const d = drag.current;
    drag.current = null;
    if (!d) return;
    if (d.node && d.moved) {
      if (d.node.id !== center) {
        d.node.fx = null;
        d.node.fy = null;
      }
      sim.current?.alphaTarget(0);
      return;
    }
    canvas.current!.style.cursor = hit(e) ? "pointer" : "grab";
    if (d.moved || !d.node) return;
    const id = d.node.id;
    // A click centres on it, unless a second click makes it a double-click (open).
    window.clearTimeout(clickTimer.current);
    if (e.detail >= 2) {
      cb.current.onOpen(id);
      return;
    }
    clickTimer.current = window.setTimeout(() => id !== center && cb.current.onCentre(id), 240);
  };
  const onWheel = (e: React.WheelEvent) => {
    const r = canvas.current!.getBoundingClientRect();
    const px = e.clientX - r.left;
    const py = e.clientY - r.top;
    const { k, x, y } = view.current;
    // A pinch arrives as a wheel with ctrlKey; a two-finger scroll pans.
    if (e.ctrlKey || e.metaKey) {
      const nk = Math.min(4, Math.max(0.1, k * Math.exp(-e.deltaY * 0.01)));
      moved.current = true;
      view.current = { k: nk, x: px - ((px - x) / k) * nk, y: py - ((py - y) / k) * nk };
    } else {
      moved.current = true;
      view.current = { k, x: x - e.deltaX, y: y - e.deltaY };
    }
    draw();
  };

  useEffect(() => () => window.clearTimeout(clickTimer.current), []);

  return (
    <div ref={wrap} className="gcanvas">
      <canvas
        ref={canvas}
        role="img"
        aria-label={`A graph of ${g.nodes.length} files and ${g.edges.length} links`}
        style={{ cursor: "grab" }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={() => {
          if (!drag.current && hover.current) {
            hover.current = null;
            draw();
          }
        }}
        onWheel={onWheel}
      />
    </div>
  );
}
