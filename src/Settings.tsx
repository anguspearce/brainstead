// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { Fragment, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, AppInfo, CliInfo, CliName, Skill, Theme } from "./api";
import { extensionOutdatedText, rejectedText } from "./syncStatus";
import appIconDark from "../src-tauri/icons/128x128@2x.png";
import appIconLight from "../src-tauri/icons/about-light.png";
import { Icon, TrayMark } from "./icons";
import { nav, place, SettingsPane } from "./nav";
import { chooseVault, FullDiskAccessCard, useRecheckOnFocus } from "./Permissions";
import { lastSeen, useVaultVersion, vaultStatus } from "./state";
import { LookPicker } from "./LookPicker";
import { DEFAULT_SCRIPTS, TemplaterDocsLink } from "./notes/TemplateRun";
import { useDocDefaults, useDocLook } from "./docLook";
import { Markdown } from "./md/Markdown";
import { applyTheme, settings, settingsError, useStore } from "./store";
import { INTERFACE_SIZE, clampInterfaceSize, setInterfaceSize } from "./interfaceSize";
import { toast } from "./Toast";
import { errText } from "./notes/actions";
import { Switchover } from "./Switchover";
import { TopBar } from "./TopBar";
import { Jobs } from "./Jobs";
import { CLI_LABEL, clis, DEFAULT_MODEL, findClis, MODEL_JOBS, modelLabel } from "./askState";
import { ModelMenu } from "./Ask";
import { ago, fmtCount, Popover, Seg, Switch } from "./ui";
import { sayTest, speechSettingsChanged, voices } from "./speech/player";
import { SpeedSlider, VoiceSelect } from "./speech/ReadAloud";
import { DocButton, useIsGone } from "./docButton";
import { accelLabel, captureDefault, fileManager, hasSync, isMac, isWindows, keyLabel, terminalApp } from "./platform";
import { useFeatures } from "./features";

const PANES: [SettingsPane, string, string][] = [
  ["general", "General", "settings"],
  ["notes", "Notes", "note"],
  ["vault", "Vault", "folder"],
  ["assistants", "AI assistants", "cpu"],
  ["jobs", "Jobs & schedule", "calendar"],
  ["capture", "Capture extensions", "source"],
  ["sync", "Teams and Outlook sync", "refresh"],
  ["permissions", "Permissions", "shield"],
  ["about", "About", "info"],
];

export function SettingsScreen() {
  const p = useStore(place).place;
  const features = useFeatures();
  const panes = PANES.filter(([id]) => (id !== "permissions" || features.fullDiskAccess) && (id !== "sync" || hasSync));
  const pane = p.pane === "permissions" && !features.fullDiskAccess ? "general" : (p.pane ?? "general");
  const title = PANES.find((x) => x[0] === pane)?.[1] ?? "Settings";
  return (
    <main className="main">
      <TopBar title="Settings" sub={title} ask={{ about: `Settings › ${title}`, prompt: `About Settings › ${title}: ` }} />
      <div className="body settings">
        <nav className="spanes" aria-label="Settings">
          {panes.map(([id, label, icon]) => (
            <button
              key={id}
              type="button"
              className={`navi ${id === pane ? "on" : ""}`}
              aria-current={id === pane ? "page" : undefined}
              title={`${label} settings`}
              onClick={() => nav.go({ screen: "settings", pane: id })}
            >
              <Icon name={icon} />
              {label}
            </button>
          ))}
        </nav>
        <div className="spane">
          {pane === "general" && <General />}
          {pane === "notes" && <Notes />}
          {pane === "vault" && <Vault />}
          {pane === "assistants" && <Assistants />}
          {pane === "jobs" && <Jobs />}
          {pane === "capture" && <CapturePane />}
          {pane === "sync" && <SyncPane />}
          {pane === "permissions" && <PermissionsPane />}
          {pane === "about" && <About />}
        </div>
      </div>
    </main>
  );
}

/** "Control+Alt+Space" as the platform writes it: ⌃⌥Space on macOS, Ctrl+Alt+Space elsewhere. */
export function prettyShortcut(spec: string): string {
  return accelLabel(spec);
}

/** The shortcut a key press makes, in the plugin's terms ("Control+Alt+Space"), or null for a bare key. */
export function shortcutFromEvent(e: {
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  code: string;
}): string | null {
  const mods = [e.ctrlKey && "Control", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean) as string[];
  if (!mods.length || /^(Control|Alt|Shift|Meta)/.test(e.code)) return null;
  const key = e.code.replace(/^Key/, "").replace(/^Digit/, "");
  return [...mods, key].join("+");
}

function CaptureShortcut() {
  const s = useStore(settings);
  const [status, setStatus] = useState<[string | null, string | null]>([null, null]);
  const [recording, setRecording] = useState(false);
  const spec = s.captureShortcut ?? captureDefault();
  useEffect(() => {
    let live = true;
    // The shortcut is registered when settings are saved, so ask a moment later.
    const t = window.setTimeout(() => {
      api
        .captureShortcutStatus()
        .then((x) => live && setStatus(x))
        .catch(() => {});
    }, 600);
    return () => {
      live = false;
      window.clearTimeout(t);
    };
  }, [spec]);
  return (
    <section className="sgroup">
      <h2 className="h3">Quick capture</h2>
      <p className="muted">
        A shortcut that works over any app and opens a capture box. Tasks go to the To Do list, thoughts to the Scratchpad.
      </p>
      <div className="row">
        <button
          type="button"
          className={`btn ${recording ? "pri" : ""}`}
          title={
            recording
              ? "Press the keys for the new shortcut; click elsewhere to keep the old one"
              : "Click, then press a new shortcut for quick capture"
          }
          onClick={() => setRecording(true)}
          onBlur={() => setRecording(false)}
          onKeyDown={(e) => {
            if (!recording) return;
            e.preventDefault();
            if (e.key === "Escape") return setRecording(false);
            const sc = shortcutFromEvent(e);
            if (sc) {
              settings.update({ captureShortcut: sc });
              setRecording(false);
            }
          }}
        >
          {recording ? "Press the new shortcut…" : prettyShortcut(spec)}
        </button>
        {spec !== captureDefault() && (
          <button
            type="button"
            className="btn ghost sm"
            title={`Put the quick capture shortcut back to ${accelLabel(captureDefault())}`}
            onClick={() => settings.update({ captureShortcut: captureDefault() })}
          >
            Reset to {accelLabel(captureDefault())}
          </button>
        )}
      </div>
      {status[1] && <p className="err">{status[1]}</p>}
    </section>
  );
}

