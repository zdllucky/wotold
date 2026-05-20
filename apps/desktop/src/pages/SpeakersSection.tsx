// #26 (M3.5): секция подтверждения спикеров на CallDetailPage.
//
// R2 паспорта: финальная привязка спикер↔контакт ТОЛЬКО через явное действие.
// Suggestion (от embedding ranking + LLM hint в #25) — только подсказка,
// без auto-bind. UI рисует:
//   - speaker_tag (S1, S2, ...)
//   - текущий contact_display_name если confirmed=true, иначе селектор
//   - suggestion как нерейзенный hint с confidence + источник
//   - кнопка "Подтвердить" / "Отвязать"
//
// [B17] Atelier v2 redesign per docs/design/atelier-v2/MIGRATION.md §5.
// Каждый спикер — .card с avatar + .conf bar + actions row.

import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';
import { createContact, listContacts, type Contact } from '../api/contacts';
import {
  confirmCallSpeaker,
  listCallSpeakers,
  unbindCallSpeaker,
  type CallSpeakerView,
} from '../api/speakers';
import { Empty } from '../ui';

interface SpeakersSectionProps {
  callId: string;
}

function sourceLabel(s: string | null): string {
  if (!s) return '—';
  if (s === 'both') return 'голос + LLM';
  if (s === 'embedding') return 'голос';
  if (s === 'llm') return 'LLM';
  return s;
}

function initials(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) return '·';
  const parts = trimmed.split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]?.toUpperCase() ?? '').join('') || '·';
}

