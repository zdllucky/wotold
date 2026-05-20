// Высокоуровневые хелперы для авторизации (#37):
// - startStateFlow → создаёт state-токен для OIDC redirect
// - createSession → создаёт активную сессию для аккаунта
// - readSessionFromRequest → извлекает Bearer / cookie session_id

import type { Context } from 'hono';
import type { Env } from '../env.js';
import {
  consumeState,
  putSession,
  putState,
  type OidcProvider,
  type Session,
  type StateRecord,
} from './storage.js';

function uuid(): string {
  // Workers runtime exposes crypto.randomUUID на globalThis.
  return crypto.randomUUID();
}

/** Создаёт state CSRF-токен. Возвращает stateId — клиент его передаёт IdP в `state` query. */
export async function startStateFlow(
  env: Env,
  provider: OidcProvider,
  redirectUri: string,
  deviceId: string | null,
): Promise<string> {
  const id = uuid();
  const rec: StateRecord = {
    provider,
    redirectUri,
    deviceId,
    createdAt: new Date().toISOString(),
  };
  await putState(env, id, rec);
  return id;
}

/** Проверяет state ↔ provider, возвращает payload. Single-use. */
export async function verifyState(
  env: Env,
  stateId: string,
  expectedProvider: OidcProvider,
): Promise<StateRecord | null> {
  const rec = await consumeState(env, stateId);
  if (!rec) return null;
  if (rec.provider !== expectedProvider) {
    return null;
  }
  return rec;
}

/** Создаёт session record для аккаунта, кладёт в KV с TTL. */
export async function createSession(env: Env, accountId: string): Promise<Session> {
  const ttlSec = Number(env.AUTH_SESSION_TTL_SECONDS) || 2_592_000;
  const now = new Date();
  const session: Session = {
    id: uuid(),
    accountId,
    createdAt: now.toISOString(),
    expiresAt: new Date(now.getTime() + ttlSec * 1000).toISOString(),
  };
  await putSession(env, session);
  return session;
}

/** Извлекает session id из `Authorization: Bearer <id>` или cookie `wotold_session`. */
export function readSessionId(c: Context): string | null {
  const auth = c.req.header('authorization');
  if (auth) {
    const m = auth.match(/^Bearer\s+(.+)$/i);
    if (m && m[1]) return m[1].trim();
  }
  const cookie = c.req.header('cookie') ?? '';
  for (const part of cookie.split(';')) {
    const [name, ...rest] = part.trim().split('=');
    if (name === 'wotold_session') return rest.join('=').trim() || null;
  }
  return null;
}
