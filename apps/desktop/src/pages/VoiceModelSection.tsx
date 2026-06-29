// [B3.7c] Voice embedder model management.
//
// UI для скачивания/удаления WeSpeaker ONNX модели (25MB) которая
// включает biometric matching по голосу. Без неё pipeline fallback'ит на
// manual confirm (юзер сам выбирает кто говорит в каждом call_speaker'е).
//
// Слушает Tauri-события `voice-model:progress` + `voice-model:done` для
// real-time прогресса.
//
// [B18.5b] Wotold v2 restyle: card → `.panel`, checkboxes → `.setting-row` +
// `.switch`, badges → `.chip`, progress → token bar. Логика/listeners 1-в-1.

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
  localEngineModelDownload,
  localEngineModelStatus,
} from '../api/local-engine';
import type { ModelStatus } from '@wotold/contracts';
import {
  getSetting,
  setSetting,
  SETTINGS_DEFAULTS,
  SETTINGS_KEYS,
} from '../api/settings';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';
import { SettingRow, Skeleton, Switch } from '../ui';

type TFn = ReturnType<typeof useI18n>['t'];

function formatMB(bytes: number, t: TFn): string {
  return t('voiceModel.mb', { n: (bytes / (1024 * 1024)).toFixed(1) });
}

interface ToggleRowProps {
  label: string;
  hint: string;
  checked: boolean;
  disabled?: boolean;
  onToggle: (next: boolean) => void;
}

