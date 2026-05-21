// [B17] SpeakersSection — calling-card flow для confirm speakers (M3.5).
//
// SpeakerCard вынесен в `components/SpeakerCard.tsx` — тот же компонент
// используется и inline-popup'ом из транскрипта (TranscriptConfirmModal).
//
// R2 паспорта: no auto-bind, только явный confirm.

import { useEffect, useMemo, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';

import { humanError } from '../api/errors';
import { getCallAudioPath, readCallArtifact } from '../api/calls';
import { createContact, listContacts, type Contact } from '../api/contacts';
import {
  confirmCallSpeaker,
  listCallSpeakers,
  unbindCallSpeaker,
  type CallSpeakerView,
} from '../api/speakers';
import { Empty } from '../ui';
import {
  SpeakerCard,
  type SpeakerSample,
} from '../components/SpeakerCard';
import { humanSpeakerLabel } from '../utils/callMeta';

const SP_COLORS = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];
const OWNER_TAG = 'owner';

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

// [V5.2] speakerColorIdx больше не нужен — merged-confirmed list использует
// порядок группы (i) напрямую для палитры.

interface SpeakersSectionProps {
  callId: string;
  /** Вызывается после каждой мутации (confirm/unbind/createAndConfirm) чтобы
   *  родитель (CallDetailPage) обновил Header ParticipantsRow + транскрипт. */
  onSpeakersChanged?: () => void;
}

