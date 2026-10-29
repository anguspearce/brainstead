// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The app's icons: 24-unit stroke drawings in currentColor, so they take the text colour around them.

import { useId, type CSSProperties } from "react";

const PATHS: Record<string, string> = {
  today:
    "M12 3v2M12 19v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M3 12h2M19 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
  inbox: "M3 13h5l1.5 3h5L16 13h5M5.5 5h13L21 13v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-5z",
  tasks: "M9 11l2.5 2.5L16 9M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z",
  project: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  review:
    "M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2M9 5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v0a2 2 0 0 1-2 2h-2a2 2 0 0 1-2-2zM9 14l2 2 4-4",
  ask: "M21 12a8 8 0 0 1-11.6 7.1L4 20.5l1.4-4.9A8 8 0 1 1 21 12z",
  note: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M9 13h6M9 17h4",
  wiki: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20V3H6.5A2.5 2.5 0 0 0 4 5.5zM4 19.5A2.5 2.5 0 0 0 6.5 22H20v-5",
  source: "M21.4 11.1l-9.2 9.2a6 6 0 0 1-8.5-8.5l9.2-9.2a4 4 0 0 1 5.7 5.7l-9.2 9.2a2 2 0 0 1-2.8-2.8l8.5-8.5",
  template: "M4 4h16v5H4zM4 13h7v7H4zM15 13h5M15 17h5",
  graph: "M6 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 15a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM18 3a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM8.6 7.5l6.8 9M8.8 5h6.4",
  health: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z",
  activity: "M22 12h-4l-3 9L9 3l-3 9H2",
  trash: "M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6",
  settings:
    "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z",
  search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4",
  plus: "M12 5v14M5 12h14",
  back: "M15 18l-6-6 6-6",
  forward: "M9 18l6-6-6-6",
  chevdown: "M6 9l6 6 6-6",
  x: "M18 6L6 18M6 6l12 12",
  check: "M20 6L9 17l-5-5",
  refresh: "M21 12a9 9 0 1 1-2.6-6.4L21 8M21 3v5h-5",
  undo: "M9 14L4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11",
  // The kinds of thing in the Inbox (inboxItemKind): line drawings, never a vendor's logo.
  mail: "M4 6h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1zM3.5 7l8.5 6 8.5-6",
  chat: "M20 14a2 2 0 0 1-2 2H9l-5 4V6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2zM8 9h8M8 12h5",
  mic: "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM5 11a7 7 0 0 0 14 0M12 18v3M9 21h6",
  bulb: "M9 18h6M10 21h4M12 3a6 6 0 0 0-3.6 10.8c.7.6 1.1 1.3 1.1 2.2h5c0-.9.4-1.6 1.1-2.2A6 6 0 0 0 12 3z",
  "file-word": "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M8 12l1.5 6L12 13l2.5 5L16 12",
  "file-pdf": "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M9 18v-6h2a2 2 0 0 1 0 4H9",
  slides: "M3 4h18M4 4v10a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1V4M12 15v4M8 20l4-1 4 1M8 11l3-3 2 2 3-3",
  sheet: "M4 4h16v16H4zM4 9h16M4 14h16M10 4v16",
  image: "M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM3 16l5-5 4 4 3-3 6 6M15.5 9.5h.01",
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  lock: "M7 11V7a5 5 0 0 1 10 0v4M5 11h14v10H5z",
  shield: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10zM9 12l2 2 4-4",
  key: "M15 7a4 4 0 1 1-3.9 5H8v3H5v-3H3V9h8.1A4 4 0 0 1 15 7zM15 10v.01",
  info: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 16v-4M12 8h.01",
  pencil: "M17 3l4 4L8 20H4v-4zM14 6l4 4",
  clipboard: "M9 3h6v3H9zM9 4.5H6a1 1 0 0 0-1 1V20a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V5.5a1 1 0 0 0-1-1h-3M9 12h6M9 16h4",
  flame: "M12 22a7 7 0 0 0 7-7c0-4-3-6-4-10-2 2-3 4-3 6-1-1-2-2-2-4-2 2-5 5-5 8a7 7 0 0 0 7 7z",
  help: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01",
  alert: "M10.3 3.9L1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0zM12 9v4M12 17h.01",
  zap: "M13 2L3 14h9l-1 8 10-12h-9z",
  bug: "M9 7.5V6a3 3 0 0 1 6 0v1.5M12 20c-3.3 0-6-2.7-6-6v-3a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v3c0 3.3-2.7 6-6 6zM12 20v-9M6 13H2M22 13h-4M6.5 9L3 7M17.5 9L21 7M6.5 17L3 19M17.5 17L21 19",
  list: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
  quote: "M3 20c3 0 6-2 6-7V5H3v7h4c0 3-1.5 4.5-4 5zM14 20c3 0 6-2 6-7V5h-6v7h4c0 3-1.5 4.5-4 5z",
  pin: "M12 17v5M9 3h6l-1 7 4 3v2H6v-2l4-3z",
  eye: "M1 12s4-8 11-8 11 8 11 8-4 8-11 8S1 12 1 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
  globe: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM2 12h20M12 2a15 15 0 0 1 4 10 15 15 0 0 1-4 10 15 15 0 0 1-4-10 15 15 0 0 1 4-10z",
  finder: "M14 3v18M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM8 9v1M17 9v1M8 15c2.5 2 5.5 2 8 0",
  sun: "M12 3v2M12 19v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M3 12h2M19 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
  minus: "M5 12h14",
  grip: "M9 5h.01M15 5h.01M9 12h.01M15 12h.01M9 19h.01M15 19h.01",
  waiting: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 6v6l4 2",
  someday: "M8 2v4M16 2v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM10 16h4",
  textsize: "M2.25 18.75L6.75 6l4.5 12.75M3.75 15h6M13.5 18.75l3.75-9 3.75 9M14.7 16.2h5.1",
  fit: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
  expand: "M14 4h6v6M20 4l-7 7M10 20H4v-6M4 20l7-7",
  more: "M5 12h.01M12 12h.01M19 12h.01",
  copy: "M9 9h11v11H9zM5 15H4V4h11v1",
  chevup: "M18 15l-6-6-6 6",
  chevright: "M9 18l6-6-6-6",
  calendar: "M3 6h18v15H3zM3 10h18M8 3v4M16 3v4",
  hash: "M4 9h16M4 15h16M10 3L8 21M16 3l-2 18",
  tag: "M20.6 13.4l-7.2 7.2a2 2 0 0 1-2.8 0L3 13V3h10l7.6 7.6a2 2 0 0 1 0 2.8zM7.5 7.5h.01",
  text: "M4 7V5h16v2M9 19h6M12 5v14",
  moon: "M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z",
  // Task dates (src/taskDates.ts).
  // An hourglass: caps top and bottom, two bulbs meeting at a waist, sand left above, falling through and piled below.
  defer:
    "M5 2.5h14M5 21.5h14M7 2.5v2.2c0 3.3 4 5.1 4 7.3s-4 4-4 7.3v2.2M17 2.5v2.2c0 3.3-4 5.1-4 7.3s4 4 4 7.3v2.2M9.3 7.5h5.4M12 12.5v4M8.5 20c.8-1.8 2-2.7 3.5-2.7s2.7.9 3.5 2.7",
  start: "M5 21V4M5 4h12l-2.5 4L17 12H5",
  // A power symbol: Open at login.
  power: "M12 3.75v7.5M7.8 6.6a7.5 7.5 0 1 0 8.4 0",
  created: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 8v8M8 12h8",
  done: "M21 12a9 9 0 1 1-5.3-8.2M21 5l-9 9-3-3",
  // Menus: rename (a pencil) and export (a page with an arrow out).
  rename: "M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16v4zM13.5 6.5l4 4",
  export: "M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9zM14 3v6h6M12 18v-6M9 15l3-3 3 3",
  // A task's effort: a stopwatch.
  stopwatch: "M12 22a8 8 0 1 0 0-16 8 8 0 0 0 0 16zM12 14l3-3M10 2h4M12 2v4M19 6l1.5-1.5",
  // The other task fields (src/taskFields.ts).
  cancelled: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM5.6 5.6l12.8 12.8",
  priority: "M12 20V5M6 11l6-6 6 6",
  "priority-low": "M12 4v15M6 13l6 6 6-6",
  repeat: "M17 2l3 3-3 3M4 11V9a4 4 0 0 1 4-4h12M7 22l-3-3 3-3M20 13v2a4 4 0 0 1-4 4H4",
  "task-id": "M4 9h16M4 15h16M10 3L8 21M16 3l-2 18",
  "depends-on": "M9 15l6-6M11 6l1.5-1.5a4.2 4.2 0 0 1 6 6L17 12M13 18l-1.5 1.5a4.2 4.2 0 0 1-6-6L7 12",
  "on-completion": "M5 21V4M5 4h6l1 2h7v8h-6l-1-2H5",
  // A circle with a dot in it, as an in-progress task's box shows (not a half, which reads as how far along).
  "in-progress": "M12 3a9 9 0 1 1 0 18a9 9 0 1 1 0-18M12 11.2v1.6M11.2 12h1.6",
  send: "M22 2L11 13M22 2l-7 20-4-9-9-4z",
  stop: "M7 7h10v10H7z",
  speaker: "M4 9h3l4.5-4v14L7 15H4zM15.5 8.5a5 5 0 0 1 0 7M18.3 5.7a9 9 0 0 1 0 12.6",
  play: "M7 4.5v15l12-7.5z",
  pause: "M7 5h3.5v14H7zM13.5 5H17v14h-3.5z",
  "skip-back": "M18 6v12l-8.5-6zM6 6v12",
  "skip-forward": "M6 6v12l8.5-6zM18 6v12",
  cpu: "M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3M7 5h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2zM10 10h4v4h-4z",
  history: "M3 12a9 9 0 1 0 2.6-6.4L3 8M3 3v5h5M12 7v5l3 2",
  // Save a chat to the vault: a tray with an arrow into it.
  save: "M12 3v12M7 10l5 5 5-5M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4",
  // The Workspace: a terminal window with a prompt.
  terminal: "M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM6.5 9l3 3-3 3M12 15h5",
};

