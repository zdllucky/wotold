// #48 (M7.5 follow-up): фронтенд клиент для proxy /v1/usage.
//
// Возвращает свежий snapshot квоты текущего deviceId. Offline-safe:
// при network/HTTP-ошибке throws — UI рисует «недоступно».

import { invoke } from '@tauri-apps/api/core';
import { DEVICE_ID_HEADER, type UsageResponse } from '@wotold/contracts';
import { getSetting, SETTINGS_DEFAULTS, SETTINGS_KEYS } from './settings';

async function getProxyBaseUrl(): Promise<string> {
  const v = (await getSetting(SETTINGS_KEYS.PROXY_BASE_URL))?.trim();
  return (v && v.length > 0 ? v : SETTINGS_DEFAULTS.PROXY_BASE_URL).replace(/\/+$/, '');
}

export async function fetchUsage(): Promise<UsageResponse> {
  const [base, deviceId] = await Promise.all([
    getProxyBaseUrl(),
    invoke<string>('get_device_id'),
  ]);
  const resp = await fetch(`${base}/v1/usage`, {
    headers: { [DEVICE_ID_HEADER]: deviceId },
  });
  if (!resp.ok) {
    throw new Error(`/v1/usage failed (${resp.status})`);
  }
  return (await resp.json()) as UsageResponse;
}
