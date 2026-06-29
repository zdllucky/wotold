// [B18.5a] Appearance — theme (Segmented) + interface language. Accent picker
// removed: Wotold v2 is mono-graphite (B18.0 decision); accent is fixed `ink`.

import { Segmented, Select, type SegOption } from '../ui';
import { SUPPORTED_LOCALES, useI18n, type Locale } from '../i18n';
import { useTheme, type Theme } from '../theme/useTheme';

export function AppearanceSection() {
  const { theme, setTheme } = useTheme();
  const { locale, setLocale, t } = useI18n();

  const themeOptions: SegOption<Theme>[] = [
    { value: 'light', label: t('settings.themeLight'), icon: 'sun' },
    { value: 'dark', label: t('settings.themeDark'), icon: 'moon' },
    { value: 'system', label: t('settings.themeSystem') },
  ];

  return (
    <div>
      <div className="field" style={{ marginBottom: 24 }}>
        <label className="field-label">{t('settings.fieldTheme')}</label>
        <Segmented<Theme>
          options={themeOptions}
          value={theme}
          onChange={setTheme}
          ariaLabel={t('settings.fieldTheme')}
          style={{ marginTop: 8 }}
        />
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
