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
import {
  getSetting,
  setSetting,
  SETTINGS_DEFAULTS,
  SETTINGS_KEYS,
} from '../api/settings';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';
import { Skeleton } from '../ui';

type TFn = ReturnType<typeof useI18n>['t'];

function formatMB(bytes: number, t: TFn): string {
  return t('voiceModel.mb', { n: (bytes / (1024 * 1024)).toFixed(1) });
}

export function VoiceModelSection() {
  const { t } = useI18n();
  const [status, setStatus] = useState<VoiceModelStatus | null>(null);
  const [info, setInfo] = useState<VoiceModelInfo | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<VoiceModelProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [autoBindEnabled, setAutoBindEnabled] = useState<boolean>(
    SETTINGS_DEFAULTS.AUTO_BIND_ENABLED,
  );
  const [micDiarizationEnabled, setMicDiarizationEnabled] = useState<boolean>(
    SETTINGS_DEFAULTS.MIC_DIARIZATION_ENABLED,
  );

  useEffect(() => {
    void (async () => {
      const raw = await getSetting(SETTINGS_KEYS.AUTO_BIND_ENABLED).catch(() => null);
      setAutoBindEnabled(raw === '1');
      // [M13 follow-up] Mic diarization — default ON. Explicit '0'/'false' = OFF;
      // null/missing/anything else = treat as ON (matches backend interpretation).
      const micRaw = await getSetting(SETTINGS_KEYS.MIC_DIARIZATION_ENABLED).catch(
        () => null,
      );
      setMicDiarizationEnabled(micRaw !== '0' && micRaw !== 'false');
    })();
  }, []);

  const persistAutoBind = async (next: boolean) => {
    setAutoBindEnabled(next);
    try {
      await setSetting(SETTINGS_KEYS.AUTO_BIND_ENABLED, next ? '1' : '0');
    } catch (e) {
      setError(humanError(e));
    }
  };

  const persistMicDiarization = async (next: boolean) => {
    setMicDiarizationEnabled(next);
    try {
      await setSetting(SETTINGS_KEYS.MIC_DIARIZATION_ENABLED, next ? '1' : '0');
    } catch (e) {
      setError(humanError(e));
    }
  };

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
        setError(t('voiceModel.verifyFailed'));
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
    return (
      <div aria-busy="true" style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <Skeleton width="60%" height="0.85em" />
        <Skeleton width="100%" height="2.5rem" />
        <Skeleton width="40%" height="0.75em" />
      </div>
    );
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
      {t('voiceModel.featureOff')}
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
            <div className="small-caps">{t('voiceModel.modelEyebrow')}</div>
            <div
              style={{
                fontFamily: 'var(--font-serif)',
                fontSize: 18,
                color: 'var(--ink)',
                marginTop: 2,
              }}
            >
              {t('voiceModel.modelName')}
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
            ? t('voiceModel.descValid')
            : status.status === 'corrupted'
              ? t('voiceModel.descCorrupted')
              : t('voiceModel.descMissing')}
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
              <span>{formatMB(progress.downloaded, t)} / {formatMB(progress.total, t)}</span>
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
                ? t('voiceModel.btnDownloading')
                : status.status === 'corrupted'
                  ? t('voiceModel.btnRedownload')
                  : t('voiceModel.btnDownload', { size: formatMB(info.size_hint, t) })}
            </button>
          )}
          {status.status !== 'missing' && !downloading && (
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => void handleDelete()}
              disabled={deleting}
            >
              {deleting ? t('voiceModel.btnDeleting') : t('voiceModel.btnDelete')}
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
            {t('voiceModel.techDetails')}
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
            <dt>{t('voiceModel.techUrl')}</dt>
            <dd style={{ margin: 0, wordBreak: 'break-all' }}>{info.url}</dd>
            <dt>{t('voiceModel.techSha')}</dt>
            <dd style={{ margin: 0, wordBreak: 'break-all' }}>{info.sha256}</dd>
            <dt>{t('voiceModel.techSize')}</dt>
            <dd style={{ margin: 0 }}>{formatMB(info.size_hint, t)}</dd>
            <dt>{t('voiceModel.techFeature')}</dt>
            <dd style={{ margin: 0 }}>
              {info.feature_enabled
                ? t('voiceModel.featureEnabled')
                : t('voiceModel.featureDisabled')}
            </dd>
          </dl>
        </details>
      </div>

      {/* Auto-bind toggle — связан со списком voiced спикеров.
          Гасим если модель не скачана: без эмбеддингов матчинга нет. */}
      <div
        style={{
          marginTop: 22,
          padding: 18,
          border: '1px solid var(--line-soft)',
          borderRadius: 'var(--radius-card, 8px)',
          background: 'var(--bg)',
          opacity: status.status === 'valid' ? 1 : 0.6,
        }}
      >
        <label
          style={{
            display: 'flex',
            alignItems: 'flex-start',
            gap: 12,
            cursor: status.status === 'valid' ? 'pointer' : 'not-allowed',
          }}
        >
          <input
            type="checkbox"
            checked={autoBindEnabled}
            disabled={status.status !== 'valid'}
            onChange={(e) => void persistAutoBind(e.target.checked)}
            style={{ marginTop: 4 }}
          />
          <div style={{ flex: 1 }}>
            <div
              style={{
                fontFamily: 'var(--font-sans)',
                fontSize: 14,
                color: 'var(--ink)',
                fontWeight: 500,
                marginBottom: 4,
              }}
            >
              {t('settings.speakersAutoBindLabel')}
            </div>
            <div
              style={{
                fontSize: 12,
                color: 'var(--subtle)',
                lineHeight: 1.5,
              }}
            >
              {t('settings.speakersAutoBindHint')}
            </div>
          </div>
        </label>
      </div>

      {/* [M13 follow-up] Mic diarization toggle — для записей где на mic
          попадает несколько голосов (live meetings в одной комнате).
          Default ON. Hint предупреждает о ~10-20% slowdown.
          Не gating'уется по status.status — даже без WeSpeaker модели
          sortformer работает; owner identification только без biometric
          (fallback на primary-speaker heuristic). */}
      <div
        style={{
          marginTop: 12,
          padding: 18,
          border: '1px solid var(--line-soft)',
          borderRadius: 'var(--radius-card, 8px)',
          background: 'var(--bg)',
        }}
      >
        <label
          style={{
            display: 'flex',
            alignItems: 'flex-start',
            gap: 12,
            cursor: 'pointer',
          }}
        >
          <input
            type="checkbox"
            checked={micDiarizationEnabled}
            onChange={(e) => void persistMicDiarization(e.target.checked)}
            style={{ marginTop: 4 }}
          />
          <div style={{ flex: 1 }}>
            <div
              style={{
                fontFamily: 'var(--font-sans)',
                fontSize: 14,
                color: 'var(--ink)',
                fontWeight: 500,
                marginBottom: 4,
              }}
            >
              {t('settings.speakersMicDiarizationLabel')}
            </div>
            <div
              style={{
                fontSize: 12,
                color: 'var(--subtle)',
                lineHeight: 1.5,
              }}
            >
              {t('settings.speakersMicDiarizationHint')}
            </div>
          </div>
        </label>
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
  const { t } = useI18n();
  const styles: Record<string, { bg: string; fg: string; text: string }> = {
    valid: { bg: 'var(--accent-soft)', fg: 'var(--accent)', text: t('voiceModel.statusValid') },
    missing: { bg: 'var(--bg-2)', fg: 'var(--muted)', text: t('voiceModel.statusMissing') },
    corrupted: { bg: 'var(--bg-2)', fg: 'var(--warning)', text: t('voiceModel.statusCorrupted') },
    downloading: { bg: 'var(--bg-2)', fg: 'var(--accent)', text: t('voiceModel.statusDownloading') },
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
