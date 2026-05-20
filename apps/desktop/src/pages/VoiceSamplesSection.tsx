// #45 (M3.6): просмотр накопленных voice samples для контакта + manual delete.
//
// C3 паспорта: пользователь имеет полный контроль над биометрическими
// данными — может удалить любой семпл вручную (например, ошибочное
// подтверждение спикера). При удалении контакта (#41) семплы зачищаются
// каскадом — но это удаление контакта; здесь — точечное.

import { useCallback, useEffect, useState } from 'react';
import { humanError } from '../api/errors';
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
      .catch((e: unknown) => setError(humanError(e)));
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
      setError(humanError(e));
    }
  };

  if (samples === null && !error) {
    return <p className="muted">Загрузка…</p>;
  }

  const empty = !samples || samples.length === 0;
  if (empty && !alwaysShow) return null;

  return (
    <div style={{ marginTop: 14 }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          marginBottom: 10,
        }}
      >
        <span className="small-caps">Голосовые семплы</span>
        {samples && samples.length > 0 && (
          <Badge tone="neutral">{samples.length}</Badge>
        )}
      </div>
      {error && (
        <p
          style={{
            color: 'var(--signal)',
            fontFamily: 'var(--font-sans)',
            marginBottom: 12,
          }}
        >
          {error}
        </p>
      )}
      {empty ? (
        <Empty
          title="Образцов голоса пока нет"
          description="Подтверди этого человека в любом звонке — Wotold начнёт сохранять короткие образцы голоса для авто-определения в будущем. Требует включённой опции «Запоминать голос»."
        />
      ) : (
        <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
          {samples!.map((s) => (
            <li
              key={s.id}
              style={{
                display: 'grid',
                gridTemplateColumns: '1fr auto',
                gap: 12,
                padding: '10px 0',
                borderTop: '1px solid var(--line-soft)',
                alignItems: 'center',
              }}
            >
              <div
                style={{
                  display: 'flex',
                  gap: 14,
                  flexWrap: 'wrap',
                  alignItems: 'baseline',
                }}
              >
                <span
                  className="mono"
                  style={{ fontSize: 12, color: 'var(--ink)' }}
                >
                  {formatCreatedAt(s.created_at)}
                </span>
                <span className="muted" style={{ fontSize: 12 }}>
                  качество {formatQuality(s.quality)}
                </span>
                <span className="subtle" style={{ fontSize: 11 }}>
                  {s.embedding_bytes} байт
                </span>
                {s.source_call && (
                  <span className="subtle mono" style={{ fontSize: 10 }}>
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
