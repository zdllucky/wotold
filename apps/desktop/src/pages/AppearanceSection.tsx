// [B17] Appearance section — theme + accent + interface language picker.
// Тема/акцент через useTheme(), язык — через useI18n() с persist в
// SETTINGS_KEYS.UI_LOCALE.

import { Select } from '../ui';
import { SUPPORTED_LOCALES, useI18n, type Locale } from '../i18n';
import { useTheme, type Accent, type Theme } from '../theme/useTheme';

export function AppearanceSection() {
  const { theme, setTheme, accent, setAccent } = useTheme();
  const { locale, setLocale, t } = useI18n();

  const themeOptions: Array<{ id: Theme; label: string }> = [
    { id: 'light', label: t('settings.themeLight') },
    { id: 'dark', label: t('settings.themeDark') },
    { id: 'system', label: t('settings.themeSystem') },
  ];

  const accentOptions: Array<{ id: Accent; label: string }> = [
    { id: 'bordeaux', label: t('settings.accentBordeaux') },
    { id: 'persian', label: t('settings.accentPersian') },
    { id: 'ink', label: t('settings.accentInk') },
  ];

  return (
    <div>
      <div className="field" style={{ marginBottom: 24 }}>
        <label className="field-label">{t('settings.fieldTheme')}</label>
        <div style={{ display: 'flex', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
          {themeOptions.map((opt) => (
            <button
              key={opt.id}
              type="button"
              className={`btn ${theme === opt.id ? 'btn--primary' : 'btn--ghost'}`}
              onClick={() => setTheme(opt.id)}
              aria-pressed={theme === opt.id}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      <div className="field" style={{ marginBottom: 24 }}>
        <label className="field-label">{t('settings.fieldAccent')}</label>
        <div style={{ display: 'flex', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
          {accentOptions.map((opt) => (
            <button
              key={opt.id}
              type="button"
              className={`btn ${accent === opt.id ? 'btn--primary' : 'btn--ghost'}`}
              onClick={() => setAccent(opt.id)}
              aria-pressed={accent === opt.id}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      <div className="field" style={{ maxWidth: 320 }}>
        <label className="field-label">{t('settings.fieldLanguage')}</label>
        <Select<Locale>
          value={locale}
          options={SUPPORTED_LOCALES.map((l) => ({
            value: l.code,
            label: l.nativeLabel,
          }))}
          onChange={(v) => setLocale(v)}
        />
        <span style={{ fontSize: 12, color: 'var(--subtle)', marginTop: 6 }}>
          {t('settings.languageHint')}
        </span>
      </div>
    </div>
  );
}
