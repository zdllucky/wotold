// [B17] SpeakersSection — exact match per docs/design/atelier-v2/_reference/atelier-2.jsx §7.
//
// Per-speaker calling-card:
//   - Header bilines: "Голос N из M" + "из звонка · {title}" small-caps
//   - Title 28 "Кто этот голос?"
//   - Sample row: 56×56 avatar (S{N}) + italic-serif quoted sample + MiniWave + "▶ 4 сек"
//   - "Похоже на" → 38×38 sp-avatar + serif name + role muted + 120px conf bar
//   - Three buttons: "✓ Да, это X" (primary stretch) · "Не она" ghost · "Новый контакт" ghost
//   - Footer hint with underline "подробнее"
//
// R2 паспорта: no auto-bind, всё через явное подтверждение пользователем.

import { useEffect, useMemo, useState } from 'react';
import { humanError } from '../api/errors';
import { readCallArtifact } from '../api/calls';
import { createContact, listContacts, type Contact } from '../api/contacts';
import {
  confirmCallSpeaker,
  listCallSpeakers,
  unbindCallSpeaker,
  type CallSpeakerView,
} from '../api/speakers';
import { Empty } from '../ui';
import { MiniWave } from '../components/Waveform';

interface SpeakersSectionProps {
  callId: string;
}

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
  return ((idx + 1) % 5);
}

