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
import { Empty, Skeleton } from '../ui';
import {
  SpeakerCard,
  type SpeakerSample,
} from '../components/SpeakerCard';
import { useI18n } from '../i18n';
import { humanSpeakerLabel } from '../utils/callMeta';
import { SP_COLORS } from './CallDetailUtils';

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
  const { t } = useI18n();
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
      setError(t('speakers.needContactName'));
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
      setError(t('speakers.needContactFirst'));
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
    // [V8.1] 3 calling-card skeletons mimic финальные SpeakerCard'ы.
    return (
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))',
          gap: 14,
        }}
        aria-busy="true"
      >
        {[0, 1, 2].map((i) => (
          <div
            key={i}
            className="card"
            style={{
              padding: 14,
              display: 'flex',
              flexDirection: 'column',
              gap: 8,
              pointerEvents: 'none',
            }}
          >
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 10,
                marginBottom: 4,
              }}
            >
              <Skeleton width="32px" height="32px" radius="50%" />
              <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 6 }}>
                <Skeleton width="10ch" height="0.95em" />
                <Skeleton width="6ch" height="0.7em" />
              </div>
            </div>
            <Skeleton width="100%" height="0.75em" />
            <Skeleton width="60%" height="0.75em" />
          </div>
        ))}
      </div>
    );
  }
  if (speakers.length === 0) {
    return (
      <Empty
        title={t('speakers.emptyTitle')}
        description={t('speakers.emptyBody')}
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
          style={{ color: 'var(--danger)', fontFamily: 'var(--font)' }}
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
            {t('speakers.confirmedTitle', { n: mergedConfirmed.length })}
            {mergedConfirmed.length !== confirmed.length && (
              <span style={{ color: 'var(--text-3)', textTransform: 'none', marginLeft: 8 }}>
                {t('speakers.mergedVoices', { n: confirmed.length })}
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
                    borderBottom: '1px dotted var(--border-2)',
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
                        fontFamily: 'var(--font)',
                        fontSize: 16,
                        color: 'var(--text)',
                      }}
                    >
                      {first.contact_display_name}
                    </div>
                    <div className="muted" style={{ fontSize: 12 }}>
                      {group.speakers.length === 1
                        ? humanSpeakerLabel(first.speaker_tag)
                        : t('speakers.voiceMergedNote', {
                            tags: group.speakers
                              .map((s) => humanSpeakerLabel(s.speaker_tag))
                              .join(' + '),
                            n: group.speakers.length,
                          })}
                    </div>
                  </div>
                  <div style={{ display: 'flex', gap: 6 }}>
                    {group.speakers.map((s) => (
                      <button
                        key={s.id}
                        type="button"
                        className="btn btn--quiet btn--sm"
                        onClick={() => void handleUnbind(s)}
                        title={t('speakers.unbindAria', { label: humanSpeakerLabel(s.speaker_tag) })}
                      >
                        {group.speakers.length === 1
                          ? t('speakers.unbindOne')
                          : t('speakers.unbindLabeled', {
                              label: humanSpeakerLabel(s.speaker_tag),
                            })}
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

// [B17] Per-speaker first sample из raw_stt.json.
//
// [P5.3] Rewrite на per-track lookup из `mic.segments` + `system.segments`
// (вместо merged-only heuristic `tag === OWNER_TAG ? micSrc : systemSrc`).
// Heuristic ломался для anonymous mic speakers (post-P1.2 диаризация
// выделяет `speaker:N` на mic-дорожке) — они рендерились с systemSrc → тишина.
//
// Algorithm: для каждого speaker_tag собрать сегменты из обеих дорожек,
// pick track где найден longest segment (по text length). Если только в
// одной track → та track. Fallback на legacy merged-only heuristic если
// mic/system отсутствуют (старые звонки до P5.3 либо malformed JSON).
interface RawSttSegment {
  speakerTag: string;
  text: string;
  start: number;
  end: number;
}

interface RawSttTrack {
  segments?: unknown;
}

interface RawSttRoot {
  mic?: RawSttTrack;
  system?: RawSttTrack;
  merged?: unknown;
}

const MIN_SAMPLE_LEN = 5; // символов
const MAX_SAMPLE_LEN = 140;

/** Parse JSON array of raw segments — defensive, skip malformed entries. */
function parseSegments(raw: unknown): RawSttSegment[] {
  if (!Array.isArray(raw)) return [];
  const out: RawSttSegment[] = [];
  for (const s of raw) {
    if (!s || typeof s !== 'object') continue;
    const o = s as Record<string, unknown>;
    if (
      typeof o.speakerTag !== 'string' ||
      typeof o.text !== 'string' ||
      typeof o.start !== 'number' ||
      typeof o.end !== 'number'
    )
      continue;
    out.push({ speakerTag: o.speakerTag, text: o.text, start: o.start, end: o.end });
  }
  return out;
}

/** Pick longest segment by text length for given tag. */
function bestForTag(segments: RawSttSegment[], tag: string): RawSttSegment | null {
  let best: RawSttSegment | null = null;
  for (const s of segments) {
    if (s.speakerTag !== tag) continue;
    if (!best || s.text.length > best.text.length) best = s;
  }
  return best;
}

export function extractSamples(
  json: string | null,
  micSrc: string | null,
  systemSrc: string | null,
): Map<string, SpeakerSample> {
  const out = new Map<string, SpeakerSample>();
  if (!json) return out;
  try {
    const data = JSON.parse(json) as RawSttRoot;
    const micSegs = parseSegments(data.mic?.segments);
    const sysSegs = parseSegments(data.system?.segments);
    const hasPerTrack = micSegs.length > 0 || sysSegs.length > 0;

    if (hasPerTrack) {
      // [P5.3] Per-track lookup — correct attribution для anonymous mic speakers.
      const tags = new Set<string>();
      for (const s of micSegs) tags.add(s.speakerTag);
      for (const s of sysSegs) tags.add(s.speakerTag);

      for (const tag of tags) {
        const micBest = bestForTag(micSegs, tag);
        const sysBest = bestForTag(sysSegs, tag);
        let pick: { seg: RawSttSegment; src: string | null } | null = null;
        if (micBest && sysBest) {
          // Tie-break: longer text wins; equal length → mic preferred (owner heuristic).
          pick =
            sysBest.text.length > micBest.text.length
              ? { seg: sysBest, src: systemSrc }
              : { seg: micBest, src: micSrc };
        } else if (micBest) {
          pick = { seg: micBest, src: micSrc };
        } else if (sysBest) {
          pick = { seg: sysBest, src: systemSrc };
        }
        if (!pick || !pick.src) continue;
        const trimmed = pick.seg.text.trim();
        if (trimmed.length < MIN_SAMPLE_LEN) continue;
        out.set(tag, {
          text:
            trimmed.length > MAX_SAMPLE_LEN
              ? trimmed.slice(0, MAX_SAMPLE_LEN - 1) + '…'
              : trimmed,
          start: pick.seg.start,
          end: pick.seg.end,
          src: pick.src,
        });
      }
      return out;
    }

    // Legacy fallback: merged-only JSON (старые звонки) → heuristic
    // OWNER → mic / else → system. Может silent'ить anonymous mic speakers,
    // но это backwards-compat path для записей сделанных ДО P5.3.
    const merged = parseSegments(data.merged);
    const buckets = new Map<string, RawSttSegment[]>();
    for (const s of merged) {
      const arr = buckets.get(s.speakerTag) ?? [];
      if (arr.length < 5) {
        arr.push(s);
        buckets.set(s.speakerTag, arr);
      }
    }
    for (const [tag, arr] of buckets.entries()) {
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
