// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useState } from "react";
import { api } from "./api";
import { ActivityScreen } from "./Activity";
import { ErrorBoundary, ScreenError } from "./ErrorBoundary";
import { DocCheckScreen } from "./skills/DocCheck";
import { CompareScreen } from "./skills/Compare";
import { ReplyScreen } from "./skills/Reply";
import { TriageScreen } from "./skills/Triage";
import { AskScreen } from "./Ask";
import { DocScreen } from "./Doc";
import { InboxScreen } from "./Inbox";
import { ProjectsScreen } from "./Projects";
import { WeeklyScreen } from "./Weekly";
import { RevertConflict, ReviewScreen } from "./Review";
import { FollowUp } from "./Ingest";
import { FixNameHost } from "./FixName";
import { ImageViewerHost } from "./ImageViewer";
import { ShortcutsHost } from "./Shortcuts";
import { QuitDialog } from "./Quit";
import { GuideOfferHost } from "./help/GuideOffer";
import { HelpDrawer, startHelpMenu } from "./help/HelpDrawer";
import { startMcpBridge } from "./mcpActions";
import { startSpeech } from "./speech/player";
import { ContradictionsScreen } from "./Contradictions";
import { MeetingScreen } from "./Meeting";
import { GraphScreen } from "./Graph";
import { HealthScreen } from "./Health";
import { NotesScreen, SourcesScreen, TemplatesScreen, WikiScreen } from "./Lists";
import { QueryBuilderHost } from "./editor/QueryBuilder";
import { SpellMenuHost } from "./editor/SpellMenu";
import { startNoteMenu } from "./noteMenu";
import { startInterfaceSizeMenu } from "./interfaceSize";
import { NoteDialogs } from "./notes/NoteDialogs";
import { TrashScreen } from "./notes/Trash";
import { nav, place, Screen, whenFree } from "./nav";
import { Palette } from "./Palette";
import { Welcome } from "./Permissions";
import { Scene } from "./scene";
import { Placeholder } from "./Screens";
import { SearchScreen } from "./Search";
import { TasksScreen } from "./Tasks";
import { undoLast } from "./taskModel";
import { TodayScreen } from "./Today";
import { SettingsScreen } from "./Settings";
import { HOTKEYS, Sidebar } from "./Sidebar";
import { focusMode, startFocus } from "./focus";
import { hideSplash } from "./splash";
import { permissions } from "./state";
import { settings, useStore } from "./store";
import { Toasts } from "./Toast";
import { startUpdates, UpdateHost, updateView } from "./Update";
import { Tooltips } from "./ui";
import { isMod, strayCtrl } from "./platform";

const SCREENS: Partial<Record<Screen, () => React.ReactElement>> = {
  settings: SettingsScreen,
  doc: DocScreen,
  notes: NotesScreen,
  wiki: WikiScreen,
  sources: SourcesScreen,
  templates: TemplatesScreen,
  search: SearchScreen,
  today: TodayScreen,
  tasks: TasksScreen,
  trash: TrashScreen,
  ask: AskScreen,
  inbox: InboxScreen,
  projects: ProjectsScreen,
  weekly: WeeklyScreen,
  review: ReviewScreen,
  health: HealthScreen,
  contradictions: ContradictionsScreen,
  meeting: MeetingScreen,
  graph: GraphScreen,
  activity: ActivityScreen,
  triage: TriageScreen,
  reply: ReplyScreen,
  doccheck: DocCheckScreen,
  compare: CompareScreen,
};

/** Whether the shell can show yet: a vault is chosen, and Full Disk Access is granted or was declined. */
export function ready(s: { vaultPath: string | null; skippedFullDiskAccess: boolean }, fda: boolean | null | undefined, applies: boolean) {
  if (applies && fda === false && !s.skippedFullDiskAccess) return false;
  return !!s.vaultPath;
}

