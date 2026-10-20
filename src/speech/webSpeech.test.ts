// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const calls: [string, unknown][] = [];
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, a?: unknown) => {
    calls.push([cmd, a]);
  }),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { platform, setPlatform } from "../platform";
import { settings } from "../store";
import { _reset, loadVoices, player, startReading, stopReading, toggleReading, voices } from "./player";
import { pieces, SpeechEvent, webPause, webSpeak, webStop, webVoices } from "./webSpeech";

// A stand-in for the web view's speech: it keeps what it was asked to say, and the test plays the
// engine's part (boundaries, ends, the late error a cancel fires).
class Utterance {
  text: string;
  voice: { voiceURI: string; lang: string } | null = null;
  lang = "";
  rate = 1;
  onboundary: ((e: { name: string; charIndex: number; charLength?: number }) => void) | null = null;
  onend: (() => void) | null = null;
  onerror: (() => void) | null = null;
  constructor(text: string) {
    this.text = text;
  }
}

const VOICES = [
  { voiceURI: "david", name: "Microsoft David - English (United States)", lang: "en-US", default: true },
  { voiceURI: "aria", name: "Microsoft Aria (Natural) - English (United States)", lang: "en-US", default: false },
  { voiceURI: "katja", name: "Microsoft Katja - German (Germany)", lang: "de-DE", default: false },
];

let said: Utterance[] = [];
let listed: typeof VOICES = [];
const listeners: (() => void)[] = [];
const synth = {
  getVoices: () => listed,
  addEventListener: (_: string, f: () => void) => listeners.push(f),
  speak: vi.fn((u: Utterance) => said.push(u)),
  cancel: vi.fn(),
  pause: vi.fn(),
  resume: vi.fn(),
};
const lastSaid = () => said[said.length - 1];

const start = platform();
beforeEach(() => {
  said = [];
  listed = VOICES;
  listeners.length = 0;
  calls.length = 0;
  vi.clearAllMocks();
  Object.assign(window, { speechSynthesis: synth });
  Object.assign(globalThis, { SpeechSynthesisUtterance: Utterance });
});
afterEach(() => {
  webStop();
  setPlatform(start);
});

