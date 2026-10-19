// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Screenshot mode (tools/screenshots.py, tools/screenshots/scenes.json): BRAINSTEAD_SCENE holds a
// scene as JSON, and the window sets it up and saves nothing.

import { convertFileSrc } from "@tauri-apps/api/core";
import { focusMode } from "./focus";
import { imageView } from "./ImageViewer";
import { toast } from "./Toast";
import { UpdateView, updateView } from "./Update";
import { fixNameOpen } from "./FixName";
import { shortcutsOpen } from "./Shortcuts";
import { helpOpen, HelpView } from "./help/HelpDrawer";
import { readingScene } from "./speech/player";
import type { ClarifySuggestion, Theme, WeekPrepStatus } from "./api";
import { ask, blankChat } from "./askState";
import { nav, Screen, SettingsPane } from "./nav";
import { noteDialog } from "./notes/dialogs";
import { applyTheme, settings, Store } from "./store";

export interface Scene {
  name?: string;
  screen?: Screen;
  pane?: SettingsPane;
  theme?: Exclude<Theme, "system">;
  palette?: boolean;
  /** For "doc": the vault path to open; for "search" and "doc": the query. */
  path?: string;
  q?: string;
  /** Keep the splash up. */
  splash?: boolean;
  /** A note dialog to open: New note (optionally from `template`), or Rename `path` to `title`. */
  dialog?: "new-note" | "rename" | "fix-name" | "shortcuts";
  template?: string;
  title?: string;
  /** For "doc": live, source or read (the default, as the earlier screenshots were taken). */
  docMode?: "live" | "source" | "read";
  /** For "doc": the find bar open with these words. */
  find?: string;
  /** For "doc" in live or source: the caret put where `|` is in this text (as a click puts it),
   *  or with `hover` the mouse resting there instead. */
  caret?: string;
  hover?: boolean;
  /** For "doc": the Linked from pane collapsed. */
  sideFolded?: boolean;
  /** For "doc": in focus mode (⌘.). */
  focus?: boolean;
  /** For "doc": the note shown being read aloud at a block and spoken character, saying nothing. */
  reading?: { block: number; char: number };
  /** The menu-bar window alone (src/Tray.tsx), shown by Rust. */
  tray?: boolean;
  /** The help drawer open: on the screen's topic, or `topic`, a guide at a step, or a search. */
  help?: HelpView;
  /** Screens' remembered choices (`useViewState` keys), e.g. the selected task. */
  view?: Record<string, unknown>;
  /** For "ask": show a sample chat. */
  chat?: boolean;
  /** An image in the vault (its path) shown full size over the window. */
  image?: string;
  /** For "weekly": sample suggestions for the week, instead of preparing them. */
  weekprep?: boolean;
  /** For "inbox": a sample suggestion for the open item, as Suggest would make. */
  inboxSuggest?: boolean;
  /** A task's menu open, by its `path:line` (the task list's key). */
  taskMenu?: string;
  /** A toast to show, with an Undo button. */
  toast?: string;
  /** The update dialog: offering a sample version, downloading it, or saying it's up to date. */
  update?: "offer" | "downloading" | "message";
}

/** The Ask scene's chat: a question about the fixture vault and an answer with links. */
function sampleChat() {
  const c = blankChat("claude:sonnet");
  return {
    ...c,
    id: "scene",
    title: "Orbit App launch date",
    filename: "Chat. Orbit App launch date.md",
    contextTokens: 31000,
    contextWindow: 200000,
    transcript: [
      { id: "1", role: "user" as const, text: "What changed on the Orbit App since the 18 Sep roadmap update? Anything I need to act on?" },
      { id: "2", role: "tool" as const, text: "Searching the vault" },
      { id: "3", role: "tool" as const, text: "Reading Orbit App" },
      { id: "4", role: "tool" as const, text: "Reading Meeting. Orbit App Steerco - 2026-09-30" },
      {
        id: "5",
        role: "assistant" as const,
        text: "Three things moved since the roadmap update:\n\n- **The launch date is contested.** [[Orbit App]] still says 14 November, but the [[Meeting. Orbit App Steerco - 2026-09-30|30 Sep steerco]] moved it to 28 November.\n- **A new gating item:** the penetration test has to close by 30 October.\n- **Origination is in build**, with the servicing APIs to follow.\n\nYou have one open task on it: confirming the comms plan, which depends on the date.",
        meta: "claude:sonnet",
      },
      { id: "6", role: "user" as const, text: "And who owns the pen test?" },
    ],
    busy: true,
    streaming: "",
    status: "Reading wiki/entities/Orbit App",
    looked: ["Searching the vault"],
    queue: ["Draft three talking points for the steerco"],
  };
}

/** The Inbox scene's suggestion is on (`inboxSuggest`). */
export const sceneInboxSuggest = new Store(false);

