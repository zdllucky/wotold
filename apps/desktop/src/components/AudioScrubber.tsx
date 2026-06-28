// [B17 V3.4] Sticky bottom audio scrubber pill — dumb presentation component.
// useCallAudio() hook играет ОБА трека одновременно (mic + system), нет
// track switcher. Browser mixer сводит их в один stream.
//
// Layout pill (слева направо):
//   [▶] [00:04] [combined peaks waveform] [SpeakerChip|пауза] [01:11]

import type { CallAudioHandle } from '../hooks/useCallAudio';
import type { CurrentSpeakerInfo } from '../utils/callMeta';
import { useI18n } from '../i18n';
import { Waveform } from './Waveform';

const SP_COLORS = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];

// Re-export для удобства callers'ов (CallDetailPage).
export type { CurrentSpeakerInfo };

interface AudioScrubberProps {
  audio: CallAudioHandle;
  /** seed для seeded random fallback (когда peaks не загружены). */
  seed: number;
  /** Скрыть scrubber если false. */
  enabled?: boolean;
  /** Текущий говорящий (computed на CallDetailPage из rawStt + currentTime). */
  currentSpeaker: CurrentSpeakerInfo | null;
  /** Клик по speaker chip → switch на transcript tab (auto-scroll к active row
   *  делает InteractiveTranscript сам через currentTime sync). */
  onJumpToSpeaker?: () => void;
}

export function AudioScrubber({
  audio,
  seed,
  enabled = true,
  currentSpeaker,
  onJumpToSpeaker,
}: AudioScrubberProps) {
  const { t } = useI18n();
  if (!enabled) return null;
  if (audio.bothMissing) return null;

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
        // [B17 V3.8] marginTop:auto в flex-column parent pushes scrubber
        // к низу .app-main scroll viewport даже при коротком контенте.
        // Sticky bottom держит у низа при длинном контенте/scrolling.
        marginTop: 'auto',
        pointerEvents: 'auto',
      }}
    >
      {/* Pill */}
      <div
        style={{
          background: 'var(--paper)',
          border: '1px solid var(--border)',
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
          aria-label={audio.playing ? t('scrubber.pause') : t('scrubber.play')}
          style={{
            width: 32,
            height: 32,
            borderRadius: '50%',
            background: 'var(--text)',
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
            color: 'var(--text-3)',
            flexShrink: 0,
            minWidth: 50,
          }}
        >
          {formatTime(audio.currentTime)}
        </div>

        {/* Waveform + progress fill via clip-path overlay. Use real peaks
            when decoded, fallback to seeded random. */}
        <div
          onClick={onWaveformClick}
          style={{
            flex: 1,
            height: 22,
            position: 'relative',
            cursor: 'pointer',
            minWidth: 80,
          }}
          role="slider"
          aria-label={t('scrubber.progressAria')}
          aria-valuemin={0}
          aria-valuemax={Math.floor(audio.duration)}
          aria-valuenow={Math.floor(audio.currentTime)}
        >
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
              peaks={audio.peaks ?? undefined}
              color="currentColor"
              width={600}
              height={22}
              count={200}
              gap={1.5}
            />
          </div>
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
              peaks={audio.peaks ?? undefined}
              color="currentColor"
              width={600}
              height={22}
              count={200}
              gap={1.5}
            />
          </div>
        </div>

        {/* Speaker chip — fixed-width контейнер, чтобы waveform не прыгал
            при смене speaker/пауза. 140px = max chip width + breathing. */}
        <div
          style={{
            width: 140,
            flexShrink: 0,
            display: 'flex',
            justifyContent: 'center',
          }}
        >
          <SpeakerChip
            speaker={currentSpeaker}
            onClick={onJumpToSpeaker}
          />
        </div>

        <div
          className="mono"
          style={{
            fontSize: 11,
            color: 'var(--text-3)',
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

// SpeakerChip — current speaker indicator inline (без border/bg карточки).
// Click → onJumpToSpeaker. Если none — italic «пауза».
function SpeakerChip({
  speaker,
  onClick,
}: {
  speaker: CurrentSpeakerInfo | null;
  onClick?: () => void;
}) {
  const { t } = useI18n();
  if (!speaker) {
    return (
      <span
        className="muted"
        style={{
          fontFamily: 'var(--font)',
          fontStyle: 'italic',
          fontSize: 12,
        }}
      >
        {t('scrubber.pausedItalic')}
      </span>
    );
  }
  const color = SP_COLORS[speaker.colorIdx % SP_COLORS.length];
  const firstName = speaker.displayName.split(/\s+/)[0] ?? speaker.displayName;
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={!onClick}
      title={
        onClick
          ? t('scrubber.speakerJumpTitle', { name: speaker.displayName })
          : undefined
      }
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 6,
        padding: 0,
        background: 'none',
        border: 'none',
        fontSize: 12,
        fontWeight: 500,
        color: 'var(--text)',
        fontFamily: 'var(--font)',
        letterSpacing: '-0.005em',
        cursor: onClick ? 'pointer' : 'default',
        whiteSpace: 'nowrap',
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        maxWidth: '100%',
        transition: 'color var(--duration-fast)',
      }}
      onMouseEnter={(e) => {
        if (onClick) e.currentTarget.style.color = 'var(--accent)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.color = 'var(--text)';
      }}
    >
      <span
        style={{
          width: 16,
          height: 16,
          borderRadius: '50%',
          background: color,
          color: '#fff',
          display: 'inline-flex',
          alignItems: 'center',
          justifyContent: 'center',
          fontSize: 8,
          fontWeight: 600,
          letterSpacing: '0.02em',
          flexShrink: 0,
        }}
      >
        {initials(speaker.displayName)}
      </span>
      <span
        style={{
          overflow: 'hidden',
          textOverflow: 'ellipsis',
        }}
      >
        {firstName}
      </span>
    </button>
  );
}

function initials(name: string): string {
  return (
    name
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .join('') || '·'
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