describe("the web view's speech", () => {
  it("lists the voices, natural ones as the best, once the system has them", async () => {
    listed = [];
    const got = webVoices();
    listed = VOICES;
    listeners.forEach((f) => f());
    const vs = await got;
    expect(vs.map((v) => [v.id, v.quality, v.default])).toEqual([
      ["david", 1, true],
      ["aria", 3, false],
      ["katja", 1, false],
    ]);
  });

  it("says the text in the chosen voice and speed, with word and end events", () => {
    const got: SpeechEvent[] = [];
    webSpeak(7, "Hello there.", "aria", 1.25, (e) => got.push(e));
    const u = lastSaid();
    expect(u.text).toBe("Hello there.");
    expect(u.voice?.voiceURI).toBe("aria");
    expect(u.lang).toBe("en-US");
    expect(u.rate).toBe(1.25);
    u.onboundary!({ name: "word", charIndex: 0, charLength: 5 });
    // A voice that leaves out the length: up to the end of the word.
    u.onboundary!({ name: "word", charIndex: 6 });
    u.onboundary!({ name: "sentence", charIndex: 0 });
    u.onend!();
    expect(got).toEqual([
      { id: 7, kind: "word", char: 0, len: 5 },
      { id: 7, kind: "word", char: 6, len: 5 },
      { id: 7, kind: "end", char: 0, len: 0 },
    ]);
  });

  it("says a long paragraph a few sentences at a time, with the paragraph's word positions and one end", () => {
    const s = "This sentence is about forty characters. ";
    const text = s.repeat(12).trim();
    const got: SpeechEvent[] = [];
    webSpeak(3, text, null, 1, (e) => got.push(e));
    const first = lastSaid();
    expect(first.text.length).toBeLessThanOrEqual(220);
    expect(first.text.endsWith(". ")).toBe(true);
    first.onend!();
    const second = lastSaid();
    expect(second).not.toBe(first);
    second.onboundary!({ name: "word", charIndex: 0, charLength: 4 });
    expect(got).toEqual([{ id: 3, kind: "word", char: first.text.length, len: 4 }]);
    // On to the end: one end event, after the last piece.
    while (!got.some((e) => e.kind === "end")) lastSaid().onend!();
    expect(said.map((u) => u.text).join("")).toBe(text);
    expect(got.filter((e) => e.kind === "end")).toHaveLength(1);
  });

  it("drops events from a reading that was stopped or replaced", () => {
    const got: SpeechEvent[] = [];
    webSpeak(1, "One.", null, 1, (e) => got.push(e));
    const old = lastSaid();
    webSpeak(2, "Two.", null, 1, (e) => got.push(e));
    expect(synth.cancel).toHaveBeenCalled();
    // The cancel's late error and a stray boundary from the old one do nothing.
    old.onerror!();
    old.onboundary!({ name: "word", charIndex: 0, charLength: 3 });
    expect(said).toHaveLength(2);
    webStop();
    lastSaid().onend!();
    expect(got).toEqual([]);
  });

  it("moves on after an error, so reading doesn't hang", () => {
    const got: SpeechEvent[] = [];
    webSpeak(4, "One.", null, 1, (e) => got.push(e));
    lastSaid().onerror!();
    expect(got).toEqual([{ id: 4, kind: "end", char: 0, len: 0 }]);
  });

  it("pauses and resumes", () => {
    webPause(true);
    expect(synth.pause).toHaveBeenCalled();
    webPause(false);
    expect(synth.resume).toHaveBeenCalled();
  });

  it("refuses where the web view can't speak", async () => {
    Object.assign(window, { speechSynthesis: undefined });
    expect(() => webSpeak(1, "One.", null, 1, () => {})).toThrow();
    expect(await webVoices()).toEqual([]);
  });

  it("cuts at sentence ends, then commas, then spaces, and the pieces join back up", () => {
    expect(pieces("Short.")).toEqual([{ start: 0, text: "Short." }]);
    expect(pieces("")).toEqual([{ start: 0, text: "" }]);
    const t = "One two three, four five six seven eight nine ten eleven twelve";
    const p = pieces(t, 20);
    expect(p[0].text).toBe("One two three, ");
    expect(p.map((x) => x.text).join("")).toBe(t);
    for (const x of p) {
      expect(x.text.length).toBeLessThanOrEqual(20);
      expect(t.slice(x.start, x.start + x.text.length)).toBe(x.text);
    }
    // No space at all: a hard cut.
    expect(pieces("x".repeat(25), 10).map((x) => x.text.length)).toEqual([10, 10, 5]);
  });
});

describe("reading aloud on Windows", () => {
  beforeEach(() => {
    setPlatform("windows");
    _reset();
    settings.loadFrom({ ...settings.get(), speechVoice: undefined, speechRate: 1 });
  });

  it("reads through the web view, highlighting words and going on to the next block", async () => {
    await loadVoices();
    // English only, the natural voice first.
    expect(voices.get().map((v) => v.id)).toEqual(["aria", "david"]);
    startReading("N.md", "N", ["One.", "Two."]);
    expect(calls.some(([c]) => c === "tts_speak")).toBe(false);
    // The system's default English voice, as on macOS.
    expect(lastSaid().text).toBe("One.");
    expect(lastSaid().voice?.voiceURI).toBe("david");
    lastSaid().onboundary!({ name: "word", charIndex: 0, charLength: 3 });
    expect(player.get().char).toBe(0);
    lastSaid().onend!();
    expect(lastSaid().text).toBe("Two. That concludes N.");
    toggleReading();
    expect(synth.pause).toHaveBeenCalled();
    expect(player.get().paused).toBe(true);
    toggleReading();
    expect(synth.resume).toHaveBeenCalled();
    lastSaid().onend!();
    expect(player.get().on).toBe(false);
  });

  it("a stop ends it: the cancelled utterance's late end doesn't start the next block", () => {
    startReading("N.md", "N", ["One.", "Two."]);
    const u = lastSaid();
    stopReading();
    u.onend!();
    expect(said).toHaveLength(1);
    expect(player.get().on).toBe(false);
  });
});
