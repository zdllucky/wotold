// #38 (M10.2): фронтенд клиент для proxy /v1/auth/*.
//
// Session token хранится в keychain (через Tauri commands). JS-память не
// удерживает значение — read_account_session_token вызывается прямо перед
// HTTP-запросом и переменная сразу уходит из scope.

import { invoke } from '@tauri-apps/api/core';
import { getSetting, SETTINGS_KEYS } from './settings';

export type OidcProvider = 'google' | 'apple' | 'microsoft';

export interface StartSignInResponse {
  authorizeUrl: string;
  state: string;
}

export interface AccountIdentity {
  id: string;
  provider: OidcProvider;
  email: string | null;
  displayName: string | null;
}

export interface MeResponse {
  account: AccountIdentity;
  session: { id: string; expiresAt: string };
}

export interface AccountSessionStatus {
  present: boolean;
}

// ---------- proxy URL ----------

/**
 * Базовый URL прокси из settings.proxy_base_url. Без него ни /auth/start,
 * ни /me не выйдет.
 */
async function getProxyBaseUrl(): Promise<string> {
  const v = await getSetting(SETTINGS_KEYS.PROXY_BASE_URL);
  if (!v) {
    throw new Error(
      'Proxy URL не настроен. Settings → Proxy URL (после развёртывания CF Workers).',
    );
  }
  return v.replace(/\/+$/, '');
}

// ---------- keychain session ----------

export async function getAccountSessionStatus(): Promise<AccountSessionStatus> {
  return invoke<AccountSessionStatus>('get_account_session_status');
}

export async function setAccountSession(token: string): Promise<void> {
  return invoke('set_account_session', { token });
}

export async function clearAccountSession(): Promise<void> {
  return invoke('clear_account_session');
}

async function readAccountSessionToken(): Promise<string | null> {
  return invoke<string | null>('read_account_session_token');
}

// ---------- proxy /v1/auth ----------

/**
 * Шаг 1: запросить authorize URL у прокси. Клиент открывает URL в external
 * браузере (через @tauri-apps/plugin-shell open()). После OIDC-flow IdP
 * редиректит на /callback прокси:
 * - `redirectMode='deeplink'` ([B9]): 302 на `wotold://auth/callback?session=...`,
 *   Tauri deep-link plugin перехватывает и emit'ит 'auth:deep-link'.
 * - `redirectMode='json'` (default): прокси возвращает JSON, юзер копирует session
 *   и вставляет вручную в AccountSection (fallback при недоступности deep-link).
 */
export async function startSignIn(
  provider: OidcProvider,
  deviceId?: string,
  redirectMode: 'json' | 'deeplink' = 'json',
): Promise<StartSignInResponse> {
  const base = await getProxyBaseUrl();
  const body: Record<string, string> = { redirectMode };
  if (deviceId) body.deviceId = deviceId;
  const resp = await fetch(`${base}/v1/auth/${provider}/start`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    const text = await resp.text().catch(() => '');
    throw new Error(`start sign-in failed (${resp.status}): ${text.slice(0, 200)}`);
  }
  return (await resp.json()) as StartSignInResponse;
}

/**
 * Возвращает текущую identity если session в keychain валиден. null если
 * session отсутствует / истёк / прокси недоступен.
 */
export async function fetchMe(): Promise<MeResponse | null> {
  const token = await readAccountSessionToken();
  if (!token) return null;

  const base = await getProxyBaseUrl();
  const resp = await fetch(`${base}/v1/auth/me`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (resp.status === 401) {
    // Истёк — чистим keychain, чтобы не висел.
    await clearAccountSession();
    return null;
  }
  if (!resp.ok) {
    throw new Error(`/v1/auth/me failed (${resp.status})`);
  }
  return (await resp.json()) as MeResponse;
}

/**
 * Удаляет session на прокси (best-effort) + чистит локально.
 * Прокси-вызов может фейлиться (offline) — локальное удаление в любом случае.
 */
export async function signOut(): Promise<void> {
  const token = await readAccountSessionToken();
  if (token) {
    try {
      const base = await getProxyBaseUrl();
      await fetch(`${base}/v1/auth/signout`, {
        method: 'POST',
        headers: { authorization: `Bearer ${token}` },
      });
    } catch {
      // ignore — main goal = local cleanup.
    }
  }
  await clearAccountSession();
}