/** A model for each background job, or the default (Settings › AI assistants). */
function JobModels({ found, fallback }: { found: CliInfo[] | null; fallback: string }) {
  const s = useStore(settings);
  const [at, setAt] = useState<{ job: string; rect: DOMRect } | null>(null);
  const models = s.jobModels ?? {};
  // The weekly review's preparation falls back to the summaries' model (src-tauri/src/weekprep.rs).
  const fallbackFor = (job: string) => (job === "weekprep" && models.summaries ? models.summaries : fallback);
  const fallbackName = (job: string) => (job === "weekprep" && models.summaries ? "As the summaries" : "As new chats");
  return (
    <section className="sgroup">
      <h2 className="h3">Models by job</h2>
      <div className="card">
        {MODEL_JOBS.map(([job, label, icon]) => (
          <div key={job} className="srow">
            <Icon name={icon} />
            <div className="t">
              <div className="pt">{label}</div>
            </div>
            <button
              type="button"
              className="btn sm"
              title={`Choose the model for ${label.toLowerCase()}`}
              onClick={(e) => setAt({ job, rect: e.currentTarget.getBoundingClientRect() })}
            >
              {models[job] ? (
                modelLabel(models[job], found)
              ) : (
                <span className="faint">
                  {fallbackName(job)} ({modelLabel(fallbackFor(job), found)})
                </span>
              )}
              <Icon name="chevdown" size={12} />
            </button>
          </div>
        ))}
      </div>
      {at && (
        <Popover anchor={at.rect} onClose={() => setAt(null)} width={300}>
          {models[at.job] && (
            <div className="menu">
              <button
                type="button"
                title={`Use the same model as ${fallbackName(at.job).replace(/^As /, "")} for this job`}
                onClick={() => {
                  const { [at.job]: _, ...rest } = models;
                  settings.update({ jobModels: rest });
                  setAt(null);
                }}
              >
                <Icon name="refresh" size={14} />
                {fallbackName(at.job)}
              </button>
              <div className="sep" />
            </div>
          )}
          <ModelMenu
            found={found}
            value={models[at.job] ?? fallbackFor(at.job)}
            onPick={(m) => {
              settings.update({ jobModels: { ...models, [at.job]: m } });
              setAt(null);
            }}
          />
        </Popover>
      )}
    </section>
  );
}

/** Open at login (macOS's login item for the app; not shown where macOS can't say, as for a build
 *  run from a folder), and the menu-bar icon (src-tauri/src/tray.rs): shown, and whether closing
 *  the window leaves Brainstead in the menu bar alone. */
function OpenAtLogin() {
  const s = useStore(settings);
  const [on, setOn] = useState<boolean | null>(null);
  useEffect(() => {
    // Read again when the window comes back: System Settings › Login Items can change it too.
    const load = () =>
      void api
        .loginItem()
        .then(setOn)
        .catch(() => {});
    load();
    window.addEventListener("focus", load);
    const off = api.onLoginItemChanged(setOn);
    return () => {
      window.removeEventListener("focus", load);
      void off.then((f) => f());
    };
  }, []);
  const menuBar = s.menuBar !== false;
  return (
    <section className="sgroup">
      <div className="card">
        <div className="srow">
          <TrayMark />
          <div className="t">
            <div className="pt">Show in the menu bar</div>
            <div className="faint">
              An icon in the menu bar opens a small window: today at a glance, a capture box, what's running and what waits for you. It
              shows dots while a run is going, and a dot when something waits.
            </div>
          </div>
          <Switch label="Show in the menu bar" on={menuBar} onChange={(v) => settings.update({ menuBar: v })} />
        </div>
        {menuBar && isMac && (
          <div className="srow">
            <Icon name="eye" />
            <div className="t">
              <div className="pt">Only in the menu bar when the window is closed</div>
              <div className="faint">Closing the window takes Brainstead out of the Dock; the menu-bar icon brings it back.</div>
            </div>
            <Switch
              label="Only in the menu bar when the window is closed"
              on={!!s.menuBarOnly}
              onChange={(v) => settings.update({ menuBarOnly: v })}
            />
          </div>
        )}
        <div className="srow">
          <Icon name="start" />
          <div className="t">
            <div className="pt">Offer Getting started when Brainstead opens</div>
            <div className="faint">
              A welcome that opens the help's step-by-step guides. They're always in the help: press ? and click Getting started.
            </div>
          </div>
          <Switch
            label="Offer Getting started when Brainstead opens"
            on={s.offerGuide !== false}
            onChange={(v) => settings.update({ offerGuide: v })}
          />
        </div>
        {on !== null && (
          <div className="srow">
            <Icon name="power" />
            <div className="t">
              <div className="pt">Open at login</div>
              <div className="faint">
                Brainstead starts when you log in, so quick capture, captures from the browser, the daily summary and the weekly summary are
                ready.
              </div>
            </div>
            <Switch
              label="Open at login"
              on={on}
              onChange={(v) =>
                void api
                  .loginItemSet(v)
                  .then(setOn)
                  .catch((e) => toast(String(e), undefined, "bad"))
              }
            />
          </div>
        )}
      </div>
    </section>
  );
}

function General() {
  const s = useStore(settings);
  return (
    <>
      <Switchover />
      <OpenAtLogin />
      <CaptureShortcut />
      <section className="sgroup">
        <h2 className="h3">Your name</h2>
        <label className="inp" style={{ maxWidth: 320 }}>
          <input
            value={s.ownerName ?? ""}
            onChange={(e) => settings.update({ ownerName: e.target.value })}
            placeholder="First name, as Teams writes it"
          />
        </label>
        <p className="faint">
          Today greets you by your first name. For notes from transcripts: which speaker is you, so your actions become tasks and a
          two-person meeting is a 1-1.
        </p>
      </section>
      <section className="sgroup">
        <h2 className="h3">Appearance</h2>
        <Seg<Theme>
          label="Appearance"
          value={s.theme}
          options={[
            ["system", "System"],
            ["light", "Light"],
            ["dark", "Dark"],
          ]}
          onChange={(t) => {
            settings.update({ theme: t, docTheme: undefined });
            applyTheme(t);
          }}
        />
        <InterfaceSize />
      </section>
    </>
  );
}

/** Interface size: the whole window at a percentage (src/interfaceSize.ts), applied as the slider moves. */
function InterfaceSize() {
  const size = clampInterfaceSize(useStore(settings).interfaceSize);
  return (
    <div className="ifacesize">
      <div className="eyebrow">Interface size · {size}%</div>
      <div className="row">
        <input
          type="range"
          min={INTERFACE_SIZE.min}
          max={INTERFACE_SIZE.max}
          step={INTERFACE_SIZE.step}
          value={size}
          onChange={(e) => void setInterfaceSize(+e.target.value)}
          aria-label="Interface size"
          style={{ flex: 1 }}
        />
        <span
          title={
            size === INTERFACE_SIZE.default
              ? "The interface is already the usual size"
              : keyLabel("Set the interface back to the usual size (⇧⌘0)")
          }
        >
          <button
            type="button"
            className="btn sm"
            disabled={size === INTERFACE_SIZE.default}
            onClick={() => void setInterfaceSize(INTERFACE_SIZE.default)}
          >
            Reset to {INTERFACE_SIZE.default}%
          </button>
        </span>
      </div>
      <p className="faint">
        Everything in the window, from the sidebar to the text, bigger or smaller together. {keyLabel("⇧⌘=")} and {keyLabel("⇧⌘-")} change
        it anywhere and {keyLabel("⇧⌘0")} puts it back. A note's Text size comes on top of it.
      </p>
    </div>
  );
}

const SAMPLE = `# Quarterly review

A short sample of how documents look, with a [[link]] and some \`code\`.

## Decisions

- [x] Ship the editor
- [ ] Plan the next stage

> Keep the vault the only source of truth.

| Area | Owner | Status |
|---|---|---|
| Editor | Maya | Done |
| Ask | Lena | Next |
`;

/** "en_ZA" as "English (South Africa)". */
export function languageName(code: string): string {
  try {
    return new Intl.DisplayNames(["en"], { type: "language" }).of(code.replace("_", "-")) ?? code;
  } catch {
    return code;
  }
}

