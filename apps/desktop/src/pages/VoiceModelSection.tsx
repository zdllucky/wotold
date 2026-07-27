// [B3.7c, B21] «Спикеры» — voice embedder model + biometric toggles.
//
// Канон SecSpeakers (wk-settings.jsx): компактная Panel p14 «Голосовой модуль»
// (icon-квадрат + имя + размер + Chip-статус + sm-кнопки), под ней плотные
// SettingRow: авто-привязка, порог уверенности (⊕ B21 — ключ читался
// backend'ом, UI не было), несколько голосов на микрофоне (при отсутствии
// pyannote — кнопка установки модуля с реальным прогрессом model:progress).
//
// Слушает `voice-model:progress`/`voice-model:done` (WeSpeaker) и
// `model:progress`/`model:done` (pyannote) — логика download/delete 1-в-1.

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
  AUTO_BIND_THRESHOLDS,
  getSetting,
  setSetting,
  SETTINGS_DEFAULTS,
  SETTINGS_KEYS,
  type AutoBindThreshold,
} from '../api/settings';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';
import { Button, Chip, Icon, IconBtn, Progress, Select, SettingRow, Skeleton, Switch } from '../ui';

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
  // [B21 ⊕] Порог авто-привязки — backend читал ключ всегда, UI не было.
  const [autoBindThreshold, setAutoBindThreshold] = useState<AutoBindThreshold>(
    SETTINGS_DEFAULTS.AUTO_BIND_THRESHOLD,
  );
  const [micDiarizationEnabled, setMicDiarizationEnabled] = useState<boolean>(
    SETTINGS_DEFAULTS.MIC_DIARIZATION_ENABLED,
  );
  // [Bug-fix #4] Pyannote-segmentation status — необходим для mic diarization.
  const [pyannoteStatus, setPyannoteStatus] = useState<ModelStatus | null>(null);
  const [pyannoteDownloading, setPyannoteDownloading] = useState(false);
  const [pyannotePct, setPyannotePct] = useState<number | null>(null);

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
      const rawThreshold = await getSetting(SETTINGS_KEYS.AUTO_BIND_THRESHOLD).catch(() => null);
      if (rawThreshold && (AUTO_BIND_THRESHOLDS as string[]).includes(rawThreshold)) {
        setAutoBindThreshold(rawThreshold as AutoBindThreshold);
      }
      // [P-fix7, B21] Mic diarization — default OFF: backend включает только
      // на явное '1'/'true' (matches! в recording.rs). Раньше loader считал
      // missing=ON → тумблер показывал ВКЛ при фактически выключенной
      // диаризации.
      const micRaw = await getSetting(SETTINGS_KEYS.MIC_DIARIZATION_ENABLED).catch(
        () => null,
      );
      setMicDiarizationEnabled(micRaw === '1' || micRaw === 'true');
      await refreshPyannote();
    })();
  }, [refreshPyannote]);

  // [B21] Реальный прогресс установки pyannote. Листенер вешается сразу на
  // mount (не на флаг): listen() — async IPC, гейт на pyannoteDownloading
  // проигрывал гонку быстрым загрузкам (~6МБ) и % не успевал отрисоваться.
  useEffect(() => {
    let cancelled = false;
    let un: UnlistenFn | undefined;
    (async () => {
      un = await listen<{ id: string; pct: number }>('model:progress', (e) => {
        if (e.payload.id === 'pyannote-segmentation') setPyannotePct(e.payload.pct);
      });
      if (cancelled) un();
    })();
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  const handleInstallPyannote = async () => {
    if (pyannoteDownloading) return;
    setPyannoteDownloading(true);
    setPyannotePct(0);
    try {
      await localEngineModelDownload('pyannote-segmentation');
      await refreshPyannote();
    } catch (e) {
      setError(humanError(e, t));
    } finally {
      setPyannoteDownloading(false);
      setPyannotePct(null);
    }
  };

  const pyannoteReady = pyannoteStatus?.state === 'present';

  const persistAutoBind = async (next: boolean) => {
    setAutoBindEnabled(next);
    try {
      await setSetting(SETTINGS_KEYS.AUTO_BIND_ENABLED, next ? '1' : '0');
    } catch (e) {
      setError(humanError(e, t));
    }
  };

  const persistThreshold = async (next: AutoBindThreshold) => {
    setAutoBindThreshold(next);
    try {
      await setSetting(SETTINGS_KEYS.AUTO_BIND_THRESHOLD, next);
    } catch (e) {
      setError(humanError(e, t));
    }
  };

  const persistMicDiarization = async (next: boolean) => {
    setMicDiarizationEnabled(next);
    try {
      await setSetting(SETTINGS_KEYS.MIC_DIARIZATION_ENABLED, next ? '1' : '0');
    } catch (e) {
      setError(humanError(e, t));
    }
  };

  const refresh = useCallback(async () => {
    try {
      const [s, i] = await Promise.all([voiceModelStatus(), voiceModelInfo()]);
      setStatus(s);
      setInfo(i);
      setError(null);
    } catch (e) {
      setError(humanError(e, t));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Subscribe to backend progress events (WeSpeaker).
  // [B21] cancelled-guard (mirror LocalEngineSection [Review HIGH-2]): без
  // него cleanup до резолва listen() оставлял listener-leak на всю сессию.
  useEffect(() => {
    let cancelled = false;
    let unProgress: UnlistenFn | undefined;
    let unDone: UnlistenFn | undefined;
    (async () => {
      unProgress = await listen<VoiceModelProgress>('voice-model:progress', (e) => {
        setProgress(e.payload);
      });
      if (cancelled) {
        unProgress();
        return;
      }
      unDone = await listen<VoiceModelDoneStatus>('voice-model:done', (e) => {
        setDownloading(false);
        setProgress(null);
        const payload = e.payload;
        if (payload.status === 'verify_failed') {
          setError(t('voiceModel.verifyFailed'));
        } else if (payload.status === 'io_error') {
          setError(payload.message);
        }
        void refresh();
      });
      if (cancelled) unDone();
    })();
    return () => {
      cancelled = true;
      unProgress?.();
      unDone?.();
    };
  }, [refresh, t]);

  const handleDownload = async () => {
    setDownloading(true);
    setError(null);
    setProgress({ downloaded: 0, total: info?.size_hint ?? 0, percent: 0 });
    try {
      await voiceModelDownload();
    } catch (e) {
      setError(humanError(e, t));
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
      setError(humanError(e, t));
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

  const valid = status.status === 'valid';

  return (
    <div>
      {!info.feature_enabled && (
        <div
          className="panel"
          role="status"
          style={{ padding: '10px 14px', marginBottom: 14, fontSize: 12.5, color: 'var(--warn)' }}
        >
          {t('voiceModel.featureOff')}
        </div>
      )}

      {error && (
        <p role="alert" style={{ color: 'var(--danger)', margin: '0 0 12px' }}>
          {error}
        </p>
      )}

      {/* Компактная карточка модуля (канон :293-302). */}
      <div
        className="panel"
        style={{ padding: 14, display: 'flex', alignItems: 'center', gap: 12 }}
      >
        <span
          style={{
            width: 32,
            height: 32,
            borderRadius: 'var(--r-sm)',
            flex: '0 0 auto',
            display: 'inline-flex',
            alignItems: 'center',
            justifyContent: 'center',
            background: valid ? 'var(--accent)' : 'var(--sunken)',
            color: valid ? 'var(--on-accent)' : 'var(--text-3)',
          }}
        >
          <Icon name="users" size={17} />
        </span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontWeight: 600, fontSize: 13.5 }}>{t('voiceModel.modelName')}</div>
          <div className="u-faint" style={{ fontSize: 11.5 }}>
            {formatMB(info.size_hint, t)} ·{' '}
            {valid
              ? t('voiceModel.descValid')
              : status.status === 'corrupted'
                ? t('voiceModel.descCorrupted')
                : t('voiceModel.descMissing')}
          </div>
        </div>
        <StatusChip status={status} downloading={downloading} />
        {!valid && !downloading && (
          <Button
            variant="primary"
            size="sm"
            leading={<Icon name="download" size={14} />}
            onClick={() => void handleDownload()}
          >
            {status.status === 'corrupted'
              ? t('voiceModel.btnRedownload')
              : t('voiceModel.btnDownload', { size: formatMB(info.size_hint, t) })}
          </Button>
        )}
        {valid && !downloading && (
          <IconBtn
            icon="trash"
            size="sm"
            label={t('voiceModel.btnDelete')}
            onClick={() => void handleDelete()}
            disabled={deleting}
          />
        )}
      </div>

      {downloading && progress && (
        <div style={{ marginTop: 10 }}>
          <Progress value={progress.percent} ariaLabel={t('voiceModel.btnDownloading')} />
          <div
            className="mono"
            style={{
              fontSize: 11,
              color: 'var(--text-3)',
              marginTop: 6,
              display: 'flex',
              justifyContent: 'space-between',
            }}
          >
            <span>
              {formatMB(progress.downloaded, t)} / {formatMB(progress.total, t)}
            </span>
            <span>{progress.percent.toFixed(0)}%</span>
          </div>
        </div>
      )}

      {/* Плотные Row-настройки (канон :304-313). Авто-привязка гасится пока
          модель не скачана: без эмбеддингов матчинга нет. */}
      <div style={{ marginTop: 8 }}>
        <SettingRow
          label={t('settings.speakersAutoBindLabel')}
          hint={t('settings.speakersAutoBindHint')}
          align="top"
          disabled={!valid}
        >
          <Switch
            checked={autoBindEnabled}
            onChange={(v) => valid && void persistAutoBind(v)}
            label={t('settings.speakersAutoBindLabel')}
            disabled={!valid}
          />
        </SettingRow>
        {autoBindEnabled && valid && (
          <SettingRow
            label={t('settings.autoBindThresholdLabel')}
            hint={t('settings.autoBindThresholdHint')}
            align="top"
          >
            <Select<AutoBindThreshold>
              value={autoBindThreshold}
              options={AUTO_BIND_THRESHOLDS.map((n) => ({
                value: n,
                label: t('settings.autoBindThresholdOption', { n }),
              }))}
              onChange={(v) => void persistThreshold(v)}
            />
          </SettingRow>
        )}
        <SettingRow
          label={t('settings.speakersMicDiarizationLabel')}
          hint={
            pyannoteReady ? (
              t('settings.speakersMicDiarizationHint')
            ) : (
              <>
                {t('settings.speakersMicDiarizationHint')}{' '}
                {t('settings.micDiarizationModelMissing')}
              </>
            )
          }
          align="top"
          last
        >
          {pyannoteReady ? (
            <Switch
              checked={micDiarizationEnabled}
              onChange={(v) => void persistMicDiarization(v)}
              label={t('settings.speakersMicDiarizationLabel')}
            />
          ) : (
            <Button
              variant="default"
              size="sm"
              leading={<Icon name="download" size={14} />}
              onClick={() => void handleInstallPyannote()}
              disabled={pyannoteDownloading}
            >
              {pyannoteDownloading
                ? pyannotePct != null
                  ? `${Math.round(pyannotePct)}%`
                  : t('settings.micDiarizationInstalling')
                : t('settings.micDiarizationInstall')}
            </Button>
          )}
        </SettingRow>
      </div>
    </div>
  );
}

type ChipVariant = 'ok' | 'line' | 'warn' | 'accent';

// [B18.5b] Voice-model state → Chip. valid=ok, missing=line,
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
    <Chip tone={m.variant} size="sm">
      {m.text}
    </Chip>
  );
}
