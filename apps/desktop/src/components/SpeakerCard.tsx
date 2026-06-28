// [B17] SpeakerCard — переиспользуемая «calling-card» карточка для confirm
// flow. Раньше жила inline в SpeakersSection; вынесена чтобы тот же UI
// мог использоваться из inline-popup'а в транскрипте (когда юзер кликает
// на не-определённого спикера).
//
// Sample playback: «▶ сэмпл» проигрывает реальный фрагмент аудио из
// mic.wav/system.wav через скрытый <audio>. Sample выбирается по
// speaker_tag (owner→mic, прочие→system). Если sample не передан —
// кнопка отключена.

import { useEffect, useMemo, useRef, useState } from 'react';
import { Waveform } from './Waveform';
import type { Contact } from '../api/contacts';
import type { CallSpeakerView } from '../api/speakers';
import { decodeWavPeaksRange } from '../lib/audioPeaks';
import { useI18n } from '../i18n';
import { Select } from '../ui';
import { humanSpeakerLabel, shortSpeakerLabel } from '../utils/callMeta';

const SP_COLORS = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];

/** [P-fix8] Кол-во баров реальной скраб-дорожки сэмпла. */
const SAMPLE_BAR_COUNT = 72;

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

function speakerColorIdx(s: CallSpeakerView, idx: number): number {
  if (s.speaker_tag === 'owner' || s.speaker_tag === 'S0') return 0;
  return (idx + 1) % 5;
}

/** Образец голоса — для proigrыvания + цитаты в карточке. */
export interface SpeakerSample {
  text: string;
  /** В секундах от начала файла. */
  start: number;
  end: number;
  /** Готовый src для `<audio>` (через convertFileSrc). */
  src: string;
}

export interface SpeakerCardProps {
  speaker: CallSpeakerView;
  idx: number;
  total: number;
  contacts: Contact[];
  sample: SpeakerSample | null;
  pickedContactId: string;
  onPick: (id: string) => void;
  onConfirm: (contactId?: string) => void;
  onReject: () => void;
  adding: boolean;
  newName: string;
  newConsent: boolean;
  busyAdd: boolean;
  onStartAdd: () => void;
  onCancelAdd: () => void;
  onChangeNewName: (v: string) => void;
  onChangeNewConsent: (v: boolean) => void;
  onSubmitNewContact: () => void;
}