/** Which English to check in: one macOS has, or as macOS is set. */
function SpellLanguage() {
  const s = useStore(settings);
  const [langs, setLangs] = useState<[string[], string] | null>(null);
  useEffect(() => {
    void api
      .spellLanguages()
      .then(setLangs)
      .catch(() => {});
  }, []);
  if (!langs || !langs[0].length) return null;
  const [have, system] = langs;
  return (
    <div className="srow">
      <Icon name="text" />
      <div className="t">
        <div className="pt">Spelling language</div>
        <div className="faint">Which English words are checked against: colour or color, organise or organize.</div>
      </div>
      <select
        title="The language spelling is checked in"
        className="sel"
        aria-label="Spelling language"
        value={s.spellLanguage && have.includes(s.spellLanguage) ? s.spellLanguage : ""}
        onChange={(e) => settings.update({ spellLanguage: e.target.value || undefined })}
      >
        <option value="">{/^en(_|$)/.test(system) ? `As ${isMac ? "macOS" : "the system"} (${languageName(system)})` : "English"}</option>
        {have.map((l) => (
          <option key={l} value={l}>
            {languageName(l)}
          </option>
        ))}
      </select>
    </div>
  );
}

/** Read aloud (src/speech/): the voice, the speed and the highlight, with where better voices come
 *  from. Hidden where the system has no voices to offer. */
function ReadAloudSettings() {
  const s = useStore(settings);
  const vs = useStore(voices);
  if (!vs.length) return null;
  return (
    <section className="sgroup">
      <h2 className="h3">Read aloud</h2>
      <div className="card">
        <div className="srow">
          <Icon name="speaker" />
          <div className="t">
            <div className="pt">Voice</div>
            <div className="faint">
              {isMac
                ? "More voices: System Settings › Accessibility › Spoken Content › System voice › Manage Voices. Premium voices sound the most natural (Zoe or Ava for US English, Jamie for British); download one there and it's listed here. Siri's voices can't be used by other apps."
                : isWindows
                  ? "More voices: Windows Settings › Time & language › Speech › Manage voices; add one there and it's listed here. Natural voices, where Windows has them, are listed first."
                  : "More voices: install one in the system's speech settings and it's listed here."}
            </div>
          </div>
          <VoiceSelect onChange={speechSettingsChanged} />
          <button
            type="button"
            className="btn sm"
            title="Say a sentence in this voice and speed"
            onClick={() => sayTest("This is how your notes will sound when Brainstead reads them aloud.")}
          >
            Test
          </button>
        </div>
        <div className="srow">
          <Icon name="stopwatch" />
          <div className="t">
            <div className="pt">Speed</div>
          </div>
          <SpeedSlider />
        </div>
        <div className="srow">
          <Icon name="eye" />
          <div className="t">
            <div className="pt">Highlight each word</div>
            <div className="faint">Off: the whole paragraph being read is tinted instead.</div>
          </div>
          <Switch label="Highlight each word" on={s.speechHighlight !== false} onChange={(v) => settings.update({ speechHighlight: v })} />
        </div>
      </div>
    </section>
  );
}

function Notes() {
  const s = useStore(settings);
  const d = useDocDefaults();
  const features = useFeatures();
  const look = useDocLook("");
  const own = Object.keys(s.docLooks ?? {}).length;
  return (
    <>
      {(features.spelling || features.grammar) && (
        <section className="sgroup">
          <h2 className="h3">Spelling and grammar</h2>
          <div className="card">
            {features.spelling && (
              <div className="srow">
                <Icon name="text" />
                <div className="t">
                  <div className="pt">Check spelling</div>
                  <div className="faint">
                    A red wavy line under words {isMac ? "macOS" : "the system"} doesn't know, as you write. Click one for guesses, Learn or
                    Ignore.
                  </div>
                </div>
                <Switch label="Check spelling" on={s.spellCheck !== false} onChange={(v) => settings.update({ spellCheck: v })} />
              </div>
            )}
            {features.grammar && (
              <div className="srow">
                <Icon name="check" />
                <div className="t">
                  <div className="pt">Check grammar</div>
                  <div className="faint">
                    A blue wavy line where {isMac ? "macOS" : "the system"} sees a grammar problem; click for its explanation and fixes.
                  </div>
                </div>
                <Switch label="Check grammar" on={s.grammarCheck !== false} onChange={(v) => settings.update({ grammarCheck: v })} />
              </div>
            )}
            {features.spelling && <SpellLanguage />}
          </div>
        </section>
      )}
      <ReadAloudSettings />
      <section className="sgroup">
        <h2 className="h3">Document look</h2>
        <p className="muted">How notes, wiki pages and their PDFs look unless a note has its own. Change one note's from its screen.</p>
        <LookPicker
          style={d.style}
          accent={d.accent}
          onStyle={(x) => settings.update({ docStyle: x })}
          onAccent={(x) => settings.update({ docAccent: x })}
        />
        <div
          className={`doc docsurf lookdemo ${d.style === "brainstead" ? "" : "styled"} ${look.ownTheme ? "owntheme" : ""}`}
          data-doc-theme={look.theme}
          data-doc-style={d.style}
          data-doc-accent={d.accent}
        >
          <Markdown content={SAMPLE} path="" root="" />
        </div>
      </section>
      <section className="sgroup">
        <h2 className="h3">Light or dark</h2>
        <Seg<"app" | "light" | "dark">
          label="Documents"
          value={s.docTheme ?? "app"}
          options={[
            ["app", "As the app"],
            ["light", "Light"],
            ["dark", "Dark"],
          ]}
          onChange={(t) => settings.update({ docTheme: t === "app" ? undefined : t })}
        />
        <p className="muted">PDFs always print light.</p>
      </section>
      <section className="sgroup">
        <h2 className="h3">Templates</h2>
        <p className="muted">
          New note runs the templates in the vault’s Templates folder (Templater syntax). Their user scripts (
          <span className="mono">tp.user</span>) are the .js files in this folder.
        </p>
        <div className="row">
          <span className="inp grow mono">
            <Icon name="folder" size={14} />
            <input
              defaultValue={s.templateScripts || DEFAULT_SCRIPTS}
              aria-label="User scripts folder"
              spellCheck={false}
              onBlur={(e) => settings.update({ templateScripts: scriptsFolder(e.target.value) })}
            />
          </span>
          <TemplaterDocsLink className="btn" />
        </div>
      </section>
      {own > 0 && (
        <section className="sgroup">
          <h2 className="h3">Notes with their own look</h2>
          <div className="row">
            <span className="muted">
              {fmtCount(own)} {own === 1 ? "note has" : "notes have"} a theme or colour of their own.
            </span>
            <button
              type="button"
              className="btn sm"
              title="Forget each note's own theme and colour, so every note uses the defaults"
              onClick={() => settings.update({ docLooks: {} })}
            >
              Use the defaults for all
            </button>
          </div>
        </section>
      )}
    </>
  );
}

