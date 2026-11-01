// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// An image full size over the whole window, as a diagram's Full screen shows it: a click on an
// image in View, its expand button in Edit, or `open` with an image. It fits the window; a click
// shows it at its actual size (scrolled), another fits it again. Escape, a click outside or the
// button closes it. On Windows it starts below the window's own title bar (styles.css,
// .scrim.mfull), whose buttons would otherwise sit on its Close, and a press on those buttons or on
// the bar leaves it open.

import { useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./icons";
import { Store, useStore } from "./store";
import { keepOpen, useDismiss } from "./ui";

/** The image showing full size: its URL, and its alt text for the dialog's label. */
export const imageView = new Store<{ url: string; alt: string } | null>(null);

export function ImageViewerHost() {
  const v = useStore(imageView);
  return v ? <ImageViewer key={v.url} url={v.url} alt={v.alt} onClose={() => imageView.set(null)} /> : null;
}

function ImageViewer({ url, alt, onClose }: { url: string; alt: string; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [actual, setActual] = useState(false);
  useDismiss(ref, onClose, true, keepOpen);
  return createPortal(
    <div className="scrim mfull">
      <div
        ref={ref}
        role="dialog"
        aria-label={alt || "Image"}
        className={`imgfull${actual ? " actual" : ""}`}
        // A click beside the image closes it, as one outside does.
        onMouseDown={(e) => e.target === e.currentTarget && onClose()}
      >
        <img
          src={url}
          alt={alt}
          title={actual ? "Fit the image in the window" : "Show the image at its actual size"}
          onClick={() => setActual(!actual)}
        />
        <button type="button" className="ibtn imgclose" aria-label="Close full size" title="Close full size (Escape)" onClick={onClose}>
          <Icon name="x" size={14} />
        </button>
      </div>
    </div>,
    document.body,
  );
}
