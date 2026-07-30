// [B3.7c, B21] «Спикеры» — voice embedder model + biometric toggles.
//
// Канон SecSpeakers (wk-settings.jsx): компактная Panel p14 «Голосовой модуль»
// (icon-квадрат + имя + размер + Chip-статус + sm-кнопки), под ней плотные
// SettingRow: авто-привязка, порог уверенности (⊕ B21 — ключ читался
// backend'ом, UI не было), несколько голосов на микрофоне (при отсутствии
// pyannote — кнопка установки модуля с реальным прогрессом model:progress).
//
// Голосовой эмбеддер стал каталожной моделью (`voice-embedder`): и он, и
// pyannote качаются одной командой и рапортуют одними событиями
// `model:progress`/`model:done`. До этого у эмбеддера была своя качалка со
// своими `voice-model:*`, и таблица моделей в «Обработке» слушала не тот
// канал — кнопка скачивания выглядела мёртвой, хотя файл качался.

import { useCallback, useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  localEngineModelDelete,
  localEngineModelDownload,
  localEngineModelStatus,
} from '../api/local-engine';
import { voiceEmbedderFeatureEnabled } from '../api/speakers';
import type { ModelProgressEvent, ModelStatus } from '@wotold/contracts';
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

const EMBEDDER_ID = 'voice-embedder';
const PYANNOTE_ID = 'pyannote-segmentation';

/** Статус загрузки модели из `model:done` (union по `status`). */
type ModelDoneEvent =
  | { id: string; status: 'ok' | 'already_present' }
  | { id: string; status: 'verify_failed'; expected: string; got: string }
  | { id: string; status: 'io_error'; message: string };

function formatMB(bytes: number, t: TFn): string {
  return t('voiceModel.mb', { n: (bytes / (1024 * 1024)).toFixed(1) });
}

export function VoiceModelSection() {
  const { t } = useI18n();
  const [status, setStatus] = useState<ModelStatus | null>(null);
  const [featureEnabled, setFeatureEnabled] = useState<boolean | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<ModelProgressEvent | null>(null);
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
      setPyannoteStatus(await localEngineModelStatus(PYANNOTE_ID));
    } catch {
      // local-engine catalog не доступен — не критично, скрываем UI блок.
      setPyannoteStatus(null);
    }
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [s, feature] = await Promise.all([
        localEngineModelStatus(EMBEDDER_ID),
        voiceEmbedderFeatureEnabled(),
      ]);
      setStatus(s);
      setFeatureEnabled(feature);
      setError(null);
    } catch (e) {
      setError(humanError(e, t));
    }
  }, [t]);

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

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Один листенер на оба модуля. [B21] Вешается сразу на mount (не на флаг):
  // listen() — async IPC, гейт на *Downloading проигрывал гонку быстрым
  // загрузкам (~6МБ) и % не успевал отрисоваться. cancelled-guard (mirror
  // LocalEngineSection [Review HIGH-2]): без него cleanup до резолва listen()
  // оставлял listener-leak на всю сессию.
  useEffect(() => {
    let cancelled = false;
    let unProgress: UnlistenFn | undefined;
    let unDone: UnlistenFn | undefined;
    (async () => {
      unProgress = await listen<ModelProgressEvent>('model:progress', (e) => {
        if (e.payload.id === EMBEDDER_ID) setProgress(e.payload);
        if (e.payload.id === PYANNOTE_ID) setPyannotePct(e.payload.pct);
      });
      if (cancelled) {
        unProgress();
        return;
      }
      unDone = await listen<ModelDoneEvent>('model:done', (e) => {
        const payload = e.payload;
        if (payload.id === PYANNOTE_ID) {
          setPyannotePct(null);
          void refreshPyannote();
          return;
        }
        if (payload.id !== EMBEDDER_ID) return;
        setDownloading(false);
        setProgress(null);
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
  }, [refresh, refreshPyannote, t]);

  const handleInstallPyannote = async () => {
    if (pyannoteDownloading) return;
    setPyannoteDownloading(true);
    setPyannotePct(0);
    try {
      await localEngineModelDownload(PYANNOTE_ID);
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

  const sizeBytes = status?.bytes_total ?? 0;

  const handleDownload = async () => {
    setDownloading(true);
    setError(null);
    setProgress({ id: EMBEDDER_ID, pct: 0, bytes_done: 0, bytes_total: sizeBytes });
    try {
      await localEngineModelDownload(EMBEDDER_ID);
    } catch (e) {
      setError(humanError(e, t));
    } finally {
      // `model:done` тоже гасит флаг, но команда может отказать до старта
      // закачки — тогда события не будет вовсе.
      setDownloading(false);
      setProgress(null);
      await refresh();
    }
  };

  const handleDelete = async () => {
    setDeleting(true);
    setError(null);
    try {
      await localEngineModelDelete(EMBEDDER_ID);
      await refresh();
    } catch (e) {
      setError(humanError(e, t));
    } finally {
      setDeleting(false);
    }
  };

  if (!status || featureEnabled === null) {
    return (
      <div aria-busy="true" style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <Skeleton width="60%" height="0.85em" />
        <Skeleton width="100%" height="2.5rem" />
        <Skeleton width="40%" height="0.75em" />
      </div>
    );
  }

  const valid = status.state === 'present';

  return (
    <div>
      {!featureEnabled && (
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
            {formatMB(sizeBytes, t)} ·{' '}
            {valid
              ? t('voiceModel.descValid')
              : status.state === 'corrupted'
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
            {status.state === 'corrupted'
              ? t('voiceModel.btnRedownload')
              : t('voiceModel.btnDownload', { size: formatMB(sizeBytes, t) })}
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
          <Progress value={progress.pct} ariaLabel={t('voiceModel.btnDownloading')} />
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
              {formatMB(progress.bytes_done, t)} / {formatMB(progress.bytes_total, t)}
            </span>
            <span>{progress.pct.toFixed(0)}%</span>
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

// [B18.5b] Статус модели → Chip. present=ok, absent=line,
// corrupted=warn, downloading=accent.
function StatusChip({
  status,
  downloading,
}: {
  status: ModelStatus;
  downloading: boolean;
}) {
  const { t } = useI18n();
  const meta: Record<string, { variant: ChipVariant; text: string }> = {
    present: { variant: 'ok', text: t('voiceModel.statusValid') },
    absent: { variant: 'line', text: t('voiceModel.statusMissing') },
    corrupted: { variant: 'warn', text: t('voiceModel.statusCorrupted') },
    downloading: { variant: 'accent', text: t('voiceModel.statusDownloading') },
  };
  const m = meta[downloading ? 'downloading' : status.state] ?? meta.absent!;
  return (
    <Chip tone={m.variant} size="sm">
      {m.text}
    </Chip>
  );
}
