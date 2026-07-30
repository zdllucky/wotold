// [B21] «Спикеры» — настройки биометрии голоса.
//
// [design-gate] Surface: pages/VoiceModelSection
// Reference: docs/design/wotold-v2/_reference/wk-settings.jsx (SecSpeakers)
// Tokens: --danger (только текст ошибки), --warn (бейдж сборки без фичи)
// Classes: .panel, .setting-row (через SettingRow), .switch (через Switch)
// New tokens: нет
// Logic preserved: чтение и запись auto_bind_enabled / auto_bind_threshold.
//
// Карточки модуля здесь больше нет. Голосовой эмбеддер стал обязательным
// базовым модулем: пользователь им не управляет, ставится он сам, а про
// нехватку модулей говорит баннер готовности с любого экрана. Карточка
// показывала статус того, на что нельзя повлиять, — и дублировала баннер
// вторым индикатором прогресса на тот же файл.
//
// Осталось предупреждение о сборке без `voice-onnx`: другого места, где это
// видно, нет, а молча обещать работающую привязку при заглушке вместо
// эмбеддера нельзя.

import { useEffect, useState } from 'react';

import { voiceEmbedderFeatureEnabled } from '../api/speakers';
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
import { Select, SettingRow, Switch } from '../ui';

export function VoiceModelSection() {
  const { t } = useI18n();
  const [featureEnabled, setFeatureEnabled] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [autoBindEnabled, setAutoBindEnabled] = useState<boolean>(
    SETTINGS_DEFAULTS.AUTO_BIND_ENABLED,
  );
  // [B21 ⊕] Порог авто-привязки — backend читал ключ всегда, UI не было.
  const [autoBindThreshold, setAutoBindThreshold] = useState<AutoBindThreshold>(
    SETTINGS_DEFAULTS.AUTO_BIND_THRESHOLD,
  );

  useEffect(() => {
    void (async () => {
      const raw = await getSetting(SETTINGS_KEYS.AUTO_BIND_ENABLED).catch(() => null);
      setAutoBindEnabled(raw === '1');
      const rawThreshold = await getSetting(SETTINGS_KEYS.AUTO_BIND_THRESHOLD).catch(() => null);
      if (rawThreshold && (AUTO_BIND_THRESHOLDS as string[]).includes(rawThreshold)) {
        setAutoBindThreshold(rawThreshold as AutoBindThreshold);
      }
      // Ошибка чтения — считаем фичу собранной: дефолтная сборка её включает,
      // и ложный бейдж «привязка не работает» вреднее его отсутствия.
      setFeatureEnabled(await voiceEmbedderFeatureEnabled().catch(() => true));
    })();
  }, []);

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

      <SettingRow
        label={t('settings.speakersAutoBindLabel')}
        hint={t('settings.speakersAutoBindHint')}
        align="top"
        // Порог показывается только при включённой привязке — тогда
        // последний он, иначе висячий разделитель под группой.
        last={!autoBindEnabled}
      >
        <Switch
          checked={autoBindEnabled}
          onChange={(v) => void persistAutoBind(v)}
          label={t('settings.speakersAutoBindLabel')}
        />
      </SettingRow>

      {autoBindEnabled && (
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
  );
}
