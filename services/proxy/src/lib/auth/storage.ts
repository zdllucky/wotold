// M10 auth (#37): KV storage схема для accounts, sessions, state-токенов.
//
// W5 security:
// - account_id = UUID. Никогда не основан на email/имени из IdP.
// - Никаких raw OAuth-токенов в KV — храним только нужное для linking
//   (provider + provider_user_id + display_name + email при наличии).
// - Session token = crypto-random UUID, KV TTL ограничен AUTH_SESSION_TTL_SECONDS.
// - State token = crypto-random UUID, KV TTL = AUTH_STATE_TTL_SECONDS (5 min default).

import type { Env } from '../env.js';

export type OidcProvider = 'google' | 'apple' | 'microsoft';

export const OIDC_PROVIDERS: OidcProvider[] = ['google', 'apple', 'microsoft'];

/** Запись аккаунта. Stable id = UUID, провайдер identity отдельно. */
export interface Account {
  /** UUID v4 — внутренний идентификатор. */
  id: string;
  provider: OidcProvider;
  /** `sub` из ID-token. У одного IdP уникален per-account. */
  providerUserId: string;
  email: string | null;
  displayName: string | null;
  createdAt: string;
  /** device-id первого link'а — для аудита, не используется для авторизации. */
  linkedDeviceId: string | null;
}

/** Active session, returned клиенту как cookie / Bearer token. */
export interface Session {
  /** UUID v4 — secret bearer token. */
  id: string;
  accountId: string;
  createdAt: string;
  /** ISO-stamp когда KV TTL истечёт — продублировано для удобства /me ответов. */
  expiresAt: string;
}

/** Short-lived state-токен для OIDC CSRF. */
export interface StateRecord {
  provider: OidcProvider;
  /** redirect_uri зафиксированный при start — должен совпасть на callback. */
  redirectUri: string;
  /** device-id (если клиент его прислал в start) — линкуется в Account при первом auth. */
  deviceId: string | null;
  createdAt: string;
}

// ----- key prefixes -----

const K_ACCOUNT = 'account:'; // account:{id}
const K_ACCOUNT_BY_PROVIDER = 'account_by_provider:'; // account_by_provider:{provider}:{providerUserId} → account.id
const K_SESSION = 'session:'; // session:{id}
const K_STATE = 'state:'; // state:{id}

// ----- account ops -----

export async function getAccount(env: Env, id: string): Promise<Account | null> {
  const raw = await env.AUTH.get(K_ACCOUNT + id);
  return raw ? (JSON.parse(raw) as Account) : null;
}

export async function findAccountByProvider(
  env: Env,
  provider: OidcProvider,
  providerUserId: string,
): Promise<Account | null> {
  const id = await env.AUTH.get(K_ACCOUNT_BY_PROVIDER + provider + ':' + providerUserId);
  return id ? getAccount(env, id) : null;
}

export async function putAccount(env: Env, account: Account): Promise<void> {
  await env.AUTH.put(K_ACCOUNT + account.id, JSON.stringify(account));
  await env.AUTH.put(
    K_ACCOUNT_BY_PROVIDER + account.provider + ':' + account.providerUserId,
    account.id,
  );
}

// ----- session ops -----

function sessionTtl(env: Env): number {
  return Number(env.AUTH_SESSION_TTL_SECONDS) || 2_592_000;
}

export async function putSession(env: Env, session: Session): Promise<void> {
  await env.AUTH.put(K_SESSION + session.id, JSON.stringify(session), {
    expirationTtl: sessionTtl(env),
  });
}

export async function getSession(env: Env, id: string): Promise<Session | null> {
  const raw = await env.AUTH.get(K_SESSION + id);
  return raw ? (JSON.parse(raw) as Session) : null;
}

export async function deleteSession(env: Env, id: string): Promise<void> {
  await env.AUTH.delete(K_SESSION + id);
}

// ----- state ops -----

function stateTtl(env: Env): number {
  return Number(env.AUTH_STATE_TTL_SECONDS) || 300;
}

export async function putState(env: Env, id: string, record: StateRecord): Promise<void> {
  await env.AUTH.put(K_STATE + id, JSON.stringify(record), {
    expirationTtl: stateTtl(env),
  });
}

/** Single-use: state consumed после первого consumeState и удаляется. */
export async function consumeState(env: Env, id: string): Promise<StateRecord | null> {
  const raw = await env.AUTH.get(K_STATE + id);
  if (!raw) return null;
  await env.AUTH.delete(K_STATE + id);
  return JSON.parse(raw) as StateRecord;
}
