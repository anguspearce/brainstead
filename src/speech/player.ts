// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Reading a note aloud: the player's state and what it does, one block per utterance, through the
// system's own synthesiser on macOS (src-tauri/src/speech.rs; WebKit's speech hides downloaded
// Premium voices), and the web view's speech elsewhere (./webSpeech.ts, Windows' voices). The word about to be spoken drives the highlight. The rules that keep it right:
//
// - Every speak, stop, skip and pause between blocks bumps the generation, and each utterance's
//   id is the generation it was spoken in. An event, timer or load from an older generation does
//   nothing, so a stop can't be followed by the old reading carrying on.
// - Pausing mid-block holds the utterance (play carries on with it); pausing just as a block ends
//   remembers the next block (play starts there), or reading would carry on while paused.
// - A change of speed or voice restarts the block at once; while paused, play restarts it.
// - Now Playing says what's being read (so the media keys come here) and is cleared on stop, and
//   the screen is kept awake while playing, not while paused.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { platform } from "../platform";
import { settings, Store } from "../store";
import { SpeechEvent, webPause, webSpeak, webStop, webVoices } from "./webSpeech";

export interface Voice {
  id: string;
  name: string;
  lang: string;
  /** 1 default, 2 Enhanced, 3 Premium. */
  quality: number;
  default: boolean;
}

export interface PlayerState {
  on: boolean;
  paused: boolean;
  /** The note being read. */
  path: string;
  title: string;
  /** The block being read, from 0, and how many there are. */
  index: number;
  count: number;
  /** The spoken character the current word starts at (-1 before the first), and the block's length. */
  char: number;
  len: number;
}

const IDLE: PlayerState = { on: false, paused: false, path: "", title: "", index: 0, count: 0, char: -1, len: 0 };
export const player = new Store<PlayerState>(IDLE);

/** The speeds offered, and the default. */
export const SPEEDS = [0.75, 1, 1.25, 1.5, 2];

// Apple's character voices, in every language: never picked automatically.
const NOVELTY =
  /^(Eddy|Flo|Grandma|Grandpa|Reed|Rocko|Sandy|Shelley|Bahh|Bells|Boing|Bubbles|Cellos|Jester|Organ|Trinoids|Whisper|Zarvox|Wobble|Bad News|Good News|Superstar|Albert|Fred|Junior|Kathy|Ralph)\b/;

/** Voices best first: Premium, then Enhanced, then the rest, character voices last. */
export const rankVoices = (vs: Voice[]) =>
  [...vs].sort(
    (a, b) => Number(NOVELTY.test(a.name)) - Number(NOVELTY.test(b.name)) || b.quality - a.quality || a.name.localeCompare(b.name),
  );

