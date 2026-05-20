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
} as const;

export type SttProvider = 'auto' | 'soniox' | 'gladia';
export type ProviderPath = 'managed' | 'byo';