export function SpeakerCard({
  speaker,
  idx,
  total,
  contacts,
  sample,
  pickedContactId,
  onPick,
  onConfirm,
  onReject,
  adding,
  newName,
  newConsent,
  busyAdd,
  onStartAdd,
  onCancelAdd,
  onChangeNewName,
  onChangeNewConsent,
  onSubmitNewContact,
}: SpeakerCardProps) {
  const { t } = useI18n();
  const color = SP_COLORS[speakerColorIdx(speaker, idx) % SP_COLORS.length];
  const suggestionContactName = speaker.suggestion_contact_display_name;
  const suggestionScore = speaker.suggestion_score ?? 0;
  const suggestedContact = contacts.find(
    (c) => c.id === speaker.suggestion_contact_id,
  );
  const pickedContact = contacts.find((c) => c.id === pickedContactId);

  // [B17 V5] «Не он/она» переключает карточку в picker mode для этого
  // speaker'а (внутреннее состояние) — иначе юзер кликнул, suggestion
  // остался, а picker'а нет, и кнопка «✓ Подтвердить» disabled.
  // Сбрасывается если speaker.id меняется (для модала где prop меняется).
  const [suggestionRejected, setSuggestionRejected] = useState(false);
  useEffect(() => {
    setSuggestionRejected(false);
  }, [speaker.id]);

  const suggestionName = suggestionRejected ? null : suggestionContactName;
  const showPicker = !suggestionName && contacts.length > 0;
  // Кого подтверждаем primary-кнопкой:
  //   - suggestion активен → suggestion contact
  //   - иначе picked
  const primaryTarget = useMemo(() => {
    if (suggestionName && speaker.suggestion_contact_id) {
      return {
        name: suggestionName.split(/\s+/)[0] ?? suggestionName,
        contactId: speaker.suggestion_contact_id,
      };
    }
    if (pickedContact) {
      return {
        name: pickedContact.display_name.split(/\s+/)[0] ?? pickedContact.display_name,
        contactId: pickedContact.id,
      };
    }
    return null;
  }, [suggestionName, speaker.suggestion_contact_id, pickedContact]);

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  // [P-fix8] Реальная скраб-дорожка сэмпла: peaks диапазона + позиция playback.
  const [samplePeaks, setSamplePeaks] = useState<number[] | null>(null);
  const [pos, setPos] = useState(0);
  const waveRef = useRef<HTMLDivElement | null>(null);
  const draggingRef = useRef(false);
  const reducedMotion =
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
      : false;
  const waveSeed = speaker.id.charCodeAt(0) + idx * 11;
  const sampleDur = sample ? Math.max(0.001, sample.end - sample.start) : 1;
  const progressPct = sample
    ? Math.max(0, Math.min(100, ((pos - sample.start) / sampleDur) * 100))
    : 0;

  // Stop playback при размонтаже / смене sample.
  useEffect(() => {
    const el = audioRef.current;
    return () => {
      if (el) {
        el.pause();
        el.currentTime = 0;
      }
    };
  }, [sample?.src, sample?.start, sample?.end]);

  // Watcher: останавливаем когда дошли до end sample'а.
  useEffect(() => {
    const el = audioRef.current;
    if (!el || !sample) return;
    const onTime = () => {
      // [P-fix6] Если playback оказался ДО окна сэмпла (seek не сел — гонка с
      // загрузкой метаданных большого WAV → играем с 0 = «промах»), докручиваем
      // на start. Сходится за один tick как только seek доступен.
      if (el.currentTime < sample.start - 0.5) {
        el.currentTime = sample.start;
        setPos(sample.start);
        return;
      }
      if (el.currentTime >= sample.end) {
        el.pause();
        el.currentTime = sample.start;
        setPos(sample.start);
        setPlaying(false);
        return;
      }
      // [P-fix8] Двигаем прогресс скраб-дорожки за позицией playback.
      setPos(el.currentTime);
    };
    const onEnded = () => setPlaying(false);
    const onPause = () => setPlaying(false);
    const onPlay = () => setPlaying(true);
    el.addEventListener('timeupdate', onTime);
    el.addEventListener('ended', onEnded);
    el.addEventListener('pause', onPause);
    el.addEventListener('play', onPlay);
    return () => {
      el.removeEventListener('timeupdate', onTime);
      el.removeEventListener('ended', onEnded);
      el.removeEventListener('pause', onPause);
      el.removeEventListener('play', onPlay);
    };
  }, [sample]);

  // [P-fix8] Декодим реальные peaks участка сэмпла [start,end] (Web Audio).
  // До готовности — seeded MiniWave-плейсхолдер (peaks=null). Сброс позиции
  // на start при смене сэмпла.
  useEffect(() => {
    if (!sample?.src) {
      setSamplePeaks(null);
      return;
    }
    let cancelled = false;
    setSamplePeaks(null);
    setPos(sample.start);
    decodeWavPeaksRange(sample.src, SAMPLE_BAR_COUNT, sample.start, sample.end)
      .then((p) => {
        if (!cancelled) setSamplePeaks(p);
      })
      .catch(() => {
        if (!cancelled) setSamplePeaks(null);
      });
    return () => {
      cancelled = true;
    };
  }, [sample?.src, sample?.start, sample?.end]);

  // [P-fix8] Seek в окне сэмпла + (опц.) play. readiness-guard как в toggle.
  const seekTo = (target: number, autoplay: boolean) => {
    const el = audioRef.current;
    if (!el || !sample) return;
    const clamped = Math.max(sample.start, Math.min(sample.end - 0.05, target));
    const apply = () => {
      try {
        el.currentTime = clamped;
      } catch {
        /* not seekable yet — watcher докрутит */
      }
      setPos(clamped);
      if (autoplay && el.paused) void el.play().catch(() => {});
    };
    if (el.readyState >= 1) apply();
    else {
      el.addEventListener('loadedmetadata', apply, { once: true });
      el.load();
    }
  };

  const clientXToTime = (clientX: number): number => {
    const lane = waveRef.current;
    if (!lane || !sample) return sample?.start ?? 0;
    const rect = lane.getBoundingClientRect();
    const f = rect.width > 0 ? (clientX - rect.left) / rect.width : 0;
    return sample.start + Math.max(0, Math.min(1, f)) * (sample.end - sample.start);
  };

  const onScrubPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!sample) return;
    draggingRef.current = true;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    seekTo(clientXToTime(e.clientX), true);
  };
  const onScrubPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current || !sample) return;
    seekTo(clientXToTime(e.clientX), true);
  };
  const onScrubPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    draggingRef.current = false;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
  };
  const onScrubKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (!sample) return;
    const cur = audioRef.current?.currentTime ?? sample.start;
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      seekTo(cur - 1, false);
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      seekTo(cur + 1, false);
    }
  };

  const handleToggleSample = () => {
    const el = audioRef.current;
    if (!el || !sample) return;
    if (playing) {
      el.pause();
      el.currentTime = sample.start;
      return;
    }
    // [P-fix6] Seek + play только когда метаданные (duration) готовы — иначе
    // currentTime=start игнорируется браузером (нельзя сикать без duration) и
    // playback стартует с 0 → «промах»/«не туда». Для больших WAV (28МБ) на
    // свежей модалке метаданные часто ещё не подгружены к моменту клика → это
    // и есть «то не играет, то промахивается». Watcher (выше) дополнительно
    // докрутит если seek всё же не сел.
    const seekAndPlay = () => {
      const target = Number.isFinite(el.duration)
        ? Math.min(sample.start, Math.max(0, el.duration - 0.1))
        : sample.start;
      try {
        el.currentTime = target;
      } catch {
        /* seek may throw if not seekable yet — watcher докрутит */
      }
      void el.play().catch(() => {
        /* autoplay policy / file missing — silent fail, onPause очистит state */
      });
    };
    if (el.readyState >= 1 /* HAVE_METADATA → duration известна, можно сикать */) {
      seekAndPlay();
    } else {
      el.addEventListener('loadedmetadata', seekAndPlay, { once: true });
      el.load();
    }
  };

  const sampleDurationSec = sample ? Math.max(1, Math.round(sample.end - sample.start)) : null;

  return (
    <div className="index-card" style={{ position: 'relative', maxWidth: 720 }}>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'baseline',
          marginBottom: 6,
        }}
      >
        <div className="small-caps">
          {t('speakers.cardEyebrow', { idx: idx + 1, total })}
        </div>
        <div className="small-caps muted">{humanSpeakerLabel(speaker.speaker_tag)}</div>
      </div>

      <div className="title" style={{ fontSize: 28, marginBottom: 28 }}>
        {t('speakers.cardTitle')}
      </div>

      {/* Sample bubble row */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 22,
          padding: '16px 0',
          borderTop: '1px solid var(--border-2)',
          borderBottom: '1px solid var(--border-2)',
          marginBottom: 22,
        }}
      >
        <div
          style={{
            width: 56,
            height: 56,
            borderRadius: '50%',
            background: color,
            color: 'var(--panel)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontFamily: 'var(--mono)',
            fontWeight: 600,
            fontSize: 16,
            letterSpacing: '0.04em',
            flexShrink: 0,
          }}
        >
          {shortSpeakerLabel(speaker.speaker_tag)}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            data-selectable
            style={{
              fontFamily: 'var(--font)',
              fontStyle: 'italic',
              fontSize: 16,
              marginBottom: 8,
              color: 'var(--text)',
              letterSpacing: '-0.01em',
              display: '-webkit-box',
              WebkitLineClamp: 2,
              WebkitBoxOrient: 'vertical',
              overflow: 'hidden',
            }}
          >
            «{sample?.text ?? t('speakers.cardSampleFallback')}»
          </div>
          {/* [P-fix8] Реальная скраб-дорожка сэмпла. Два слоя Waveform (dim +
              clip-path прогресс) поверх реальных peaks участка; клик/драг =
              перемотка в окне [start,end]. Паттерн как в AudioScrubber. */}
          <div
            ref={waveRef}
            role="slider"
            tabIndex={sample ? 0 : -1}
            aria-label={t('speakers.sampleScrubAria')}
            aria-valuemin={0}
            aria-valuemax={sample ? Math.round(sampleDur) : 0}
            aria-valuenow={sample ? Math.round(Math.max(0, pos - sample.start)) : 0}
            aria-disabled={!sample}
            onPointerDown={onScrubPointerDown}
            onPointerMove={onScrubPointerMove}
            onPointerUp={onScrubPointerUp}
            onKeyDown={onScrubKeyDown}
            style={{
              position: 'relative',
              height: 24,
              color: 'var(--accent)',
              cursor: sample ? 'pointer' : 'default',
              touchAction: 'none',
              outlineOffset: 2,
            }}
          >
            <div style={{ position: 'absolute', inset: 0, opacity: 0.28 }}>
              <Waveform
                seed={waveSeed}
                peaks={samplePeaks ?? undefined}
                color="currentColor"
                width={400}
                height={24}
                count={SAMPLE_BAR_COUNT}
              />
            </div>
            <div
              style={{
                position: 'absolute',
                inset: 0,
                clipPath: `inset(0 ${Math.max(0, 100 - progressPct)}% 0 0)`,
                transition: reducedMotion ? undefined : 'clip-path 100ms linear',
              }}
            >
              <Waveform
                seed={waveSeed}
                peaks={samplePeaks ?? undefined}
                color="currentColor"
                width={400}
                height={24}
                count={SAMPLE_BAR_COUNT}
              />
            </div>
          </div>
        </div>
        <button
          type="button"
          className="btn btn--ghost"
          style={{ padding: '8px 12px', fontSize: 12, flexShrink: 0 }}
          onClick={handleToggleSample}
          disabled={!sample}
          aria-label={playing ? t('speakers.sampleStopAria') : t('speakers.samplePlayAria')}
          title={!sample ? t('speakers.sampleUnavailable') : undefined}
        >
          {playing
            ? t('speakers.sampleStop')
            : sample
              ? t('speakers.samplePlay', { sec: sampleDurationSec ?? '·' })
              : t('speakers.samplePlayFallback')}
        </button>
        {/* Hidden audio element — один на карточку, srcset by sample.src. */}
        {sample?.src && (
          <audio
            ref={audioRef}
            src={sample.src}
            preload="metadata"
            style={{ display: 'none' }}
          />
        )}
      </div>

      {/* Suggestion */}
      {suggestionName && (
        <>
          <div className="small-caps" style={{ marginBottom: 10 }}>
            {t('speakers.suggestion')}
          </div>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 14,
              marginBottom: 24,
              flexWrap: 'wrap',
            }}
          >
            <div
              className="sp-avatar"
              style={{
                background: color,
                width: 38,
                height: 38,
                fontSize: 12,
              }}
            >
              {initials(suggestionName)}
            </div>
            <div style={{ flex: 1, minWidth: 200 }}>
              <div
                style={{
                  fontFamily: 'var(--font)',
                  fontSize: 17,
                  letterSpacing: '-0.01em',
                  color: 'var(--text)',
                }}
              >
                {suggestionName}
              </div>
              <div className="muted" style={{ fontSize: 12 }}>
                {suggestedContact?.role ?? t('speakers.suggestionRoleNone')}
                {speaker.suggestion_source &&
                  ` · ${sourceLabel(speaker.suggestion_source, t)}`}
              </div>
            </div>
            <div style={{ width: 120 }}>
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  marginBottom: 4,
                  fontSize: 11,
                }}
              >
                <span className="small-caps">{t('speakers.confidence')}</span>
                <span className="mono">{Math.round(suggestionScore * 100)}%</span>
              </div>
              <div className="conf">
                <div
                  className="conf-fill"
                  style={{ width: `${suggestionScore * 100}%` }}
                />
              </div>
            </div>
          </div>
        </>
      )}

      {/* Picker (показывается если suggestion не активен) */}
      {showPicker && (
        <div className="field" style={{ marginBottom: 18 }}>
          <label className="field-label" htmlFor={`speaker-${speaker.id}-pick`}>
            {t('speakers.pickContact')}
          </label>
          <Select
            id={`speaker-${speaker.id}-pick`}
            value={pickedContactId}
            searchable={contacts.length > 5}
            searchPlaceholder={t('speakers.pickPlaceholder')}
            options={[
              { value: '', label: t('speakers.pickerNone') },
              ...contacts.map((c) => {
                const detailParts = [c.role, c.org].filter(
                  (s): s is string => !!s && s.trim().length > 0,
                );
                const description = detailParts.join(' · ') || undefined;
                const searchText = [c.display_name, c.role, c.org]
                  .filter(Boolean)
                  .join(' ');
                return {
                  value: c.id,
                  label: c.is_owner
                    ? `${c.display_name} (${t('contacts.owner')})`
                    : c.display_name,
                  description,
                  searchText,
                };
              }),
            ]}
            onChange={(v) => onPick(v)}
          />
        </div>
      )}

      {/* Inline new-contact form */}
      {adding && (
        <div
          style={{
            padding: 14,
            background: 'var(--sunken)',
            borderRadius: 8,
            marginBottom: 18,
          }}
        >
          <div className="field" style={{ marginBottom: 10 }}>
            <label className="field-label" htmlFor={`speaker-${speaker.id}-new`}>
              {t('speakers.newContactName')}
            </label>
            <input
              id={`speaker-${speaker.id}-new`}
              type="text"
              className="input input--box"
              autoFocus
              placeholder="Иван Петров"
              value={newName}
              onChange={(e) => onChangeNewName(e.target.value)}
            />
          </div>
          <label
            style={{
              display: 'flex',
              alignItems: 'flex-start',
              gap: 8,
              fontSize: 13,
              color: 'var(--text-2)',
              marginBottom: 10,
            }}
          >
            <input
              type="checkbox"
              checked={newConsent}
              onChange={(e) => onChangeNewConsent(e.target.checked)}
            />
            <span>{t('speakers.rememberVoice')}</span>
          </label>
          <div style={{ display: 'flex', gap: 8 }}>
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={onCancelAdd}
              disabled={busyAdd}
            >
              {t('common.cancel')}
            </button>
            <button
              type="button"
              className="btn btn--primary btn--sm"
              onClick={onSubmitNewContact}
              disabled={busyAdd || !newName.trim()}
            >
              {busyAdd ? t('speakers.addingAndBinding') : t('speakers.addAndBind')}
            </button>
          </div>
        </div>
      )}

      {/* Action row */}
      <div
        style={{
          display: 'flex',
          gap: 10,
          borderTop: '1px solid var(--border-2)',
          paddingTop: 18,
          flexWrap: 'wrap',
        }}
      >
        <button
          type="button"
          className="btn btn--primary"
          style={{ flex: 1, justifyContent: 'center', minWidth: 200 }}
          onClick={() => {
            if (primaryTarget) {
              onConfirm(primaryTarget.contactId);
            }
          }}
          disabled={!primaryTarget}
        >
          {primaryTarget
            ? t('speakers.confirmYes', { name: primaryTarget.name })
            : showPicker
              ? t('speakers.confirmPickBelow')
              : t('speakers.confirmAddNew')}
        </button>
        {suggestionName && (
          <button
            type="button"
            className="btn btn--ghost"
            onClick={() => {
              // [V5] Внутренний reject: убираем suggestion с UI и переходим
              // в picker mode для этого speaker'а. Caller-callback тоже
              // вызываем (для совместимости / clear'а parent state).
              setSuggestionRejected(true);
              onReject();
            }}
          >
            {t('speakers.notHimHer')}
          </button>
        )}
        {!adding && (
          <button type="button" className="btn btn--ghost" onClick={onStartAdd}>
            {t('speakers.newContact')}
          </button>
        )}
      </div>

      <div style={{ marginTop: 14, textAlign: 'center', fontSize: 12 }}>
        <span className="muted">{t('speakers.finePrint')}</span>
      </div>
    </div>
  );
}

type TFn = ReturnType<typeof useI18n>['t'];

function sourceLabel(s: string | null, t: TFn): string {
  if (!s) return '';
  if (s === 'both') return t('speakers.sourceVoiceLlm');
  if (s === 'embedding') return t('speakers.sourceVoice');
  if (s === 'llm') return t('speakers.sourceLlm');
  return s;
}