/** A voice as the menus show it: "Zoe (Premium)", "Daniel (Enhanced)". */
export const voiceLabel = (v: Voice) =>
  /\(/.test(v.name) || v.quality < 2 ? v.name : `${v.name} (${v.quality === 3 ? "Premium" : "Enhanced"})`;

// The synthesiser: macOS's through the backend, the web view's anywhere else. Both send the same
// word and end events, to onSpeechEvent.
const native = () => platform() === "macos";
const tts = {
  voices: () => (native() ? invoke<Voice[]>("tts_voices") : webVoices()),
  speak: async (id: number, text: string, voice: string | null, rate: number) =>
    native() ? invoke<void>("tts_speak", { id, text, voice, rate }) : webSpeak(id, text, voice, rate, onSpeechEvent),
  pause: async (on: boolean) => (native() ? invoke<void>("tts_pause", { on }) : webPause(on)),
  stop: async () => (native() ? invoke<void>("tts_stop") : webStop()),
};

/** The English voices installed, best first; read again when the window comes back, so a voice
 *  downloaded in System Settings appears without a restart. */
export const voices = new Store<Voice[]>([]);
export async function loadVoices() {
  const all = await tts.voices().catch(() => [] as Voice[]);
  voices.set(rankVoices(all.filter((v) => v.lang.toLowerCase().startsWith("en"))));
}

/** The voice to read in: the one chosen in Settings (by id, or a name kept from before), else the
 *  system's default English voice, else the best one. */
export function voiceFor(): string | null {
  const vs = voices.get();
  const want = settings.get().speechVoice;
  const v = (want && (vs.find((x) => x.id === want) ?? vs.find((x) => x.name === want))) || vs.find((x) => x.default) || vs[0];
  return v?.id ?? null;
}

const rate = () => settings.get().speechRate ?? 1;

// What's being read, and the bookkeeping that keeps stale events out.
let texts: string[] = [];
let gen = 0;
let held = false; // paused mid-utterance: play carries on with it
let resumeAt: number | null = null; // paused just as a block ended: play starts here
let utt: { id: number; index: number; prefix: number } | null = null;
// Said before the first paragraph read: the note's title and date (once, at the start).
let intro = "";

function set(next: PlayerState) {
  const was = player.get();
  player.set(next);
  // Now Playing and keeping awake follow playing and pausing.
  if (was.on !== next.on || was.paused !== next.paused || was.title !== next.title) {
    void invoke("media_state", { title: next.on ? next.title : null, playing: next.on && !next.paused }).catch(() => {});
    void invoke("keep_awake", { on: next.on && !next.paused }).catch(() => {});
  }
}

function speakFrom(i: number) {
  held = false;
  resumeAt = null;
  const g = ++gen;
  const s = player.get();
  if (i >= texts.length) return stop();
  const text = texts[i];
  if (!text.trim()) return speakFrom(i + 1);
  // The title and date go before the first paragraph, and "That concludes …" after the last; the
  // word positions leave them out.
  const head = intro ? `${intro} ` : "";
  const tail = i === texts.length - 1 ? spokenOutro(s.title) : "";
  intro = "";
  utt = { id: g, index: i, prefix: head.length };
  set({ ...s, paused: false, index: i, char: -1, len: text.length });
  tts.speak(g, head + text + tail, voiceFor(), rate()).catch(() => {
    if (g === gen) stop();
  });
}

/** Said after the last paragraph: " That concludes Orbit App Steerco." */
export const spokenOutro = (title: string) => (title.trim() ? ` That concludes ${title.trim().replace(/[.!?]+$/, "")}.` : "");

/** The synthesiser's word and end events; anything not from the current utterance is stale. */
export function onSpeechEvent(e: SpeechEvent) {
  const u = utt;
  if (!u || e.id !== u.id || e.id !== gen) return;
  if (e.kind === "word") {
    // Outside the block while saying the title and date, or the closing words: nothing highlighted.
    const k = e.char - u.prefix;
    const s = player.get();
    player.set({ ...s, char: k < 0 || k >= s.len ? -1 : k });
    return;
  }
  utt = null;
  if (player.get().paused) {
    held = false;
    resumeAt = u.index + 1;
  } else speakFrom(u.index + 1);
}

/** Starts reading `blocks` (their spoken texts) of the note at `path`, from block `from`, saying
 *  `first` (the title and date) before it. */
export function startReading(path: string, title: string, blocks: string[], from = 0, first = "") {
  texts = blocks;
  intro = first;
  set({ ...IDLE, on: true, path, title, count: blocks.length });
  speakFrom(Math.max(0, Math.min(from, blocks.length - 1)));
}

export function toggleReading() {
  const s = player.get();
  if (!s.on) return;
  if (s.paused) {
    if (held && utt?.id === gen) {
      held = false;
      set({ ...s, paused: false });
      void tts.pause(false);
    } else speakFrom(resumeAt ?? s.index);
  } else {
    if (utt?.id === gen) {
      held = true;
      void tts.pause(true);
    } else {
      // Between blocks: nothing is held, so play starts the block again.
      gen++;
      void tts.stop();
    }
    set({ ...s, paused: true });
  }
}

/** The next (1) or previous (-1) block, or a given block (`to`), read from its start. */
export function skipReading(d: number, to?: number) {
  const s = player.get();
  if (!s.on) return;
  speakFrom(Math.max(0, Math.min(s.count - 1, to ?? s.index + d)));
}

export function stopReading() {
  stop();
}

function stop() {
  gen++;
  intro = "";
  utt = null;
  held = false;
  resumeAt = null;
  texts = [];
  if (player.get().on) void tts.stop();
  set(IDLE);
}

/** The speed or voice changed: the block starts again with it, or, paused, play restarts it. */
export function speechSettingsChanged() {
  const s = player.get();
  if (!s.on) return;
  if (!s.paused) speakFrom(s.index);
  else {
    held = false;
    resumeAt = resumeAt ?? s.index;
  }
}

/** Says a sample sentence in the chosen voice (Settings' Test). Id 0 is never a reading's, so its
 *  events are ignored; any reading is paused first. */
export function sayTest(text: string) {
  const s = player.get();
  if (s.on && !s.paused) toggleReading();
  tts.speak(0, text, voiceFor(), rate()).catch(() => {});
}

/** Screenshot mode: the block and spoken character to show the player at (src/scene.ts). */
export const readingScene = new Store<{ block: number; char: number } | null>(null);

/** Screenshot mode: the player shown at a block, saying nothing. */
export function stillReading(path: string, title: string, count: number, index: number, char: number) {
  player.set({ on: true, paused: false, path, title, index, count, char, len: 0 });
}

/** Listens once, at startup: the synthesiser's events, the media keys, and new voices on focus. */
export function startSpeech() {
  void loadVoices();
  window.addEventListener("focus", () => void loadVoices());
  const offs = [
    listen<SpeechEvent>("tts", (e) => onSpeechEvent(e.payload)),
    listen<string>("media", (e) => {
      const s = player.get();
      if (!s.on) return;
      if (e.payload === "next") skipReading(1);
      else if (e.payload === "previous") skipReading(-1);
      else if (e.payload === "toggle" || (e.payload === "play" && s.paused) || (e.payload === "pause" && !s.paused)) toggleReading();
    }),
  ];
  return () => offs.forEach((o) => void o.then((f) => f()));
}

/** For tests: back to nothing. */
export function _reset() {
  intro = "";
  texts = [];
  gen = 0;
  held = false;
  resumeAt = null;
  utt = null;
  player.set(IDLE);
}
