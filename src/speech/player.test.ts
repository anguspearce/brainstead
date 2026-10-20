// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { beforeEach, describe, expect, it, vi } from "vitest";

type Call = [string, Record<string, unknown> | undefined];
const calls: Call[] = [];
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, a?: Record<string, unknown>) => {
    calls.push([cmd, a]);
    if (cmd === "tts_voices")
      return [
        { id: "albert", name: "Albert", lang: "en-US", quality: 1, default: false },
        { id: "zoe", name: "Zoe", lang: "en-US", quality: 3, default: false },
        { id: "sam", name: "Samantha", lang: "en-US", quality: 1, default: true },
        { id: "anna", name: "Anna", lang: "de-DE", quality: 3, default: false },
      ];
    return undefined;
  }),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { setPlatform } from "../platform";
import { settings } from "../store";
import {
  _reset,
  loadVoices,
  onSpeechEvent,
  player,
  speechSettingsChanged,
  startReading,
  stopReading,
  toggleReading,
  voiceFor,
  voiceLabel,
  voices,
} from "./player";

const spoken = () => calls.filter(([c]) => c === "tts_speak").map(([, a]) => a as { id: number; text: string });
const last = () => spoken()[spoken().length - 1];
const word = (id: number, char: number) => onSpeechEvent({ id, kind: "word", char, len: 4 });
const end = (id: number) => onSpeechEvent({ id, kind: "end", char: 0, len: 0 });

describe("reading aloud", () => {
  beforeEach(() => {
    // macOS's synthesiser, through the backend (webSpeech.test.ts has the web view's).
    setPlatform("macos");
    calls.length = 0;
    _reset();
    settings.loadFrom({ ...settings.get(), speechVoice: undefined, speechRate: 1 });
  });

  it("reads block after block, and ends by stopping", () => {
    startReading("N.md", "N", ["One.", "Two."]);
    expect(last().text).toBe("One.");
    word(last().id, 0);
    expect(player.get().char).toBe(0);
    end(last().id);
    expect(last().text).toBe("Two. That concludes N.");
    word(last().id, "Two. That".length);
    expect(player.get().char).toBe(-1); // the closing words aren't highlighted
    end(last().id);
    expect(player.get().on).toBe(false);
    expect(calls.some(([c, a]) => c === "media_state" && a?.title === null)).toBe(true);
  });

  it("says the title and date before the first paragraph, and keeps word positions on the paragraph", () => {
    startReading("N.md", "N", ["One two.", "Three."], 0, "Steerco. 30 September 2026.");
    expect(last().text).toBe("Steerco. 30 September 2026. One two.");
    const id = last().id;
    word(id, 3);
    expect(player.get().char).toBe(-1); // still saying the title
    word(id, "Steerco. 30 September 2026. ".length + 4);
    expect(player.get().char).toBe(4);
    end(id);
    expect(last().text).toBe("Three. That concludes N."); // the title said once only
  });

  it("ignores events from an utterance that's no longer current", () => {
    startReading("N.md", "N", ["One.", "Two.", "Three."]);
    const first = last().id;
    toggleReading(); // pause
    toggleReading(); // play: carries on the same utterance
    stopReading();
    startReading("N.md", "N", ["One.", "Two."]);
    end(first); // the old reading's end, arriving late
    expect(player.get().index).toBe(0);
    expect(spoken().filter((s) => s.text === "Two.").length).toBe(0);
  });

  it("pausing mid-block holds it; play carries on rather than starting again", () => {
    startReading("N.md", "N", ["One.", "Two."]);
    const n = spoken().length;
    toggleReading();
    expect(player.get().paused).toBe(true);
    expect(calls.some(([c, a]) => c === "tts_pause" && a?.on === true)).toBe(true);
    toggleReading();
    expect(calls.some(([c, a]) => c === "tts_pause" && a?.on === false)).toBe(true);
    expect(spoken().length).toBe(n);
  });

  it("pausing just as a block ends doesn't carry on; play starts the next block", () => {
    startReading("N.md", "N", ["One.", "Two."]);
    const id = last().id;
    toggleReading();
    end(id); // the end arrives while paused
    expect(spoken().length).toBe(1);
    toggleReading();
    expect(last().text).toBe("Two. That concludes N.");
  });

  it("a stop ends it: a late event can't start the next block", () => {
    startReading("N.md", "N", ["One.", "Two."]);
    const id = last().id;
    stopReading();
    end(id);
    expect(spoken().length).toBe(1);
    expect(player.get().on).toBe(false);
    expect(calls.some(([c]) => c === "tts_stop")).toBe(true);
  });

  it("a change of speed restarts the block; paused, play restarts it", () => {
    startReading("N.md", "N", ["One.", "Two."]);
    settings.update({ speechRate: 1.5 });
    speechSettingsChanged();
    expect(spoken().length).toBe(2);
    expect(last()).toMatchObject({ text: "One.", rate: 1.5 });
    toggleReading();
    speechSettingsChanged();
    toggleReading();
    expect(spoken().length).toBe(3);
  });

  it("keeps the screen awake while playing, not while paused", () => {
    startReading("N.md", "N", ["One."]);
    const awake = () => calls.filter(([c]) => c === "keep_awake").map(([, a]) => a?.on);
    expect(awake().at(-1)).toBe(true);
    toggleReading();
    expect(awake().at(-1)).toBe(false);
    stopReading();
    expect(awake().at(-1)).toBe(false);
  });

  it("picks English voices, best first, character voices last, and the chosen one by id or name", async () => {
    await loadVoices();
    expect(voices.get().map((v) => v.id)).toEqual(["zoe", "sam", "albert"]);
    expect(voiceLabel(voices.get()[0])).toBe("Zoe (Premium)");
    expect(voiceFor()).toBe("sam"); // the system's default
    settings.update({ speechVoice: "zoe" });
    expect(voiceFor()).toBe("zoe");
    settings.update({ speechVoice: "Zoe" });
    expect(voiceFor()).toBe("zoe");
    settings.update({ speechVoice: "gone" });
    expect(voiceFor()).toBe("sam");
  });
});
