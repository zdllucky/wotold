// [B18.5a] Appearance — theme (Segmented) + interface language. Accent picker
// removed: Wotold v2 is mono-graphite (B18.0 decision); accent is fixed `ink`.

import { Select } from '../ui';
import { Icon } from '../ui/Icon';
import { SUPPORTED_LOCALES, useI18n, type Locale } from '../i18n';
import { useTheme, type Theme } from '../theme/useTheme';

export function AppearanceSection() {
  const { theme, setTheme } = useTheme();
  const { locale, setLocale, t } = useI18n();

  const themeOptions: Array<{ id: Theme; label: string; icon?: 'sun' | 'moon' }> = [
    { id: 'light', label: t('settings.themeLight'), icon: 'sun' },
    { id: 'dark', label: t('settings.themeDark'), icon: 'moon' },
    { id: 'system', label: t('settings.themeSystem') },
  ];

  return (
    <div>
      <div className="field" style={{ marginBottom: 24 }}>
        <label className="field-label">{t('settings.fieldTheme')}</label>
        <div className="seg" role="tablist" aria-label={t('settings.fieldTheme')} style={{ marginTop: 8 }}>
          {themeOptions.map((opt) => (
            <button
              key={opt.id}
              type="button"
              data-active={theme === opt.id ? 'true' : undefined}
              aria-selected={theme === opt.id}
              onClick={() => setTheme(opt.id)}
            >
              {opt.icon && <Icon name={opt.icon} size={14} />}
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
        <span style={{ fontSize: 12, color: 'var(--text-3)', marginTop: 6 }}>
          {t('settings.languageHint')}
        </span>
      </div>
    </div>
  );
}
