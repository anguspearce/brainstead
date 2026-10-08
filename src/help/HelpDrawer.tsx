// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The help drawer: `?`, the top bar's ? button or Help › Brainstead Help opens it on the right, on
// the current screen's topic. Typing filters its sections and searches every topic; below them,
// the getting-started guides (one step at a time) and the screen's keys. It stays open while you
// follow a link into the app, so a guide can be walked through; Esc or × closes it.

import { openUrl } from "@tauri-apps/plugin-opener";
import { ReactNode, useEffect, useMemo, useState } from "react";
import ReactMarkdown, { Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { api } from "../api";
import { askWith } from "../askState";
import { Icon } from "../icons";
import { nav, place, Place } from "../nav";
import { shortcutsOpen } from "../Shortcuts";
import { settings, Store, useStore } from "../store";
import { SearchBox } from "../ui";
import { appLink, GUIDES, helpUrl, openSection, HelpSection, HelpTopic, KEYS, searchHelp, TOPICS, topicFor } from ".";
import { accelLabel, captureDefault, keyLabel } from "../platform";

/** What the drawer shows: a topic (the screen's when none is named), or a guide at a step. */
export type HelpView = { topic?: string; guide?: string; step?: number; q?: string };
export const helpOpen = new Store<HelpView | null>(null);

export const openHelp = (v: HelpView = {}) => helpOpen.set(v);
export const toggleHelp = () => helpOpen.set(helpOpen.get() ? null : {});

/** The keyboard section for a screen, besides Anywhere. */
const KEY_SECTION: Partial<Record<Place["screen"], string>> = {
  doc: "A note",
  inbox: "Inbox",
  triage: "Bookmarks triage",
  review: "Changes",
  ask: "Ask",
  tasks: "Task lists",
  today: "Task lists",
  weekly: "Task lists",
  notes: "Lists",
  wiki: "Lists",
  sources: "Lists",
  templates: "Lists",
  trash: "Lists",
  projects: "Task lists",
};

function HelpText({ text }: { text: string }) {
  const components = useMemo<Components>(
    () => ({
      a: ({ href = "", children }) => {
        const to = appLink(href);
        const topic = /^help:([\w-]+)$/.exec(href)?.[1];
        return (
          <a
            href={href}
            onClick={(e) => {
              e.preventDefault();
              if (topic) openHelp({ topic });
              else if (to) nav.go(to);
              else if (/^https?:/.test(href)) void openUrl(href);
            }}
          >
            {children}
          </a>
        );
      },
    }),
    [],
  );
  return (
    <div className="help-text">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components} urlTransform={helpUrl}>
        {keyLabel(text)}
      </ReactMarkdown>
    </div>
  );
}

function Section({ s, open }: { s: HelpSection; open?: boolean }) {
  return (
    <details className="help-sec" open={open}>
      <summary>
        <Icon name="chevright" size={13} className="help-chev" />
        {keyLabel(s.title)}
      </summary>
      <HelpText text={s.body} />
    </details>
  );
}

function Keys({ screen }: { screen: Place["screen"] }) {
  const s = useStore(settings);
  const wanted = ["Anywhere", KEY_SECTION[screen]].filter(Boolean);
  const capture = accelLabel(s.captureShortcut ? s.captureShortcut : captureDefault());
  return (
    <section className="help-block">
      <div className="help-blockhead">
        <span className="eyebrow">Keyboard</span>
        <button type="button" className="btn sm ghost" title="Show every keyboard shortcut" onClick={() => shortcutsOpen.set(true)}>
          All shortcuts
        </button>
      </div>
      {KEYS.filter(([t]) => wanted.includes(t)).map(([t, rows]) => (
        <div key={t} className="help-keys">
          {wanted.length > 1 && <div className="faint small">{keyLabel(t)}</div>}
          {rows.map(([k, what]) => (
            <div key={k + what} className="help-key">
              <span>{what}</span>
              <span>
                {keyLabel(k.replace("⌃⌥Space", capture))
                  .split("  ")
                  .map((x) => (
                    <span key={x} className="kbd">
                      {x}
                    </span>
                  ))}
              </span>
            </div>
          ))}
        </div>
      ))}
    </section>
  );
}

function GuideList() {
  const s = useStore(settings);
  return (
    <section className="help-block">
      <span className="eyebrow">Getting started</span>
      <div className="help-guides">
        {GUIDES.map((g) => (
          <button
            key={g.id}
            type="button"
            className="help-guide"
            title={`Walk through “${g.title}”, a step at a time`}
            onClick={() => openHelp({ guide: g.id, step: 0 })}
          >
            <span className="help-guide-t">{g.title}</span>
            {s.helpGuides?.[g.id] ? (
              <span className="chip green">
                <Icon name="check" size={12} />
                Done
              </span>
            ) : (
              <span className="faint small">{g.sections.length} steps</span>
            )}
          </button>
        ))}
      </div>
    </section>
  );
}

