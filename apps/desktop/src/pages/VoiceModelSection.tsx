// [B3.7c] Voice embedder model management.
//
// UI для скачивания/удаления WeSpeaker ONNX модели (25MB) которая
// включает biometric matching по голосу. Без неё pipeline fallback'ит на
// manual confirm (юзер сам выбирает кто говорит в каждом call_speaker'е).
//
// Слушает Tauri-события `voice-model:progress` + `voice-model:done` для
// real-time прогресса.

import { useCallback, useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  voiceModelDelete,
  voiceModelDownload,
  voiceModelInfo,
  voiceModelStatus,
  type VoiceModelDoneStatus,
  type VoiceModelInfo,
  type VoiceModelProgress,
  type VoiceModelStatus,
} from '../api/voiceModel';
import { humanError } from '../api/errors';

function formatMB(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
}

export function VoiceModelSection() {
  const [status, setStatus] = useState<VoiceModelStatus | null>(null);
  const [info, setInfo] = useState<VoiceModelInfo | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<VoiceModelProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [s, i] = await Promise.all([voiceModelStatus(), voiceModelInfo()]);
      setStatus(s);
      setInfo(i);
      setError(null);
    } catch (e) {
      setError(humanError(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Subscribe to backend progress events.
  useEffect(() => {
    let unProgress: UnlistenFn | undefined;
    let unDone: UnlistenFn | undefined;
    void listen<VoiceModelProgress>('voice-model:progress', (e) => {
      setProgress(e.payload);
    }).then((fn) => {
      unProgress = fn;
    });
    void listen<VoiceModelDoneStatus>('voice-model:done', (e) => {
      setDownloading(false);
      setProgress(null);
      const payload = e.payload;
      if (payload.status === 'verify_failed') {
        setError(
          `SHA256 не совпал — файл повреждён или сменилась версия модели. Попробуй снова.`,
        );
      } else if (payload.status === 'io_error') {
        setError(payload.message);
      }
      void refresh();
    }).then((fn) => {
      unDone = fn;
    });
    return () => {
      unProgress?.();
      unDone?.();
    };
  }, [refresh]);

  const handleDownload = async () => {
    setDownloading(true);
    setError(null);
    setProgress({ downloaded: 0, total: info?.size_hint ?? 0, percent: 0 });
    try {
      await voiceModelDownload();
    } catch (e) {
      setError(humanError(e));
      setDownloading(false);
      setProgress(null);
    }
  };

  const handleDelete = async () => {
    setDeleting(true);
    setError(null);
    try {
      await voiceModelDelete();
      await refresh();
    } catch (e) {
      setError(humanError(e));
    } finally {
      setDeleting(false);
    }
  };

  if (!status || !info) {
    return <p className="muted">Загрузка…</p>;
  }

  const featureBadge = info.feature_enabled ? null : (
    <p
      role="status"
      style={{
        fontSize: 12,
        color: 'var(--muted)',
        fontFamily: 'var(--font-sans)',
        background: 'var(--bg-2)',
        padding: '10px 14px',
        borderRadius: 'var(--radius-sm)',
        marginBottom: 18,
        borderLeft: '3px solid var(--warning)',
      }}
    >
      ⚠ В этой сборке фича <code>voice-onnx</code> не включена. Модель
      можно скачать, но pipeline её не использует — биометрический
      матчинг останется выключенным. В production-сборке (`--features
      voice-onnx`) скачивание автоматически активирует матчинг.
    </p>
  );

  return (
    <div style={{ maxWidth: 620 }}>
      {featureBadge}

      {error && (
        <p
          role="alert"
          style={{
            color: 'var(--signal)',
            fontFamily: 'var(--font-sans)',
            marginBottom: 14,
          }}
        >
          {error}
        </p>
      )}

      <div
        className="card"
        style={{ padding: 22, display: 'flex', flexDirection: 'column', gap: 14 }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
          <div>
            <div className="small-caps">Модель</div>
            <div
              style={{
                fontFamily: 'var(--font-serif)',
                fontSize: 18,
                color: 'var(--ink)',
                marginTop: 2,
              }}
            >
              WeSpeaker ResNet34 LM · VoxCeleb
            </div>
          </div>
          <StatusBadge status={status} downloading={downloading} />
        </div>

        <p
          style={{
            fontFamily: 'var(--font-serif)',
            fontSize: 14,
            color: 'var(--muted)',
            margin: 0,
            lineHeight: 1.5,
          }}
        >
          {status.status === 'valid'
            ? 'Модель готова. Wotold будет предлагать кто говорит на основе совпадения голоса с уже подтверждёнными контактами (порог 50%). Финальное подтверждение — всегда за тобой (R2 паспорта).'
            : status.status === 'corrupted'
              ? 'Файл повреждён или сменилась версия. Удали и скачай заново.'
              : 'Биометрический матчинг сейчас выключен. Скачай модель чтобы Wotold предлагал кто говорит. Размер ~25 МБ, скачивается один раз в фоне.'}
        </p>

        {downloading && progress && (
          <div>
            <div
              style={{
                height: 6,
                background: 'var(--bg-2)',
                borderRadius: 4,
                overflow: 'hidden',
              }}
            >
              <div
                style={{
                  height: '100%',
                  width: `${progress.percent}%`,
                  background: 'var(--accent)',
                  transition: 'width 200ms ease',
                }}
              />
            </div>
            <div
              className="mono"
              style={{
                fontSize: 11,
                color: 'var(--muted)',
                marginTop: 6,
                display: 'flex',
                justifyContent: 'space-between',
              }}
            >
              <span>{formatMB(progress.downloaded)} / {formatMB(progress.total)}</span>
              <span>{progress.percent.toFixed(0)}%</span>
            </div>
          </div>
        )}

        <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', marginTop: 4 }}>
          {status.status !== 'valid' && (
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => void handleDownload()}
              disabled={downloading}
            >
              {downloading
                ? 'Скачиваем…'
                : status.status === 'corrupted'
                  ? '↻ Перекачать'
                  : `↓ Скачать ${formatMB(info.size_hint)}`}
            </button>
          )}
          {status.status !== 'missing' && !downloading && (
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => void handleDelete()}
              disabled={deleting}
            >
              {deleting ? 'Удаляем…' : 'Удалить'}
            </button>
          )}
        </div>

        <details style={{ marginTop: 12 }}>
          <summary
            style={{
              cursor: 'pointer',
              fontSize: 12,
              color: 'var(--subtle)',
              fontFamily: 'var(--font-sans)',
              userSelect: 'none',
            }}
          >
            Технические детали
          </summary>
          <dl
            data-selectable
            style={{
              fontSize: 12,
              color: 'var(--muted)',
              fontFamily: 'var(--font-mono)',
              marginTop: 10,
              display: 'grid',
              gridTemplateColumns: 'auto 1fr',
              gap: '4px 16px',
            }}
          >
            <dt>URL</dt>
            <dd style={{ margin: 0, wordBreak: 'break-all' }}>{info.url}</dd>
            <dt>SHA256</dt>
            <dd style={{ margin: 0, wordBreak: 'break-all' }}>{info.sha256}</dd>
            <dt>Размер</dt>
            <dd style={{ margin: 0 }}>{formatMB(info.size_hint)}</dd>
            <dt>Build feature</dt>
            <dd style={{ margin: 0 }}>{info.feature_enabled ? 'voice-onnx ✓' : '— (не включена)'}</dd>
          </dl>
        </details>
      </div>
    </div>
  );
}

function StatusBadge({
  status,
  downloading,
}: {
  status: VoiceModelStatus;
  downloading: boolean;
}) {
  const styles: Record<string, { bg: string; fg: string; text: string }> = {
    valid: { bg: 'var(--accent-soft)', fg: 'var(--accent)', text: 'установлена' },
    missing: { bg: 'var(--bg-2)', fg: 'var(--muted)', text: 'нет' },
    corrupted: { bg: 'var(--bg-2)', fg: 'var(--warning)', text: 'повреждена' },
    downloading: { bg: 'var(--bg-2)', fg: 'var(--accent)', text: 'качаем' },
  };
  const key = downloading ? 'downloading' : status.status;
  const s = styles[key] ?? styles.missing!;
  return (
    <span
      style={{
        fontFamily: 'var(--font-sans)',
        fontSize: 11,
        fontWeight: 600,
        textTransform: 'uppercase',
        letterSpacing: 0.06,
        padding: '4px 10px',
        borderRadius: 999,
        background: s.bg,
        color: s.fg,
      }}
    >
      {s.text}
    </span>
  );
}
