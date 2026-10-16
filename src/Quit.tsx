// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The question ⌘Q and the menu bar's Quit Brainstead ask (the `quit-requested` event from
// src-tauri): unsaved edits are asked about first (the leave dialog), then this. Esc or a click
// outside is Cancel. On Windows with no tray icon, closing the window quits; that close, and the
// install script's `Brainstead --quit`, ask only about unsaved edits, as closing a Windows app does.

import { useEffect, useState } from "react";
import { api } from "./api";
import { whenFree } from "./nav";
import { isMac } from "./platform";
import { settings } from "./store";
import { Dialog } from "./ui";

export function QuitDialog() {
  const [shown, setShown] = useState(false);
  useEffect(() => {
    const off = api.onQuitRequested((ask) => whenFree(() => (ask ? setShown(true) : void api.appQuit()))).catch(() => () => {});
    return () => void off.then((f) => f());
  }, []);
  if (!shown) return null;
  const keep = isMac
    ? " To keep it in the menu bar, close the window instead."
    : settings.get().menuBar !== false
      ? " To keep it in the system tray, close the window instead."
      : "";
  return (
    <Dialog label="Quit Brainstead" width={460} onClose={() => setShown(false)}>
      <div className="confirm">
        <h2 className="h2">Quit Brainstead?</h2>
        <p className="muted">Anything it's running stops, and scheduled reviews wait until it's open again.{keep}</p>
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg" title="Keep Brainstead open" onClick={() => setShown(false)}>
            Cancel
          </button>
          <button type="button" className="btn lg pri" title="Quit Brainstead" autoFocus onClick={() => void api.appQuit()}>
            Quit
          </button>
        </div>
      </div>
    </Dialog>
  );
}
