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
import { bcp47, useI18n } from '../i18n';

interface VoiceSamplesSectionProps {
  contactId: string;
  /** Если true — секция всегда видна. Иначе скрывается при отсутствии семплов. */
  alwaysShow?: boolean;
}

function formatCreatedAt(iso: string, locale: string): string {
  try {
    return new Date(iso).toLocaleString(bcp47(locale as Parameters<typeof bcp47>[0]), {
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
  const { locale, t } = useI18n();
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
      t('voiceSamples.deleteConfirmBody', { created: formatCreatedAt(s.created_at, locale) }),
      {
        title: 'Wotold',
        kind: 'warning',
        okLabel: t('common.delete'),
        cancelLabel: t('common.cancel'),
      },
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
    return <p className="muted">{t('common.loading')}</p>;
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
        <span className="small-caps">{t('voiceSamples.title')}</span>
        {samples && samples.length > 0 && (
          <Badge tone="neutral">{samples.length}</Badge>
        )}
      </div>
      {error && (
        <p
          role="alert"
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
          title={t('voiceSamples.emptyTitle')}
          description={t('voiceSamples.emptyBody')}
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
                  {formatCreatedAt(s.created_at, locale)}
                </span>
                <span className="muted" style={{ fontSize: 12 }}>
                  {t('voiceSamples.quality', { pct: formatQuality(s.quality) })}
                </span>
                <span className="subtle" style={{ fontSize: 11 }}>
                  {t('voiceSamples.embedBytes', { n: s.embedding_bytes })}
                </span>
                {s.source_call && (
                  <span className="subtle mono" style={{ fontSize: 10 }}>
                    {t('voiceSamples.callTag', { short: s.source_call.slice(0, 8) })}
                  </span>
                )}
              </div>
              <Button
                type="button"
                variant="danger"
                size="sm"
                onClick={() => void handleDelete(s)}
                aria-label={t('voiceSamples.deleteAria')}
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
