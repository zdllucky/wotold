// #47: BYO API keys в системном keychain. Никаких значений в JS-памяти —
// записываем через invoke, читаем только статус (has key / empty).

import { invoke } from '@tauri-apps/api/core';

export type ByoProvider = 'soniox' | 'gladia' | 'anthropic';

export interface ByoStatus {
  provider: ByoProvider;
  present: boolean;
}

export const BYO_PROVIDERS: ByoProvider[] = ['soniox', 'gladia', 'anthropic'];

export function listByoStatus(): Promise<ByoStatus[]> {
  return invoke<ByoStatus[]>('list_byo_status');
}

/** Пустая строка = удаление (через delete_key idempotent). */
export function setByoKey(provider: ByoProvider, value: string): Promise<void> {
  return invoke('set_byo_key', { provider, value });
}

export function deleteByoKey(provider: ByoProvider): Promise<void> {
  return invoke('delete_byo_key', { provider });
}
