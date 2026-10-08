// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The menu a click on a marked word opens (src/editor/spelling.ts): macOS's guesses for a
// misspelling, or its explanation and fixes for a grammar problem, with Learn and Ignore.

import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "../icons";
import { useStore } from "../store";
import { Popover } from "../ui";
import { acceptWord, applyFix, spellLang, spellMenu } from "./spelling";
import { isMac } from "../platform";

export function SpellMenuHost() {
  const m = useStore(spellMenu);
  const [guesses, setGuesses] = useState<string[] | null>(null);
  useEffect(() => {
    setGuesses(null);
    if (!m || m.kind !== "spelling") return;
    let live = true;
    void api
      .spellGuesses(m.text, spellLang())
      .then((g) => live && setGuesses(g))
      .catch(() => live && setGuesses([]));
    return () => {
      live = false;
    };
  }, [m]);
  if (!m) return null;
  const close = () => spellMenu.set(null);
  const fixes = m.kind === "spelling" ? (guesses ?? []) : (m.corrections ?? []);
  return (
    <Popover anchor={m.rect} onClose={close} width={260}>
      <div className="menu spellmenu" role="menu">
        {m.kind === "grammar" && m.description && <p className="spelldesc">{m.description}</p>}
        {m.kind === "spelling" && guesses === null && <p className="spelldesc faint">Looking…</p>}
        {fixes.map((g) => (
          <button
            key={g}
            type="button"
            role="menuitem"
            title={`Replace the marked text with “${g}”`}
            onClick={() => (close(), applyFix(m.view, m, g))}
          >
            <b>{g}</b>
          </button>
        ))}
        {m.kind === "spelling" && guesses?.length === 0 && <p className="spelldesc faint">No guesses.</p>}
        <div className="sep" />
        <button
          type="button"
          role="menuitem"
          title={
            m.kind === "spelling"
              ? "Stop marking this word for now, without adding it to the dictionary"
              : "Stop marking this problem for now"
          }
          onClick={() => (close(), acceptWord(m.view, m, false))}
        >
          <Icon name="x" size={14} />
          Ignore
        </button>
        {m.kind === "spelling" && (
          <button
            type="button"
            role="menuitem"
            title={`Add this word to the ${isMac ? "macOS" : "system"} dictionary so it is never marked again`}
            onClick={() => (close(), acceptWord(m.view, m, true))}
          >
            <Icon name="plus" size={14} />
            Learn “{m.text}”
          </button>
        )}
      </div>
    </Popover>
  );
}
