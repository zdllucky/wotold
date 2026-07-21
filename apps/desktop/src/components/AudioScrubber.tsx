// Wotold v2 player dock — .player-dock / .player (uikit, см. wk-screens CallPlayer).
// useCallAudio() играет ОБА трека одновременно (mic + system) — Browser mixer
// сводит в один stream, нет track switcher. Бары рендерятся из combined peaks
// (fallback — псевдослучайная высота от seed пока peaks не декодированы).
//
// Layout (.player, слева направо):
//   [▶ accent-кружок] [00:04] [130 баров + playhead] [01:11]

import { useRef } from 'react';
import type { CallAudioHandle } from '../hooks/useCallAudio';
import { useElementWidth } from '../hooks/useElementWidth';
import { useI18n } from '../i18n';
import { DEFAULT_WAVE_BARS, waveBarCount } from '../lib/waveBars';
import { Icon } from '../ui';

interface AudioScrubberProps {
  audio: CallAudioHandle;
  /** seed для псевдослучайной высоты баров когда peaks ещё не декодированы. */
  seed: number;
  /** Скрыть плеер если false. */
  enabled?: boolean;
}

export function AudioScrubber({ audio, seed, enabled = true }: AudioScrubberProps) {
  const { t } = useI18n();
  const waveRef = useRef<HTMLDivElement | null>(null);
  // [UI-fix A] Адаптивное число баров от реальной ширины дорожки — фикс.
  // count на резиновом контейнере давал суб-пиксельные бары при ресайзе.
  // 0 (нет замера / jsdom) → DEFAULT_WAVE_BARS. Хук ДО early-returns.
  const waveWidth = useElementWidth(waveRef);
  if (!enabled) return null;
  if (audio.bothMissing) return null;
  const barCount = waveWidth > 0 ? waveBarCount(waveWidth) : DEFAULT_WAVE_BARS;

  const pct =
    audio.duration > 0
      ? Math.max(0, Math.min(1, audio.currentTime / audio.duration))
      : 0;

  const seekAt = (clientX: number) => {
    const el = waveRef.current;
    if (!el || !audio.duration) return;
    const r = el.getBoundingClientRect();
    const x = Math.max(0, Math.min(1, (clientX - r.left) / r.width));
    audio.seek(x * audio.duration);
  };

  // Click + drag scrubbing (как в прототипе CallPlayer).
  const onDown = (e: React.MouseEvent) => {
    e.preventDefault();
    seekAt(e.clientX);
    const move = (ev: MouseEvent) => seekAt(ev.clientX);
    const up = () => {
      document.removeEventListener('mousemove', move);
      document.removeEventListener('mouseup', up);
    };
    document.addEventListener('mousemove', move);
    document.addEventListener('mouseup', up);
  };

  // Высота баров: реальные peaks (combined mic+system, 0..1) либо
  // детерминированный fallback от seed (звонок узнаваем, но не загружен).
  const peaks = audio.peaks;
  const bars = Array.from({ length: barCount }, (_, i) => {
    const v =
      peaks && peaks.length > 0
        ? peaks[Math.floor((i / barCount) * peaks.length)] ?? 0
        : ((i * 53 + seed) % 18) / 18;
    return 4 + Math.round(v * 18);
  });

  return (
    <div className="player-dock">
      <div className="player">
        <button
          type="button"
          className="player-play"
          onClick={audio.togglePlay}
          disabled={!audio.ready}
          aria-label={audio.playing ? t('scrubber.pause') : t('scrubber.play')}
        >
          <Icon name={audio.playing ? 'pause' : 'play'} size={16} />
        </button>
        <span
          className="mono"
          style={{
            fontSize: 'var(--t-12)',
            color: 'var(--text-2)',
            width: 44,
            textAlign: 'right',
            flex: '0 0 auto',
          }}
        >
          {formatTime(audio.currentTime)}
        </span>
        <div
          className="player-wave"
          ref={waveRef}
          onMouseDown={onDown}
          role="slider"
          tabIndex={0}
          aria-label={t('scrubber.progressAria')}
          aria-valuemin={0}
          aria-valuemax={Math.floor(audio.duration)}
          aria-valuenow={Math.floor(audio.currentTime)}
          onKeyDown={(e) => {
            if (e.key === 'ArrowLeft') {
              e.preventDefault();
              audio.seek(Math.max(0, audio.currentTime - 5));
            } else if (e.key === 'ArrowRight') {
              e.preventDefault();
              audio.seek(Math.min(audio.duration, audio.currentTime + 5));
            }
          }}
        >
          {bars.map((h, i) => (
            <i
              key={i}
              style={{
                height: h,
                background:
                  i / barCount <= pct ? 'var(--accent)' : 'var(--border-strong)',
              }}
            />
          ))}
          <span className="player-head" style={{ left: pct * 100 + '%' }} />
        </div>
        <span
          className="mono"
          style={{
            fontSize: 'var(--t-12)',
            color: 'var(--text-faint)',
            width: 44,
            flex: '0 0 auto',
          }}
        >
          {formatTime(audio.duration)}
        </span>
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
