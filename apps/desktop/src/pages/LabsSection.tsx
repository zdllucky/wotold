// [M14 T-14] Labs section — experimental feature flags.
//
// Toggles:
// - Summary v2 (T-14, default ON) — cloud v2 prompt path
// - Speculative decoding (T-16 P2, default OFF) — 0.5B draft model для
//   Quality preset speedup
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
      settingId="summary-v2"
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
  const [speculativeEnabled, setSpeculativeEnabled] = useState<boolean>(
    SETTINGS_DEFAULTS.SUMMARY_SPECULATIVE_DECODING,
  );
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
      // Speculative — default OFF: explicit '1' enables.
      const rawSpec = await getSetting(SETTINGS_KEYS.SUMMARY_SPECULATIVE_DECODING).catch(
        () => null,
      );
      setSpeculativeEnabled(rawSpec === '1');
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

  const persistSpeculative = async (next: boolean) => {
    setSpeculativeEnabled(next);
    try {
      await setSetting(SETTINGS_KEYS.SUMMARY_SPECULATIVE_DECODING, next ? '1' : '0');
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

  // [B21] Все три контрола — единообразные Row (канон SecLabs).
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

      {/* [M14 T-16 P2] Speculative decoding toggle. */}
      <ToggleRow
        label={t('settings.speculativeDecodingLabel')}
        hint={t('settings.speculativeDecodingHint')}
        checked={speculativeEnabled}
        onToggle={(next) => void persistSpeculative(next)}
      />

      {/* [P1.2] Force-N-speakers Labs override. Whitelist 4 options → Select. */}
      <SettingRow
        settingId="force-num-speakers"
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
