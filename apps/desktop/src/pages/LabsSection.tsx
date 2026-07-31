// [M14 T-14] Labs — экспериментальные флаги.
//
// Остался один: новый формат саммари (default ON, аварийное отключение).
//
// Ушли отсюда: тумблер ускорения генерации (черновая модель обязательна и
// применяется всегда, а тумблер ничего не скачивал) и «число собеседников» —
// он был костылём вокруг потолка в три спикера. Потолок поднят до санитарных
// десяти, число кластеров определяет сам диаризатор.
//
// [B18.5b] Wotold v2 restyle: native checkbox → `.setting-row` + `.switch`
// (role=switch, как у call-detect в SettingsPage).

import { useEffect, useState } from 'react';

import { getSetting, setSetting, SETTINGS_KEYS, SETTINGS_DEFAULTS } from '../api/settings';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';
import { SettingRow, Switch } from '../ui';

export function LabsSection() {
  const { t } = useI18n();
  const [v2Enabled, setV2Enabled] = useState<boolean>(SETTINGS_DEFAULTS.SUMMARY_V2_ENABLED);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      // V2 — default ON: явные '0'/'false' = OFF, иначе ON.
      const rawV2 = await getSetting(SETTINGS_KEYS.SUMMARY_V2_ENABLED).catch(() => null);
      setV2Enabled(rawV2 !== '0' && rawV2 !== 'false');
    })();
  }, []);

  const persistV2 = async (next: boolean) => {
    setV2Enabled(next);
    try {
      await setSetting(SETTINGS_KEYS.SUMMARY_V2_ENABLED, next ? '1' : '0');
    } catch (e) {
      setError(humanError(e, t));
    }
  };

  return (
    <div>
      {error && (
        <p role="alert" style={{ color: 'var(--danger)', margin: '0 0 12px' }}>
          {error}
        </p>
      )}

      <SettingRow
        settingId="summary-v2"
        label={t('settings.summaryV2Label')}
        hint={t('settings.summaryV2Hint')}
        align="top"
        last
        control={
          <Switch
            checked={v2Enabled}
            onChange={(next) => void persistV2(next)}
            label={t('settings.summaryV2Label')}
          />
        }
      />
    </div>
  );
}
