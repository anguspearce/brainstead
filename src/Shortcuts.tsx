// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The shortcut sheet: every key the app answers to, by where it works (the list is the help's,
// src/help/keyboard.md). ⌘K's "Keyboard shortcuts", Help › Keyboard Shortcuts and the help drawer
// open it; `?` opens the help drawer.

import type { ReactNode } from "react";
import { Icon } from "./icons";
import { Dialog } from "./ui";
import { settings, Store, useStore } from "./store";
import { KEYS } from "./help";
import { accelLabel, captureDefault, keyLabel } from "./platform";

export const shortcutsOpen = new Store(false);

/** The capture shortcut as the sheet shows it: "Control+Alt+Space" → "⌃⌥Space" on macOS, "Ctrl+Alt+Space" elsewhere. */
export const fmtAccel = accelLabel;

/** Every key by where it works, from the help (src/help/keyboard.md), with the capture shortcut
 *  as it's set. */
export const SECTIONS = (capture: string): [string, [string, string][]][] =>
  KEYS.map(([title, rows]) => [title, rows.map(([k, what]): [string, string] => [keyLabel(k.replace("⌃⌥Space", capture)), what])]);

/** A key's description as the help writes it, without its markdown showing: `keys` as keys,
 *  **bold** bold, and a [link](app:…) as its words (the help drawer is where it opens). */
export function inline(text: string): ReactNode[] {
  return text.split(/(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/).map((part, i) => {
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2)
      return (
        <span key={i} className="kbd">
          {keyLabel(part.slice(1, -1))}
        </span>
      );
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) return <b key={i}>{part.slice(2, -2)}</b>;
    const link = part.match(/^\[([^\]]+)\]\([^)]+\)$/);
    return link ? link[1] : part;
  });
}

export function ShortcutsHost() {
  const open = useStore(shortcutsOpen);
  const s = useStore(settings);
  if (!open) return null;
  const capture = fmtAccel(s.captureShortcut || captureDefault());
  return (
    <Dialog onClose={() => shortcutsOpen.set(false)} width={720} label="Keyboard shortcuts">
      <div className="confirm shortcuts">
        <div className="row">
          <h2 className="h2 grow">Keyboard shortcuts</h2>
          <button
            type="button"
            className="ibtn"
            aria-label="Close"
            title="Close the shortcuts (Esc)"
            onClick={() => shortcutsOpen.set(false)}
          >
            <Icon name="x" />
          </button>
        </div>
        <div className="kgrid">
          {SECTIONS(capture).map(([title, keys]) => (
            <section key={title}>
              <div className="eyebrow">{keyLabel(title)}</div>
              <dl>
                {keys.map(([k, what]) => (
                  <div key={k + what} className="krow">
                    <dt>
                      {k.split("  ").map((x) => (
                        <span key={x} className="kbd">
                          {x}
                        </span>
                      ))}
                    </dt>
                    <dd>{inline(what)}</dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </div>
    </Dialog>
  );
}
