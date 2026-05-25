// #45 (M3.6): просмотр накопленных voice samples для контакта + manual delete.
//
// C3 паспорта: пользователь имеет полный контроль над биометрическими
// данными — может удалить любой семпл вручную (например, ошибочное
// подтверждение спикера). При удалении контакта (#41) семплы зачищаются
// каскадом — но это удаление контакта; здесь — точечное.

import { useCallback, useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { humanError } from '../api/errors';
import { ask } from '@tauri-apps/plugin-dialog';
import { Badge, Button, Empty, Skeleton } from '../ui';
import { getCallAudioPath } from '../api/calls';
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

  // [P3] Inline playback из source_call. Schema voice_samples хранит только
  // embedding vector (256-dim f32), без raw audio bytes — поэтому проигрываем
  // полную mic.wav из source_call. Slice по start/end_ms невозможен пока schema
  // не расширена. См. CHUNKED_PIPELINE_BACKLOG для упомянутого migration.
  const audioRef = useRef<HTMLAudioElement | null>(null);
  // `loadingId` — пока fetch'им path; `playingId` — после <audio>.play() resolve.
  // Один <audio> на секцию: переключение sample останавливает предыдущий.
  const [loadingId, setLoadingId] = useState<string | null>(null);
  const [playingId, setPlayingId] = useState<string | null>(null);

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

  // [P3] Cleanup audio element on unmount / contact switch.
  useEffect(() => {
    return () => {
      const el = audioRef.current;
      if (el) {
        el.pause();
        el.removeAttribute('src');
        el.load();
      }
    };
  }, [contactId]);

  const stopPlayback = useCallback(() => {
    const el = audioRef.current;
    if (el) {
      el.pause();
      el.currentTime = 0;
    }
    setPlayingId(null);
    setLoadingId(null);
  }, []);

  const handlePlay = useCallback(
    async (s: VoiceSampleView) => {
      if (!s.source_call) return; // disabled state выше
      // Toggle off если этот sample уже играет.
      if (playingId === s.id) {
        stopPlayback();
        return;
      }
      // Switching на другой sample — останавливаем предыдущий перед load'ом.
      stopPlayback();
      setLoadingId(s.id);
      try {
        const path = await getCallAudioPath(s.source_call, 'mic');
        const el = audioRef.current;
        if (!el) {
          setLoadingId(null);
          return;
        }
        el.src = convertFileSrc(path);
        // Race-prevention: пользователь мог уже кликнуть другой sample.
        // Перепроверяем что мы всё ещё loading'им именно этот id.
        await el.play();
        setLoadingId((prev) => (prev === s.id ? null : prev));
        setPlayingId(s.id);
      } catch (e) {
        setLoadingId(null);
        setError(humanError(e));
      }
    },
    [playingId, stopPlayback],
  );

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
    return (
      <div
        aria-busy="true"
        style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
      >
        {[0, 1, 2].map((i) => (
          <div
            key={i}
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr auto auto auto',
              gap: 12,
              padding: '8px 0',
              alignItems: 'center',
              pointerEvents: 'none',
            }}
          >
            <Skeleton width="60%" height="0.85em" />
            <Skeleton width="1.5rem" height="1rem" />
            <Skeleton width="1.5rem" height="1rem" />
          </div>
        ))}
      </div>
    );
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
          {samples!.map((s) => {
            const canPlay = Boolean(s.source_call);
            const isLoading = loadingId === s.id;
            const isPlaying = playingId === s.id;
            const playLabel = !canPlay
              ? t('voiceSamples.playDisabledHint')
              : isPlaying
                ? t('voiceSamples.pauseAria')
                : t('voiceSamples.playAria');
            return (
              <li
                key={s.id}
                style={{
                  display: 'grid',
                  gridTemplateColumns: '1fr auto auto',
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
                  variant="ghost"
                  size="sm"
                  onClick={() => void handlePlay(s)}
                  disabled={!canPlay || isLoading}
                  aria-label={playLabel}
                  title={playLabel}
                >
                  {/* Glyph: ▶ idle, ❚❚ playing, … loading. Disabled — outline ▷. */}
                  {isLoading ? '…' : isPlaying ? '❚❚' : canPlay ? '▶' : '▷'}
                </Button>
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
            );
          })}
        </ul>
      )}
      {/* [P3] Single shared <audio> для всех samples — single-concurrent
          playback. Hidden control (manage via refs). onEnded чистит state. */}
      <audio
        ref={audioRef}
        preload="none"
        onEnded={() => {
          setPlayingId(null);
          setLoadingId(null);
        }}
        onError={() => {
          setPlayingId(null);
          setLoadingId(null);
        }}
        style={{ display: 'none' }}
      />
    </div>
  );
}