export function SpeakersSection({
  callId,
  onSpeakersChanged,
}: SpeakersSectionProps) {
  const [speakers, setSpeakers] = useState<CallSpeakerView[] | null>(null);
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [pickFor, setPickFor] = useState<Record<string, string>>({});
  const [addingFor, setAddingFor] = useState<string | null>(null);
  const [newName, setNewName] = useState('');
  const [newConsent, setNewConsent] = useState(false);
  const [busyAdd, setBusyAdd] = useState(false);
  const [rawSttJson, setRawSttJson] = useState<string | null>(null);
  const [micSrc, setMicSrc] = useState<string | null>(null);
  const [systemSrc, setSystemSrc] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const [s, c, raw, mic, sys] = await Promise.allSettled([
        listCallSpeakers(callId),
        listContacts(),
        readCallArtifact(callId, 'raw_stt'),
        getCallAudioPath(callId, 'mic'),
        getCallAudioPath(callId, 'system'),
      ]);
      if (s.status === 'fulfilled') setSpeakers(s.value);
      if (c.status === 'fulfilled') setContacts(c.value);
      if (raw.status === 'fulfilled') setRawSttJson(raw.value);
      else setRawSttJson(null);
      setMicSrc(mic.status === 'fulfilled' ? convertFileSrc(mic.value) : null);
      setSystemSrc(sys.status === 'fulfilled' ? convertFileSrc(sys.value) : null);
      setError(null);
    } catch (e) {
      setError(humanError(e));
    }
  };

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [callId]);

  const samplesByTag = useMemo(
    () => extractSamples(rawSttJson, micSrc, systemSrc),
    [rawSttJson, micSrc, systemSrc],
  );

  const notifyChanged = async () => {
    await refresh();
    onSpeakersChanged?.();
  };

  const handleAddAsContact = async (s: CallSpeakerView) => {
    const trimmed = newName.trim();
    if (!trimmed) {
      setError('Введи имя контакта.');
      return;
    }
    setBusyAdd(true);
    setError(null);
    try {
      const contact = await createContact({
        display_name: trimmed,
        identifiers: [],
        attributes: newConsent ? { consent_voice: 'true' } : {},
      });
      await confirmCallSpeaker(s.id, contact.id);
      setAddingFor(null);
      setNewName('');
      setNewConsent(false);
      await notifyChanged();
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusyAdd(false);
    }
  };

  const handleConfirm = async (s: CallSpeakerView, contactId?: string) => {
    const picked = contactId ?? pickFor[s.id] ?? s.suggestion_contact_id ?? '';
    if (!picked) {
      setError('Сначала выбери контакт из списка.');
      return;
    }
    try {
      await confirmCallSpeaker(s.id, picked);
      setPickFor((m) => ({ ...m, [s.id]: '' }));
      await notifyChanged();
    } catch (e) {
      setError(humanError(e));
    }
  };

  const handleReject = (s: CallSpeakerView) => {
    setPickFor((m) => ({ ...m, [s.id]: '' }));
  };

  const handleUnbind = async (s: CallSpeakerView) => {
    try {
      await unbindCallSpeaker(s.id);
      await notifyChanged();
    } catch (e) {
      setError(humanError(e));
    }
  };

  if (speakers === null) {
    return <p className="muted">Загрузка…</p>;
  }
  if (speakers.length === 0) {
    return (
      <Empty
        title="Участники не распознаны"
        description="В этом звонке не обнаружено отдельных голосов, либо обработка ещё идёт."
      />
    );
  }

  const unconfirmed = speakers.filter((s) => !s.confirmed);
  const confirmed = speakers.filter((s) => s.confirmed);

  // [V5.2] Группируем confirmed по contact_id — STT диаризация может одного
  // человека разбить на 2+ speaker_tag (S1+S2) при смене громкости/тона.
  // Юзер хочет видеть «одного человека = одна карточка» с пометкой что
  // STT нашёл N голосов под него. Каждый отдельный отвязывается per-tag.
  const mergedConfirmed: Array<{
    contactId: string;
    speakers: CallSpeakerView[];
  }> = [];
  {
    const byContact = new Map<string, CallSpeakerView[]>();
    for (const s of confirmed) {
      const cid = s.contact_id ?? `__no_contact_${s.id}`;
      const arr = byContact.get(cid) ?? [];
      arr.push(s);
      byContact.set(cid, arr);
    }
    for (const [contactId, arr] of byContact.entries()) {
      // Сортируем speaker_tag для стабильного отображения «S0, S1, owner».
      arr.sort((a, b) => a.speaker_tag.localeCompare(b.speaker_tag));
      mergedConfirmed.push({ contactId, speakers: arr });
    }
  }

  return (
    <div>
      {error && (
        <p
          role="alert"
          style={{ color: 'var(--signal)', fontFamily: 'var(--font-sans)' }}
        >
          {error}
        </p>
      )}

      {unconfirmed.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 18 }}>
          {unconfirmed.map((s, idx) => (
            <SpeakerCard
              key={s.id}
              speaker={s}
              idx={idx}
              total={unconfirmed.length}
              contacts={contacts}
              sample={samplesByTag.get(s.speaker_tag) ?? null}
              pickedContactId={pickFor[s.id] ?? s.suggestion_contact_id ?? ''}
              onPick={(id) => setPickFor((m) => ({ ...m, [s.id]: id }))}
              onConfirm={(contactId) => void handleConfirm(s, contactId)}
              onReject={() => handleReject(s)}
              adding={addingFor === s.id}
              newName={newName}
              newConsent={newConsent}
              busyAdd={busyAdd}
              onStartAdd={() => {
                setAddingFor(s.id);
                setNewName('');
                setNewConsent(false);
              }}
              onCancelAdd={() => {
                setAddingFor(null);
                setNewName('');
                setNewConsent(false);
              }}
              onChangeNewName={setNewName}
              onChangeNewConsent={setNewConsent}
              onSubmitNewContact={() => void handleAddAsContact(s)}
            />
          ))}
        </div>
      )}

      {confirmed.length > 0 && (
        <div style={{ marginTop: unconfirmed.length > 0 ? 36 : 0 }}>
          <div className="small-caps" style={{ marginBottom: 14 }}>
            Подтверждены · {mergedConfirmed.length}
            {mergedConfirmed.length !== confirmed.length && (
              <span style={{ color: 'var(--muted)', textTransform: 'none', marginLeft: 8 }}>
                ({confirmed.length} голосов объединены)
              </span>
            )}
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            {mergedConfirmed.map((group, i) => {
              const color = SP_COLORS[i % SP_COLORS.length];
              const first = group.speakers[0]!;
              return (
                <div
                  key={group.contactId}
                  style={{
                    display: 'grid',
                    gridTemplateColumns: '40px 1fr auto',
                    gap: 14,
                    alignItems: 'center',
                    padding: '10px 0',
                    borderBottom: '1px dotted var(--line-soft)',
                  }}
                >
                  <span
                    className="sp-avatar"
                    style={{ background: color, width: 36, height: 36, fontSize: 12 }}
                  >
                    {initials(first.contact_display_name ?? first.speaker_tag)}
                  </span>
                  <div>
                    <div
                      data-selectable
                      style={{
                        fontFamily: 'var(--font-serif)',
                        fontSize: 16,
                        color: 'var(--ink)',
                      }}
                    >
                      {first.contact_display_name}
                    </div>
                    <div className="muted" style={{ fontSize: 12 }}>
                      {group.speakers.length === 1
                        ? humanSpeakerLabel(first.speaker_tag)
                        : `${group.speakers
                            .map((s) => humanSpeakerLabel(s.speaker_tag))
                            .join(' + ')} · распознавание разделило на ${group.speakers.length}`}
                    </div>
                  </div>
                  <div style={{ display: 'flex', gap: 6 }}>
                    {group.speakers.map((s) => (
                      <button
                        key={s.id}
                        type="button"
                        className="btn btn--quiet btn--sm"
                        onClick={() => void handleUnbind(s)}
                        title={`Отвязать ${humanSpeakerLabel(s.speaker_tag)}`}
                      >
                        {group.speakers.length === 1
                          ? 'Отвязать'
                          : `Отвязать ${humanSpeakerLabel(s.speaker_tag)}`}
                      </button>
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

// [B17] Per-speaker first sample из raw_stt.json.merged segments.
// Возвращает map speaker_tag → {text, start, end, src}. Src по правилу:
//   - owner_tag → mic
//   - прочие → system
// (та же диаризация что и в pipeline cluster extraction).
interface RawSttSegment {
  speakerTag: string;
  text: string;
  start: number;
  end: number;
}

const MIN_SAMPLE_LEN = 5; // символов
const MAX_SAMPLE_LEN = 140;

export function extractSamples(
  json: string | null,
  micSrc: string | null,
  systemSrc: string | null,
): Map<string, SpeakerSample> {
  const out = new Map<string, SpeakerSample>();
  if (!json) return out;
  try {
    const data = JSON.parse(json) as { merged?: unknown };
    if (!Array.isArray(data.merged)) return out;
    // Bucket первых 5 сегментов на тег.
    const buckets = new Map<string, RawSttSegment[]>();
    for (const s of data.merged) {
      if (!s || typeof s !== 'object') continue;
      const o = s as Record<string, unknown>;
      if (
        typeof o.speakerTag !== 'string' ||
        typeof o.text !== 'string' ||
        typeof o.start !== 'number' ||
        typeof o.end !== 'number'
      )
        continue;
      const tag = o.speakerTag;
      const arr = buckets.get(tag) ?? [];
      if (arr.length < 5) {
        arr.push({ speakerTag: tag, text: o.text, start: o.start, end: o.end });
        buckets.set(tag, arr);
      }
    }
    for (const [tag, arr] of buckets.entries()) {
      // Берём самый длинный текстом из первых 5.
      const best = arr.reduce(
        (a, b) => (b.text.length > a.text.length ? b : a),
        arr[0]!,
      );
      const trimmed = best.text.trim();
      if (trimmed.length < MIN_SAMPLE_LEN) continue;
      const src = tag === OWNER_TAG ? micSrc : systemSrc;
      if (!src) continue;
      out.set(tag, {
        text:
          trimmed.length > MAX_SAMPLE_LEN
            ? trimmed.slice(0, MAX_SAMPLE_LEN - 1) + '…'
            : trimmed,
        start: best.start,
        end: best.end,
        src,
      });
    }
  } catch {
    /* corrupt raw_stt — empty map → SpeakerCard рендерится без playback */
  }
  return out;
}
