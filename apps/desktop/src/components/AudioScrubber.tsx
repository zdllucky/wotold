// [B17 V3.2] Sticky bottom audio scrubber pill — dumb presentation component.
// State + handlers приходят из useCallAudio() hook (см. CallDetailPage).
// Это позволяет InteractiveTranscript подписываться на тот же audio state
// для highlight current row + click-to-seek.
//
// Изменения vs V3.1:
//   - state lifted в useCallAudio hook (single audio element для всей page)
//   - progress fill через clip-path (bars align с background идеально)
//   - listeners биндятся в hook (надёжно)

import type { CallAudioHandle } from '../hooks/useCallAudio';
import { Waveform } from './Waveform';

interface AudioScrubberProps {
  audio: CallAudioHandle;
  /** seed для stable waveform shape — обычно hash от call.id. */
  seed: number;
  /** Скрыть scrubber если false (например call status='failed'). */
  enabled?: boolean;
}

export function AudioScrubber({ audio, seed, enabled = true }: AudioScrubberProps) {
  if (!enabled) return null;
  if (audio.micMissing && audio.systemMissing) return null;

  const onWaveformClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!audio.duration) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    audio.seek(ratio * audio.duration);
  };

  const progressPct =
    audio.duration > 0 ? (audio.currentTime / audio.duration) * 100 : 0;

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
      {/* Track switcher */}
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
          onClick={() => audio.switchTrack('mic')}
          disabled={audio.micMissing}
          aria-pressed={audio.activeTrack === 'mic'}
          style={{
            background: 'none',
            border: 'none',
            padding: '4px 8px',
            cursor: audio.micMissing ? 'not-allowed' : 'pointer',
            color: audio.activeTrack === 'mic' ? 'var(--ink)' : 'var(--subtle)',
            opacity: audio.micMissing ? 0.3 : 1,
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
          onClick={() => audio.switchTrack('system')}
          disabled={audio.systemMissing}
          aria-pressed={audio.activeTrack === 'system'}
          style={{
            background: 'none',
            border: 'none',
            padding: '4px 8px',
            cursor: audio.systemMissing ? 'not-allowed' : 'pointer',
            color:
              audio.activeTrack === 'system' ? 'var(--ink)' : 'var(--subtle)',
            opacity: audio.systemMissing ? 0.3 : 1,
            borderRadius: 'var(--radius-sm)',
          }}
          title="Звук собеседника (системный аудио)"
        >
          Собеседник
        </button>
      </div>

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
          onClick={audio.togglePlay}
          disabled={!audio.ready}
          aria-label={audio.playing ? 'Пауза' : 'Воспроизведение'}
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
            cursor: audio.ready ? 'pointer' : 'not-allowed',
            opacity: audio.ready ? 1 : 0.4,
            flexShrink: 0,
            transition: 'transform var(--duration-fast) var(--ease-out-expo)',
          }}
        >
          {audio.playing ? '❚❚' : '▶'}
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
          {formatTime(audio.currentTime)}
        </div>

        {/* Waveform + progress fill via clip-path overlay. Both layers
            render at SAME canvas dimensions, clip-path reveals only played
            portion in full opacity — bars perfectly aligned. */}
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
          aria-valuemax={Math.floor(audio.duration)}
          aria-valuenow={Math.floor(audio.currentTime)}
        >
          {/* Background — unplayed, low opacity */}
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
          {/* Foreground — played, full opacity, clipped right */}
          <div
            style={{
              position: 'absolute',
              inset: 0,
              color: 'var(--accent)',
              clipPath: `inset(0 ${Math.max(0, 100 - progressPct)}% 0 0)`,
              transition: 'clip-path 100ms linear',
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
          {formatTime(audio.duration)}
        </div>
      </div>
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
