// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The top bar's Teams and Outlook dot: green when both are handing over a live sign-in, amber when
// something needs the user (gone stale, signed out, an old extension), grey when not signed in.
// Shown only while the sync is on. Clicking it opens Settings › Teams and Outlook sync.

import { useEffect, useState } from "react";
import { api, SyncSignIn } from "./api";
import { nav } from "./nav";
import { hasSync } from "./platform";
import { Store, settings, useStore } from "./store";
import { syncStatus } from "./syncStatus";

/** Screenshot mode: a sign-in state to show instead of the real one (scene.ts `syncDot`). */
export const sceneSignIn = new Store<SyncSignIn | null>(null);

export function SyncDot() {
  const on = !!useStore(settings).syncEnabled && hasSync;
  const scene = useStore(sceneSignIn);
  const [signIn, setSignIn] = useState<SyncSignIn | null>(null);
  useEffect(() => {
    if (!on || scene) return;
    const check = () =>
      void api
        .syncSignInState()
        .then(setSignIn)
        .catch(() => setSignIn(null));
    check();
    // A hand-over arrives through the extensions' helper, not this window, so check now and then.
    const timer = window.setInterval(check, 60_000);
    window.addEventListener("focus", check);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", check);
    };
  }, [on, scene]);
  if (!on) return null;
  const { tone, text } = syncStatus(scene ?? signIn);
  return (
    <button
      type="button"
      className="ibtn syncdot"
      aria-label={text}
      title={`${text} Click for Settings › Teams and Outlook sync.`}
      onClick={() => nav.go({ screen: "settings", pane: "sync" })}
    >
      <span className={`d ${tone}`} />
    </button>
  );
}