// [B18.5b] v2 toggle row with disabled semantics — gasим switch когда модель
// не готова (без эмбеддингов матчинга нет / pyannote отсутствует).
// [B18.7c] Internals now compose canonical SettingRow + Switch wrappers.
// Switch uses native `disabled` (inert when not ready); SettingRow `disabled`
// keeps the row opacity. Behaviour 1-в-1 с прежним raw markup.
function ToggleRow({ label, hint, checked, disabled, onToggle }: ToggleRowProps) {
  return (
    <SettingRow
      label={label}
      hint={hint}
      disabled={disabled}
      control={
        <Switch
          checked={checked}
          onChange={onToggle}
          label={label}
          disabled={disabled}
          style={{ marginTop: 2 }}
        />
      }
    />
  );
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
  // [Bug-fix #4] Pyannote-segmentation status — необходим для mic diarization.
  // Когда missing, sortformer silently skip'ает, и юзер не видит почему голоса
  // не разделились. Здесь делаем gap явным.
  const [pyannoteStatus, setPyannoteStatus] = useState<ModelStatus | null>(null);
  const [pyannoteDownloading, setPyannoteDownloading] = useState(false);

  const refreshPyannote = useCallback(async () => {
    try {
      const s = await localEngineModelStatus('pyannote-segmentation');
      setPyannoteStatus(s);
    } catch {
      // local-engine catalog не доступен — не критично, скрываем UI блок.
      setPyannoteStatus(null);
    }
  }, []);

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
      await refreshPyannote();
    })();
  }, [refreshPyannote]);

  const handleInstallPyannote = async () => {
    if (pyannoteDownloading) return;
    setPyannoteDownloading(true);
    try {
      await localEngineModelDownload('pyannote-segmentation');
      await refreshPyannote();
    } catch (e) {
      setError(humanError(e));
    } finally {
      setPyannoteDownloading(false);
    }
  };

  const pyannoteReady = pyannoteStatus?.state === 'present';

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
        color: 'var(--warn)',
        background: 'var(--warn-soft)',
        padding: '10px 14px',
        borderRadius: 'var(--r-sm)',
        borderLeft: '3px solid var(--warn)',
        margin: 0,
        lineHeight: 1.5,
      }}
    >
      {t('voiceModel.featureOff')}
    </p>
  );

  return (
    <div style={{ maxWidth: 620, display: 'flex', flexDirection: 'column', gap: 16 }}>
      {featureBadge}

      {error && (
        <p role="alert" style={{ color: 'var(--danger)', margin: 0 }}>
          {error}
        </p>
      )}

      <div
        className="panel"
        style={{ padding: 22, display: 'flex', flexDirection: 'column', gap: 14 }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
          <div>
            <div className="set-eyebrow" style={{ marginBottom: 4 }}>
              {t('voiceModel.modelEyebrow')}
            </div>
            <div style={{ fontSize: 'var(--t-16)', fontWeight: 600, color: 'var(--text)' }}>
              {t('voiceModel.modelName')}
            </div>
          </div>
          <StatusChip status={status} downloading={downloading} />
        </div>

        <p style={{ fontSize: 14, color: 'var(--text-2)', margin: 0, lineHeight: 1.5 }}>
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
                background: 'var(--sunken)',
                borderRadius: 'var(--r-pill)',
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
              style={{
                fontFamily: 'var(--mono)',
                fontSize: 11,
                color: 'var(--text-3)',
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

        {/* [Bug-fix #4] Tech details expander убран — обезличиваем модуль
            (не показываем имя архитектуры/URL/SHA256/feature flag). Размер
            модели остался в кнопке "Скачать (~25МБ)". */}
      </div>

      {/* Auto-bind toggle — связан со списком voiced спикеров.
          Гасим если модель не скачана: без эмбеддингов матчинга нет. */}
      <div className="panel" style={{ padding: 18 }}>
        <ToggleRow
          label={t('settings.speakersAutoBindLabel')}
          hint={t('settings.speakersAutoBindHint')}
          checked={autoBindEnabled}
          disabled={status.status !== 'valid'}
          onToggle={(next) => void persistAutoBind(next)}
        />
      </div>

      {/* [M13 follow-up + Bug-fix #4] Mic diarization toggle. Default ON.
          Backend silently skip'ает diarization когда pyannote-segmentation
          модель отсутствует — здесь явный gating: toggle disabled до
          установки модели, with inline install button. Размер модели в
          catalog: ~6МБ ONNX. */}
      <div className="panel" style={{ padding: 18 }}>
        <ToggleRow
          label={t('settings.speakersMicDiarizationLabel')}
          hint={t('settings.speakersMicDiarizationHint')}
          checked={micDiarizationEnabled && pyannoteReady}
          disabled={!pyannoteReady}
          onToggle={(next) => void persistMicDiarization(next)}
        />

        {!pyannoteReady && (
          <div
            style={{
              marginTop: 12,
              paddingTop: 12,
              borderTop: '1px dashed var(--border)',
              display: 'flex',
              flexDirection: 'column',
              gap: 10,
            }}
          >
            <div style={{ fontSize: 12, color: 'var(--warn)', lineHeight: 1.5 }}>
              {t('settings.micDiarizationModelMissing')}
            </div>
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => void handleInstallPyannote()}
              disabled={pyannoteDownloading}
              style={{ alignSelf: 'flex-start' }}
            >
              {pyannoteDownloading
                ? t('settings.micDiarizationInstalling')
                : t('settings.micDiarizationInstall')}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

type ChipVariant = 'ok' | 'line' | 'warn' | 'accent';

// [B18.5b] Voice-model state → v2 `.chip`. valid=ok, missing=line,
// corrupted=warn, downloading=accent.
function StatusChip({
  status,
  downloading,
}: {
  status: VoiceModelStatus;
  downloading: boolean;
}) {
  const { t } = useI18n();
  const meta: Record<string, { variant: ChipVariant; text: string }> = {
    valid: { variant: 'ok', text: t('voiceModel.statusValid') },
    missing: { variant: 'line', text: t('voiceModel.statusMissing') },
    corrupted: { variant: 'warn', text: t('voiceModel.statusCorrupted') },
    downloading: { variant: 'accent', text: t('voiceModel.statusDownloading') },
  };
  const key = downloading ? 'downloading' : status.status;
  const m = meta[key] ?? meta.missing!;
  return (
    <span className={`chip chip--${m.variant}`} data-size="sm">
      {m.text}
    </span>
  );
}
