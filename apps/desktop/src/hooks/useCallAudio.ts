// [B17 V3.4] useCallAudio — sync playback двух треков (mic + system)
// одновременно. Browser mixer сам сводит. Если один отсутствует — играет
// доступный.
//
// State: currentTime/duration берётся из «master» (system если есть, else
// mic). togglePlay/seek проксируется обоим audio элементам. Peaks combined
// (element-wise max двух декодированных треков).

import { useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { getCallAudioPath } from '../api/calls';
import { humanError } from '../api/errors';
import { decodeWavPeaks } from '../lib/audioPeaks';
import { useI18n } from '../i18n';

const PEAK_COUNT = 200;

export interface CallAudioState {
  playing: boolean;
  currentTime: number;
  duration: number;
  /** True если оба трека (mic + system) отсутствуют — scrubber тогда скрыт. */
  bothMissing: boolean;
  ready: boolean;
  error: string | null;
  /** Combined peaks (element-wise max между mic + system), 0..1. */
  peaks: number[] | null;
}

export interface CallAudioActions {
  togglePlay: () => void;
  seek: (seconds: number) => void;
}

export type CallAudioHandle = CallAudioState & CallAudioActions;

export function useCallAudio(callId: string, fallbackDuration = 0): CallAudioHandle {
  // [TD-25] Тексты ошибок берутся из словаря — humanError требует `t`.
  const { t } = useI18n();
  const micRef = useRef<HTMLAudioElement | null>(null);
  const systemRef = useRef<HTMLAudioElement | null>(null);
  if (!micRef.current && typeof window !== 'undefined') {
    micRef.current = new Audio();
    micRef.current.preload = 'metadata';
  }
  if (!systemRef.current && typeof window !== 'undefined') {
    systemRef.current = new Audio();
    systemRef.current.preload = 'metadata';
  }

  const [micSrc, setMicSrc] = useState<string | null>(null);
  const [systemSrc, setSystemSrc] = useState<string | null>(null);
  const [micMissing, setMicMissing] = useState(false);
  const [systemMissing, setSystemMissing] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(fallbackDuration);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const peaksCacheRef = useRef<Map<string, number[]>>(new Map());
  const [, setPeaksTick] = useState(0);

  // Load paths.
  useEffect(() => {
    let cancelled = false;
    setError(null);
    setReady(false);
    (async () => {
      const results = await Promise.allSettled([
        getCallAudioPath(callId, 'mic'),
        getCallAudioPath(callId, 'system'),
      ]);
      if (cancelled) return;
      const m = results[0];
      const s = results[1];
      setMicSrc(m.status === 'fulfilled' ? convertFileSrc(m.value) : null);
      setSystemSrc(s.status === 'fulfilled' ? convertFileSrc(s.value) : null);
      setMicMissing(m.status === 'rejected');
      setSystemMissing(s.status === 'rejected');
      if (m.status === 'rejected' && s.status === 'rejected') {
        const reason =
          m.reason instanceof Error
            ? m.reason.message
            : s.reason instanceof Error
              ? s.reason.message
              : String(m.reason ?? s.reason);
        setError(humanError(reason, t));
      } else {
        setReady(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [callId]);

  // Bind events на оба элемента. Master = system (если есть), else mic —
  // источник currentTime / duration / play state. Слейв тоже играет, sync
  // по currentTime через seek.
  useEffect(() => {
    const mic = micRef.current;
    const system = systemRef.current;
    if (!mic || !system) return;
    const onTime = () => {
      const t = pickMaster(mic, system, micMissing, systemMissing).currentTime;
      setCurrentTime(t);
    };
    const onDur = () => {
      const m = pickMaster(mic, system, micMissing, systemMissing);
      if (Number.isFinite(m.duration) && m.duration > 0) setDuration(m.duration);
    };
    const onPlay = () => setPlaying(true);
    const onPause = () => {
      // Считаем «paused» только когда ОБА paused. Иначе пользователь увидит
      // ❚❚ на secondary pause.
      if (mic.paused && system.paused) setPlaying(false);
    };
    const onEnded = () => {
      if (mic.ended || mic.paused) {
        if (system.ended || system.paused) setPlaying(false);
      }
    };
    const targets: HTMLAudioElement[] = [];
    if (!micMissing) targets.push(mic);
    if (!systemMissing) targets.push(system);
    for (const el of targets) {
      el.addEventListener('timeupdate', onTime);
      el.addEventListener('durationchange', onDur);
      el.addEventListener('loadedmetadata', onDur);
      el.addEventListener('play', onPlay);
      el.addEventListener('pause', onPause);
      el.addEventListener('ended', onEnded);
    }
    return () => {
      for (const el of targets) {
        try {
          el.pause();
        } catch {
          /* noop */
        }
        el.removeEventListener('timeupdate', onTime);
        el.removeEventListener('durationchange', onDur);
        el.removeEventListener('loadedmetadata', onDur);
        el.removeEventListener('play', onPlay);
        el.removeEventListener('pause', onPause);
        el.removeEventListener('ended', onEnded);
      }
    };
  }, [micMissing, systemMissing]);

  // Sync src при изменении путей.
  useEffect(() => {
    const el = micRef.current;
    if (!el || !micSrc || el.src === micSrc) return;
    el.src = micSrc;
    el.load();
  }, [micSrc]);
  useEffect(() => {
    const el = systemRef.current;
    if (!el || !systemSrc || el.src === systemSrc) return;
    el.src = systemSrc;
    el.load();
  }, [systemSrc]);

  // Decode peaks для обоих треков и combine (element-wise max).
  useEffect(() => {
    if (!micSrc && !systemSrc) return;
    let cancelled = false;
    const decodeIfNeeded = async (src: string) => {
      if (peaksCacheRef.current.has(src)) return peaksCacheRef.current.get(src)!;
      const peaks = await decodeWavPeaks(src, PEAK_COUNT);
      if (cancelled) return null;
      peaksCacheRef.current.set(src, peaks);
      return peaks;
    };
    void (async () => {
      try {
        const promises: Array<Promise<number[] | null>> = [];
        if (micSrc) promises.push(decodeIfNeeded(micSrc));
        if (systemSrc) promises.push(decodeIfNeeded(systemSrc));
        const results = await Promise.allSettled(promises);
        if (cancelled) return;
        setPeaksTick((n) => n + 1);
        void results; // peaks через cache + tick
      } catch (e) {
        console.warn('[useCallAudio] peaks decode failed', e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [micSrc, systemSrc]);

  const peaks = combinePeaks(
    micSrc ? peaksCacheRef.current.get(micSrc) ?? null : null,
    systemSrc ? peaksCacheRef.current.get(systemSrc) ?? null : null,
  );

  const togglePlay = () => {
    const mic = micRef.current;
    const system = systemRef.current;
    if (!mic || !system) return;
    const anyPlaying = !mic.paused || !system.paused;
    if (anyPlaying) {
      if (!mic.paused) mic.pause();
      if (!system.paused) system.pause();
    } else {
      // Sync currentTime before play (drift compensation).
      const master = pickMaster(mic, system, micMissing, systemMissing);
      const pos = master.currentTime;
      if (!micMissing && Math.abs(mic.currentTime - pos) > 0.05) {
        try {
          mic.currentTime = pos;
        } catch {
          /* ignore */
        }
      }
      if (!systemMissing && Math.abs(system.currentTime - pos) > 0.05) {
        try {
          system.currentTime = pos;
        } catch {
          /* ignore */
        }
      }
      if (!micMissing) void mic.play().catch(() => undefined);
      if (!systemMissing) void system.play().catch(() => undefined);
    }
  };

  const seek = (seconds: number) => {
    if (!Number.isFinite(seconds) || seconds < 0) return;
    const mic = micRef.current;
    const system = systemRef.current;
    if (!micMissing && mic) {
      try {
        mic.currentTime = seconds;
      } catch {
        /* ignore */
      }
    }
    if (!systemMissing && system) {
      try {
        system.currentTime = seconds;
      } catch {
        /* ignore */
      }
    }
    setCurrentTime(seconds);
  };

  return {
    playing,
    currentTime,
    duration,
    bothMissing: micMissing && systemMissing,
    ready,
    error,
    peaks,
    togglePlay,
    seek,
  };
}

function pickMaster(
  mic: HTMLAudioElement,
  system: HTMLAudioElement,
  micMissing: boolean,
  systemMissing: boolean,
): HTMLAudioElement {
  if (!systemMissing) return system;
  if (!micMissing) return mic;
  return system;
}

function combinePeaks(
  a: number[] | null,
  b: number[] | null,
): number[] | null {
  if (!a && !b) return null;
  if (!a) return b;
  if (!b) return a;
  const len = Math.min(a.length, b.length);
  const out = new Array<number>(len);
  for (let i = 0; i < len; i++) {
    out[i] = Math.max(a[i] ?? 0, b[i] ?? 0);
  }
  return out;
}
