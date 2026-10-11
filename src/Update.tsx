// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The update dialog: src-tauri/src/updates.rs offers a newer version here, reports the download's
// progress and says what it has to say (up to date, couldn't check, the update will wait).

import { api, UpdateOffer, UpdateProgress } from "./api";
import { Mark } from "./icons";
import { Markdown } from "./md/Markdown";
import { Store, useStore } from "./store";
import { Dialog, fmtBytes } from "./ui";

export type UpdateView =
  { kind: "offer"; offer: UpdateOffer; progress: UpdateProgress | null } | { kind: "message"; title: string; text: string } | null;

export const updateView = new Store<UpdateView>(null);

/** Listens for the updater's events; call once from the main window. */
export function startUpdates() {
  const off = [
    api.onUpdateOffer((offer) => updateView.set({ kind: "offer", offer, progress: null })),
    api.onUpdateProgress((progress) => {
      const v = updateView.get();
      if (v?.kind === "offer") updateView.set({ ...v, progress });
    }),
    api.onUpdateMessage(({ title, text }) => updateView.set({ kind: "message", title, text })),
  ].map((o) => o.catch(() => () => {}));
  // An offer made before this window was listening (a reload) is still waiting.
  void api
    .updateOffered()
    .then((offer) => offer && !updateView.get() && updateView.set({ kind: "offer", offer, progress: null }))
    .catch(() => {});
  return () => off.forEach((o) => void o.then((f) => f()));
}

/** "9 October 2026" from latest.json's pub_date, or null. */
function released(date: string | null): string | null {
  const d = date ? new Date(date) : null;
  return d && !Number.isNaN(+d) ? d.toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric" }) : null;
}

export function UpdateHost() {
  const v = useStore(updateView);
  if (!v) return null;
  if (v.kind === "message") return <UpdateMessage title={v.title} text={v.text} />;
  return <UpdateOfferDialog offer={v.offer} progress={v.progress} />;
}

function UpdateMessage({ title, text }: { title: string; text: string }) {
  const close = () => updateView.set(null);
  return (
    <Dialog onClose={close} width={420} label={title}>
      <div className="confirm upd">
        <div className="updhead">
          <Mark size={40} />
          <h2 className="h2">{title}</h2>
        </div>
        <p className="muted">{text}</p>
        <div className="row">
          <span className="grow" />
          <button type="button" className="btn lg pri" autoFocus title="Close this message" onClick={close}>
            OK
          </button>
        </div>
      </div>
    </Dialog>
  );
}

function UpdateOfferDialog({ offer, progress }: { offer: UpdateOffer; progress: UpdateProgress | null }) {
  // Once it's installing there's nothing to answer: Esc and clicks outside do nothing.
  const answer = (f: () => Promise<void>) => () => {
    if (progress) return;
    updateView.set(null);
    void f().catch(() => {});
  };
  const later = answer(api.updateLater);
  const date = released(offer.date);
  return (
    <Dialog onClose={later} width={520} label={`${offer.name} ${offer.version} is available`}>
      <div className="confirm upd">
        <div className="updhead">
          <Mark size={40} />
          <div className="grow">
            <h2 className="h2">
              {offer.name} {offer.version} is available
            </h2>
            <p className="muted">
              You have {offer.current}
              {date && ` · released ${date}`}
            </p>
          </div>
        </div>
        {offer.notes && (
          <div className="updnotes" tabIndex={0} aria-label="What's new">
            <Markdown content={offer.notes} path="" root="" />
          </div>
        )}
        {progress ? (
          <Installing progress={progress} />
        ) : (
          <div className="row">
            <button type="button" className="btn lg" title="Don't offer this version again" onClick={answer(api.updateSkip)}>
              Skip This Version
            </button>
            <span className="grow" />
            <button type="button" className="btn lg" title="Ask again at the next day's check" onClick={later}>
              Later
            </button>
            <button
              type="button"
              className="btn lg pri"
              autoFocus
              title="Download the update and restart into it; unsaved edits are asked about first"
              onClick={() => {
                // The dialog stays up to show the download; updates.rs says if it has to wait.
                updateView.set({ kind: "offer", offer, progress: { phase: "downloading", downloaded: 0, total: null } });
                void api.updateInstall().catch(() => {});
              }}
            >
              Install and Restart
            </button>
          </div>
        )}
      </div>
    </Dialog>
  );
}

function Installing({ progress: p }: { progress: UpdateProgress }) {
  const known = p.phase === "downloading" && !!p.total;
  const pct = known ? Math.min(100, Math.round((p.downloaded / (p.total ?? 1)) * 100)) : 0;
  const label =
    p.phase === "restarting"
      ? "Restarting…"
      : p.phase === "installing"
        ? "Installing…"
        : known
          ? `Downloading ${fmtBytes(p.downloaded)} of ${fmtBytes(p.total ?? 0)}`
          : p.downloaded
            ? `Downloading ${fmtBytes(p.downloaded)}`
            : "Downloading…";
  return (
    <div className="jprog updprog" aria-live="polite">
      <div
        className={`bar ${known ? "" : "busy"}`}
        role="progressbar"
        aria-label="Update"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={known ? pct : undefined}
        aria-valuetext={label}
      >
        <i style={known ? { width: `${Math.max(2, pct)}%` } : undefined} />
      </div>
      <span className="faint updlabel">{label}</span>
    </div>
  );
}
