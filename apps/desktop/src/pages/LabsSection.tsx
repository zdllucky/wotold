// [M14 T-14] Labs section — experimental feature flags.
//
// Toggles:
// - Summary v2 (T-14, default ON) — cloud v2 prompt path
// - Speculative decoding (T-16 P2, default OFF) — 0.5B draft model для
//   Quality preset speedup
//
// Mirror VoiceModelSection.tsx mic-diarization toggle pattern (.card-like
// wrapper + native checkbox + label/hint stack).

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
      // [P1.2] Force N speakers — whitelist enforce: '2'|'3'|'4' → keep;
      // всё прочее → 'auto'.
      const rawNum = await getSetting(SETTINGS_KEYS.MIC_DIARIZATION_NUM_SPEAKERS).catch(
        () => null,
      );
      if (rawNum === '2' || rawNum === '3' || rawNum === '4') {
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
      setError(humanError(e));
    }
  };

  const persistSpeculative = async (next: boolean) => {
    setSpeculativeEnabled(next);
    try {
      await setSetting(SETTINGS_KEYS.SUMMARY_SPECULATIVE_DECODING, next ? '1' : '0');
    } catch (e) {
      setError(humanError(e));
    }
  };

  // [P1.2] Force-N-speakers persist. 'auto' тоже пишется явно (а не удаляется),
  // чтобы UI consistently показывал актуальное значение после reset.
  const persistNumSpeakers = async (next: MicDiarizationNumSpeakers) => {
    setNumSpeakers(next);
    try {
      await setSetting(SETTINGS_KEYS.MIC_DIARIZATION_NUM_SPEAKERS, next);
    } catch (e) {
      setError(humanError(e));
    }
  };

  const cardStyle: React.CSSProperties = {
    padding: 18,
    border: '1px solid var(--line-soft)',
    borderRadius: 'var(--radius-card, 8px)',
    background: 'var(--bg)',
  };
  const labelStyle: React.CSSProperties = {
    display: 'flex',
    alignItems: 'flex-start',
    gap: 12,
    cursor: 'pointer',
  };
  const titleStyle: React.CSSProperties = {
    fontFamily: 'var(--font-sans)',
    fontSize: 14,
    color: 'var(--ink)',
    fontWeight: 500,
    marginBottom: 4,
  };
  const hintStyle: React.CSSProperties = {
    fontSize: 12,
    color: 'var(--subtle)',
    lineHeight: 1.5,
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 18, maxWidth: 600 }}>
      {error && (
        <p role="alert" style={{ color: 'var(--signal)', fontFamily: 'var(--font-sans)' }}>
          {error}
        </p>
      )}

      <div style={cardStyle}>
        <label style={labelStyle}>
          <input
            type="checkbox"
            checked={v2Enabled}
            onChange={(e) => void persistV2(e.target.checked)}
            style={{ marginTop: 4 }}
          />
          <div style={{ flex: 1 }}>
            <div style={titleStyle}>{t('settings.summaryV2Label')}</div>
            <div style={hintStyle}>{t('settings.summaryV2Hint')}</div>
          </div>
        </label>
      </div>

      {/* [M14 T-16 P2] Speculative decoding toggle. */}
      <div style={cardStyle}>
        <label style={labelStyle}>
          <input
            type="checkbox"
            checked={speculativeEnabled}
            onChange={(e) => void persistSpeculative(e.target.checked)}
            style={{ marginTop: 4 }}
          />
          <div style={{ flex: 1 }}>
            <div style={titleStyle}>{t('settings.speculativeDecodingLabel')}</div>
            <div style={hintStyle}>{t('settings.speculativeDecodingHint')}</div>
          </div>
        </label>
      </div>

      {/* [P1.2] Force-N-speakers Labs override. Native <select> — minimal
          chrome, нет нужды в кастомных radio когда whitelist 4 options. */}
      <div style={cardStyle}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div style={titleStyle}>{t('settings.forceNumSpeakersLabel')}</div>
          <div style={hintStyle}>{t('settings.forceNumSpeakersHint')}</div>
          <select
            value={numSpeakers}
            onChange={(e) =>
              void persistNumSpeakers(e.target.value as MicDiarizationNumSpeakers)
            }
            aria-label={t('settings.forceNumSpeakersLabel')}
            style={{
              marginTop: 4,
              padding: '6px 10px',
              fontFamily: 'var(--font-sans)',
              fontSize: 13,
              color: 'var(--ink)',
              background: 'var(--bg-2, var(--bg))',
              border: '1px solid var(--line-soft)',
              borderRadius: 6,
              maxWidth: 240,
            }}
          >
            {MIC_DIARIZATION_NUM_SPEAKERS_OPTIONS.map((opt) => (
              <option key={opt} value={opt}>
                {t(`settings.forceNumSpeakersOptions.${opt}`)}
              </option>
            ))}
          </select>
        </div>
      </div>
    </div>
  );
}
