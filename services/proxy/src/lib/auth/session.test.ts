import { beforeEach, describe, expect, test } from 'vitest';
import { Hono } from 'hono';
import type { Env } from '../env.js';
import { consumeState, getSession } from './storage.js';
import {
  createSession,
  readSessionId,
  startStateFlow,
  verifyState,
} from './session.js';

function mockKv() {
  const store = new Map<string, string>();
  return {
    async get(key: string) {
      return store.get(key) ?? null;
    },
    async put(key: string, value: string, _opts?: { expirationTtl?: number }) {
      store.set(key, value);
    },
    async delete(key: string) {
      store.delete(key);
    },
  };
}

function mockEnv(): Env {
  return {
    QUOTA: mockKv() as unknown as Env['QUOTA'],
    AUTH: mockKv() as unknown as Env['AUTH'],
    AUTH_STATE_TTL_SECONDS: '60',
    AUTH_SESSION_TTL_SECONDS: '3600',
    GOOGLE_OAUTH_CLIENT_ID: 't',
    APPLE_OAUTH_CLIENT_ID: '',
    MICROSOFT_OAUTH_CLIENT_ID: '',
    PUBLIC_BASE_URL: 'http://test.proxy.local',
  } as Env;
}

describe('startStateFlow + verifyState', () => {
  let env: Env;
  beforeEach(() => {
    env = mockEnv();
  });

  test('roundtrip: start → verify returns payload', async () => {
    const stateId = await startStateFlow(env, 'google', 'http://x/callback', 'dev-1');
    const rec = await verifyState(env, stateId, 'google');
    expect(rec?.provider).toBe('google');
    expect(rec?.redirectUri).toBe('http://x/callback');
    expect(rec?.deviceId).toBe('dev-1');
  });

  test('verifyState is single-use', async () => {
    const stateId = await startStateFlow(env, 'google', 'http://x', null);
    expect(await verifyState(env, stateId, 'google')).not.toBeNull();
    expect(await verifyState(env, stateId, 'google')).toBeNull();
  });

  test('verifyState rejects provider mismatch and burns state', async () => {
    const stateId = await startStateFlow(env, 'google', 'http://x', null);
    expect(await verifyState(env, stateId, 'apple')).toBeNull();
    // State уже потреблён — повтор не работает.
    expect(await consumeState(env, stateId)).toBeNull();
  });

  test('verifyState returns null for unknown state', async () => {
    expect(await verifyState(env, 'never-stored', 'google')).toBeNull();
  });

  test('startStateFlow returns unique ids', async () => {
    const ids = new Set<string>();
    for (let i = 0; i < 5; i++) {
      ids.add(await startStateFlow(env, 'google', 'http://x', null));
    }
    expect(ids.size).toBe(5);
  });

  test('startStateFlow defaults redirectMode to json', async () => {
    const stateId = await startStateFlow(env, 'google', 'http://x', null);
    const rec = await verifyState(env, stateId, 'google');
    expect(rec?.redirectMode).toBe('json');
  });

  test('startStateFlow propagates deeplink mode', async () => {
    const stateId = await startStateFlow(env, 'google', 'http://x', null, 'deeplink');
    const rec = await verifyState(env, stateId, 'google');
    expect(rec?.redirectMode).toBe('deeplink');
  });
});

describe('createSession', () => {
  test('persists session and sets expiresAt = createdAt + TTL', async () => {
    const env = mockEnv();
    const s = await createSession(env, 'acc-42');
    expect(s.accountId).toBe('acc-42');
    expect(s.id).toMatch(/^[0-9a-f-]{36}$/);
    expect(new Date(s.expiresAt).getTime()).toBeGreaterThan(new Date(s.createdAt).getTime());
    const back = await getSession(env, s.id);
    expect(back?.accountId).toBe('acc-42');
  });
});

describe('readSessionId', () => {
  function buildAppHandler(captured: { sid: string | null }) {
    const app = new Hono();
    app.get('/', (c) => {
      captured.sid = readSessionId(c);
      return c.text('ok');
    });
    return app;
  }

  test('extracts Bearer token', async () => {
    const captured = { sid: null as string | null };
    const res = await buildAppHandler(captured).request('/', {
      headers: { authorization: 'Bearer sess-abc-123' },
    });
    expect(res.status).toBe(200);
    expect(captured.sid).toBe('sess-abc-123');
  });

  test('extracts wotold_session cookie', async () => {
    const captured = { sid: null as string | null };
    await buildAppHandler(captured).request('/', {
      headers: { cookie: 'other=x; wotold_session=cookie-sid; another=y' },
    });
    expect(captured.sid).toBe('cookie-sid');
  });

  test('returns null when no auth header / cookie', async () => {
    const captured = { sid: null as string | null };
    await buildAppHandler(captured).request('/');
    expect(captured.sid).toBeNull();
  });

  test('Bearer beats cookie when both present', async () => {
    const captured = { sid: null as string | null };
    await buildAppHandler(captured).request('/', {
      headers: {
        authorization: 'Bearer bearer-wins',
        cookie: 'wotold_session=cookie-loses',
      },
    });
    expect(captured.sid).toBe('bearer-wins');
  });
});
