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
  /** [V7] Auto-bind speaker когда suggestion_score >= AUTO_BIND_THRESHOLD/100.
   *  '1' = включено, иначе off. Default OFF (R2 паспорта). */
  AUTO_BIND_ENABLED: 'auto_bind_enabled',
  /** [V7] Min similarity для auto-bind (90 | 95 | 98). Default '95'. */
  AUTO_BIND_THRESHOLD: 'auto_bind_threshold',
  /** [W1] Configurable hotkey для recording toggle (start↔stop).
   *  Format: `Cmd+Shift+KeyR` (см. utils/hotkey.ts serializeHotkey).
   *  Empty value = use DEFAULT_TOGGLE_HOTKEY. */
  RECORDING_HOTKEY_TOGGLE: 'recording.hotkey.toggle',
  /** [W1] Configurable hotkey для pause↔resume (W2 wires the action). */
  RECORDING_HOTKEY_PAUSE: 'recording.hotkey.pause',
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
  /** [V7] Default OFF — R2 паспорта: opt-in только. */
  AUTO_BIND_ENABLED: false,
  AUTO_BIND_THRESHOLD: '95' as AutoBindThreshold,
} as const;

/** [V7] Whitelisted threshold values. '90' / '95' / '98' — три уровня риска.
 *  String union because Select<V extends string> generic constraint. */
export type AutoBindThreshold = '90' | '95' | '98';
export const AUTO_BIND_THRESHOLDS: AutoBindThreshold[] = ['90', '95', '98'];

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
