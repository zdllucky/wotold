/// <reference types="@cloudflare/vitest-pool-workers" />
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { SELF, env } from 'cloudflare:test';
import type { Env } from '../lib/env.js';

declare module 'cloudflare:test' {
  interface ProvidedEnv extends Env {}
}

beforeEach(async () => {
  const keys = await env.AUTH.list();
  for (const k of keys.keys) await env.AUTH.delete(k.name);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function b64urlEncode(s: string): string {
  const bytes = new TextEncoder().encode(s);
  let bin = '';
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function buildIdToken(payload: Record<string, unknown>): string {
  // [B16] decodeIdTokenPayload теперь валидирует iss/aud/exp claims —
  // тестовые токены должны их содержать чтобы callback не падал 500.
  // GOOGLE_OAUTH_CLIENT_ID в wrangler.test.toml = 'test-google-client-id'.
  const claims: Record<string, unknown> = {
    iss: 'https://accounts.google.com',
    aud: 'test-google-client-id',
    exp: Math.floor(Date.now() / 1000) + 3600,
    iat: Math.floor(Date.now() / 1000),
    ...payload,
  };
  const header = b64urlEncode(JSON.stringify({ alg: 'RS256', typ: 'JWT' }));
  const body = b64urlEncode(JSON.stringify(claims));
  return `${header}.${body}.sig`;
}

describe('POST /v1/auth/:provider/start', () => {
  test('returns 404 for unknown provider', async () => {
    const res = await SELF.fetch('http://proxy/v1/auth/yandex/start', { method: 'POST' });
    expect(res.status).toBe(404);
  });

  test('google start returns authorize URL with state', async () => {
    const res = await SELF.fetch('http://proxy/v1/auth/google/start', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ deviceId: '11111111-1111-4111-8111-111111111111' }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { authorizeUrl: string; state: string };
    expect(body.state).toMatch(/^[0-9a-f-]{36}$/);
    const parsed = new URL(body.authorizeUrl);
    expect(parsed.host).toBe('accounts.google.com');
    expect(parsed.searchParams.get('state')).toBe(body.state);
  });

  test('apple start returns 500 — provider stub', async () => {
    const res = await SELF.fetch('http://proxy/v1/auth/apple/start', {
      method: 'POST',
      body: '{}',
      headers: { 'content-type': 'application/json' },
    });
    expect(res.status).toBe(500);
    const body = (await res.json()) as { code: string; message: string };
    expect(body.code).toBe('provider_error');
    expect(body.message).toMatch(/not yet implemented/);
  });
});

describe('GET /v1/auth/:provider/callback', () => {
  test('returns 400 when state or code missing', async () => {
    const res = await SELF.fetch('http://proxy/v1/auth/google/callback?code=x');
    expect(res.status).toBe(400);
  });

  test('returns 400 when idp returned error', async () => {
    const res = await SELF.fetch('http://proxy/v1/auth/google/callback?error=access_denied');
    expect(res.status).toBe(400);
    const body = (await res.json()) as { code: string };
    expect(body.code).toBe('idp_error');
  });

  test('returns 400 for unknown state', async () => {
    const res = await SELF.fetch(
      'http://proxy/v1/auth/google/callback?code=x&state=does-not-exist',
    );
    expect(res.status).toBe(400);
    const body = (await res.json()) as { code: string };
    expect(body.code).toBe('invalid_state');
  });

  test('full happy flow: start → callback → /me → signout', async () => {
    // 1. start — получаем state.
    const startRes = await SELF.fetch('http://proxy/v1/auth/google/start', {
      method: 'POST',
      body: JSON.stringify({ deviceId: '77777777-7777-4777-8777-777777777777' }),
      headers: { 'content-type': 'application/json' },
    });
    const { state } = (await startRes.json()) as { state: string };
    expect(state).toBeTruthy();

    // 2. callback с mocked Google token endpoint.
    const idToken = buildIdToken({
      sub: 'google-sub-99',
      email: 'damir@example.com',
      name: 'Damir',
    });
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const url = typeof input === 'string' ? input : input.toString();
        if (url.startsWith('https://oauth2.googleapis.com/token')) {
          return new Response(
            JSON.stringify({ id_token: idToken, access_token: 'at' }),
            { status: 200, headers: { 'content-type': 'application/json' } },
          );
        }
        throw new Error(`unexpected fetch in test: ${url}`);
      }),
    );

    const cbRes = await SELF.fetch(
      `http://proxy/v1/auth/google/callback?code=authcode&state=${state}`,
    );
    expect(cbRes.status).toBe(200);
    const cb = (await cbRes.json()) as {
      sessionId: string;
      account: { id: string; provider: string; email: string; displayName: string };
    };
    expect(cb.sessionId).toMatch(/^[0-9a-f-]{36}$/);
    expect(cb.account.provider).toBe('google');
    expect(cb.account.email).toBe('damir@example.com');
    expect(cb.account.displayName).toBe('Damir');

    // 3. /me с Bearer должен вернуть identity.
    const meRes = await SELF.fetch('http://proxy/v1/auth/me', {
      headers: { authorization: `Bearer ${cb.sessionId}` },
    });
    expect(meRes.status).toBe(200);
    const me = (await meRes.json()) as { account: { id: string } };
    expect(me.account.id).toBe(cb.account.id);

    // 4. signout удаляет сессию.
    const soRes = await SELF.fetch('http://proxy/v1/auth/signout', {
      method: 'POST',
      headers: { authorization: `Bearer ${cb.sessionId}` },
    });
    expect(soRes.status).toBe(200);

    // 5. /me после signout — 401.
    const me2 = await SELF.fetch('http://proxy/v1/auth/me', {
      headers: { authorization: `Bearer ${cb.sessionId}` },
    });
    expect(me2.status).toBe(401);
  });

  test('second callback with same state burns state and returns invalid_state', async () => {
    const startRes = await SELF.fetch('http://proxy/v1/auth/google/start', {
      method: 'POST',
      body: '{}',
      headers: { 'content-type': 'application/json' },
    });
    const { state } = (await startRes.json()) as { state: string };

    const idToken = buildIdToken({ sub: 'g-1' });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ id_token: idToken }), { status: 200 }),
      ),
    );
    const first = await SELF.fetch(
      `http://proxy/v1/auth/google/callback?code=x&state=${state}`,
    );
    expect(first.status).toBe(200);

    const second = await SELF.fetch(
      `http://proxy/v1/auth/google/callback?code=x&state=${state}`,
    );
    expect(second.status).toBe(400);
  });
});

