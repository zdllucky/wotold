// #45 (M3.6): просмотр накопленных voice samples для контакта + manual delete.
//
// C3 паспорта: пользователь имеет полный контроль над биометрическими
// данными — может удалить любой семпл вручную (например, ошибочное
// подтверждение спикера). При удалении контакта (#41) семплы зачищаются
// каскадом — но это удаление контакта; здесь — точечное.

import { useCallback, useEffect, useRef, useState } from 'react';
import { humanError } from '../api/errors';
import { ask } from '@tauri-apps/plugin-dialog';
import { Badge, Button, Empty, Skeleton } from '../ui';
import {
  deleteVoiceSample,
  getVoiceSampleAudio,
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

  // [P4] Inline playback короткого slice (1.5–10 sec) из правильной track.
  // Backend `get_voice_sample_audio` возвращает WAV bytes (start_sec..end_sec
  // из mic.wav либо system.wav по track_kind). Frontend оборачивает в Blob
  // URL для HTMLAudioElement.src. Legacy samples (NULL slice metadata) —
  // play button disabled.
  const audioRef = useRef<HTMLAudioElement | null>(null);
  // Blob URL текущего playing sample — нужен для cleanup через
  // URL.revokeObjectURL (предотвращает memory leak).
  const currentBlobUrlRef = useRef<string | null>(null);
  // `loadingId` — пока fetch'им bytes + создаём Blob URL.
  // `playingId` — после audio.play() resolve.
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

  // [P4] Cleanup audio element + blob URL on unmount / contact switch.
  useEffect(() => {
    return () => {
      const el = audioRef.current;
      if (el) {
        el.pause();
        el.removeAttribute('src');
        el.load();
      }
      if (currentBlobUrlRef.current) {
        URL.revokeObjectURL(currentBlobUrlRef.current);
        currentBlobUrlRef.current = null;
      }
    };
  }, [contactId]);

  const stopPlayback = useCallback(() => {
    const el = audioRef.current;
    if (el) {
      el.pause();
      el.currentTime = 0;
    }
    // Revoke blob URL — не оставляем GC-leak.
    if (currentBlobUrlRef.current) {
      URL.revokeObjectURL(currentBlobUrlRef.current);
      currentBlobUrlRef.current = null;
    }
    setPlayingId(null);
    setLoadingId(null);
  }, []);

  const handlePlay = useCallback(
    async (s: VoiceSampleView) => {
      // Legacy sample (NULL slice metadata) — disabled в UI, защитный возврат.
      if (!s.track_kind || s.start_sec === null || s.end_sec === null) return;
      // Toggle off если этот sample уже играет.
      if (playingId === s.id) {
        stopPlayback();
        return;
      }
      // Switching на другой sample — останавливаем предыдущий.
      stopPlayback();
      setLoadingId(s.id);
      try {
        const bytes = await getVoiceSampleAudio(s.id);
        // Race-prevention: если за время fetch'а юзер кликнул другой sample,
        // bail. loadingId на этот момент может уже не быть s.id.
        const el = audioRef.current;
        if (!el) {
          setLoadingId(null);
          return;
        }
        // ArrayBuffer от Tauri invoke — wrap в Blob audio/wav → ObjectURL.
        // Cast Uint8Array → BlobPart: ArrayBufferLike covers SharedArrayBuffer
        // case ts compiler defensive'ит — у нас всегда regular ArrayBuffer.
        const blob = new Blob([bytes as BlobPart], { type: 'audio/wav' });
        const url = URL.createObjectURL(blob);
        currentBlobUrlRef.current = url;
        el.src = url;
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
            color: 'var(--danger)',
            fontFamily: 'var(--font)',
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
            // [P4] Play разрешён только когда slice metadata complete.
            // Legacy rows (NULL по migration 0017) — disabled.
            const canPlay =
              s.track_kind != null && s.start_sec != null && s.end_sec != null;
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
                  borderTop: '1px solid var(--border-2)',
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
                    style={{ fontSize: 12, color: 'var(--text)' }}
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
          // [P4] Revoke blob URL по completion playback — memory hygiene.
          if (currentBlobUrlRef.current) {
            URL.revokeObjectURL(currentBlobUrlRef.current);
            currentBlobUrlRef.current = null;
          }
          setPlayingId(null);
          setLoadingId(null);
        }}
        onError={() => {
          if (currentBlobUrlRef.current) {
            URL.revokeObjectURL(currentBlobUrlRef.current);
            currentBlobUrlRef.current = null;
          }
          setPlayingId(null);
          setLoadingId(null);
        }}
        style={{ display: 'none' }}
      />
    </div>
  );
}
