// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import React from "react";
import ReactDOM from "react-dom/client";
import { api } from "./api";
import App from "./App";
import { CaptureWindow } from "./Capture";
import { TrayWindow } from "./Tray";
import { listen } from "@tauri-apps/api/event";
import { toast, Toasts } from "./Toast";
import { startAsk } from "./askState";
import { ingest } from "./Ingest";
import { nav, openDoc, restorePlaces, Screen, SettingsPane } from "./nav";
import { applyScene, parseScene } from "./scene";
import { recheckPermissions, startStatus } from "./state";
import { applyReadSize, applyTheme, installFlushers, settings } from "./store";
import { startMoves } from "./moves";
import { isTranscriptPath } from "./meetingFlow";
import { loadFeatures } from "./features";
import { applyPlatform, blockBrowserKeys } from "./platform";
import { applyInterfaceSize } from "./interfaceSize";

// WebKit's own right-click menu offers Reload, which throws away the app's state. Keep it only
// where it's useful — in text fields and on selected text — and leave custom menus to their owners.
document.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement | null;
  if (t?.closest("input, textarea, [contenteditable]:not([contenteditable='false'])")) return;
  if (window.getSelection()?.toString()) return;
  e.preventDefault();
});

async function start() {
  // The stylesheet reads data-platform; the backend's own report of the platform wins once it arrives.
  applyPlatform();
  blockBrowserKeys();
  const [s, sceneJson] = await Promise.all([api.settingsRead(), api.scene().catch(() => null), loadFeatures()]);
  settings.loadFrom(s);
  applyTheme(s.theme);
  applyReadSize(s.readSize);
  installFlushers();
  const scene = parseScene(sceneJson);
  // A note that's gone since (renamed or deleted elsewhere) opens Today instead of an error.
  const back = scene ? undefined : restorePlaces(s.places);
  if (back?.screen === "doc" && back.path) api.docRead(back.path).catch(() => nav.replace({ screen: "today" }));
  if (scene) applyScene(scene);
  const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
  // The quick-capture window (src-tauri/src/capture.rs) runs the same page with ?view=capture.
  if (new URLSearchParams(location.search).get("view") === "capture") {
    document.documentElement.classList.add("capture-view");
    document.getElementById("splash")?.remove();
    root.render(
      <React.StrictMode>
        <CaptureWindow />
        <Toasts />
      </React.StrictMode>,
    );
    return;
  }
  // The menu-bar window (src-tauri/src/tray.rs) runs the same page with ?view=tray.
  if (new URLSearchParams(location.search).get("view") === "tray") {
    document.documentElement.classList.add("tray-view");
    document.getElementById("splash")?.remove();
    root.render(
      <React.StrictMode>
        <TrayWindow />
      </React.StrictMode>,
    );
    return;
  }
  // The main window alone (the two above are sized to their content), before anything is drawn.
  await applyInterfaceSize(s.interfaceSize);
  await Promise.all([startStatus(), recheckPermissions(), startAsk().catch((e) => console.error("ask", e))]);
  // A note renamed or trashed by an assistant, a run or Changes: the screens follow it.
  startMoves();
  // The menu-bar window asks for a screen when it opens the main window; "settings/vault" is a Settings pane.
  void listen<string>("navigate", (e) => {
    const [screen, pane] = e.payload.split("/");
    nav.go(pane ? { screen: screen as Screen, pane: pane as SettingsPane } : (screen as Screen));
  });
  // A capture from the Outlook or Teams extension (src-tauri/src/nativehost.rs).
  void api.onCapture((c) => {
    if (!c.path) return toast(`${c.what} not captured: ${c.error ?? "refused"}`, undefined, "bad");
    toast(`${c.what} captured`, { label: "Open", run: () => openDoc(c.path!) });
    // Settings › AI assistants › Ingest: a new source is ingested as it arrives.
    // A transcript waits for its meeting note instead (src/meetingFlow.ts). A capture the Teams and
    // Outlook sync made (c.sync) is ingested unattended, so a change that fails a check is held.
    if (settings.get().ingestOnArrival && !isTranscriptPath(c.path)) ingest([c.path], c.sync === true);
  });
  root.render(
    <React.StrictMode>
      <App scene={scene} />
    </React.StrictMode>,
  );
}

start().catch((e) => {
  console.error(e);
  document.getElementById("splash")?.remove();
  document.getElementById("root")!.textContent = `Brainstead couldn’t start: ${e}`;
});
