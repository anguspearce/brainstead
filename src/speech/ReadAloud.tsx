// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Read aloud on a note or wiki page: the top bar's speaker button, the boxes drawn behind the
// word being said (from the word's client rects, so the page's markup is never touched), and the
// player bar at the foot of the note. src/Doc.tsx wires them to the note; ./player.ts does the
// reading and ./blocks.ts works out what's said.

import { RefObject, useEffect, useRef, useState } from "react";
import { Icon } from "../icons";
import { settings, useStore } from "../store";
import { Popover, Switch } from "../ui";
import { SpeechBlock, wordRange } from "./blocks";
import { player, skipReading, SPEEDS, speechSettingsChanged, stopReading, toggleReading, voiceLabel, voices } from "./player";
import { keyLabel } from "../platform";

/** The top bar's speaker: starts reading the note, then plays or pauses it. Hidden where the
 *  system has no voices (not macOS). */
export function ReadAloudButton({ path, onStart }: { path: string; onStart: () => void }) {
  const s = useStore(player);
  const vs = useStore(voices);
  if (!vs.length) return null;
  const mine = s.on && s.path === path;
  const tip = keyLabel(!mine ? "Read aloud (⇧⌘P)" : s.paused ? "Carry on reading (⇧⌘P · Space)" : "Pause reading (⇧⌘P · Space)");
  return (
    <button
      type="button"
      className={`ibtn ${mine ? "on" : ""}`}
      aria-label={tip}
      title={tip}
      onClick={() => (mine ? toggleReading() : onStart())}
    >
      <Icon name="speaker" />
    </button>
  );
}

/** The highlight behind the word being read (or the whole block, when Settings says so), and the
 *  block kept in view as reading moves on. `wrap` is the positioned box around the rendered note. */
export function SpeakLayer({ wrap, blocks }: { wrap: RefObject<HTMLDivElement | null>; blocks: RefObject<SpeechBlock[]> }) {
  const s = useStore(player);
  const words = settings.get().speechHighlight !== false;
  const [boxes, setBoxes] = useState<{ left: number; top: number; width: number; height: number; cls: string }[]>([]);
  const [tick, setTick] = useState(0);

  // Placed again when the note's box changes size (window, text size, side pane).
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setTick((t) => t + 1));
    ro.observe(el);
    return () => ro.disconnect();
  }, [wrap]);

  const b = s.on ? blocks.current?.[s.index] : undefined;
  useEffect(() => {
    const el = wrap.current;
    if (!el || !b) return setBoxes([]);
    const base = el.getBoundingClientRect();
    const rel = (r: DOMRect, cls: string) => ({ left: r.left - base.left, top: r.top - base.top, width: r.width, height: r.height, cls });
    if (!words) return setBoxes([rel(b.el.getBoundingClientRect(), "speaktint")]);
    const r = wordRange(b, s.char);
    setBoxes(r ? [...r.getClientRects()].filter((x) => x.width > 0).map((x) => rel(x, "speakbox")) : []);
  }, [b, s.char, words, tick, wrap]);

  // The block being read is kept in the middle of the view as reading moves on, unless paused.
  useEffect(() => {
    if (b && !player.get().paused) b.el.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [b]);

  return (
    <div className="speaklayer" aria-hidden="true">
      {boxes.map((x, i) => (
        <span key={i} className={x.cls} style={{ left: x.left, top: x.top, width: x.width, height: x.height }} />
      ))}
    </div>
  );
}

/** A speed as it's shown: "1×", "1.1×", "1.25×". */
export const fmtRate = (r: number) => `${Math.round(r * 100) / 100}×`;

/** The speed, from 0.5× to 2× in steps of 0.05. The label follows the slider as it moves; the
 *  setting is kept, and a paragraph being read restarts at the new speed, when it's let go. */
