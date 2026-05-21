// [B17 V3.2] useCallAudio — single source of truth для audio playback в
// CallDetailPage. Owns HTMLAudioElement (через `new Audio()` без JSX),
// resolves track paths via Tauri convertFileSrc, exposes state + handlers
// для AudioScrubber + InteractiveTranscript.

import { useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { getCallAudioPath } from '../api/calls';
import { humanError } from '../api/errors';

export type AudioTrack = 'mic' | 'system';

const PEAK_COUNT = 200;

export interface CallAudioState {
  activeTrack: AudioTrack;
  playing: boolean;
  currentTime: number;
  duration: number;
  micMissing: boolean;
  systemMissing: boolean;
  ready: boolean;
  error: string | null;
  /** [B17 V3.3] Real WAV peaks (length PEAK_COUNT), 0..1, для active track.
   *  null пока декодирование не завершено. */
  peaks: number[] | null;
}

export interface CallAudioActions {
  togglePlay: () => void;
  seek: (seconds: number) => void;
  switchTrack: (next: AudioTrack) => void;
}

export type CallAudioHandle = CallAudioState & CallAudioActions;

export function useCallAudio(callId: string, fallbackDuration = 0): CallAudioHandle {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  if (!audioRef.current && typeof window !== 'undefined') {
    audioRef.current = new Audio();
    audioRef.current.preload = 'metadata';
  }

  const [activeTrack, setActiveTrack] = useState<AudioTrack>('system');
  const [micSrc, setMicSrc] = useState<string | null>(null);
  const [systemSrc, setSystemSrc] = useState<string | null>(null);
  const [micMissing, setMicMissing] = useState(false);
  const [systemMissing, setSystemMissing] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(fallbackDuration);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // [B17 V3.3] Per-track decoded peak buckets — cached между swaps.
  const peaksCacheRef = useRef<Map<string, number[]>>(new Map());
  const [, setPeaksTick] = useState(0); // force re-render когда peaks ready

  // Load both track paths.
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
      const micPath = m.status === 'fulfilled' ? convertFileSrc(m.value) : null;
      const systemPath = s.status === 'fulfilled' ? convertFileSrc(s.value) : null;
      setMicSrc(micPath);
      setSystemSrc(systemPath);
      setMicMissing(m.status === 'rejected');
      setSystemMissing(s.status === 'rejected');
      if (m.status === 'rejected' && s.status === 'fulfilled') setActiveTrack('system');
      if (s.status === 'rejected' && m.status === 'fulfilled') setActiveTrack('mic');
      if (m.status === 'rejected' && s.status === 'rejected') {
        const reason =
          m.reason instanceof Error
            ? m.reason.message
            : s.reason instanceof Error
              ? s.reason.message
              : String(m.reason ?? s.reason);
        setError(humanError(reason));
      } else {
        setReady(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [callId]);

  // Bind audio events ONCE (audio element exists for whole hook lifetime).
  useEffect(() => {
    const el = audioRef.current;
    if (!el) return;
    const onTime = () => setCurrentTime(el.currentTime);
    const onDur = () => {
      if (Number.isFinite(el.duration) && el.duration > 0) setDuration(el.duration);
    };
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onEnded = () => setPlaying(false);
    el.addEventListener('timeupdate', onTime);
    el.addEventListener('durationchange', onDur);
    el.addEventListener('loadedmetadata', onDur);
    el.addEventListener('play', onPlay);
    el.addEventListener('pause', onPause);
    el.addEventListener('ended', onEnded);
    return () => {
      el.pause();
      el.removeEventListener('timeupdate', onTime);
      el.removeEventListener('durationchange', onDur);
      el.removeEventListener('loadedmetadata', onDur);
      el.removeEventListener('play', onPlay);
      el.removeEventListener('pause', onPause);
      el.removeEventListener('ended', onEnded);
    };
  }, []);

  // Sync src when activeTrack или paths change. Preserve currentTime + play
  // state across track switch.
  useEffect(() => {
    const el = audioRef.current;
    if (!el) return;
    const src = activeTrack === 'mic' ? micSrc : systemSrc;
    if (!src) return;
    if (el.src === src) return;
    const pos = el.currentTime;
    const wasPlaying = !el.paused;
    el.src = src;
    el.load();
    const restore = () => {
      try {
        el.currentTime = pos;
      } catch {
        /* may not be seekable yet */
      }
      if (wasPlaying) {
        void el.play().catch(() => undefined);
      }
      el.removeEventListener('loadedmetadata', restore);
    };
    el.addEventListener('loadedmetadata', restore);
  }, [activeTrack, micSrc, systemSrc]);

  const togglePlay = () => {
    const el = audioRef.current;
    if (!el || !el.src) return;
    if (el.paused) {
      void el.play().catch(() => undefined);
    } else {
      el.pause();
    }
  };

  const seek = (seconds: number) => {
    const el = audioRef.current;
    if (!el) return;
    if (!Number.isFinite(seconds) || seconds < 0) return;
    try {
      el.currentTime = seconds;
      setCurrentTime(seconds);
    } catch {
      /* ignore */
    }
  };

  const switchTrack = (next: AudioTrack) => {
    if (next === activeTrack) return;
    if (next === 'mic' && micMissing) return;
    if (next === 'system' && systemMissing) return;
    setActiveTrack(next);
  };

  // [B17 V3.3] Decode WAV → peak buckets per track. Cached в peaksCacheRef.
  // Async на background — UI пока показывает peaks=null (или предыдущие).
  useEffect(() => {
    const src = activeTrack === 'mic' ? micSrc : systemSrc;
    if (!src) return;
    if (peaksCacheRef.current.has(src)) {
      // Already decoded — force consumer re-read.
      setPeaksTick((n) => n + 1);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const peaks = await decodeWavPeaks(src, PEAK_COUNT);
        if (cancelled) return;
        peaksCacheRef.current.set(src, peaks);
        setPeaksTick((n) => n + 1);
      } catch (e) {
        console.warn('[useCallAudio] decode peaks failed', e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeTrack, micSrc, systemSrc]);

  const activeSrc = activeTrack === 'mic' ? micSrc : systemSrc;
  const peaks = activeSrc ? (peaksCacheRef.current.get(activeSrc) ?? null) : null;

  return {
    activeTrack,
    playing,
    currentTime,
    duration,
    micMissing,
    systemMissing,
    ready,
    error,
    peaks,
    togglePlay,
    seek,
    switchTrack,
  };
}

// [B17 V3.3] Fetch + decode WAV file → array of `count` peaks (max abs per
// bucket, normalized 0..1). Использует AudioContext.decodeAudioData. Heavy
// для длинных файлов но one-shot + cached.
async function decodeWavPeaks(url: string, count: number): Promise<number[]> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`fetch ${response.status}`);
  const buffer = await response.arrayBuffer();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const Ctx = window.AudioContext ?? (window as any).webkitAudioContext;
  const ctx: AudioContext = new Ctx();
  try {
    const audio = await ctx.decodeAudioData(buffer.slice(0));
    const channel = audio.getChannelData(0);
    const bucketSize = Math.max(1, Math.floor(channel.length / count));
    const peaks = new Array<number>(count).fill(0);
    let maxAbs = 0;
    for (let i = 0; i < count; i++) {
      const start = i * bucketSize;
      const end = Math.min(channel.length, start + bucketSize);
      let peak = 0;
      for (let j = start; j < end; j++) {
        const v = Math.abs(channel[j]!);
        if (v > peak) peak = v;
      }
      peaks[i] = peak;
      if (peak > maxAbs) maxAbs = peak;
    }
    // Normalize 0..1 (avoid divide-by-zero на тишине).
    if (maxAbs > 0) {
      for (let i = 0; i < count; i++) peaks[i]! /= maxAbs;
    }
    return peaks;
  } finally {
    void ctx.close().catch(() => undefined);
  }
}
