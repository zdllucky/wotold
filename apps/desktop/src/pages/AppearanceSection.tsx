// [B18.5a, B21] Appearance — theme (Segmented) + interface language, плотными
// SettingRow (канон SecAppearance). Accent picker removed: Wotold v2 is
// mono-graphite (B18.0 decision); accent is fixed `ink`.

import { Segmented, Select, SettingRow, type SegOption } from '../ui';
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
      <SettingRow label={t('settings.fieldTheme')}>
        <Segmented<Theme>
          options={themeOptions}
          value={theme}
          onChange={setTheme}
          ariaLabel={t('settings.fieldTheme')}
        />
      </SettingRow>
      <SettingRow label={t('settings.fieldLanguage')} hint={t('settings.languageHint')} last>
        <Select<Locale>
          value={locale}
          options={SUPPORTED_LOCALES.map((l) => ({
            value: l.code,
            label: l.nativeLabel,
          }))}
          onChange={(v) => setLocale(v)}
        />
      </SettingRow>
    </div>
  );
}
