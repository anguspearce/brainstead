// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The one suggestion component (D-20261119-06): wherever the AI suggests something (Tasks and
// Projects' Find tasks and projects, the Inbox, Triage, the weekly review), the suggestions are a
// one-line chip ("2 suggestions", a spinner while they're made) that opens to the list, each row with
// Accept, Edit and Skip, and one verb to ask again: Suggest, then Suggest again. The weekly review
// uses the rows and the verb but is always open. Whether a screen's list is open is remembered.

import { ReactNode, useState } from "react";
import { Icon } from "./icons";
import { useViewState } from "./nav";
import { keyLabel } from "./platform";

/** The one verb for asking the AI: Suggest the first time, Suggest again after. */
export const suggestVerb = (again: boolean) => (again ? "Suggest again" : "Suggest");

/** Whether a screen's suggestions are open, remembered per screen (`tasks`, `projects`). */
export function useSuggestFold(screen: string): [boolean, (open: boolean) => void] {
  const [open, setOpen] = useViewState<boolean>(`${screen}:suggestions`, false);
  return [open, (v) => setOpen(v)];
}

/** Suggest, or Suggest again: the one refresh button, with a spinner while the AI works. */
export function SuggestButton({
  again,
  busy,
  title,
  disabled,
  onClick,
  big,
}: {
  again: boolean;
  busy?: boolean;
  title: string;
  disabled?: boolean;
  onClick: () => void;
  /** The top bar's size, rather than a row's. */
  big?: boolean;
}) {
  return (
    <span title={busy ? "The AI is working on its suggestions" : title}>
      <button type="button" className={`btn ${big ? "" : "sm"}`} disabled={disabled || busy} onClick={onClick}>
        {busy ? <span className="spin" /> : <Icon name="cpu" size={big ? 14 : 12} />}
        {busy ? "Suggesting…" : suggestVerb(again)}
      </button>
    </span>
  );
}

/** "2 suggestions" in a line: opens and folds the list (where there's no list, goes to the next
 *  item with one), with a spinner while they're being made; nothing when there are none. */
export function SuggestChip({
  n,
  busy,
  open,
  onToggle,
  title,
}: {
  n: number;
  busy?: boolean;
  /** Undefined where there's no list to fold (the Inbox and Triage show each on its item). */
  open?: boolean;
  onToggle?: () => void;
  title?: string;
}) {
  // Nothing to count and nothing being made: the Suggest button beside it says it all.
  if (!n && !busy) return null;
  const words = n ? `${n} ${n === 1 ? "suggestion" : "suggestions"}` : "Suggesting…";
  const tip =
    title ?? (open === undefined ? "Go to the next item with a suggestion" : open ? "Fold the suggestions" : "Show the suggestions");
  return (
    <button
      type="button"
      className={`chip suggchip ${n ? "on" : ""}`}
      aria-expanded={open}
      disabled={!n || !onToggle}
      title={n ? tip : "The AI is working on its suggestions"}
      onClick={onToggle}
    >
      {busy ? <span className="spin" /> : <Icon name="zap" size={12} />}
      {words}
      {n > 0 && open !== undefined && <Icon name={open ? "chevup" : "chevdown"} size={11} />}
    </button>
  );
}

export interface SuggestField {
  key: string;
  label: string;
  value: string;
  placeholder?: string;
}

/** One suggestion: what it is, its detail and source in grey, then Accept, Edit and Skip. Edit
 *  (where the suggestion has words to change) opens its fields in place; Enter accepts them, Escape
 *  leaves them. `extra` is a surface's own further choice, after Skip. */
export function SuggestionRow({
  id,
  text,
  detail,
  source,
  accept,
  edit,
  skip,
  busy,
  extra,
  enterKey,
}: {
  id: string;
  text: ReactNode;
  detail?: ReactNode;
  source?: ReactNode;
  accept?: { title: string; run: (edited?: Record<string, string>) => void };
  edit?: { title: string; fields: SuggestField[] };
  skip?: { title: string; run: () => void };
  busy?: boolean;
  extra?: ReactNode;
  /** Accept's key (↩) shown on it, where the screen takes it. */
  enterKey?: boolean;
}) {
  const [editing, setEditing] = useState<Record<string, string> | null>(null);
  const go = () => accept?.run(editing ?? undefined);
  return (
    <div className="suggrow" data-id={id}>
      <div className="t">
        {editing && edit ? (
          edit.fields.map((f, n) => (
            <label key={f.key} className="inp">
              <input
                aria-label={f.label}
                placeholder={f.placeholder ?? f.label}
                value={editing[f.key] ?? ""}
                autoFocus={n === 0}
                onChange={(e) => setEditing({ ...editing, [f.key]: e.target.value })}
                onKeyDown={(e) => {
                  if (e.key === "Enter") go();
                  if (e.key === "Escape") setEditing(null);
                }}
              />
            </label>
          ))
        ) : (
          <div className="pt">{text}</div>
        )}
        {(detail || source) && (
          <div className="faint small suggmeta">
            {detail}
            {source}
          </div>
        )}
      </div>
      <div className="suggbtns">
        {accept && (
          <button type="button" className="btn sm pri" disabled={busy} title={enterKey ? `${accept.title} (↩)` : accept.title} onClick={go}>
            Accept
            {enterKey && <span className="kbd">{keyLabel("↩")}</span>}
          </button>
        )}
        {edit && !editing && (
          <button
            type="button"
            className="btn sm"
            disabled={busy}
            title={edit.title}
            onClick={() => setEditing(Object.fromEntries(edit.fields.map((f) => [f.key, f.value])))}
          >
            Edit
          </button>
        )}
        {skip && (
          <button type="button" className="btn sm ghost" disabled={busy} title={skip.title} onClick={skip.run}>
            Skip
          </button>
        )}
        {extra}
      </div>
    </div>
  );
}