/** The Inbox scene's suggestion, for the item `id`: the fixture's launch-date email, waited on. */
export const sampleSuggestion = (id: string): ClarifySuggestion => ({
  id,
  becomes: "waiting",
  text: "Theo to update the comms plan for the new launch date",
  project: "Orbit App launch",
  context: null,
  effort: null,
  due: "2026-11-07",
  why: "The steerco moved the launch, and Theo is updating the comms plan: nothing for you to do until he has.",
});

/** The weekly review's scene suggestions are on (`weekprep`). */
export const scenePrep = new Store(false);

/** The weekly scene's suggestions: prompts from the fixture vault's Steerco note. */
export function samplePrep(week: string): WeekPrepStatus {
  const steerco = "Meeting. Orbit App Steerco - 2026-09-30.md";
  const prompt = (id: string, step: "loose" | "creative", text: string, quote: string) => ({
    id,
    step,
    text,
    source: { path: steerco, quote },
    action: null,
  });
  return {
    running: false,
    error: null,
    prep: {
      week,
      preparedAt: `${new Date().toISOString().slice(0, 10)}T07:30:00`,
      model: "claude:sonnet",
      stamp: "scene",
      dropped: 0,
      suggestions: [
        prompt("s1", "loose", "Has the launch date been confirmed with Sam? It was due on 3 Oct.", "Confirm the launch date with Sam"),
        prompt(
          "s2",
          "loose",
          "Is Zara still to sign off the operating model, or has that happened?",
          "Zara to sign off the operating model",
        ),
        prompt(
          "s3",
          "loose",
          "Role-level layering for the tables was a follow-up: does it need a next action?",
          "Role-level layering for the tables",
        ),
        prompt("s4", "creative", "The learning budget has sat in Someday for a while: is now the time?", "Learning budget"),
      ],
    },
  };
}

export function parseScene(json: string | null): Scene | null {
  if (!json) return null;
  try {
    return JSON.parse(json) as Scene;
  } catch {
    return null;
  }
}

export function applyScene(sc: Scene) {
  document.documentElement.dataset.scene = sc.name ?? "1";
  if (sc.splash) document.documentElement.dataset.keepSplash = "1";
  document.documentElement.dataset.docMode = sc.docMode ?? "read";
  if (sc.find) document.documentElement.dataset.find = sc.find;
  if (sc.caret) document.documentElement.dataset.caret = sc.caret;
  if (sc.hover) document.documentElement.dataset.hover = "1";
  if (sc.taskMenu) document.documentElement.dataset.taskMenu = sc.taskMenu;
  settings.frozen = true;
  if (sc.theme) {
    settings.update({ theme: sc.theme });
    applyTheme(sc.theme);
  }
  if (sc.sideFolded) settings.update({ docSideFolded: true });
  if (sc.view) settings.update({ viewMemory: { ...settings.get().viewMemory, ...sc.view } });
  if (sc.screen) nav.replace({ screen: sc.screen, pane: sc.pane, path: sc.path, q: sc.q, view: sc.view });
  if (sc.focus) focusMode.set(true);
  if (sc.dialog === "new-note") noteDialog.set({ kind: "new", template: sc.template });
  if (sc.dialog === "rename" && sc.path) noteDialog.set({ kind: "rename", path: sc.path, title: sc.title });
  // Fix name: `q` the name as written, `title` the right one.
  if (sc.dialog === "shortcuts") shortcutsOpen.set(true);
  if (sc.dialog === "fix-name") fixNameOpen.set({ wrong: sc.q, right: sc.title });
  if (sc.weekprep) scenePrep.set(true);
  if (sc.inboxSuggest) sceneInboxSuggest.set(true);
  if (sc.image) imageView.set({ url: convertFileSrc(`${(settings.get().vaultPath ?? "").replace(/\/$/, "")}/${sc.image}`), alt: sc.image });
  if (sc.chat) ask.set({ chats: [sampleChat()], active: "scene" });
  if (sc.help) helpOpen.set(sc.help);
  if (sc.reading) readingScene.set(sc.reading);
  if (sc.toast) toast(sc.toast, { label: "Undo", run: () => {} });
  if (sc.update) updateView.set(sampleUpdate(sc.update));
}

/** The update scenes' dialog: a made-up next version with notes as tools/update_files.py writes them. */
function sampleUpdate(kind: "offer" | "downloading" | "message"): UpdateView {
  if (kind === "message") return { kind, title: "You're up to date", text: "Brainstead 1.1.3 is the newest version." };
  const offer = {
    name: "Brainstead",
    version: "1.2.0",
    current: "1.1.3",
    date: "2026-10-09T08:00:00Z",
    notes:
      "- **Weekly review** remembers where you stopped.\n- Ask quotes its sources in the answer.\n- Search finds words inside PDFs in `sources/`.\n- Fixed: a renamed note kept its old title in the sidebar.",
  };
  return {
    kind: "offer",
    offer,
    progress: kind === "downloading" ? { phase: "downloading", downloaded: 18_350_080, total: 41_943_040 } : null,
  };
}
