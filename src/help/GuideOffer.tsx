// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// When the app opens on a vault: a welcome offering the help's Getting started guides, until
// "Don't ask me again" is ticked or Settings › General turns it off (settings.offerGuide). Once a
// launch, and not in screenshot mode unless the scene asks for it.

import { useEffect, useState } from "react";
import { Mark } from "../icons";
import { settings, Store, useStore } from "../store";
import { Dialog } from "../ui";
import { openHelp } from "./HelpDrawer";

export const guideOffer = new Store(false);
let offered = false;

export function GuideOfferHost() {
  const s = useStore(settings);
  const open = useStore(guideOffer);
  const [never, setNever] = useState(false);
  const offer = settings.loaded && !settings.frozen && s.offerGuide !== false;
  useEffect(() => {
    if (!offer || offered) return;
    // A moment after the shell has drawn, so it isn't lost under the first screen's loading.
    const t = window.setTimeout(() => {
      offered = true;
      guideOffer.set(true);
    }, 800);
    return () => window.clearTimeout(t);
  }, [offer]);
  if (!open) return null;
  const close = (start: boolean) => {
    guideOffer.set(false);
    if (never) settings.update({ offerGuide: false });
    if (start) openHelp({ guide: "guide-start", step: 0 });
  };
  return (
    <Dialog onClose={() => close(false)} width={500} label="Welcome to Brainstead">
      <div className="confirm upd">
        <div className="updhead">
          <Mark size={40} />
          <div className="grow">
            <h2 className="h2">Welcome to Brainstead</h2>
            <p className="muted">
              Getting started walks you through the app a step at a time, beginning with your vault and finding your way around.
            </p>
          </div>
        </div>
        <p className="faint">To come back to it, open the help (press ? or click ? at the top right) and click Getting started.</p>
        <div className="row">
          <label className="row" style={{ gap: 6 }} title="Don't offer Getting started when Brainstead opens">
            <input type="checkbox" checked={never} onChange={(e) => setNever(e.target.checked)} />
            Don't ask me again
          </label>
          <span className="grow" />
          <button type="button" className="btn lg" title="Close this and carry on" onClick={() => close(false)}>
            Not Now
          </button>
          <button type="button" className="btn lg pri" autoFocus title="Open the first guide in the help" onClick={() => close(true)}>
            Open Getting Started
          </button>
        </div>
      </div>
    </Dialog>
  );
}
