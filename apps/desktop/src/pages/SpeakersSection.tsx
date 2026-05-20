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
import { listContacts, type Contact } from '../api/contacts';
import {
  confirmCallSpeaker,
  listCallSpeakers,
  unbindCallSpeaker,
  type CallSpeakerView,
} from '../api/speakers';
import { Badge, Button, Card, Empty } from '../ui';

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

  const refresh = async () => {
    try {
      const [s, c] = await Promise.all([listCallSpeakers(callId), listContacts()]);
      setSpeakers(s);
      setContacts(c);
      setError(null);
    } catch (e) {
      setError(String(e));
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
      setError(String(e));
    }
  };

  const handleUnbind = async (s: CallSpeakerView) => {
    try {
      await unbindCallSpeaker(s.id);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  if (speakers === null) {
    return <p className="text-muted">Загрузка…</p>;
  }

  if (speakers.length === 0) {
    return (
      <Empty
        title="Спикеры не определены"
        description="STT не выделил отдельных спикеров в этом звонке, либо identify_speakers ещё не отработал."
      />
    );
  }

  return (
    <div className="speakers-section">
      {error && <p className="error">{error}</p>}
      <p className="text-muted" style={{ marginBottom: 'var(--space-3)' }}>
        Подсказки от системы — только ориентир. Финальная привязка спикера к
        контакту требует твоего явного действия (R2 паспорта).
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
                  {s.suggestion_contact_id && (
                    <p className="text-subtle" style={{ fontSize: 'var(--text-xs)' }}>
                      Подсказка:{' '}
                      <strong>{s.suggestion_contact_display_name ?? '—'}</strong>
                      {' · '}
                      <span className="text-muted">{sourceLabel(s.suggestion_source)}</span>
                      {' · '}
                      <span className="text-muted">{formatScore(s.suggestion_score)}</span>
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
                </>
              )}
            </Card>
          </li>
        ))}
      </ul>
    </div>
  );
}
