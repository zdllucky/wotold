// #45 (M3.6): просмотр накопленных voice samples для контакта + manual delete.
//
// C3 паспорта: пользователь имеет полный контроль над биометрическими
// данными — может удалить любой семпл вручную (например, ошибочное
// подтверждение спикера). При удалении контакта (#41) семплы зачищаются
// каскадом — но это удаление контакта; здесь — точечное.

import { useCallback, useEffect, useState } from 'react';
import { ask } from '@tauri-apps/plugin-dialog';
import { Badge, Button, Empty } from '../ui';
import {
  deleteVoiceSample,
  listVoiceSamples,
  type VoiceSampleView,
} from '../api/voiceSamples';

interface VoiceSamplesSectionProps {
  contactId: string;
  /** Если true — секция всегда видна. Иначе скрывается при отсутствии семплов. */
  alwaysShow?: boolean;
}

function formatCreatedAt(iso: string): string {
  try {
    return new Date(iso).toLocaleString('ru-RU', {
      day: '2-digit',
      month: 'short',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

function formatQuality(q: number | null): string {
  if (q == null) return '—';
  return `${(q * 100).toFixed(0)}%`;
}

export function VoiceSamplesSection({ contactId, alwaysShow }: VoiceSamplesSectionProps) {
  const [samples, setSamples] = useState<VoiceSampleView[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    listVoiceSamples(contactId)
      .then((v) => {
        setSamples(v);
        setError(null);
      })
      .catch((e: unknown) => setError(String(e)));
  }, [contactId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleDelete = async (s: VoiceSampleView) => {
    const ok = await ask(
      `Удалить voice sample от ${formatCreatedAt(s.created_at)}?\n\nЭто навсегда удалит embedding из профиля контакта. Биометрия не восстанавливается.`,
      { title: 'Wotold', kind: 'warning', okLabel: 'Удалить', cancelLabel: 'Отмена' },
    );
    if (!ok) return;
    try {
      await deleteVoiceSample(s.id);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  if (samples === null && !error) {
    return <p className="text-muted">Загрузка…</p>;
  }

  const empty = !samples || samples.length === 0;
  if (empty && !alwaysShow) return null;

  return (
    <div className="voice-samples-section">
      <div className="row-group-head">
        <span className="row-group-title">
          Голосовые семплы
          {samples && samples.length > 0 && (
            <Badge tone="neutral">{samples.length}</Badge>
          )}
        </span>
      </div>
      {error && <p className="error">{error}</p>}
      {empty ? (
        <Empty
          title="Семплов пока нет"
          description="Embedding'и накапливаются после подтверждения спикера в звонке (M3.6). Требует consent_voice = true."
        />
      ) : (
        <ul className="voice-sample-list">
          {samples!.map((s) => (
            <li key={s.id} className="voice-sample-row">
              <div className="voice-sample-meta">
                <span className="voice-sample-date">{formatCreatedAt(s.created_at)}</span>
                <span className="voice-sample-quality text-muted">
                  качество: {formatQuality(s.quality)}
                </span>
                <span className="voice-sample-bytes text-subtle">
                  {s.embedding_bytes} байт
                </span>
                {s.source_call && (
                  <span className="text-subtle text-mono" style={{ fontSize: 'var(--text-xs)' }}>
                    call:{s.source_call.slice(0, 8)}
                  </span>
                )}
              </div>
              <Button
                type="button"
                variant="danger"
                size="sm"
                onClick={() => void handleDelete(s)}
                aria-label="Удалить семпл"
              >
                ×
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