function speakerColor(tag: string, idx: number): string {
  if (tag === 'owner' || tag === 'S0') return 'var(--sp-1)';
  return `var(--sp-${(idx % 4) + 2})`;
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

  const refresh = async () => {
    try {
      const [s, c] = await Promise.all([listCallSpeakers(callId), listContacts()]);
      setSpeakers(s);
      setContacts(c);
      setError(null);
    } catch (e) {
      setError(humanError(e));
    }
  };

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [callId]);

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

  const handleConfirm = async (s: CallSpeakerView) => {
    const picked = pickFor[s.id] ?? s.suggestion_contact_id ?? '';
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

  return (
    <div>
      {error && (
        <p style={{ color: 'var(--signal)', fontFamily: 'var(--font-sans)' }}>
          {error}
        </p>
      )}
      <p
        className="subtle"
        style={{
          marginBottom: 18,
          fontFamily: 'var(--font-serif)',
          fontStyle: 'italic',
          maxWidth: '44rem',
        }}
      >
        Подсказки — только ориентир. Привязка спикера к контакту сохраняется
        только когда подтверждаешь её явно.
      </p>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        {speakers.map((s, idx) => {
          const labelName =
            s.contact_display_name ??
            s.suggestion_contact_display_name ??
            (s.speaker_tag === 'owner' ? 'Я (владелец)' : s.speaker_tag);
          const colour = speakerColor(s.speaker_tag, idx);
          const score = s.suggestion_score ?? 0;
          return (
            <div key={s.id} className="card">
              <div
                style={{
                  display: 'flex',
                  alignItems: 'flex-start',
                  gap: 16,
                  marginBottom: 14,
                }}
              >
                <span
                  className="sp-avatar"
                  style={{
                    background: colour,
                    width: 40,
                    height: 40,
                    fontSize: 13,
                    flexShrink: 0,
                  }}
                >
                  {initials(labelName)}
                </span>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="small-caps" style={{ marginBottom: 4 }}>
                    Голос {s.speaker_tag}
                  </div>
                  <div
                    style={{
                      fontFamily: 'var(--font-serif)',
                      fontSize: 18,
                      color: 'var(--ink)',
                    }}
                  >
                    {s.confirmed && s.contact_display_name ? (
                      <>
                        <strong style={{ fontWeight: 500 }}>
                          {s.contact_display_name}
                        </strong>
                        <span className="muted" style={{ marginLeft: 8, fontSize: 14 }}>
                          подтверждён
                        </span>
                      </>
                    ) : s.suggestion_contact_display_name ? (
                      <>
                        <span className="muted" style={{ fontSize: 14 }}>
                          похоже на
                        </span>{' '}
                        {s.suggestion_contact_display_name}
                      </>
                    ) : (
                      <span style={{ fontStyle: 'italic', color: 'var(--muted)' }}>
                        Кто этот голос?
                      </span>
                    )}
                  </div>
                </div>
                {!s.confirmed && s.suggestion_score != null && (
                  <div style={{ width: 140 }}>
                    <div
                      style={{
                        display: 'flex',
                        justifyContent: 'space-between',
                        marginBottom: 4,
                      }}
                    >
                      <span className="small-caps" style={{ fontSize: 10 }}>
                        Уверенность
                      </span>
                      <span className="mono" style={{ fontSize: 12 }}>
                        {Math.round(score * 100)}%
                      </span>
                    </div>
                    <div className="conf">
                      <div className="conf-fill" style={{ width: `${score * 100}%` }} />
                    </div>
                    <div
                      className="small-caps muted"
                      style={{
                        fontSize: 10,
                        marginTop: 4,
                        textAlign: 'right',
                      }}
                    >
                      {sourceLabel(s.suggestion_source)}
                    </div>
                  </div>
                )}
              </div>

              {!s.confirmed && (
                <>
                  <div className="field" style={{ marginBottom: 12 }}>
                    <label className="field-label">Привязать к контакту</label>
                    <select
                      className="input input--box"
                      style={{ fontFamily: 'var(--font-sans)' }}
                      value={pickFor[s.id] ?? s.suggestion_contact_id ?? ''}
                      onChange={(e) =>
                        setPickFor((m) => ({ ...m, [s.id]: e.target.value }))
                      }
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

                  {addingFor === s.id ? (
                    <div
                      style={{
                        padding: 14,
                        background: 'var(--bg-2)',
                        borderRadius: 'var(--radius-md)',
                        marginBottom: 12,
                      }}
                    >
                      <div className="field" style={{ marginBottom: 10 }}>
                        <label className="field-label">Имя нового контакта</label>
                        <input
                          type="text"
                          className="input input--box"
                          autoFocus
                          placeholder="Иван Петров"
                          value={newName}
                          onChange={(e) => setNewName(e.target.value)}
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
                          onChange={(e) => setNewConsent(e.target.checked)}
                        />
                        <span>Запоминать голос для авто-определения</span>
                      </label>
                      <div style={{ display: 'flex', gap: 8 }}>
                        <button
                          type="button"
                          className="btn btn--ghost btn--sm"
                          onClick={() => {
                            setAddingFor(null);
                            setNewName('');
                            setNewConsent(false);
                          }}
                          disabled={busyAdd}
                        >
                          Отмена
                        </button>
                        <button
                          type="button"
                          className="btn btn--primary btn--sm"
                          onClick={() => void handleAddAsContact(s)}
                          disabled={busyAdd || !newName.trim()}
                        >
                          {busyAdd ? 'Добавляем…' : 'Добавить и привязать'}
                        </button>
                      </div>
                    </div>
                  ) : null}

                  <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                    <button
                      type="button"
                      className="btn btn--primary"
                      onClick={() => void handleConfirm(s)}
                      disabled={
                        !pickFor[s.id] && !s.suggestion_contact_id
                      }
                    >
                      ✓ Подтвердить
                    </button>
                    {!addingFor && (
                      <button
                        type="button"
                        className="btn btn--ghost"
                        onClick={() => {
                          setAddingFor(s.id);
                          setNewName('');
                          setNewConsent(false);
                        }}
                      >
                        + Новый контакт
                      </button>
                    )}
                  </div>
                </>
              )}

              {s.confirmed && (
                <button
                  type="button"
                  className="btn btn--ghost btn--sm"
                  onClick={() => void handleUnbind(s)}
                >
                  Отвязать
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
