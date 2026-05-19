import { invoke } from '@tauri-apps/api/core';

export function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>('get_setting', { key });
}

export function setSetting(key: string, value: string): Promise<void> {
  return invoke<void>('set_setting', { key, value });
}

export const SETTINGS_KEYS = {
  ONBOARDING_DONE: 'onboarding_done',
} as const;
