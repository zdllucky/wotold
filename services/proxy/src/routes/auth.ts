// M10 auth routes (#37).
//
// Эндпоинты:
//   POST /v1/auth/{provider}/start          → возвращает authorizeUrl
//   GET  /v1/auth/{provider}/callback       → обмен code, создаёт session, возвращает sessionId
//   GET  /v1/auth/me                        → текущая identity (Bearer / cookie)
//   POST /v1/auth/signout                   → удаляет session
//
// M10.3 паспорта: аккаунт в MVP **ничего не разблокирует** — это scaffold под
// облачную синхронизацию. Локальный режим работает без auth.

import { Hono } from 'hono';
import type { Context } from 'hono';
import type { Env } from '../lib/env.js';
import { getAdapter } from '../lib/auth/providers.js';
import {
  createSession,
  readSessionId,
  startStateFlow,
  verifyState,
} from '../lib/auth/session.js';
import {
  deleteSession,
  findAccountByProvider,
  getAccount,
  getSession,
  OIDC_PROVIDERS,
  putAccount,
  type Account,
  type OidcProvider,
} from '../lib/auth/storage.js';

export const authRoutes = new Hono<{ Bindings: Env }>();

function jsonError(c: Context, code: string, message: string, status: 400 | 401 | 404 | 500) {
  return c.json({ ok: false, code, message }, status);
}

function parseProvider(raw: string): OidcProvider | null {
  return (OIDC_PROVIDERS as string[]).includes(raw) ? (raw as OidcProvider) : null;
}

function buildRedirectUri(c: Context<{ Bindings: Env }>, provider: OidcProvider): string {
  const base =
    c.env.PUBLIC_BASE_URL?.replace(/\/+$/, '') ||
    new URL(c.req.url).origin;
  return `${base}/v1/auth/${provider}/callback`;
}

authRoutes.post('/:provider/start', async (c) => {
  const provider = parseProvider(c.req.param('provider'));
  if (!provider) return jsonError(c, 'unknown_provider', 'Unknown provider', 404);

  const body = (await c.req.json().catch(() => ({}))) as { deviceId?: string };
  const deviceId = body.deviceId ?? null;

  const redirectUri = buildRedirectUri(c, provider);
  const state = await startStateFlow(c.env, provider, redirectUri, deviceId);

  try {
    const url = getAdapter(provider).buildAuthorizeUrl(c.env, { state, redirectUri });
    return c.json({ authorizeUrl: url, state });
  } catch (e) {
    return jsonError(c, 'provider_error', (e as Error).message, 500);
  }
});

authRoutes.get('/:provider/callback', async (c) => {
  const provider = parseProvider(c.req.param('provider'));
  if (!provider) return jsonError(c, 'unknown_provider', 'Unknown provider', 404);

  const code = c.req.query('code');
  const state = c.req.query('state');
  const idpError = c.req.query('error');

  if (idpError) {
    return jsonError(c, 'idp_error', `IdP returned error: ${idpError}`, 400);
  }
  if (!code || !state) {
    return jsonError(c, 'bad_request', 'code and state required', 400);
  }

  const stateRec = await verifyState(c.env, state, provider);
  if (!stateRec) {
    return jsonError(c, 'invalid_state', 'state invalid, expired, or provider mismatch', 400);
  }

  let identity;
  try {
    identity = await getAdapter(provider).exchangeCode(c.env, {
      code,
      redirectUri: stateRec.redirectUri,
    });
  } catch (e) {
    return jsonError(c, 'exchange_failed', (e as Error).message, 500);
  }

  // Link или create account.
  let account = await findAccountByProvider(c.env, provider, identity.providerUserId);
  if (!account) {
    account = {
      id: crypto.randomUUID(),
      provider,
      providerUserId: identity.providerUserId,
      email: identity.email,
      displayName: identity.displayName,
      createdAt: new Date().toISOString(),
      linkedDeviceId: stateRec.deviceId,
    } satisfies Account;
    await putAccount(c.env, account);
  } else if (account.email !== identity.email || account.displayName !== identity.displayName) {
    // Обновить identity-поля если изменились на стороне IdP.
    account = { ...account, email: identity.email, displayName: identity.displayName };
    await putAccount(c.env, account);
  }

  const session = await createSession(c.env, account.id);

  return c.json({
    sessionId: session.id,
    expiresAt: session.expiresAt,
    account: {
      id: account.id,
      provider: account.provider,
      email: account.email,
      displayName: account.displayName,
    },
  });
});

authRoutes.get('/me', async (c) => {
  const sid = readSessionId(c);
  if (!sid) return jsonError(c, 'no_session', 'Authentication required', 401);

  const session = await getSession(c.env, sid);
  if (!session) return jsonError(c, 'session_expired', 'Session not found or expired', 401);

  const account = await getAccount(c.env, session.accountId);
  if (!account) return jsonError(c, 'account_missing', 'Account record missing', 401);

  return c.json({
    account: {
      id: account.id,
      provider: account.provider,
      email: account.email,
      displayName: account.displayName,
    },
    session: { id: session.id, expiresAt: session.expiresAt },
  });
});

authRoutes.post('/signout', async (c) => {
  const sid = readSessionId(c);
  if (sid) {
    await deleteSession(c.env, sid);
  }
  return c.json({ ok: true });
});
