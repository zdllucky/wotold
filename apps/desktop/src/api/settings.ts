import { invoke } from '@tauri-apps/api/core';

export function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>('get_setting', { key });
}

export function setSetting(key: string, value: string): Promise<void> {
  return invoke<void>('set_setting', { key, value });
}

export const SETTINGS_KEYS = {
  ONBOARDING_DONE: 'onboarding_done',
  STT_PROVIDER: 'stt_provider',
  PROVIDER_PATH: 'provider_path',
  LLM_MODEL: 'llm_model',
  PROXY_BASE_URL: 'proxy_base_url',
  RECORDING_CONSENT_AT: 'recording_consent_at',
  /** [B13] BCP47 язык override для LLM-output. 'auto' = язык STT detection. */
  PREFERRED_LANGUAGE: 'preferred_language',
  /** [B16] Coachmarks показаны хотя бы раз — '1' = не показывать снова. */
  COACHMARKS_SEEN: 'coachmarks_seen',
  /** [B17] Atelier theme — 'light' | 'dark' | 'system'. */
  UI_THEME: 'ui.theme',
  /** [B17] Atelier accent — 'bordeaux' | 'persian' | 'ink'. */
  UI_ACCENT: 'ui.accent',
  /** UI locale — 'ru' | 'kk' | 'en'. Пусто = auto-detect from system. */
  UI_LOCALE: 'ui_locale',
} as const;

export const SETTINGS_DEFAULTS = {
  STT_PROVIDER: 'auto' as SttProvider,
  PROVIDER_PATH: 'managed' as ProviderPath,
  /** Пусто → прокси использует свой default (LLM_BACKEND-зависимый).
   *  Override на конкретную модель через Settings → LLM section. */
  LLM_MODEL: '',
  /** Default proxy URL — dev-сборка целится на staging, prod на production.
   *  User override через Settings → Прокси → Advanced. */
  PROXY_BASE_URL: import.meta.env.DEV
    ? 'https://wotold-proxy-staging.animereader.workers.dev'
    : 'https://wotold-proxy.animereader.workers.dev',
  PREFERRED_LANGUAGE: 'auto' as PreferredLanguage,
} as const;

/** Список языков для UI селектора. 'auto' = язык STT. */
export type PreferredLanguage = 'auto' | 'ru' | 'en' | 'kk' | string;
export const PREFERRED_LANGUAGES: Array<{ code: PreferredLanguage; label: string }> = [
  { code: 'auto', label: 'Автоматически (как в звонке)' },
  { code: 'ru', label: 'Русский' },
  { code: 'en', label: 'English' },
  { code: 'kk', label: 'Қазақша' },
];

export type SttProvider = 'auto' | 'soniox' | 'gladia';
export type ProviderPath = 'managed' | 'byo';
