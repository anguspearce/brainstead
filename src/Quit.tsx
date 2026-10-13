// The question ⌘Q and the menu bar's Quit Brainstead ask (the `quit-requested` event from
// src-tauri): unsaved edits are asked about first (the leave dialog), then this. Esc or a click
// outside is Cancel.

import { useEffect, useState } from "react";
import { api } from "./api";
import { whenFree } from "./nav";
import { Dialog } from "./ui";

export function QuitDialog() {
  const [shown, setShown] = useState(false);
  useEffect(() => {
    const off = api.onQuitRequested(() => whenFree(() => setShown(true))).catch(() => () => {});
    return () => void off.then((f) => f());
  }, []);
  if (!shown) return null;
  return (
    <Dialog label="Quit Brainstead" width={460} onClose={() => setShown(false)}>
      <div className="confirm">
        <h2 className="h2">Quit Brainstead?</h2>
        <p className="muted">
          Anything it's running stops, and scheduled reviews wait until it's open again. To keep it in the menu bar, close the window
          instead.
        </p>
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
