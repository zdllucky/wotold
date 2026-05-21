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
import { MiniWave } from './Waveform';
import type { Contact } from '../api/contacts';
import type { CallSpeakerView } from '../api/speakers';
import { Select } from '../ui';

const SP_COLORS = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];

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
      if (el.currentTime >= sample.end) {
        el.pause();
        el.currentTime = sample.start;
        setPlaying(false);
      }
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

  const handleToggleSample = () => {
    const el = audioRef.current;
    if (!el || !sample) return;
    if (playing) {
      el.pause();
      el.currentTime = sample.start;
      return;
    }
    // На случай если уже перемотали за пределы окна — вернуть в start.
    if (
      el.currentTime < sample.start ||
      el.currentTime > sample.end - 0.05
    ) {
      el.currentTime = sample.start;
    }
    void el.play().catch(() => {
      /* autoplay policy / file missing — silent fail, кнопка дальше будет
         показывать ▶ так как onPause очистит state */
    });
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
          Голос {idx + 1} из {total}
        </div>
        <div className="small-caps muted">{speaker.speaker_tag}</div>
      </div>

      <div className="title" style={{ fontSize: 28, marginBottom: 28 }}>
        Кто этот голос?
      </div>

      {/* Sample bubble row */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 22,
          padding: '16px 0',
          borderTop: '1px solid var(--line-soft)',
          borderBottom: '1px solid var(--line-soft)',
          marginBottom: 22,
        }}
      >
        <div
          style={{
            width: 56,
            height: 56,
            borderRadius: '50%',
            background: color,
            color: 'var(--paper)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontFamily: 'var(--font-mono)',
            fontWeight: 600,
            fontSize: 16,
            letterSpacing: '0.04em',
            flexShrink: 0,
          }}
        >
          {speaker.speaker_tag}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            data-selectable
            style={{
              fontFamily: 'var(--font-serif)',
              fontStyle: 'italic',
              fontSize: 16,
              marginBottom: 8,
              color: 'var(--ink)',
              letterSpacing: '-0.01em',
              display: '-webkit-box',
              WebkitLineClamp: 2,
              WebkitBoxOrient: 'vertical',
              overflow: 'hidden',
            }}
          >
            «{sample?.text ?? 'голос распознан · послушать сэмпл'}»
          </div>
          <div style={{ height: 22, color }}>
            <MiniWave
              seed={speaker.id.charCodeAt(0) + idx * 11}
              color="currentColor"
              width={400}
              height={22}
              count={64}
            />
          </div>
        </div>
        <button
          type="button"
          className="btn btn--ghost"
          style={{ padding: '8px 12px', fontSize: 12, flexShrink: 0 }}
          onClick={handleToggleSample}
          disabled={!sample}
          aria-label={playing ? 'Остановить сэмпл' : 'Послушать сэмпл'}
          title={!sample ? 'Аудиосэмпл недоступен' : undefined}
        >
          {playing
            ? '◼ стоп'
            : sample
              ? `▶ ${sampleDurationSec ?? '·'} сек`
              : '▶ сэмпл'}
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
            Похоже на
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
                  fontFamily: 'var(--font-serif)',
                  fontSize: 17,
                  letterSpacing: '-0.01em',
                  color: 'var(--ink)',
                }}
              >
                {suggestionName}
              </div>
              <div className="muted" style={{ fontSize: 12 }}>
                {suggestedContact?.role ?? '—'}
                {speaker.suggestion_source &&
                  ` · ${sourceLabel(speaker.suggestion_source)}`}
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
                <span className="small-caps">Уверенность</span>
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
            Выбрать контакт
          </label>
          <Select
            id={`speaker-${speaker.id}-pick`}
            value={pickedContactId}
            options={[
              { value: '', label: '— не выбран —' },
              ...contacts.map((c) => ({
                value: c.id,
                label: c.is_owner ? `${c.display_name} (владелец)` : c.display_name,
              })),
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
            background: 'var(--bg-2)',
            borderRadius: 8,
            marginBottom: 18,
          }}
        >
          <div className="field" style={{ marginBottom: 10 }}>
            <label className="field-label" htmlFor={`speaker-${speaker.id}-new`}>
              Имя нового контакта
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
              color: 'var(--ink-2)',
              marginBottom: 10,
            }}
          >
            <input
              type="checkbox"
              checked={newConsent}
              onChange={(e) => onChangeNewConsent(e.target.checked)}
            />
            <span>Запоминать голос для авто-определения</span>
          </label>
          <div style={{ display: 'flex', gap: 8 }}>
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={onCancelAdd}
              disabled={busyAdd}
            >
              Отмена
            </button>
            <button
              type="button"
              className="btn btn--primary btn--sm"
              onClick={onSubmitNewContact}
              disabled={busyAdd || !newName.trim()}
            >
              {busyAdd ? 'Добавляем…' : 'Добавить и привязать'}
            </button>
          </div>
        </div>
      )}

      {/* Action row */}
      <div
        style={{
          display: 'flex',
          gap: 10,
          borderTop: '1px solid var(--line-soft)',
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
          ✓{' '}
          {primaryTarget
            ? `Да, это ${primaryTarget.name}`
            : showPicker
              ? 'Выбери контакт ниже'
              : 'Добавь новый контакт'}
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
            Не он/она
          </button>
        )}
        {!adding && (
          <button type="button" className="btn btn--ghost" onClick={onStartAdd}>
            Новый контакт
          </button>
        )}
      </div>

      <div style={{ marginTop: 14, textAlign: 'center', fontSize: 12 }}>
        <span className="muted">
          Подтверждение сохранит голос в профиль контакта (если включена опция){' '}
        </span>
      </div>
    </div>
  );
}

function sourceLabel(s: string | null): string {
  if (!s) return '';
  if (s === 'both') return 'голос + LLM';
  if (s === 'embedding') return 'голос';
  if (s === 'llm') return 'LLM';
  return s;
}
