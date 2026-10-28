// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { ReactNode, useState } from "react";
import { askWith } from "./askState";
import { Icon } from "./icons";
import { nav, place } from "./nav";
import { settings, useStore } from "./store";
import { HelpButton } from "./help/HelpDrawer";
import { SyncDot } from "./SyncDot";
import { Popover } from "./ui";
import { dragRegion, keyLabel } from "./platform";

/** Back and forward, on every screen's top bar. */
export function HistButtons() {
  const p = useStore(place);
  return (
    <div className="hist">
      {/* Disabled buttons get no pointer events in WebKit, so the tooltip goes on a wrapper. */}
      <span title={keyLabel("Back (⌘[)")}>
        <button type="button" className="ibtn" aria-label="Back" disabled={!p.canBack} onClick={nav.back}>
          <Icon name="back" />
        </button>
      </span>
      <span title={keyLabel("Forward (⌘])")}>
        <button type="button" className="ibtn" aria-label="Forward" disabled={!p.canForward} onClick={nav.forward}>
          <Icon name="forward" />
        </button>
      </span>
    </div>
  );
}

/** What a screen's Ask button is about: `about` finishes "Ask about …" in its tooltip (the
 *  selection when there is one, else the screen), and `prompt` is typed into a new chat for you to
 *  finish, as "About [[Orbit App]]: ". */
export interface AskAbout {
  about: string;
  prompt: string;
  /** Small, beside a document's small buttons. */
  sm?: boolean;
}

/** The screen's Ask button: always the last before help, so it's in the same place everywhere. */
export function AskButton({ ask }: { ask: AskAbout }) {
  return (
    <button
      type="button"
      className={ask.sm ? "btn sm" : "btn"}
      title={`Ask about ${ask.about}`}
      onClick={() => {
        askWith(ask.prompt);
        nav.go("ask");
      }}
    >
      <Icon name="ask" size={ask.sm ? 13 : 14} />
      Ask
    </button>
  );
}

/** One of a top bar's less used actions, in its More menu. */
export interface MoreItem {
  label: string;
  icon: string;
  title: string;
  run: () => void;
  disabled?: boolean;
  /** Shown in red: it moves something to the Trash. */
  danger?: boolean;
}

/** A top bar's "…": the screen's less used actions, so the bar keeps one or two of its own. */
export function MoreMenu({ items, title }: { items: MoreItem[]; title: string }) {
  const [at, setAt] = useState<DOMRect | null>(null);
  return (
    <>
      <button
        type="button"
        className="ibtn"
        aria-label="More"
        aria-haspopup="menu"
        aria-expanded={!!at}
        title={title}
        // Open, a click on ⋯ closes it (kept from the menu's click-outside, which would reopen it).
        onMouseDown={(e) => at && e.nativeEvent.stopPropagation()}
        onClick={(e) => setAt(at ? null : e.currentTarget.getBoundingClientRect())}
      >
        <Icon name="more" />
      </button>
      {at && (
        <Popover anchor={at} onClose={() => setAt(null)} width={240}>
          <div className="menu" role="menu">
            {items.map((i) => (
              <button
                key={i.label}
                type="button"
                role="menuitem"
                className={i.danger ? "danger" : undefined}
                title={i.title}
                disabled={i.disabled}
                onClick={() => {
                  setAt(null);
                  i.run();
                }}
              >
                <Icon name={i.icon} size={14} />
                {i.label}
              </button>
            ))}
          </div>
        </Popover>
      )}
    </>
  );
}

/** Across a screen whose actions read-only blocks (Changes' Accept and Revert): says so once, with
 *  the switch that turns it off, rather than each button failing. */
export function ReadOnlyBanner({ blocks }: { blocks: string }) {
  return (
    <div className="robanner" role="status">
      <Icon name="lock" size={14} />
      <span className="grow">The vault is read-only, so {blocks}.</span>
      <button
        type="button"
        className="btn sm"
        title="Let Brainstead change files in the vault again (Settings › Vault has the same switch)"
        onClick={() => void settings.commit({ readOnly: false })}
      >
        Turn off read-only
      </button>
    </div>
  );
}

/** Text in curly quotes, cut short for a tooltip: “Call Sam about the launch date”. */
export function askQuote(text: string, max = 60): string {
  const t = text.replace(/\s+/g, " ").trim();
  return `“${t.length > max ? `${t.slice(0, max - 1).trimEnd()}…` : t}”`;
}

/** A page or note named so Ask links it: `[[Orbit App]]`. */
export const askLink = (path: string) =>
  `[[${path
    .replace(/\.(md|txt)$/i, "")
    .split("/")
    .pop()}]]`;

/** The bar across the top of a screen: back and forward, the title, the screen's own buttons, and
 *  its Ask button beside help. */
export function TopBar({ title, sub, children, ask }: { title: string; sub?: string; children?: ReactNode; ask?: AskAbout }) {
  return (
    <div className="top drag" {...dragRegion()}>
      <HistButtons />
      <div className="crumbs">
        <b>{title}</b>
        {sub && (
          <>
            <span>·</span>
            <span>{sub}</span>
          </>
        )}
      </div>
      {/* The bar's empty middle drags the window too: the script acts on the pressed element only. */}
      <div className="sp" {...dragRegion()} />
      {children}
      {ask && <AskButton ask={ask} />}
      <SyncDot />
      <HelpButton />
    </div>
  );
}
