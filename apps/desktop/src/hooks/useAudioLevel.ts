// [B14] Live audio levels из Swift sidecar via Tauri event `audio:level`.
// Sidecar эмитит каждые 100ms {mic: 0..1, system: 0..1} RMS values. Hook
// поддерживает rolling history (140 bars) per channel + connected flag —
// true если event пришёл в последние 2 сек.

import { useEffect, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface AudioLevels {
  /** Rolling RMS history for mic channel, 0..1, length=bufferSize. */
  mic: number[];
  /** Rolling RMS history for system audio channel, 0..1. */
  system: number[];
  /** Timestamp of last event (performance.now()). 0 если никогда не пришло. */
  lastUpdate: number;
  /** True если event пришёл в последние 2 сек — иначе considered «stale». */
  connected: boolean;
}

interface RawPayload {
  mic: number;
  system: number;
}

const BUFFER_SIZE = 140;
const STALE_MS = 2000;

export function useAudioLevel(active: boolean, bufferSize = BUFFER_SIZE): AudioLevels {
  const [levels, setLevels] = useState<AudioLevels>(() => emptyLevels(bufferSize));
  const micRef = useRef<number[]>(new Array(bufferSize).fill(0));
  const sysRef = useRef<number[]>(new Array(bufferSize).fill(0));
  const lastUpdateRef = useRef<number>(0);
  const tickRef = useRef<number | null>(null);

  // Subscribe when active.
  useEffect(() => {
    if (!active) {
      // Reset on deactivate.
      micRef.current = new Array(bufferSize).fill(0);
      sysRef.current = new Array(bufferSize).fill(0);
      lastUpdateRef.current = 0;
      setLevels(emptyLevels(bufferSize));
      return;
    }
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    listen<RawPayload>('audio:level', (event) => {
      if (cancelled) return;
      const mic = clamp01(event.payload.mic);
      const sys = clamp01(event.payload.system);
      // Shift left, push new sample.
      const m = micRef.current;
      const s = sysRef.current;
      for (let i = 0; i < bufferSize - 1; i++) {
        m[i] = m[i + 1]!;
        s[i] = s[i + 1]!;
      }
      m[bufferSize - 1] = mic;
      s[bufferSize - 1] = sys;
      lastUpdateRef.current = performance.now();
    })
      .then((fn) => {
        if (cancelled) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch((e: unknown) => console.warn('listen audio:level failed', e));

    // RAF ticker to publish updates at ~30Hz (every 33ms). Keeps React renders
    // lower than the 100ms event rate would otherwise force.
    const tick = () => {
      if (cancelled) return;
      const now = performance.now();
      setLevels({
        mic: micRef.current.slice(),
        system: sysRef.current.slice(),
        lastUpdate: lastUpdateRef.current,
        connected:
          lastUpdateRef.current !== 0 && now - lastUpdateRef.current < STALE_MS,
      });
      tickRef.current = window.setTimeout(tick, 80);
    };
    tick();

    return () => {
      cancelled = true;
      unlisten?.();
      if (tickRef.current !== null) {
        window.clearTimeout(tickRef.current);
        tickRef.current = null;
      }
    };
  }, [active, bufferSize]);

  return levels;
}

function emptyLevels(size: number): AudioLevels {
  return {
    mic: new Array(size).fill(0),
    system: new Array(size).fill(0),
    lastUpdate: 0,
    connected: false,
  };
}

function clamp01(v: number): number {
  if (!Number.isFinite(v)) return 0;
  if (v < 0) return 0;
  if (v > 1) return 1;
  return v;
}