export default function App({ scene }: { scene: Scene | null }) {
  const s = useStore(settings);
  const p = useStore(permissions);
  const { place: here } = useStore(place);
  const [palette, setPalette] = useState(!!scene?.palette);
  const focused = useStore(focusMode) && here.screen === "doc";
  useEffect(() => {
    return startFocus();
  }, []);

  useEffect(() => {
    if (p) hideSplash();
  }, [p]);

  // ⌘Q is asked in src/Quit.tsx. Restarting into an update (src-tauri/src/updates.rs) asks about
  // unsaved edits as it does, and saves the settings first.
  useEffect(() => {
    const off = [
      api.onRestartRequested(() => {
        let went = false;
        whenFree(() => {
          went = true;
          void settings.flush().then(() => api.appRestart());
        });
        // Unsaved edits are asked about first: the update dialog steps aside for the question.
        if (!went) updateView.set(null);
      }),
    ].map((o) => o.catch(() => () => {}));
    return () => off.forEach((o) => void o.then((f) => f()));
  }, []);

  // The update dialog: offers from src-tauri/src/updates.rs and what follows them.
  useEffect(() => {
    return startUpdates();
  }, []);

  // Read aloud: the synthesiser's events, the media keys and the voices (src/speech/player.ts).
  useEffect(() => {
    return startSpeech();
  }, []);

  // Actions from the MCP server, through the running app (src/mcpActions.ts).
  useEffect(() => {
    const off = startMcpBridge();
    return () => void off.then((f) => f());
  }, []);

  // The menu bar's Note, View and Help menus.
  useEffect(() => {
    const off = [startNoteMenu(), startInterfaceSizeMenu(), startHelpMenu()];
    return () => off.forEach((o) => void o.then((f) => f()));
  }, []);

  // ⌘K palette, ⌘⇧F Search, ⌘, Settings, ⌘[ ⌘] back and forward.
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      // ⌘1 to ⌘9, then ⌘0 (Ctrl+digit on Windows): the sidebar's first ten rows (⌥⌘1 to ⌥⌘3 are a
      // note's View, Edit and Source).
      const digit = /^Digit([0-9])$/.exec(e.code);
      if (isMod(e) && !e.altKey && !strayCtrl(e) && !e.shiftKey && digit) {
        nav.go(HOTKEYS[(Number(digit[1]) + 9) % 10]);
        e.preventDefault();
        return;
      }
      if (!isMod(e) || e.altKey || strayCtrl(e)) return;
      const inField = (e.target as HTMLElement)?.closest?.("input, textarea, [contenteditable]");
      if (e.key === "z" && !e.shiftKey && !inField) {
        void undoLast();
        e.preventDefault();
      } else if (e.shiftKey && e.key.toLowerCase() === "f") {
        nav.go("search");
        e.preventDefault();
      } else if (e.key === "k") {
        setPalette((x) => !x);
        e.preventDefault();
      } else if (e.key === ",") {
        nav.go("settings");
        e.preventDefault();
      } else if (e.key === "[") {
        nav.back();
        e.preventDefault();
      } else if (e.key === "]") {
        nav.forward();
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, []);

  if (!p) return null;
  if (!ready(s, p.fullDiskAccess, p.applies)) {
    return (
      <>
        <Welcome />
        <Tooltips />
      </>
    );
  }
  return (
    <div className={`shell${focused ? " focus" : ""}`}>
      {!focused && <Sidebar onSearch={() => setPalette(true)} />}
      {(() => {
        const S = SCREENS[here.screen];
        return (
          <ErrorBoundary
            resetKey={`${here.screen}\u0000${here.path ?? ""}`}
            fallback={(error, retry) => <ScreenError error={error} retry={retry} />}
          >
            {S ? <S key={here.screen} /> : <Placeholder screen={here.screen} />}
          </ErrorBoundary>
        );
      })()}
      {palette && <Palette onClose={() => setPalette(false)} />}
      <NoteDialogs />
      <FixNameHost />
      <ShortcutsHost />
      <ImageViewerHost />
      <HelpDrawer />
      <GuideOfferHost />
      <QueryBuilderHost />
      <SpellMenuHost />
      <Tooltips />
      <FollowUp />
      <RevertConflict />
      <UpdateHost />
      <QuitDialog />
      <Toasts />
    </div>
  );
}
