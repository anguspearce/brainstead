// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Update…'s box: what has happened, in a line, for an assistant to apply in the background
// (src/updates.ts). The Inbox, a task's menu and a project's detail open it.

import { useState } from "react";
import { keyLabel } from "./platform";
import { toast } from "./Toast";
import { Popover } from "./ui";
import { startUpdate, targetName, UpdateTarget } from "./updates";

/** The box itself, for a popover of its own (UpdateBox) or a menu's view (a task's ⋯ menu). */
export function UpdateForm({ target, onClose }: { target: UpdateTarget; onClose: () => void }) {
  const [text, setText] = useState("");
  const apply = () => {
    if (!text.trim()) return;
    startUpdate(target, text);
    toast(`Updating ${targetName(target)}: the Running line shows it`);
    onClose();
  };
  return (
    <div className="updbox" role="dialog" aria-label="Update">
      <div className="mhead faint small">What's happened?</div>
      <textarea
        autoFocus
        rows={3}
        value={text}
        placeholder="Lena has already replied to Maya"
        aria-label="What's happened"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            apply();
          }
        }}
      />
      <p className="faint small">
        An assistant applies it in the background: it marks things done, changes dates or adds a follow-up, as the update means. Each change
        is in Changes, with Revert.
      </p>
      <div className="row">
        <span className="grow" />
        <button type="button" className="btn sm ghost" title="Close without updating" onClick={onClose}>
          Cancel
        </button>
        <button
          type="button"
          className="btn sm pri"
          disabled={!text.trim()}
          title={keyLabel("Have an assistant apply it in the background (⌘↩)")}
          onClick={apply}
        >
          Update
        </button>
      </div>
    </div>
  );
}

export function UpdateBox({ target, at, onClose }: { target: UpdateTarget; at: DOMRect; onClose: () => void }) {
  return (
    <Popover anchor={at} onClose={onClose} width={380}>
      <UpdateForm target={target} onClose={onClose} />
    </Popover>
  );
}
