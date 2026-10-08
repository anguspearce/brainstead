// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The shortcut sheet: every key the app answers to, by where it works (the list is the help's,
// src/help/keyboard.md). ⌘K's "Keyboard shortcuts", Help › Keyboard Shortcuts and the help drawer
// open it; `?` opens the help drawer.

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
                    <dd>{what}</dd>
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
