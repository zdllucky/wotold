// #26 (M3.5): секция подтверждения спикеров на CallDetailPage.
//
// R2 паспорта: финальная привязка спикер↔контакт ТОЛЬКО через явное действие.
// Suggestion (от embedding ranking + LLM hint в #25) — только подсказка,
// без auto-bind. UI рисует:
//   - speaker_tag (S1, S2, ...)
//   - текущий contact_display_name если confirmed=true, иначе селектор
//   - suggestion как нерейзенный hint с confidence + источник
//   - кнопка "Подтвердить" / "Отвязать"

import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';
import { createContact, listContacts, type Contact } from '../api/contacts';
import {
  confirmCallSpeaker,
  listCallSpeakers,
  unbindCallSpeaker,
  type CallSpeakerView,
} from '../api/speakers';
import { Badge, Button, Card, Empty, InputField } from '../ui';

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

function formatScore(n: number | null): string {
  if (n == null) return '';
  return `${Math.round(n * 100)}%`;
}

export function SpeakersSection({ callId }: SpeakersSectionProps) {
  const [speakers, setSpeakers] = useState<CallSpeakerView[] | null>(null);
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [error, setError] = useState<string | null>(null);
  // Выбор контакта для каждой строки до клика "Подтвердить".
  const [pickFor, setPickFor] = useState<Record<string, string>>({});
  // [B11]: inline-форма «+ Добавить как контакт» открыта для конкретного speaker_id.
  const [addingFor, setAddingFor] = useState<string | null>(null);
  const [newName, setNewName] = useState('');
  const [newConsent, setNewConsent] = useState(false);
  const [busyAdd, setBusyAdd] = useState(false);

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
    return <p className="text-muted">Загрузка…</p>;
  }

  if (speakers.length === 0) {
    return (
      <Empty
        icon="🗣"
        title="Участники не распознаны"
        description="В этом звонке не обнаружено отдельных голосов, либо обработка ещё идёт."
      />
    );
  }

  return (
    <div className="speakers-section">
      {error && <p className="error">{error}</p>}
      <p className="text-muted" style={{ marginBottom: 'var(--space-3)' }}>
        Подсказки — только ориентир. Привязка спикера к контакту
        сохраняется только когда ты подтвердишь её явно.
      </p>
      <ul className="speakers-list">
        {speakers.map((s) => (
          <li key={s.id} className="speaker-row">
            <Card compact>
              <div className="speaker-row-head">
                <div className="speaker-row-tag">
                  <Badge tone="neutral">{s.speaker_tag}</Badge>
                  {s.confirmed && s.contact_display_name && (
                    <span className="speaker-confirmed">
                      → <strong>{s.contact_display_name}</strong>
                    </span>
                  )}
                </div>
                {s.confirmed ? (
                  <Button variant="ghost" size="sm" onClick={() => void handleUnbind(s)}>
                    Отвязать
                  </Button>
                ) : (
                  <Button variant="primary" size="sm" onClick={() => void handleConfirm(s)}>
                    Подтвердить
                  </Button>
                )}
              </div>

              {!s.confirmed && (
                <>
                  {s.suggestion_contact_id ? (
                    <p className="text-subtle" style={{ fontSize: 'var(--text-xs)' }}>
                      Подсказка:{' '}
                      <strong>{s.suggestion_contact_display_name ?? '—'}</strong>
                      {' · '}
                      <span className="text-muted">{sourceLabel(s.suggestion_source)}</span>
                      {' · '}
                      <span className="text-muted">{formatScore(s.suggestion_score)}</span>
                    </p>
                  ) : (
                    <p className="text-subtle" style={{ fontSize: 'var(--text-xs)' }}>
                      Анонимный спикер — кто это? Выбери контакт или добавь нового.
                    </p>
                  )}
                  <label className="speaker-pick-label">
                    Выбрать контакт:
                    <select
                      className="ds-select"
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
                  </label>
                  {addingFor === s.id ? (
                    <div className="speaker-add-form">
                      <InputField
                        label="Имя нового контакта"
                        type="text"
                        value={newName}
                        onChange={(e) => setNewName(e.target.value)}
                        autoFocus
                        placeholder="Иван Петров"
                      />
                      <label className="consent-row" style={{ fontSize: 'var(--text-sm)' }}>
                        <input
                          type="checkbox"
                          checked={newConsent}
                          onChange={(e) => setNewConsent(e.target.checked)}
                        />
                        <span>Запоминать голос для авто-определения</span>
                      </label>
                      <div className="form-actions" style={{ marginTop: 'var(--space-1)' }}>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => {
                            setAddingFor(null);
                            setNewName('');
                            setNewConsent(false);
                          }}
                          disabled={busyAdd}
                        >
                          Отмена
                        </Button>
                        <Button
                          type="button"
                          variant="primary"
                          size="sm"
                          onClick={() => void handleAddAsContact(s)}
                          disabled={busyAdd || !newName.trim()}
                          busy={busyAdd}
                        >
                          {busyAdd ? 'Добавляем…' : 'Добавить и привязать'}
                        </Button>
                      </div>
                    </div>
                  ) : (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        setAddingFor(s.id);
                        setNewName('');
                        setNewConsent(false);
                      }}
                    >
                      + Добавить как контакт
                    </Button>
                  )}
                </>
              )}
            </Card>
          </li>
        ))}
      </ul>
    </div>
  );
}