function Vault() {
  const s = useStore(settings);
  const st = useStore(vaultStatus);
  const err = useStore(settingsError);
  const [name, setName] = useState("");
  const add = () => {
    const n = name.trim();
    if (!n) return;
    settingsError.set(null);
    settings.update({ excluded: [...s.excluded, n] });
    setName("");
  };
  return (
    <>
      <section className="sgroup">
        <h2 className="h3">Vault folder</h2>
        <p className="muted">Every .md and .txt file in this folder is indexed, except system and excluded folders.</p>
        <div className="row">
          <div className="inp grow mono" title={s.vaultPath ?? undefined}>
            <Icon name="folder" size={14} />
            <span className="ell">{s.vaultPath ?? "None chosen"}</span>
          </div>
          <button type="button" className="btn" title="Choose the vault folder Brainstead reads" onClick={() => void chooseVault()}>
            Choose…
          </button>
          <span title={s.vaultPath ? `Show the vault folder in ${fileManager}` : "Choose a vault folder first"}>
            <button
              type="button"
              className="ibtn"
              aria-label={`Show in ${fileManager}`}
              disabled={!s.vaultPath}
              onClick={() => void api.reveal("").catch(() => {})}
            >
              <Icon name="finder" />
            </button>
          </span>
        </div>
      </section>
      <div className="card">
        <div className="srow">
          <Icon
            name={st.state === "error" ? "x" : "check"}
            style={{ color: st.state === "error" ? "var(--red)" : st.state === "ready" ? "var(--green)" : "var(--amber)" }}
          />
          <div className="t">
            <div className="pt">
              {st.state === "ready"
                ? `${fmtCount(st.stats.files)} files indexed`
                : st.state === "indexing"
                  ? "Indexing…"
                  : st.state === "error"
                    ? "Index failed"
                    : "No vault"}
            </div>
            <div className="faint">
              {st.state === "error"
                ? st.error
                : st.state === "ready"
                  ? // Each count keeps to one line, so "2 min ago" never breaks over two.
                    [
                      counted(st.stats.notes, "note"),
                      `${fmtCount(st.stats.wiki)} wiki`,
                      counted(st.stats.sources, "source"),
                      counted(st.stats.templates, "template"),
                      counted(st.stats.openTasks, "open task"),
                      counted(st.stats.unresolvedLinks, "unresolved link"),
                      `updated ${ago(st.stats.updatedAt)}`,
                    ].map((x, i) => (
                      <Fragment key={x}>
                        {i > 0 && " · "}
                        <span className="nowrap">{x}</span>
                      </Fragment>
                    ))
                  : ""}
            </div>
          </div>
          <span
            title={
              !s.vaultPath
                ? "Choose a vault folder first"
                : st.state === "indexing"
                  ? "Indexing now…"
                  : "Read every file in the vault again and rebuild the index from scratch; the files aren't changed"
            }
          >
            <button
              type="button"
              className="btn sm"
              disabled={!s.vaultPath || st.state === "indexing"}
              onClick={() => void api.rebuildIndex()}
            >
              <Icon name="refresh" size={12} />
              Rebuild index
            </button>
          </span>
        </div>
        <div className="srow">
          <Icon name="lock" style={{ color: s.readOnly ? "var(--green)" : "var(--amber)" }} />
          <div className="t">
            <div className="pt">Read-only</div>
            <div className="faint">
              Never change anything in the vault. Turn it off to tick tasks, set their dates and capture from Brainstead.
            </div>
          </div>
          {/* Saved at once: Rust refuses or allows writes by it, so the switch and the app mustn't disagree. */}
          <Switch label="Read-only" on={s.readOnly} onChange={(v) => void settings.commit({ readOnly: v })} />
        </div>
      </div>
      <section className="sgroup">
        <h2 className="h3">Excluded folders</h2>
        <p className="muted">Any folder with one of these names, at any depth, is skipped.</p>
        <div className="chips">
          {s.excluded.map((x) => (
            <span key={x} className="chip">
              {x}
              <button
                type="button"
                className="ibtn xs"
                aria-label={`Stop excluding ${x}`}
                title={`Stop excluding ${x}; its files are indexed again`}
                onClick={() => settings.update({ excluded: s.excluded.filter((y) => y !== x) })}
              >
                <Icon name="x" size={11} />
              </button>
            </span>
          ))}
          <label className="inp sm">
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && add()}
              placeholder="folder-name"
              aria-label="Folder name to exclude"
              spellCheck={false}
              autoCorrect="off"
              autoCapitalize="off"
            />
          </label>
          <span title={name.trim() ? "Skip folders with this name from now on (↩)" : "Type a folder name first"}>
            <button type="button" className="btn sm" onClick={add} disabled={!name.trim()}>
              <Icon name="plus" size={12} />
              Add
            </button>
          </span>
        </div>
        {err && <p className="err">{err}</p>}
      </section>
      <ExampleNotes />
      <section className="sgroup sysf">
        <h2 className="h3">System folders</h2>
        <dl>
          <dt className="mono">sources/</dt>
          <dd className="muted">Imported files and captured emails and transcripts.</dd>
          <dt className="mono">wiki/</dt>
          <dd className="muted">Entities, concepts and source summaries.</dd>
          <dt className="mono">Templates/</dt>
          <dd className="muted">Note templates (Templater syntax).</dd>
          <dt className="mono">images/</dt>
          <dd className="muted">Pasted and dropped images. Not indexed.</dd>
          <dt className="mono">.trash/ and hidden folders</dt>
          <dd className="muted">Deleted items and other apps' settings. Not indexed.</dd>
        </dl>
      </section>
    </>
  );
}

/** Adds back the example notes a new vault is made with: only the ones that aren't there. */
function ExampleNotes() {
  const s = useStore(settings);
  const [busy, setBusy] = useState(false);
  const add = () => {
    setBusy(true);
    api
      .vaultAddExamples()
      .then((n) =>
        toast(
          n ? `Added ${counted(n, "example note")}; open Start here for the list` : "The example notes are all there already",
          undefined,
          "ok",
        ),
      )
      .catch((e) => toast(errText(e), undefined, "bad"))
      .finally(() => setBusy(false));
  };
  return (
    <section className="sgroup">
      <h2 className="h3">Example notes</h2>
      <p className="muted">
        The notes a new vault is made with, which show what Brainstead can do. Only the ones that aren&apos;t in the vault are added;
        nothing already there is changed.
      </p>
      <div className="row">
        <span
          title={
            !s.vaultPath
              ? "Choose a vault folder first"
              : s.readOnly
                ? "Turn Read-only off first"
                : "Add the example notes that aren't in the vault, with Start here listing them"
          }
        >
          <button type="button" className="btn" disabled={!s.vaultPath || s.readOnly || busy} onClick={add}>
            <Icon name="plus" size={14} />
            Add the example notes
          </button>
        </span>
      </div>
    </section>
  );
}

/** "1 template", "3 templates". */
const counted = (n: number, one: string) => `${fmtCount(n)} ${one}${n === 1 ? "" : "s"}`;

/** The User scripts folder as it's kept: trimmed, without slashes at its ends, and unset for the
 *  default (an assistant's settings tool keeps it the same way). */
export function scriptsFolder(typed: string): string | undefined {
  const v = typed.trim().replace(/^\/+|\/+$/g, "");
  return v && v !== DEFAULT_SCRIPTS ? v : undefined;
}

/** Where to read how to install each assistant (How to install). */
/** Brainstead's product page (site/), linked from About and in app_status. */
export const WEBSITE = "https://brainstead.davies.co.za/";

export const INSTALL: Record<string, string> = {
  claude: "https://docs.claude.com/en/docs/claude-code/setup",
  codex: "https://developers.openai.com/codex/cli",
  antigravity: "https://antigravity.google",
  copilot: "https://docs.github.com/copilot/how-tos/set-up/install-copilot-cli",
};

