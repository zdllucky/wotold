// [B17 V3.2] useCallAudio — single source of truth для audio playback в
// CallDetailPage. Owns HTMLAudioElement (через `new Audio()` без JSX),
// resolves track paths via Tauri convertFileSrc, exposes state + handlers
// для AudioScrubber + InteractiveTranscript.

import { useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { getCallAudioPath } from '../api/calls';
import { humanError } from '../api/errors';

export type AudioTrack = 'mic' | 'system';

export interface CallAudioState {
  activeTrack: AudioTrack;
  playing: boolean;
  currentTime: number;
  duration: number;
  micMissing: boolean;
  systemMissing: boolean;
  ready: boolean;
  error: string | null;
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

  return {
    activeTrack,
    playing,
    currentTime,
    duration,
    micMissing,
    systemMissing,
    ready,
    error,
    togglePlay,
    seek,
    switchTrack,
  };
}
