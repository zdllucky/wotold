// [B3.7c, B21] «Спикеры» — voice embedder model + biometric toggles.
//
// Канон SecSpeakers (wk-settings.jsx): компактная Panel p14 «Голосовой модуль»
// (icon-квадрат + имя + размер + Chip-статус + sm-кнопки), под ней плотные
// SettingRow: авто-привязка, порог уверенности (⊕ B21 — ключ читался
// backend'ом, UI не было), несколько голосов на микрофоне (при отсутствии
// pyannote — кнопка установки модуля с реальным прогрессом model:progress).
//
// Голосовой эмбеддер — обязательный базовый модуль (`voice-embedder`), и
// качает его общий баннер готовности вместе с остальными. Здесь только статус:
// своя кнопка скачивания означала бы вторую очередь на те же файлы и второй
// прогресс-бар рядом с баннером, показывающий то же самое. Раньше у эмбеддера
// была вообще отдельная качалка со своими событиями `voice-model:*`, из-за
// чего таблица моделей слушала не тот канал и кнопка выглядела мёртвой.

import { useCallback, useEffect, useState } from 'react';

import { localEngineModelStatus } from '../api/local-engine';
import { voiceEmbedderFeatureEnabled } from '../api/speakers';
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
import { Chip, Icon, Select, SettingRow, Skeleton, Switch } from '../ui';
import { useReadiness } from '../components/readiness/ReadinessProvider';

type TFn = ReturnType<typeof useI18n>['t'];

const EMBEDDER_ID = 'voice-embedder';

function formatMB(bytes: number, t: TFn): string {
  return t('voiceModel.mb', { n: (bytes / (1024 * 1024)).toFixed(1) });
}

export function VoiceModelSection() {
  const { t } = useI18n();
  // Снимок готовности — единственный источник правды по докачке модулей.
  const { readiness, downloadingIds } = useReadiness();
  const [status, setStatus] = useState<ModelStatus | null>(null);
  const [featureEnabled, setFeatureEnabled] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [autoBindEnabled, setAutoBindEnabled] = useState<boolean>(
    SETTINGS_DEFAULTS.AUTO_BIND_ENABLED,
  );
  // [B21 ⊕] Порог авто-привязки — backend читал ключ всегда, UI не было.
  const [autoBindThreshold, setAutoBindThreshold] = useState<AutoBindThreshold>(
    SETTINGS_DEFAULTS.AUTO_BIND_THRESHOLD,
  );
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
    })();
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Статус перечитывается на каждый снимок готовности: докачку ведёт баннер,
  // и свои подписки на `model:*` здесь были бы вторым источником правды.
  useEffect(() => {
    if (readiness) void refresh();
  }, [readiness, refresh]);

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

  const sizeBytes = status?.bytes_total ?? 0;
  const downloading = downloadingIds.has(EMBEDDER_ID);

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
      </div>

      {/* Плотные Row-настройки (канон :304-313). Авто-привязка гасится пока
          модель не скачана: без эмбеддингов матчинга нет. */}
      <div style={{ marginTop: 8 }}>
        <SettingRow
          label={t('settings.speakersAutoBindLabel')}
          hint={t('settings.speakersAutoBindHint')}
          align="top"
          disabled={!valid}
          // Порог показывается только при включённой привязке — тогда
          // последний он, иначе висячий разделитель под группой.
          last={!(autoBindEnabled && valid)}
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
            last
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