describe('GET /v1/auth/me', () => {
  test('returns 401 without session', async () => {
    const res = await SELF.fetch('http://proxy/v1/auth/me');
    expect(res.status).toBe(401);
  });

  test('returns 401 for expired/unknown session', async () => {
    const res = await SELF.fetch('http://proxy/v1/auth/me', {
      headers: { authorization: 'Bearer never-existed' },
    });
    expect(res.status).toBe(401);
  });
});

describe('POST /v1/auth/signout', () => {
  test('idempotent without session', async () => {
    const res = await SELF.fetch('http://proxy/v1/auth/signout', { method: 'POST' });
    expect(res.status).toBe(200);
  });
});

describe('[B9] deep-link callback redirect', () => {
  test('callback returns 302 to wotold:// when start used redirectMode=deeplink', async () => {
    const startRes = await SELF.fetch('http://proxy/v1/auth/google/start', {
      method: 'POST',
      body: JSON.stringify({ deviceId: 'd1d1d1d1-d1d1-4d1d-8d1d-d1d1d1d1d1d1', redirectMode: 'deeplink' }),
      headers: { 'content-type': 'application/json' },
    });
    const { state } = (await startRes.json()) as { state: string };

    const idToken = buildIdToken({
      sub: 'google-dl',
      email: 'dl@example.com',
      name: 'Damir',
    });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ id_token: idToken }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );

    const cbRes = await SELF.fetch(
      `http://proxy/v1/auth/google/callback?code=authcode&state=${state}`,
      { redirect: 'manual' },
    );
    expect(cbRes.status).toBe(302);
    const location = cbRes.headers.get('location');
    expect(location).toBeTruthy();
    const parsed = new URL(location!);
    expect(parsed.protocol).toBe('wotold:');
    expect(parsed.host).toBe('auth');
    expect(parsed.pathname).toBe('/callback');
    expect(parsed.searchParams.get('session')).toMatch(/^[0-9a-f-]{36}$/);
    expect(parsed.searchParams.get('provider')).toBe('google');
    expect(parsed.searchParams.get('email')).toBe('dl@example.com');
  });

  test('callback still returns JSON when redirectMode=json (default)', async () => {
    const startRes = await SELF.fetch('http://proxy/v1/auth/google/start', {
      method: 'POST',
      body: '{}',
      headers: { 'content-type': 'application/json' },
    });
    const { state } = (await startRes.json()) as { state: string };

    const idToken = buildIdToken({ sub: 'g-json' });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ id_token: idToken }), { status: 200 }),
      ),
    );

    const cbRes = await SELF.fetch(
      `http://proxy/v1/auth/google/callback?code=x&state=${state}`,
    );
    expect(cbRes.status).toBe(200);
    expect(cbRes.headers.get('content-type')).toMatch(/json/);
  });
});