/** The same icon as markup, for DOM built outside React (the editor's widgets). */
export const iconSvg = (name: string, size = 16) =>
  `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${PATHS[name] ?? PATHS.note}"/></svg>`;

export function Icon({ name, size = 16, style, className }: { name: string; size?: number; style?: CSSProperties; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      style={{ flex: "none", ...style }}
      className={className}
    >
      <path d={PATHS[name] ?? PATHS.note} />
    </svg>
  );
}

/** An icon as a DOM element, for code outside React (the editor's widgets). */
export function iconElement(name: string, size = 16): SVGSVGElement {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  for (const [k, v] of Object.entries({
    width: String(size),
    height: String(size),
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    "stroke-width": "1.75",
    "stroke-linecap": "round",
    "stroke-linejoin": "round",
    "aria-hidden": "true",
  }))
    svg.setAttribute(k, v);
  const path = document.createElementNS(ns, "path");
  path.setAttribute("d", PATHS[name] ?? PATHS.note);
  svg.appendChild(path);
  return svg;
}

/** The house holding a brain: sidebar, splash (index.html) and app icon (design/mark.svg). */
export function Mark({ size = 26 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true" style={{ flex: "none" }}>
      <path d="M4 15 16 4.5 28 15v11a2.5 2.5 0 0 1-2.5 2.5h-19A2.5 2.5 0 0 1 4 26z" fill="var(--accent)" />
      <path d={BRAIN} fill="#fff" />
      <path d={FOLDS} stroke="var(--accent)" strokeWidth=".9" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/** The menu-bar icon (tools/make_icons.py's tray icon): the mark's brain, its folds cut out, in the
 *  text colour. For what's about the menu bar. */
export function TrayMark({ size = 16, style }: { size?: number; style?: CSSProperties }) {
  const id = `tray-${useId().replace(/:/g, "")}`;
  return (
    <svg width={size} height={size} viewBox="7.4 11.4 17.2 17.2" aria-hidden="true" style={{ flex: "none", ...style }}>
      <mask id={id}>
        <rect x="7.4" y="11.4" width="17.2" height="17.2" fill="#fff" />
        <path d={FOLDS} stroke="#000" strokeWidth="1.1" fill="none" strokeLinecap="round" />
      </mask>
      <path d={BRAIN} fill="currentColor" mask={`url(#${id})`} />
    </svg>
  );
}

const BRAIN =
  "M15.4 14.3c-.9-.7-2.4-.7-3.2.2-1.5-.2-2.8 1-2.6 2.5-1.2.6-1.6 2.1-.9 3.2-.6 1.2-.1 2.7 1.2 3.1.2 1.4 1.6 2.3 3 1.9.7.7 1.7.9 2.5.4V14.3zM16.6 14.3c.9-.7 2.4-.7 3.2.2 1.5-.2 2.8 1 2.6 2.5 1.2.6 1.6 2.1.9 3.2.6 1.2.1 2.7-1.2 3.1-.2 1.4-1.6 2.3-3 1.9-.7.7-1.7.9-2.5.4V14.3z";
const FOLDS =
  "M12.4 17.6c.7-.6 1.8-.4 2.1.4M11.2 20.4c.6.6 1.7.6 2.2 0 .4.7 1.3 1 2 .6M12.6 23.2c.3-.7 1.1-1 1.8-.7M19.6 17.6c-.7-.6-1.8-.4-2.1.4M20.8 20.4c-.6.6-1.7.6-2.2 0-.4.7-1.3 1-2 .6M19.4 23.2c-.3-.7-1.1-1-1.8-.7";
