// [B17 V3.1] Sticky bottom audio scrubber pill per reference §5 (transcript
// artboard). Self-contained:
//   - hidden <audio> elements pre-loaded per track (mic + system)
//   - visible pill UI: round play btn (ink) + mono time + accent waveform с
//     progress fill + mono duration
//   - small track switcher above pill: "я · собеседник" toggle
//   - position: sticky bottom — float'ит над transcript / recap / tasks
//
// При смене track сохраняет playback position. Waveform — seeded из call.id
// (одинаковый seed → одинаковые bars между renders).

import { useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { getCallAudioPath } from '../api/calls';
import { humanError } from '../api/errors';
import { Waveform } from './Waveform';

type Track = 'mic' | 'system';

interface AudioScrubberProps {
  callId: string;
  /** Длительность звонка в секундах — для seed-стабильности + duration label. */
  durationSec: number;
  /** Если false (например status='failed') — компонент не рендерится. */
  enabled?: boolean;
}

interface SourceState {
  src: string | null;
  missing: boolean;
}

export function AudioScrubber({
  callId,
  durationSec,
  enabled = true,
}: AudioScrubberProps) {
  const [active, setActive] = useState<Track>('system');
  const [mic, setMic] = useState<SourceState>({ src: null, missing: false });
  const [system, setSystem] = useState<SourceState>({ src: null, missing: false });
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(durationSec || 0);
  const [error, setError] = useState<string | null>(null);

  const audioRef = useRef<HTMLAudioElement | null>(null);

  // Load both track paths in parallel.
  useEffect(() => {
    let cancelled = false;
    setError(null);
    (async () => {
      const results = await Promise.allSettled([
        getCallAudioPath(callId, 'mic'),
        getCallAudioPath(callId, 'system'),
      ]);
      if (cancelled) return;
      const m = results[0];
      const s = results[1];
      setMic(
        m.status === 'fulfilled'
          ? { src: convertFileSrc(m.value), missing: false }
          : { src: null, missing: true },
      );
      setSystem(
        s.status === 'fulfilled'
          ? { src: convertFileSrc(s.value), missing: false }
          : { src: null, missing: true },
      );
      // Если active track недоступен — auto-switch на доступный.
      if (m.status === 'rejected' && s.status === 'fulfilled') setActive('system');
      if (s.status === 'rejected' && m.status === 'fulfilled') setActive('mic');
      if (m.status === 'rejected' && s.status === 'rejected') {
        const msg =
          m.reason instanceof Error
            ? m.reason.message
            : s.reason instanceof Error
              ? s.reason.message
              : String(m.reason ?? s.reason);
        setError(humanError(msg));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [callId]);

  // Sync audio events.
  useEffect(() => {
    const el = audioRef.current;
    if (!el) return;
    const onTime = () => setCurrentTime(el.currentTime);
    const onDur = () => {
      if (Number.isFinite(el.duration)) setDuration(el.duration);
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
      el.removeEventListener('timeupdate', onTime);
      el.removeEventListener('durationchange', onDur);
      el.removeEventListener('loadedmetadata', onDur);
      el.removeEventListener('play', onPlay);
      el.removeEventListener('pause', onPause);
      el.removeEventListener('ended', onEnded);
    };
  }, [active]);

  if (!enabled) return null;

  const activeSrc = active === 'mic' ? mic.src : system.src;
  const bothMissing = mic.missing && system.missing;
  if (bothMissing && error) {
    return null;
  }

  const togglePlay = () => {
    const el = audioRef.current;
    if (!el || !activeSrc) return;
    if (el.paused) {
      void el.play().catch(() => undefined);
    } else {
      el.pause();
    }
  };

  const switchTrack = (next: Track) => {
    if (next === active) return;
    if (next === 'mic' && mic.missing) return;
    if (next === 'system' && system.missing) return;
    const el = audioRef.current;
    const pos = el?.currentTime ?? 0;
    const wasPlaying = el ? !el.paused : false;
    setActive(next);
    // Restore position + playback после src swap (async через next tick).
    requestAnimationFrame(() => {
      const next_el = audioRef.current;
      if (next_el) {
        next_el.currentTime = pos;
        if (wasPlaying) void next_el.play().catch(() => undefined);
      }
    });
  };

  const onWaveformClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const el = audioRef.current;
    if (!el || !duration) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    el.currentTime = ratio * duration;
    setCurrentTime(el.currentTime);
  };

  const progressPct = duration > 0 ? (currentTime / duration) * 100 : 0;
  const seed = hashId(callId);

  return (
    <div
      style={{
        position: 'sticky',
        bottom: 12,
        zIndex: 20,
        marginTop: 16,
        pointerEvents: 'auto',
      }}
    >
      {/* Track switcher — small mono labels above pill */}
      {!bothMissing && (
        <div
          style={{
            display: 'flex',
            justifyContent: 'center',
            gap: 12,
            marginBottom: 6,
            fontSize: 10,
            letterSpacing: '0.12em',
            textTransform: 'uppercase',
            fontFamily: 'var(--font-sans)',
            fontWeight: 600,
          }}
        >
          <button
            type="button"
            onClick={() => switchTrack('mic')}
            disabled={mic.missing}
            aria-pressed={active === 'mic'}
            style={{
              background: 'none',
              border: 'none',
              padding: '4px 8px',
              cursor: mic.missing ? 'not-allowed' : 'pointer',
              color: active === 'mic' ? 'var(--ink)' : 'var(--subtle)',
              opacity: mic.missing ? 0.3 : 1,
              borderRadius: 'var(--radius-sm)',
            }}
            title="Свой микрофон"
          >
            Я
          </button>
          <span className="subtle" style={{ alignSelf: 'center' }}>
            ·
          </span>
          <button
            type="button"
            onClick={() => switchTrack('system')}
            disabled={system.missing}
            aria-pressed={active === 'system'}
            style={{
              background: 'none',
              border: 'none',
              padding: '4px 8px',
              cursor: system.missing ? 'not-allowed' : 'pointer',
              color: active === 'system' ? 'var(--ink)' : 'var(--subtle)',
              opacity: system.missing ? 0.3 : 1,
              borderRadius: 'var(--radius-sm)',
            }}
            title="Звук собеседника (системный аудио)"
          >
            Собеседник
          </button>
        </div>
      )}

      {/* Pill */}
      <div
        style={{
          background: 'var(--paper)',
          border: '1px solid var(--line)',
          borderRadius: 999,
          padding: '8px 14px 8px 8px',
          display: 'flex',
          alignItems: 'center',
          gap: 12,
          boxShadow: '0 6px 24px rgba(26,22,18,0.08)',
          backdropFilter: 'blur(8px)',
        }}
      >
        <button
          type="button"
          onClick={togglePlay}
          disabled={!activeSrc}
          aria-label={playing ? 'Пауза' : 'Воспроизведение'}
          style={{
            width: 32,
            height: 32,
            borderRadius: '50%',
            background: 'var(--ink)',
            color: 'var(--paper)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontSize: 11,
            border: 'none',
            cursor: activeSrc ? 'pointer' : 'not-allowed',
            opacity: activeSrc ? 1 : 0.4,
            flexShrink: 0,
          }}
        >
          {playing ? '❚❚' : '▶'}
        </button>
        <div
          className="mono"
          style={{
            fontSize: 11,
            color: 'var(--muted)',
            flexShrink: 0,
            minWidth: 50,
          }}
        >
          {formatTime(currentTime)}
        </div>
        <div
          onClick={onWaveformClick}
          style={{
            flex: 1,
            height: 22,
            position: 'relative',
            cursor: 'pointer',
          }}
          role="slider"
          aria-label="Аудио прогресс"
          aria-valuemin={0}
          aria-valuemax={Math.floor(duration)}
          aria-valuenow={Math.floor(currentTime)}
        >
          {/* Background waveform — unplayed portion, low opacity */}
          <div
            style={{
              position: 'absolute',
              inset: 0,
              opacity: 0.25,
              color: 'var(--accent)',
            }}
          >
            <Waveform
              seed={seed}
              color="currentColor"
              width={600}
              height={22}
              count={160}
              gap={1.5}
            />
          </div>
          {/* Played portion — clipped по progress, full opacity */}
          <div
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              bottom: 0,
              width: `${progressPct}%`,
              overflow: 'hidden',
              color: 'var(--accent)',
            }}
          >
            <div
              style={{
                width: progressPct > 0 ? `${(100 / progressPct) * 100}%` : '100%',
                height: '100%',
              }}
            >
              <Waveform
                seed={seed}
                color="currentColor"
                width={600}
                height={22}
                count={160}
                gap={1.5}
              />
            </div>
          </div>
        </div>
        <div
          className="mono"
          style={{
            fontSize: 11,
            color: 'var(--muted)',
            flexShrink: 0,
            minWidth: 50,
            textAlign: 'right',
          }}
        >
          {formatTime(duration)}
        </div>
      </div>

      {activeSrc && (
        // eslint-disable-next-line jsx-a11y/media-has-caption
        <audio
          ref={audioRef}
          src={activeSrc}
          preload="metadata"
          style={{ display: 'none' }}
        />
      )}
    </div>
  );
}

function formatTime(sec: number): string {
  if (!Number.isFinite(sec) || sec < 0) return '00:00';
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = Math.floor(sec % 60);
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  }
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
}

function hashId(id: string): number {
  let h = 0;
  for (const ch of id) h = (h * 31 + ch.charCodeAt(0)) | 0;
  return Math.abs(h) % 1000;
}
