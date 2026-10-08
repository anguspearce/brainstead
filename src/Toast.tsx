// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect } from "react";
import { Icon } from "./icons";
import { Store, useStore } from "./store";
import { keyLabel } from "./platform";

export interface ToastAction {
  label: string;
  kbd?: string;
  run: () => void;
}

const msg = new Store<{ text: string; n: number; action?: ToastAction; tone?: "ok" | "bad" } | null>(null);
let n = 0;

/** A short message at the bottom, gone after a few seconds, with one optional button (Undo, Open
 *  Settings). A "bad" one says what went wrong, so it stays until it's closed or another replaces it. */
export function toast(text: string, action?: ToastAction, tone?: "ok" | "bad") {
  msg.set({ text, n: ++n, action, tone });
}

export function Toasts() {
  const m = useStore(msg);
  useEffect(() => {
    // A screenshot scene keeps its toast up; an error waits to be read and closed.
    if (!m || m.tone === "bad" || document.documentElement.dataset.scene) return;
    const t = window.setTimeout(() => msg.get()?.n === m.n && msg.set(null), m.action ? 6000 : 2600);
    return () => window.clearTimeout(t);
  }, [m]);
  return m ? (
    <div className={`toast ${m.tone ?? ""}`} role={m.tone === "bad" ? "alert" : "status"}>
      <span>{m.text}</span>
      {m.action && (
        <button
          type="button"
          className="btn sm toastbtn"
          title={`${m.action.label} and close this message`}
          onClick={() => {
            msg.set(null);
            m.action!.run();
          }}
        >
          {m.action.label}
          {m.action.kbd && <span className="kbd">{keyLabel(m.action.kbd)}</span>}
        </button>
      )}
      {m.tone === "bad" && (
        <button type="button" className="ibtn xs toastx" aria-label="Close" title="Close this message" onClick={() => msg.set(null)}>
          <Icon name="x" size={12} />
        </button>
      )}
    </div>
  ) : null;
}
