// [M14 T-14] Labs section — experimental feature flags.
//
// Toggles:
// - Summary v2 (T-14, default ON) — cloud v2 prompt path
// - Force N speakers — аварийный ограничитель диаризации
//
// Тумблер ускорения генерации убран: черновая модель входит в обязательный
// набор и применяется всегда, когда лежит на диске. Тумблер был неработающим —
// он ничего не скачивал, а резидентный сервер аргумент draft-модели вообще не
// получал.
//
// [B18.5b] Wotold v2 restyle: native checkboxes → `.setting-row` + `.switch`
// (role=switch, mirrors SettingsPage call-detect), force-N `<select>` → `Select`.

import { useEffect, useState } from 'react';

import {
  getSetting,
  setSetting,
  SETTINGS_KEYS,
  SETTINGS_DEFAULTS,
  MIC_DIARIZATION_NUM_SPEAKERS_OPTIONS,
  type MicDiarizationNumSpeakers,
} from '../api/settings';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';
import { Select, SettingRow, Switch } from '../ui';

interface ToggleRowProps {
  label: string;
  hint: string;
  checked: boolean;
  onToggle: (next: boolean) => void;
  last?: boolean;
}

// [B18.7c] v2 toggle row — thin wrapper around SettingRow + Switch wrappers.
function ToggleRow({ label, hint, checked, onToggle, last }: ToggleRowProps) {
  return (
    <SettingRow
      label={label}
      hint={hint}
      align="top"
      last={last}
      control={<Switch checked={checked} onChange={onToggle} label={label} />}
    />
  );
}

export function LabsSection() {
  const { t } = useI18n();
  const [v2Enabled, setV2Enabled] = useState<boolean>(SETTINGS_DEFAULTS.SUMMARY_V2_ENABLED);
  // [P1.2] Labs «Force N speakers» override.
  const [numSpeakers, setNumSpeakers] = useState<MicDiarizationNumSpeakers>(
    SETTINGS_DEFAULTS.MIC_DIARIZATION_NUM_SPEAKERS,
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      // V2 — default ON: explicit '0'/'false' = OFF; иначе ON.
      const rawV2 = await getSetting(SETTINGS_KEYS.SUMMARY_V2_ENABLED).catch(() => null);
      setV2Enabled(rawV2 !== '0' && rawV2 !== 'false');
      // [P1.2] Force N speakers — whitelist enforce: '2'|'3' → keep;
      // всё прочее (включая legacy '4') → 'auto'. [P14.3] MAX=3 → '4' dropped.
      const rawNum = await getSetting(SETTINGS_KEYS.MIC_DIARIZATION_NUM_SPEAKERS).catch(
        () => null,
      );
      if (rawNum === '2' || rawNum === '3') {
        setNumSpeakers(rawNum);
      } else {
        setNumSpeakers('auto');
      }
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

  // [P1.2] Force-N-speakers persist. 'auto' тоже пишется явно (а не удаляется),
  // чтобы UI consistently показывал актуальное значение после reset.
  const persistNumSpeakers = async (next: MicDiarizationNumSpeakers) => {
    setNumSpeakers(next);
    try {
      await setSetting(SETTINGS_KEYS.MIC_DIARIZATION_NUM_SPEAKERS, next);
    } catch (e) {
      setError(humanError(e, t));
    }
  };

  // [B21] Оба контрола — единообразные Row (канон SecLabs).
  return (
    <div>
      {error && (
        <p role="alert" style={{ color: 'var(--danger)', margin: '0 0 12px' }}>
          {error}
        </p>
      )}

      <ToggleRow
        label={t('settings.summaryV2Label')}
        hint={t('settings.summaryV2Hint')}
        checked={v2Enabled}
        onToggle={(next) => void persistV2(next)}
      />

      {/* [P1.2] Force-N-speakers Labs override. Whitelist 4 options → Select. */}
      <SettingRow
        label={t('settings.forceNumSpeakersLabel')}
        hint={t('settings.forceNumSpeakersHint')}
        align="top"
        last
      >
        <Select<MicDiarizationNumSpeakers>
          value={numSpeakers}
          options={MIC_DIARIZATION_NUM_SPEAKERS_OPTIONS.map((opt) => ({
            value: opt,
            label: t(`settings.forceNumSpeakersOptions.${opt}`),
          }))}
          onChange={(v) => void persistNumSpeakers(v)}
        />
      </SettingRow>
    </div>
  );
}