function Guide({ g, step }: { g: HelpTopic; step: number }) {
  const n = g.sections.length;
  const at = Math.min(Math.max(step, 0), n - 1);
  const s = g.sections[at];
  const last = at === n - 1;
  return (
    <div className="help-guidebox">
      <div className="faint small">
        Step {at + 1} of {n}
      </div>
      <h3 className="help-steptitle">{s.title}</h3>
      <HelpText text={s.body} />
      <div className="help-steps">
        <span title={at === 0 ? "This is the first step" : "Go back a step"}>
          <button type="button" className="btn" disabled={at === 0} onClick={() => openHelp({ guide: g.id, step: at - 1 })}>
            Back
          </button>
        </span>
        <span className="help-dots" aria-hidden="true">
          {g.sections.map((x, i) => (
            <i key={x.title} className={i === at ? "on" : ""} />
          ))}
        </span>
        <button
          type="button"
          className="btn pri"
          title={last ? "Finish this guide and mark it done" : "Go to the next step"}
          onClick={() => {
            if (!last) return openHelp({ guide: g.id, step: at + 1 });
            settings.update({ helpGuides: { ...(settings.get().helpGuides ?? {}), [g.id]: true } });
            openHelp({});
          }}
        >
          {last ? "Done" : "Next"}
        </button>
      </div>
    </div>
  );
}

function Results({ q }: { q: string }) {
  const hits = searchHelp(q).slice(0, 30);
  if (!hits.length) return <p className="muted">Nothing in the help matches “{q}”. Ask can look further.</p>;
  return (
    <div className="help-results">
      {hits.map((h) => (
        <div key={h.topic.id + h.section.title} className="help-hit">
          <button
            type="button"
            className="help-hit-topic"
            title={`Open the help for ${h.topic.title}`}
            onClick={() => openHelp({ topic: h.topic.id })}
          >
            {h.topic.title}
          </button>
          <Section s={h.section} open />
        </div>
      ))}
    </div>
  );
}

export function HelpDrawer() {
  const view = useStore(helpOpen);
  const p = useStore(place).place;
  const [q, setQ] = useState("");

  // `?` outside a text box opens and closes it; Esc closes it.
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.key === "Escape" && helpOpen.get() && !document.querySelector(".dialog")) {
        helpOpen.set(null);
        return;
      }
      if (e.key !== "?" || e.metaKey || e.ctrlKey || e.altKey) return;
      if ((e.target as HTMLElement)?.closest?.("input, textarea, select, [contenteditable]")) return;
      toggleHelp();
      e.preventDefault();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, []);
  useEffect(() => setQ(view?.q ?? ""), [view?.topic, view?.guide, view?.q]);

  if (!view) return null;
  const guide = view.guide ? GUIDES.find((g) => g.id === view.guide) : undefined;
  const topic = (view.topic && TOPICS.find((t) => t.id === view.topic)) || topicFor(p);
  const query = q.trim();
  const title = guide ? guide.title : topic ? topic.title : "Help";
  const first = topic ? openSection(topic, p) : undefined;

  let body: ReactNode;
  if (query) body = <Results q={query} />;
  else if (guide) body = <Guide g={guide} step={view.step ?? 0} />;
  else
    body = (
      <>
        {topic && (
          <section className="help-block">
            {topic.intro && <HelpText text={topic.intro} />}
            {topic.sections.map((s) => (
              <Section key={`${topic.id}:${s.title}`} s={s} open={s.title === first} />
            ))}
          </section>
        )}
        <GuideList />
        <Keys screen={p.screen} />
        <section className="help-block">
          <span className="eyebrow">All topics</span>
          <div className="help-topics">
            {TOPICS.filter((t) => t.kind === "screen" && t.id !== topic?.id).map((t) => (
              <button key={t.id} type="button" className="chip" title={t.summary} onClick={() => openHelp({ topic: t.id })}>
                {t.title}
              </button>
            ))}
          </div>
        </section>
      </>
    );

  return (
    <aside className="help-drawer" role="complementary" aria-label="Help">
      <div className="help-head">
        {(guide || view.topic) && (
          <button
            type="button"
            className="ibtn"
            aria-label="Back to this screen's help"
            title="Back to this screen's help"
            onClick={() => openHelp({})}
          >
            <Icon name="back" />
          </button>
        )}
        <h2 className="h2">
          {title}
          <span className="faint"> — {guide ? "getting started" : "help"}</span>
        </h2>
        <button type="button" className="ibtn" aria-label="Close help" title="Close help (Esc)" onClick={() => helpOpen.set(null)}>
          <Icon name="x" />
        </button>
      </div>
      {/* Esc closes the drawer, so it doesn't also clear the box. */}
      <SearchBox value={q} onChange={setQ} placeholder="Search the help" escAnywhere={false} />
      <div className="help-body">{body}</div>
      <div className="help-foot">
        <button
          type="button"
          className="btn"
          title="Open a new Ask chat to ask how something in Brainstead works"
          onClick={() => {
            askWith(query ? `How do I ${query} in Brainstead?` : "How do I ");
            nav.go("ask");
            helpOpen.set(null);
          }}
        >
          <Icon name="ask" size={14} />
          Ask about Brainstead
        </button>
      </div>
    </aside>
  );
}

/** Help in the menu bar: Brainstead Help and Keyboard Shortcuts. Listens once, at startup. */
export function startHelpMenu() {
  return api.onMenu((id) => {
    if (id === "help:open") openHelp({});
    else if (id === "help:shortcuts") shortcutsOpen.set(true);
  });
}

/** The top bar's ? button. */
export function HelpButton() {
  const open = useStore(helpOpen);
  return (
    <button
      type="button"
      className={`ibtn help-btn${open ? " on" : ""}`}
      aria-label="Help"
      title="Help for this screen (?)"
      onClick={toggleHelp}
    >
      <Icon name="help" />
    </button>
  );
}