/** An assistant's line under Found on this computer: its models and where it is, or what to do. */
export function foundDetail(c: CliInfo): string {
  if (!c.path) return "Not installed.";
  return c.models.length
    ? `${c.models.length} model${c.models.length === 1 ? "" : "s"} · ${c.path}`
    : `No models listed: open ${terminalApp}, run ${c.cli === "antigravity" ? "agy" : c.cli} and sign in.`;
}

function Assistants() {
  const s = useStore(settings);
  const found = useStore(clis);
  const [at, setAt] = useState<DOMRect | null>(null);
  useEffect(() => void findClis(), []);
  const model = s.askModel ?? DEFAULT_MODEL;
  return (
    <>
      <section className="sgroup">
        <h2 className="h3">New chats use</h2>
        <div>
          <button
            type="button"
            className="btn"
            title="Choose the model new chats start with"
            onClick={(e) => setAt(e.currentTarget.getBoundingClientRect())}
          >
            <Icon name="cpu" size={14} />
            {modelLabel(model, found)}
            <Icon name="chevdown" size={12} />
          </button>
        </div>
        <p className="faint">Each chat keeps the model it started with, and jobs use this one unless they have their own below.</p>
        {at && (
          <Popover anchor={at} onClose={() => setAt(null)} width={300}>
            <ModelMenu
              found={found}
              value={model}
              onPick={(m) => {
                settings.update({ askModel: m });
                setAt(null);
              }}
            />
          </Popover>
        )}
        <div className="card">
          <div className="srow">
            <Icon name="ask" style={{ color: s.askSuggest === false ? "var(--ink3)" : "var(--green)" }} />
            <div className="t">
              <div className="pt">Suggest a next message</div>
              <div className="faint">
                After each answer, a model offers a follow-up in the message box (→ types it), at the cost of one more call per answer.
              </div>
            </div>
            <Switch label="Suggest a next message" on={s.askSuggest !== false} onChange={(v) => settings.update({ askSuggest: v })} />
          </div>
        </div>
      </section>
      <section className="sgroup">
        <h2 className="h3">Changes</h2>
        <div className="card">
          <div className="srow">
            <Icon name="history" style={{ color: "var(--accent)" }} />
            <div className="t">
              <div className="pt">Keep the history of agent changes</div>
              <div className="faint">
                Every change an assistant or a run makes can be reverted in Changes. The history is kept for this many days, or until it
                takes this much space, whichever comes first.
                {/onedrive/i.test(s.vaultPath ?? "") &&
                  " Your vault is in a OneDrive folder, so older versions are also in OneDrive's version history."}
              </div>
            </div>
            <select
              className="sel"
              aria-label="Days of history to keep"
              title="How many days of changes Changes keeps"
              value={String(s.changesKeepDays ?? 90)}
              onChange={(e) => settings.update({ changesKeepDays: Number(e.target.value) })}
            >
              {[30, 90, 180, 365].map((d) => (
                <option key={d} value={d}>
                  {d} days
                </option>
              ))}
            </select>
            <select
              className="sel"
              aria-label="Most space the history takes"
              title="The most space the history of changes takes"
              value={String(s.changesKeepMb ?? 500)}
              onChange={(e) => settings.update({ changesKeepMb: Number(e.target.value) })}
            >
              {[100, 250, 500, 1000, 2000].map((m) => (
                <option key={m} value={m}>
                  {m >= 1000 ? `${m / 1000} GB` : `${m} MB`}
                </option>
              ))}
            </select>
          </div>
        </div>
      </section>
      <section className="sgroup">
        <h2 className="h3">Meeting notes</h2>
        <div className="card">
          <div className="srow">
            <Icon name="wiki" style={{ color: s.meetingIngest !== false ? "var(--green)" : "var(--ink3)" }} />
            <div className="t">
              <div className="pt">Ingest a meeting note once it's made</div>
              <div className="faint">A note written from a transcript is read into the wiki straight after.</div>
            </div>
            <Switch
              label="Ingest a meeting note once it's made"
              on={s.meetingIngest !== false}
              onChange={(v) => settings.update({ meetingIngest: v })}
            />
          </div>
          <div className="srow">
            <Icon name="trash" style={{ color: s.meetingTrash !== false ? "var(--green)" : "var(--ink3)" }} />
            <div className="t">
              <div className="pt">Then move the transcript to the Trash</div>
              <div className="faint">
                Only once the note is made (and ingested, when that's on); a transcript whose note or ingest failed is kept. With both off,
                Brainstead asks after each note.
              </div>
            </div>
            <Switch
              label="Then move the transcript to the Trash"
              on={s.meetingTrash !== false}
              onChange={(v) => settings.update({ meetingTrash: v })}
            />
          </div>
        </div>
      </section>
      <section className="sgroup">
        <h2 className="h3">Ingest</h2>
        <div className="card">
          <div className="srow">
            <Icon name="inbox" style={{ color: s.ingestOnArrival ? "var(--green)" : "var(--ink3)" }} />
            <div className="t">
              <div className="pt">Ingest new sources as they arrive</div>
              <div className="faint">
                An email or chat captured from the browser, or a file dropped or imported into Sources, is ingested straight away.
              </div>
            </div>
            <Switch
              label="Ingest new sources as they arrive"
              on={!!s.ingestOnArrival}
              onChange={(v) => settings.update({ ingestOnArrival: v })}
            />
          </div>
          <div className="srow">
            <Icon name="refresh" style={{ color: s.refreshStale ? "var(--green)" : "var(--ink3)" }} />
            <div className="t">
              <div className="pt">Also refresh pages whose sources changed</div>
              <div className="faint">
                The daily check (Settings › Jobs &amp; schedule) ingests again each source that changed since the pages citing it were
                written; the wiki is updated at once, and a change that fails a check is held for you in Changes.
              </div>
            </div>
            <Switch
              label="Also refresh pages whose sources changed"
              on={!!s.refreshStale}
              onChange={(v) => settings.update({ refreshStale: v })}
            />
          </div>
        </div>
      </section>
      <JobModels found={found} fallback={model} />
      <section className="sgroup">
        <div className="row">
          <h2 className="h3 grow">Found on this computer</h2>
          <button
            type="button"
            className="btn sm"
            title="Look for the AI assistants' command-line tools again"
            onClick={() => {
              toast("Looking for the assistants…");
              void findClis(true).then(() => {
                const n = clis.get()?.filter((c) => c.path).length ?? 0;
                toast(n ? `Found ${n} assistant${n === 1 ? "" : "s"}` : "No assistants found", undefined, n ? "ok" : "bad");
              });
            }}
          >
            <Icon name="refresh" size={12} />
            Look again
          </button>
        </div>
        <div className="card">
          {!found && <div className="srow faint">Looking…</div>}
          {found?.map((c) => (
            <div key={c.cli} className="srow">
              <Icon name={c.path ? "check" : "x"} style={{ color: c.path ? "var(--green)" : "var(--ink3)" }} />
              <div className="t">
                <div className="pt">
                  {CLI_LABEL[c.cli]} {c.version && <span className="faint">· {c.version}</span>}
                </div>
                <div className="faint">{foundDetail(c)}</div>
              </div>
              {!c.path && (
                <button
                  type="button"
                  className="btn sm"
                  title="Open the install instructions in the browser"
                  onClick={() => void openUrl(INSTALL[c.cli])}
                >
                  How to install
                </button>
              )}
            </div>
          ))}
        </div>
        <p className="faint">
          Ask runs the assistant you choose on this computer with your own account, and it never writes a file itself.
        </p>
      </section>
      <McpGroup />
      <HandedOver />
    </>
  );
}

export const SCREEN_NAME: Record<string, string> = {
  sources: "Sources (Ingest)",
  fixname: `Fix a name everywhere (${keyLabel("⌘K")})`,
  contradictions: "Contradictions (Knowledge health)",
  health: "Knowledge health",
  meeting: "Meeting note from a transcript",
  triage: "Bookmarks triage",
  reply: "Draft a reply",
  doccheck: "Doc check",
};

/** The vault's skills that Brainstead now does on its own screens (list_assistants lists them too). */
export const handedOver = async (): Promise<Skill[]> => (await api.skillsList()).filter((x) => x.opens);

/** The vault's skills that Brainstead now does on its own screens. */
function HandedOver() {
  const [skills, setSkills] = useState<Skill[]>([]);
  useEffect(
    () =>
      void handedOver()
        .then(setSkills)
        .catch(() => {}),
    [],
  );
  if (!skills.length) return null;
  return (
    <section className="sgroup">
      <h2 className="h3">Skills Brainstead does now</h2>
      <div className="card">
        {skills.map((s) => (
          <div key={s.name} className="srow">
            <Icon name="check" style={{ color: "var(--green)" }} />
            <div className="t">
              <div className="pt">/{s.name}</div>
              <div className="faint">{SCREEN_NAME[s.opens!] ?? s.opens}</div>
            </div>
          </div>
        ))}
      </div>
      <p className="faint">
        Picking one of these in Ask opens its screen; the vault's own copies in .claude/skills keep working, and Brainstead never deletes
        them.
      </p>
    </section>
  );
}

/** Each CLI's own command to register Brainstead's server for all its sessions (its user config). */
const ADD_COMMAND: Record<CliName, (exe: string) => string> = {
  claude: (exe) => `claude mcp add -s user brainstead -- "${exe}" --mcp`,
  codex: (exe) => `codex mcp add brainstead -- "${exe}" --mcp`,
  copilot: (exe) => `copilot mcp add brainstead -- "${exe}" --mcp`,
  antigravity: (exe) => `agy mcp add brainstead -- "${exe}" --mcp`,
};

/** The commands Brainstead's tools pane shows, [assistant, command], for the assistants found on
 *  this computer (list_assistants gives the same). */
export const terminalCommands = (found: { cli: CliName; path: string | null }[], exe: string): [string, string][] =>
  found.filter((c) => c.path).map((c) => [CLI_LABEL[c.cli], ADD_COMMAND[c.cli](exe)]);

/** Brainstead's MCP server: what Ask gives each CLI, and how to add it to one in a terminal. */
function McpGroup() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(
    () =>
      void api
        .appInfo()
        .then(setInfo)
        .catch(() => {}),
    [],
  );
  // Each CLI's own command to register Brainstead's server for all its sessions (its user config),
  // for the assistants found on this computer.
  const found = useStore(clis);
  useEffect(() => void findClis(), []);
  const cmds = info && found ? terminalCommands(found, info.exe) : [];
  return (
    <section className="sgroup">
      <h2 className="h3">Brainstead's tools</h2>
      <p className="faint">
        In Ask, Claude Code, Codex and Copilot get Brainstead's tools: small changes happen at once and can be undone, while prose, renames
        and deletes are made at once and listed in Changes, where you can revert them (Antigravity uses only its own read tools).
      </p>
      {found && info && !cmds.length && (
        <p className="faint">
          No assistant is installed on this computer yet (see Found on this computer); once one is, its command to use Brainstead's tools
          from {terminalApp} shows here.
        </p>
      )}
      {cmds.length > 0 && (
        <>
          <p className="faint">
            To use them from an assistant in {terminalApp}, run its command once; it registers Brainstead for all its sessions:
          </p>
          <div className="card">
            {cmds.map(([name, cmd]) => (
              <div key={name} className="srow">
                <div className="t">
                  <div className="pt">{name}</div>
                  <code className="mono small selectable">{cmd}</code>
                </div>
                <button
                  type="button"
                  className="btn sm"
                  title={`Copy the command for ${name} to the clipboard`}
                  onClick={() => void api.copyText(cmd).then(() => toast("Copied", undefined, "ok"))}
                >
                  <Icon name="copy" size={12} />
                  Copy
                </button>
              </div>
            ))}
          </div>
        </>
      )}
    </section>
  );
}

