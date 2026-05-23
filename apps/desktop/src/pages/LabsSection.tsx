// [M14 T-14] Labs section — experimental feature flags.
//
// Сейчас один toggle — Summary v2 (default ON). Future home для T-04..T-10
// local engine opt-ins и других experimental rollouts.
//
// Mirror VoiceModelSection.tsx mic-diarization toggle pattern (.card-like
// wrapper + native checkbox + label/hint stack).

import { useEffect, useState } from 'react';

import { getSetting, setSetting, SETTINGS_KEYS, SETTINGS_DEFAULTS } from '../api/settings';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';

export function LabsSection() {
  const { t } = useI18n();
  const [v2Enabled, setV2Enabled] = useState<boolean>(SETTINGS_DEFAULTS.SUMMARY_V2_ENABLED);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      // Default ON: explicit '0'/'false' = OFF; иначе ON (включая null/missing).
      const raw = await getSetting(SETTINGS_KEYS.SUMMARY_V2_ENABLED).catch(() => null);
      setV2Enabled(raw !== '0' && raw !== 'false');
    })();
  }, []);

  const persist = async (next: boolean) => {
    setV2Enabled(next);
    try {
      await setSetting(SETTINGS_KEYS.SUMMARY_V2_ENABLED, next ? '1' : '0');
    } catch (e) {
      setError(humanError(e));
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 18, maxWidth: 600 }}>
      {error && (
        <p role="alert" style={{ color: 'var(--signal)', fontFamily: 'var(--font-sans)' }}>
          {error}
        </p>
      )}

      <div
        style={{
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
            checked={v2Enabled}
            onChange={(e) => void persist(e.target.checked)}
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
              {t('settings.summaryV2Label')}
            </div>
            <div
              style={{
                fontSize: 12,
                color: 'var(--subtle)',
                lineHeight: 1.5,
              }}
            >
              {t('settings.summaryV2Hint')}
            </div>
          </div>
        </label>
      </div>
    </div>
  );
}