export function SpeedSlider() {
  const saved = useStore(settings).speechRate ?? 1;
  const [v, setV] = useState(saved);
  useEffect(() => setV(saved), [saved]);
  const commit = () => {
    if (v === (settings.get().speechRate ?? 1)) return;
    settings.update({ speechRate: v });
    speechSettingsChanged();
  };
  return (
    <div className="speedslider">
      <input
        type="range"
        min={0.5}
        max={2}
        step={0.05}
        value={v}
        aria-label="Reading speed"
        title="Reading speed"
        onChange={(e) => setV(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className="speedval">{fmtRate(v)}</span>
    </div>
  );
}

/** The voice menu: English voices, best first, labelled Premium or Enhanced. */
export function VoiceSelect({ onChange }: { onChange?: () => void }) {
  const vs = useStore(voices);
  const st = useStore(settings);
  const chosen = st.speechVoice && vs.some((v) => v.id === st.speechVoice) ? st.speechVoice : "";
  return (
    <select
      className="inp"
      value={chosen}
      title="The voice notes are read in"
      onChange={(e) => {
        settings.update({ speechVoice: e.target.value || undefined });
        onChange?.();
      }}
    >
      <option value="">System voice{vs.find((v) => v.default) ? ` (${voiceLabel(vs.find((v) => v.default)!)})` : ""}</option>
      {vs.map((v) => (
        <option key={v.id} value={v.id}>
          {voiceLabel(v)}
        </option>
      ))}
    </select>
  );
}

/** Moves the player bar by dragging it anywhere but its controls, kept on screen; back to its place
 *  when reading starts again. The click that ends a drag isn't taken as a click. */
function useDrag(on: boolean) {
  const [off, setOff] = useState({ x: 0, y: 0 });
  useEffect(() => {
    if (!on) setOff({ x: 0, y: 0 });
  }, [on]);
  const onPointerDown = (e: React.PointerEvent<HTMLElement>) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("input, select, textarea")) return;
    const el = e.currentTarget;
    const sx = e.clientX;
    const sy = e.clientY;
    const start = off;
    const r = el.getBoundingClientRect();
    let moved = false;
    const move = (m: PointerEvent) => {
      const dx = m.clientX - sx;
      const dy = m.clientY - sy;
      if (!moved && Math.hypot(dx, dy) < 4) return;
      moved = true;
      setOff({
        x: start.x + Math.max(8 - r.left, Math.min(dx, window.innerWidth - 8 - r.right)),
        y: start.y + Math.max(8 - r.top, Math.min(dy, window.innerHeight - 8 - r.bottom)),
      });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      if (!moved) return;
      const eat = (c: MouseEvent) => {
        c.stopPropagation();
        c.preventDefault();
      };
      window.addEventListener("click", eat, { capture: true, once: true });
      window.setTimeout(() => window.removeEventListener("click", eat, true), 0);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  };
  return { off, onPointerDown };
}

/** The player bar at the foot of the note being read: previous, play or pause, next, how far it
 *  has got, the speed and voice, and stop. */
export function PlayerBar({ main, onShow }: { main: RefObject<HTMLElement | null>; onShow: () => void }) {
  const s = useStore(player);
  const st = useStore(settings);
  const [speedAt, setSpeedAt] = useState<DOMRect | null>(null);
  const [centre, setCentre] = useState<number | null>(null);
  const bar = useRef<HTMLDivElement>(null);
  const drag = useDrag(s.on);

  // Centred over the note's column, clear of the sidebar.
  useEffect(() => {
    const el = main.current;
    if (!el) return;
    const place = () => {
      const r = el.getBoundingClientRect();
      setCentre(r.left + r.width / 2);
    };
    place();
    const ro = new ResizeObserver(place);
    ro.observe(el);
    return () => ro.disconnect();
  }, [main]);

  if (!s.on) return null;
  const rate = st.speechRate ?? 1;
  const progress = s.count ? (s.index + (s.len > 0 && s.char > 0 ? Math.min(1, s.char / s.len) : 0)) / s.count : 0;
  const setRate = (r: number) => {
    settings.update({ speechRate: r });
    speechSettingsChanged();
  };
  return (
    <div
      ref={bar}
      className="playerbar"
      role="region"
      aria-label="Reading aloud"
      onPointerDown={drag.onPointerDown}
      style={{
        ...(centre === null ? {} : { left: centre }),
        transform: `translate(calc(-50% + ${drag.off.x}px), ${drag.off.y}px)`,
      }}
    >
      <button
        type="button"
        className="ibtn"
        aria-label="Previous paragraph"
        title="Previous paragraph (F7)"
        onClick={() => skipReading(-1)}
      >
        <Icon name="skip-back" size={15} />
      </button>
      <button
        type="button"
        className="playerplay"
        aria-label={s.paused ? "Play" : "Pause"}
        title={s.paused ? "Carry on reading (Space or F8)" : "Pause reading (Space or F8)"}
        onClick={toggleReading}
      >
        <Icon name={s.paused ? "play" : "pause"} size={16} style={{ fill: "currentColor", stroke: "none" }} />
      </button>
      <button type="button" className="ibtn" aria-label="Next paragraph" title="Next paragraph (F9)" onClick={() => skipReading(1)}>
        <Icon name="skip-forward" size={15} />
      </button>
      <div className="playermid">
        <div className="playertop">
          <button type="button" className="playertitle ell" title="Go to the paragraph being read" onClick={onShow}>
            {s.title}
          </button>
          <span className="faint small">
            {s.index + 1} of {s.count}
          </span>
        </div>
        <div className="playerprogress">
          <i style={{ width: `${Math.round(progress * 1000) / 10}%` }} />
        </div>
      </div>
      <button
        type="button"
        className="playerspeed"
        title="Speed and voice"
        onClick={(e) => setSpeedAt(e.currentTarget.getBoundingClientRect())}
      >
        {fmtRate(rate)}
      </button>
      <button type="button" className="ibtn" aria-label="Stop reading" title="Stop reading (Esc)" onClick={stopReading}>
        <Icon name="x" size={15} />
      </button>
      {speedAt && (
        <Popover anchor={speedAt} onClose={() => setSpeedAt(null)} width={280} place="above">
          <div className="playerpop">
            <div className="eyebrow">Speed</div>
            <div className="playerspeeds">
              {SPEEDS.map((r) => (
                <button
                  key={r}
                  type="button"
                  className={`btn sm ${r === rate ? "on" : ""}`}
                  title={`Read at ${r}×`}
                  onClick={() => setRate(r)}
                >
                  {r}×
                </button>
              ))}
            </div>
            <SpeedSlider />
            <div className="eyebrow">Voice</div>
            <VoiceSelect onChange={speechSettingsChanged} />
            <div className="row">
              <span className="grow small">Highlight each word</span>
              <Switch
                label="Highlight each word"
                on={st.speechHighlight !== false}
                onChange={(v) => settings.update({ speechHighlight: v })}
              />
            </div>
          </div>
        </Popover>
      )}
    </div>
  );
}