export function SpeakersSection({ callId }: SpeakersSectionProps) {
  const [speakers, setSpeakers] = useState<CallSpeakerView[] | null>(null);
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [pickFor, setPickFor] = useState<Record<string, string>>({});
  const [addingFor, setAddingFor] = useState<string | null>(null);
  const [newName, setNewName] = useState('');
  const [newConsent, setNewConsent] = useState(false);
  const [busyAdd, setBusyAdd] = useState(false);
  const [rawSttJson, setRawSttJson] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const [s, c, raw] = await Promise.allSettled([
        listCallSpeakers(callId),
        listContacts(),
        readCallArtifact(callId, 'raw_stt'),
      ]);
      if (s.status === 'fulfilled') setSpeakers(s.value);
      if (c.status === 'fulfilled') setContacts(c.value);
      if (raw.status === 'fulfilled') setRawSttJson(raw.value);
      else setRawSttJson(null);
      setError(null);
    } catch (e) {
      setError(humanError(e));
    }
  };

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [callId]);

  // [B17] Per-speaker first sample text — parsed once from raw_stt.json.
  // Picks the longest segment in the first 5 from this speaker for richer sample.
  const samplesByTag = useMemo(() => extractSamples(rawSttJson), [rawSttJson]);

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
      await refresh();
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
      await refresh();
    } catch (e) {
      setError(humanError(e));
    }
  };

  const handleReject = async (s: CallSpeakerView) => {
    // Reject just clears the suggestion locally — speaker stays unconfirmed.
    setPickFor((m) => ({ ...m, [s.id]: '' }));
  };

  const handleUnbind = async (s: CallSpeakerView) => {
    try {
      await unbindCallSpeaker(s.id);
      await refresh();
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
        <div
          style={{ display: 'flex', flexDirection: 'column', gap: 18 }}
        >
          {unconfirmed.map((s, idx) => (
            <SpeakerCard
              key={s.id}
              speaker={s}
              idx={idx}
              total={unconfirmed.length}
              contacts={contacts}
              sampleText={samplesByTag.get(s.speaker_tag) ?? null}
              pickedContactId={pickFor[s.id] ?? s.suggestion_contact_id ?? ''}
              onPick={(id) => setPickFor((m) => ({ ...m, [s.id]: id }))}
              onConfirm={(contactId) => void handleConfirm(s, contactId)}
              onReject={() => void handleReject(s)}
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
            Подтверждены · {confirmed.length}
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            {confirmed.map((s, i) => {
              const color = SP_COLORS[speakerColorIdx(s, i) % SP_COLORS.length];
              return (
                <div
                  key={s.id}
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
                    {initials(s.contact_display_name ?? s.speaker_tag)}
                  </span>
                  <div>
                    <div
                      style={{
                        fontFamily: 'var(--font-serif)',
                        fontSize: 16,
                        color: 'var(--ink)',
                      }}
                    >
                      {s.contact_display_name}
                    </div>
                    <div className="muted" style={{ fontSize: 12 }}>
                      Голос {s.speaker_tag}
                    </div>
                  </div>
                  <button
                    type="button"
                    className="btn btn--quiet btn--sm"
                    onClick={() => void handleUnbind(s)}
                  >
                    Отвязать
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

interface SpeakerCardProps {
  speaker: CallSpeakerView;
  idx: number;
  total: number;
  contacts: Contact[];
  sampleText: string | null;
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

function SpeakerCard({
  speaker,
  idx,
  total,
  contacts,
  sampleText,
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
  const suggestionName = speaker.suggestion_contact_display_name;
  const suggestionScore = speaker.suggestion_score ?? 0;
  const suggestedContact = contacts.find(
    (c) => c.id === speaker.suggestion_contact_id,
  );
  const pickedContact = contacts.find((c) => c.id === pickedContactId);

  return (
    <div
      className="index-card"
      style={{ position: 'relative', maxWidth: 720 }}
    >
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
        <div className="small-caps muted">
          {speaker.speaker_tag}
        </div>
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
            «{sampleText ?? 'голос распознан · послушать сэмпл'}»
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
          aria-label="Послушать сэмпл"
        >
          ▶ сэмпл
        </button>
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

      {/* Picker */}
      {!suggestionName && contacts.length > 0 && (
        <div className="field" style={{ marginBottom: 18 }}>
          <label
            className="field-label"
            htmlFor={`speaker-${speaker.id}-pick`}
          >
            Выбрать контакт
          </label>
          <select
            id={`speaker-${speaker.id}-pick`}
            className="input input--box"
            style={{ fontFamily: 'var(--font-sans)' }}
            value={pickedContactId}
            onChange={(e) => onPick(e.target.value)}
          >
            <option value="">— не выбран —</option>
            {contacts.map((c) => (
              <option key={c.id} value={c.id}>
                {c.display_name}
                {c.is_owner ? ' (владелец)' : ''}
              </option>
            ))}
          </select>
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
            if (suggestionName && speaker.suggestion_contact_id) {
              onConfirm(speaker.suggestion_contact_id);
            } else if (pickedContact) {
              onConfirm(pickedContact.id);
            }
          }}
          disabled={
            !suggestionName && !pickedContact && !speaker.suggestion_contact_id
          }
        >
          ✓ Да, это{' '}
          {suggestionName
            ? suggestionName.split(' ')[0]
            : pickedContact?.display_name.split(' ')[0] ?? '…'}
        </button>
        {suggestionName && (
          <button
            type="button"
            className="btn btn--ghost"
            onClick={onReject}
          >
            Не он/она
          </button>
        )}
        {!adding && (
          <button
            type="button"
            className="btn btn--ghost"
            onClick={onStartAdd}
          >
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

// [B17] Per-speaker first sample from raw_stt.json.merged segments.
// Returns map speaker_tag → first non-trivial segment text (longest of first 5).
interface RawSttSegment {
  speakerTag: string;
  text: string;
  start: number;
  end: number;
}

function extractSamples(json: string | null): Map<string, string> {
  const out = new Map<string, string>();
  if (!json) return out;
  try {
    const data = JSON.parse(json) as { merged?: unknown };
    if (!Array.isArray(data.merged)) return out;
    // Bucket first 5 segments per speaker_tag, pick longest.
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
        arr.push({
          speakerTag: tag,
          text: o.text,
          start: o.start,
          end: o.end,
        });
        buckets.set(tag, arr);
      }
    }
    for (const [tag, arr] of buckets.entries()) {
      const best = arr.reduce(
        (a, b) => (b.text.length > a.text.length ? b : a),
        arr[0]!,
      );
      const trimmed = best.text.trim();
      if (trimmed.length >= 5) {
        out.set(tag, trimmed.length > 140 ? trimmed.slice(0, 137) + '…' : trimmed);
      }
    }
  } catch {
    /* corrupt raw_stt — fallback to placeholder */
  }
  return out;
}