function PermissionsPane() {
  useRecheckOnFocus();
  return (
    <>
      <FullDiskAccessCard />
      <p className="faint">Notifications and automation are asked for when a feature first needs them, not at startup.</p>
    </>
  );
}

const EXTENSIONS: { id: "outlook-capture" | "teams-capture"; ext: "outlook" | "teams"; name: string; what: string }[] = [
  {
    id: "outlook-capture",
    ext: "outlook",
    name: "Outlook capture",
    what: "The open email thread in Outlook on the web, or the threads ticked in the list.",
  },
  { id: "teams-capture", ext: "teams", name: "Teams capture", what: "The open Teams chat or channel thread, or a meeting's transcript." },
];

/** The Outlook and Teams extensions: where to load them from, whether the browsers know Brainstead's helper, the last captures. */
function SyncPane() {
  const s = useStore(settings);
  const [signIn, setSignIn] = useState<Awaited<ReturnType<typeof api.syncSignInState>> | null>(null);
  // The last channel list shows again on reopening Settings; the button fetches a fresh one.
  const [channels, setChannels] = useState(() => lastSeen.get<{ id: string; topic: string }[]>("sync:channels"));
  const [loadingChannels, setLoadingChannels] = useState(false);
  const [report, setReport] = useState<import("./api").SyncReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [newIgnore, setNewIgnore] = useState("");
  const refresh = () =>
    api
      .syncSignInState()
      .then(setSignIn)
      .catch(() => setSignIn(null));
  useEffect(() => {
    refresh();
    const again = () => refresh();
    window.addEventListener("focus", again);
    // A hand-over arrives through the extension's helper, not this window, so check now and then.
    const timer = window.setInterval(again, 60_000);
    return () => {
      window.removeEventListener("focus", again);
      window.clearInterval(timer);
    };
  }, []);
  const enabled = !!s.syncEnabled;
  const ignore = s.syncIgnoreSenders ?? [];
  const chosen = s.syncChannels ?? [];
  const addIgnore = () => {
    const v = newIgnore.trim().toLowerCase();
    if (v && !ignore.includes(v)) settings.update({ syncIgnoreSenders: [...ignore, v] });
    setNewIgnore("");
  };
  const syncNow = () => {
    setBusy(true);
    setReport(null);
    api
      .syncNow()
      .then((r) => {
        setReport(r);
        if (r.blocked) toast(r.blocked, undefined, "bad");
      })
      .catch((e) => toast(String(e), undefined, "bad"))
      .finally(() => setBusy(false));
  };
  const loadChannels = () => {
    setLoadingChannels(true);
    api
      .syncListChannels()
      .then((c) => setChannels(lastSeen.set("sync:channels", c)))
      .catch((e) => toast(`Couldn't list channels: ${e}`, undefined, "bad"))
      .finally(() => setLoadingChannels(false));
  };
  return (
    <>
      <section className="sgroup">
        <h2 className="h3">Teams and Outlook sync</h2>
        <p className="muted">
          Brainstead reads your own Teams chats, meeting transcripts, Inbox mail and calendar and writes what's new into sources/, ready to
          ingest and listed in the Inbox — the same as a capture, without clicking. It uses the sign-in your open Teams tab already has,
          handed over by the Teams extension, so keep Teams open in your browser; it reads only, and nothing leaves this computer but the
          calls to Microsoft.
        </p>
        <Switch label="Sync Teams and Outlook" on={enabled} onChange={(v) => settings.update({ syncEnabled: v })} />
        <label className="srow" style={{ gap: 8 }}>
          <span>On first sync, load the last</span>
          <input
            type="number"
            className="input"
            style={{ width: "4.5em" }}
            min={0}
            max={3650}
            title="How many days of history the first sync loads for each chat and the mailbox (0 = no limit). Stops a big mailbox or long chat history flooding the Inbox."
            value={s.syncInitialDays ?? 7}
            onChange={(e) => settings.update({ syncInitialDays: Math.max(0, Math.min(3650, Math.floor(Number(e.target.value) || 0))) })}
          />
          <span>days (0 = everything).</span>
        </label>
        <div className="srow">
          <Icon name="mail" style={{ color: (s.syncMailAttachments ?? true) ? "var(--green)" : "var(--ink3)" }} />
          <div className="t">
            <div className="pt">Bring in email attachments</div>
            <div className="faint">
              PDF, Word, PowerPoint, Excel and text files up to 30 MB, each its own source, linked from its thread. Others are listed by
              name.
            </div>
          </div>
          <Switch
            label="Bring in email attachments"
            on={s.syncMailAttachments ?? true}
            onChange={(v) => settings.update({ syncMailAttachments: v })}
          />
        </div>
        {s.readOnly && enabled && (
          <div className="doc-banner warn" role="status">
            <Icon name="info" size={14} />
            <span>Read-only is on (Settings › Vault), so the sync is paused until it's switched off.</span>
          </div>
        )}
      </section>

      <section className="sgroup">
        <h2 className="h3">Sign-in</h2>
        <div className="card">
          <div className="srow">
            <Icon name={signIn?.signedIn ? "check" : "info"} style={{ color: signIn?.signedIn ? "var(--green)" : "var(--ink3)" }} />
            <div className="t">
              <div className="pt">{signIn?.signedIn ? "Signed in" : "Not signed in"}</div>
              <div className="faint">
                {signIn?.signedIn
                  ? `Handed over ${signIn.handedOver ? ago(new Date(signIn.handedOver).getTime()) : "recently"}. Keep Teams, and Outlook for mail, open in Chrome tabs; the extensions keep it fresh.`
                  : "Open Teams in Chrome (with the Teams extension loaded) so it can hand the sign-in over. It arrives within a few minutes."}
                {signIn?.signedIn && signIn.stale.length > 0 && (
                  <div style={{ color: "var(--amber)" }}>
                    Gone stale: {signIn.stale.join(", ")}. Those wait until Teams renews them, within the hour while its tab is open.
                  </div>
                )}
                {signIn?.signedIn && signIn.rejected.length > 0 && (
                  <div style={{ color: "var(--amber)" }}>{rejectedText(signIn.rejected)}</div>
                )}
                {signIn?.handedOver && signIn.extensionOutdated !== null && (
                  <div style={{ color: "var(--amber)" }}>{extensionOutdatedText(signIn)}</div>
                )}
              </div>
            </div>
            {signIn?.signedIn && (
              <button
                type="button"
                className="btn sm"
                title="Forget the Teams sign-in on this computer"
                onClick={() => void api.syncSignOut().then(refresh)}
              >
                Sign out
              </button>
            )}
          </div>
        </div>
      </section>

      <section className="sgroup">
        <h2 className="h3">Sync now</h2>
        <p className="faint">The sync runs on its own every so often. Run it now to check it's working.</p>
        <div className="row">
          <button
            type="button"
            className="btn"
            title="Run one Teams and Outlook sync pass now"
            disabled={busy || !enabled}
            onClick={syncNow}
          >
            <Icon name="refresh" size={13} />
            {busy ? "Syncing…" : "Sync now"}
          </button>
        </div>
        {report && !report.blocked && (
          <p className="faint" role="status">
            Added {report.chats} chat {report.chats === 1 ? "day" : "days"}, {report.mail} {report.mail === 1 ? "email" : "emails"} and{" "}
            {report.transcripts} {report.transcripts === 1 ? "transcript" : "transcripts"}.
            {report.problems.length > 0 && ` ${report.problems.length} problem${report.problems.length === 1 ? "" : "s"}.`}
          </p>
        )}
        {report?.problems?.map((p, i) => (
          <div key={i} className="doc-banner warn" role="alert">
            <Icon name="info" size={14} />
            <span>{p}</span>
          </div>
        ))}
      </section>

      <section className="sgroup">
        <h2 className="h3">Mail to skip</h2>
        <p className="faint">
          Your Inbox syncs, apart from these senders or domains (and anything Outlook files under Other). Use “Don't sync this sender” on a
          synced email to add one, or add one here.
        </p>
        <div className="row">
          <input
            className="input"
            placeholder="name@example.com or example.com"
            value={newIgnore}
            onChange={(e) => setNewIgnore(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addIgnore()}
          />
          <button
            type="button"
            className="btn sm"
            title="Add this sender or domain to the skip list"
            onClick={addIgnore}
            disabled={!newIgnore.trim()}
          >
            Add
          </button>
        </div>
        {ignore.length > 0 && (
          <div className="card">
            {ignore.map((e) => (
              <div key={e} className="srow">
                <Icon name="mail" style={{ color: "var(--ink3)" }} />
                <div className="t">
                  <div className="pt mono">{e}</div>
                </div>
                <button
                  type="button"
                  className="btn sm"
                  title={`Stop skipping ${e}`}
                  onClick={() => settings.update({ syncIgnoreSenders: ignore.filter((x) => x !== e) })}
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="sgroup">
        <h2 className="h3">Channels</h2>
        <p className="faint">1:1 and group chats sync automatically. Channels are off unless you choose them here.</p>
        <div className="row">
          <button
            type="button"
            className="btn sm"
            title="List your Teams channels to choose which to sync"
            disabled={!signIn?.signedIn || loadingChannels}
            onClick={loadChannels}
          >
            <Icon name="refresh" size={13} className={loadingChannels ? "spin" : undefined} />
            {loadingChannels ? "Loading…" : channels ? "Refresh channel list" : "Show my channels"}
          </button>
        </div>
        {channels && channels.length === 0 && <p className="faint">No channels found.</p>}
        {channels && channels.length > 0 && (
          <div className="card">
            {channels.map((c) => (
              <div key={c.id} className="srow">
                <div className="t">
                  <div className="pt">{c.topic}</div>
                </div>
                <Switch
                  label={`Sync ${c.topic}`}
                  on={chosen.includes(c.id)}
                  onChange={(v) => settings.update({ syncChannels: v ? [...chosen, c.id] : chosen.filter((x) => x !== c.id) })}
                />
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}

function CapturePane() {
  const s = useStore(settings);
  const [st, setSt] = useState<Awaited<ReturnType<typeof api.captureStatus>> | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const v = useVaultVersion();
  // Read again whenever the window comes back to the front, as after reloading an extension in
  // the browser, so the versions and captures shown are current.
  const [focused, setFocused] = useState(0);
  useEffect(() => {
    const again = () => setFocused((n) => n + 1);
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, []);
  useEffect(() => {
    let live = true;
    api
      .captureStatus()
      .then((x) => {
        if (!live) return;
        setSt(x);
        setErr(x.folder ? null : "The extensions weren't found in this copy of Brainstead.");
      })
      .catch((e) => live && setErr(`Couldn't read the extensions' status: ${String(e)}`));
    return () => {
      live = false;
    };
  }, [v, focused]);
  const browsers = st?.browsers ?? [];
  return (
    <>
      <section className="sgroup">
        <h2 className="h3">Outlook and Teams in the browser</h2>
        <p className="muted">
          Two browser extensions send what's open in Outlook on the web or Teams into the vault's sources/ folder, ready to ingest and
          listed in the Inbox. They reach Brainstead through a small helper the browser starts, so they work while Brainstead is closed.
        </p>
        {s.readOnly && (
          <div className="doc-banner warn" role="status">
            <Icon name="info" size={14} />
            <span>Read-only is on (Settings › Vault), so captures are refused until it's switched off.</span>
          </div>
        )}
      </section>
      {err && (
        <div className="doc-banner warn" role="alert">
          <Icon name="info" size={14} />
          <span>{err}</span>
        </div>
      )}
      <div className="card">
        {EXTENSIONS.map((e) => {
          const last = st?.recent.find((c) => c.extension === e.ext);
          return (
            <div key={e.id} className="srow">
              <Icon name={last?.path ? "check" : "source"} style={{ color: last?.path ? "var(--green)" : "var(--ink3)" }} />
              <div className="t">
                <div className="pt">
                  {e.name}
                  {st?.versions[e.id] && (
                    <span
                      className="faint small"
                      title={`The version in its folder, which Load unpacked loads. The extension's card in chrome://extensions shows the version Chrome has loaded: if it's older, click reload there.`}
                    >
                      {" "}
                      · version {st.versions[e.id]}
                    </span>
                  )}
                </div>
                <div className="faint">
                  {e.what}{" "}
                  {last
                    ? last.path
                      ? `Last capture ${ago(new Date(last.at).getTime())}: ${last.what.toLowerCase()}.`
                      : `Last capture refused ${ago(new Date(last.at).getTime())}: ${last.error}`
                    : "No captures yet."}
                </div>
              </div>
              <span
                title={
                  st?.folder
                    ? `Show the ${e.name} extension's folder in ${fileManager}, to load it in Chrome`
                    : "The extensions weren't found"
                }
              >
                <button
                  type="button"
                  className="btn sm"
                  disabled={!st?.folder}
                  onClick={() => void api.captureReveal(e.id).catch((x) => setErr(String(x)))}
                >
                  <Icon name="finder" size={13} />
                  Show in {fileManager}
                </button>
              </span>
            </div>
          );
        })}
      </div>
      <section className="sgroup">
        <h2 className="h3">Install</h2>
        <ol className="steps">
          <li>
            In Chrome (or Edge, Brave), open <span className="mono">chrome://extensions</span> and switch on Developer mode.
          </li>
          <li>Click Load unpacked and choose an extension's folder (Show in {fileManager} above), then pin it to the toolbar.</li>
          <li>Open an email thread or a Teams chat and click the extension's button.</li>
        </ol>
        <p className="faint">
          {browsers.length
            ? `Brainstead's helper is registered with ${browsers
                .filter((b) => b.registered)
                .map((b) => b.name)
                .join(
                  ", ",
                )}${browsers.some((b) => !b.current) ? " (another copy of Brainstead registered it; this one takes over when it starts)" : ""}.`
            : "No Chromium browser was found on this computer: install Chrome, Edge or Brave, then open this page again."}{" "}
          The extensions' IDs are fixed, so there's nothing to copy or paste.
        </p>
      </section>
      {!!st?.recent.length && (
        <section className="sgroup">
          <h2 className="h3">Recent captures</h2>
          <div className="card">
            {st.recent.map((c) => (
              <div key={c.at} className="srow">
                <Icon name={c.path ? "check" : "x"} style={{ color: c.path ? "var(--green)" : "var(--red)" }} />
                <div className="t">
                  <div className="pt ell">{c.path ? c.path.replace(/^sources\//, "").replace(/\.md$/, "") : `${c.what} not captured`}</div>
                  <div className="faint">
                    {c.what} · {ago(new Date(c.at).getTime())}
                    {c.error ? ` · ${c.error}` : ""}
                  </div>
                </div>
                {c.path && <CaptureOpen path={c.path} />}
              </div>
            ))}
          </div>
        </section>
      )}
    </>
  );
}

function CaptureOpen({ path }: { path: string }) {
  const gone = useIsGone(path);
  return (
    <DocButton path={path} gone={gone} className="btn sm ghost" title={`Open ${path}`}>
      Open
    </DocButton>
  );
}

function About() {
  const s = useStore(settings);
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => {
    let live = true;
    api
      .appInfo()
      .then((i) => live && setInfo(i))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  return (
    <section className="sgroup">
      <div className="abouthead">
        <img className="ondark" src={appIconDark} width={64} height={64} alt="" />
        <img className="onlight" src={appIconLight} width={64} height={64} alt="" />
        <div>
          <h2 className="h3">Brainstead</h2>
          <p className="muted">A home for everything on your mind.</p>
        </div>
        <div className="aboutacts">
          <button type="button" className="btn sm" title="Open Brainstead's website in the browser" onClick={() => void openUrl(WEBSITE)}>
            <Icon name="globe" size={14} />
            Website
          </button>
          {/* The Brainstead menu has it on a Mac; Windows has no menu bar. */}
          {isWindows && (
            <button
              type="button"
              className="btn sm"
              title="Look for a new version now; Brainstead says what it found"
              onClick={() => void api.updateCheck().catch((e) => toast(errText(e), undefined, "bad"))}
            >
              <Icon name="refresh" size={14} />
              Check for Updates…
            </button>
          )}
        </div>
      </div>
      {info && (
        <dl className="kv">
          <dt>Version</dt>
          <dd>
            {info.version} ({info.build})
          </dd>
          <dt>App data</dt>
          <dd className="mono">{info.dataDir}</dd>
        </dl>
      )}
      <div className="card">
        <div className="srow">
          <Icon name="history" />
          <div className="t">
            <div className="pt">The app's logs</div>
            <div className="faint">
              What Brainstead noted while running, one file a day in the app data folder's logs. Older files are deleted.
            </div>
          </div>
          <label className="faint" htmlFor="logdays">
            Keep
          </label>
          <select
            title="How many days of Brainstead's log are kept"
            id="logdays"
            className="sel"
            value={String(s.logDays ?? 14)}
            onChange={(e) => settings.update({ logDays: Number(e.target.value) })}
          >
            {[7, 14, 30, 90].map((d) => (
              <option key={d} value={d}>
                {d} days
              </option>
            ))}
          </select>
          <button
            type="button"
            className="btn sm"
            title={`Show the folder of log files in ${fileManager}`}
            onClick={() => void api.logsReveal()}
          >
            <Icon name="finder" size={14} />
            Show in {fileManager}
          </button>
        </div>
      </div>
      <p className="faint">Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.</p>
    </section>
  );
}
