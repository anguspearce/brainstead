// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Reading aloud where there's no macOS synthesiser (Windows): the web view's own speech, and so the
// system's voices, natural ones included where they're installed. It gives the player what
// src-tauri/src/platform/speech.rs gives it on macOS: the voices, and a word event for each word
// and an end event once the text has all been said, with the utterance's id.
//
// A long paragraph is said a few sentences at a time, one utterance after another, because some
// engines stop a long utterance part-way (Chromium's well-known 15-second cut-off); the word
// positions are the paragraph's, and the end comes after the last piece. Events from a stopped or
// replaced reading are dropped, as cancelling fires them late.

import type { Voice } from "./player";

export interface SpeechEvent {
  id: number;
  kind: "word" | "end";
  char: number;
  len: number;
}

const synth = (): SpeechSynthesis | undefined => (typeof window !== "undefined" ? window.speechSynthesis : undefined);

/** Whether the web view can speak at all. */
export const hasWebSpeech = () => !!synth() && typeof SpeechSynthesisUtterance !== "undefined";

/** The voices, once the system has listed them: the first time, they come after "voiceschanged". */
export async function webVoices(): Promise<Voice[]> {
  const s = synth();
  if (!s || !hasWebSpeech()) return [];
  let all = s.getVoices();
  if (!all.length) {
    all = await new Promise<SpeechSynthesisVoice[]>((done) => {
      const got = () => done(s.getVoices());
      s.addEventListener("voiceschanged", got, { once: true });
      window.setTimeout(got, 2000);
    });
  }
  // Windows' "Natural" voices are its best, as Premium ones are on macOS. Their names already say
  // so ("Microsoft Aria (Natural) - English (United States)"), so the label adds nothing.
  return all.map((v) => ({ id: v.voiceURI, name: v.name, lang: v.lang, quality: /natural/i.test(v.name) ? 3 : 1, default: v.default }));
}

/** The longest piece said in one utterance, in characters: a few sentences, well under 15 seconds. */
export const PIECE = 220;

/** `text` cut into pieces of at most `max` characters, at sentence ends where it can, else at a
 *  comma or a space; each with where it starts. Joined, the pieces are the text. */
export function pieces(text: string, max = PIECE): { start: number; text: string }[] {
  const out: { start: number; text: string }[] = [];
  let start = 0;
  while (text.length - start > max) {
    const span = text.slice(start, start + max + 1);
    // The last sentence end (punctuation, closing quotes, then a space) inside the window, else the
    // last comma or semicolon, else the last space; failing all three, a hard cut.
    let cut = -1;
    for (const re of [/[.!?…]["'”’)\]]*\s+/g, /[,;:]\s+/g, /\s+/g]) {
      for (const m of span.matchAll(re)) if (m.index! > 0 && m.index! + m[0].length <= max) cut = m.index! + m[0].length;
      if (cut > 0) break;
    }
    if (cut <= 0) cut = max;
    out.push({ start, text: text.slice(start, start + cut) });
    start += cut;
  }
  if (start < text.length || !out.length) out.push({ start, text: text.slice(start) });
  return out;
}

/** The reading being spoken: events from any other (stopped, replaced) are dropped. */
let current: object | null = null;
// The piece being said, held only so it isn't collected before its end.
const keep: { u: SpeechSynthesisUtterance | null } = { u: null };

/** Speaks `text` as utterance `id`, at `rate` (1 is normal), in `voice` (an id from webVoices),
 *  stopping whatever was being said; word and end events go to `send`. Throws where the web view
 *  has no speech. */
export function webSpeak(id: number, text: string, voice: string | null | undefined, rate: number, send: (e: SpeechEvent) => void) {
  const s = synth();
  webStop();
  if (!s || !hasWebSpeech()) throw new Error("This system can't speak.");
  // A screenshot demo says nothing.
  if (typeof document !== "undefined" && document.documentElement.dataset.scene) return;
  const token = {};
  current = token;
  const v = voice ? s.getVoices().find((x) => x.voiceURI === voice) : undefined;
  const parts = pieces(text);
  const say = (i: number) => {
    if (current !== token) return;
    if (i >= parts.length) {
      current = null;
      return send({ id, kind: "end", char: 0, len: 0 });
    }
    const p = parts[i];
    const u = new SpeechSynthesisUtterance(p.text);
    if (v) {
      u.voice = v;
      u.lang = v.lang;
    }
    u.rate = rate;
    u.onboundary = (e) => {
      if (current !== token || e.name !== "word") return;
      // Some voices leave out the word's length: up to the next space or punctuation, then.
      const len = e.charLength || (p.text.slice(e.charIndex).match(/^[\p{L}\p{N}'’-]+/u)?.[0].length ?? 0);
      send({ id, kind: "word", char: p.start + e.charIndex, len });
    };
    // An error (a voice that can't be reached, say) moves on as the end of the piece does, so
    // reading doesn't hang; a cancelled piece is no longer current and does nothing.
    u.onend = () => say(i + 1);
    u.onerror = () => say(i + 1);
    // Kept, so the engine's events still arrive (an utterance nobody holds can be collected).
    keep.u = u;
    s.speak(u);
  };
  say(0);
}

export function webStop() {
  current = null;
  keep.u = null;
  synth()?.cancel();
}

export function webPause(on: boolean) {
  const s = synth();
  if (!s) return;
  if (on) s.pause();
  else s.resume();
}
