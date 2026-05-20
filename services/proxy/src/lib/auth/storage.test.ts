import { beforeEach, describe, expect, test } from 'vitest';
import type { Env } from '../env.js';
import {
  consumeState,
  deleteSession,
  findAccountByProvider,
  getAccount,
  getSession,
  putAccount,
  putSession,
  putState,
  type Account,
  type Session,
  type StateRecord,
} from './storage.js';

function mockKv() {
  const store = new Map<string, string>();
  return {
    store,
    async get(key: string) {
      return store.get(key) ?? null;
    },
    async put(key: string, value: string, _opts?: { expirationTtl?: number }) {
      // expirationTtl игнорируем для тестов (хранение и так in-memory).
      store.set(key, value);
    },
    async delete(key: string) {
      store.delete(key);
    },
  };
}

function mockEnv(overrides: Partial<Env> = {}): Env {
  return {
    QUOTA: mockKv() as unknown as Env['QUOTA'],
    AUTH: mockKv() as unknown as Env['AUTH'],
    AUTH_STATE_TTL_SECONDS: '60',
    AUTH_SESSION_TTL_SECONDS: '3600',
    GOOGLE_OAUTH_CLIENT_ID: 'test',
    APPLE_OAUTH_CLIENT_ID: '',
    MICROSOFT_OAUTH_CLIENT_ID: '',
    PUBLIC_BASE_URL: 'http://test.proxy.local',
    ...overrides,
  } as Env;
}

function sampleAccount(overrides: Partial<Account> = {}): Account {
  return {
    id: '11111111-1111-1111-1111-111111111111',
    provider: 'google',
    providerUserId: 'google-sub-9000',
    email: 'damir@example.com',
    displayName: 'Damir',
    createdAt: '2026-05-20T08:00:00Z',
    linkedDeviceId: '00000000-0000-0000-0000-000000000001',
    ...overrides,
  };
}

describe('auth storage — accounts', () => {
  let env: Env;
  beforeEach(() => {
    env = mockEnv();
  });

  test('putAccount + getAccount roundtrip', async () => {
    const a = sampleAccount();
    await putAccount(env, a);
    const read = await getAccount(env, a.id);
    expect(read).toEqual(a);
  });

  test('findAccountByProvider returns null when not stored', async () => {
    const found = await findAccountByProvider(env, 'google', 'no-such');
    expect(found).toBeNull();
  });

  test('findAccountByProvider returns linked account', async () => {
    const a = sampleAccount();
    await putAccount(env, a);
    const found = await findAccountByProvider(env, 'google', 'google-sub-9000');
    expect(found?.id).toBe(a.id);
  });

  test('different providers isolated', async () => {
    const google = sampleAccount({
      id: 'g-1',
      provider: 'google',
      providerUserId: 'sub-shared',
    });
    const apple = sampleAccount({
      id: 'a-1',
      provider: 'apple',
      providerUserId: 'sub-shared',
    });
    await putAccount(env, google);
    await putAccount(env, apple);
    expect((await findAccountByProvider(env, 'google', 'sub-shared'))?.id).toBe('g-1');
    expect((await findAccountByProvider(env, 'apple', 'sub-shared'))?.id).toBe('a-1');
  });
});

describe('auth storage — sessions', () => {
  let env: Env;
  beforeEach(() => {
    env = mockEnv();
  });

  test('putSession + getSession roundtrip', async () => {
    const s: Session = {
      id: 'sess-abc',
      accountId: 'acc-1',
      createdAt: '2026-05-20T08:00:00Z',
      expiresAt: '2026-05-21T08:00:00Z',
    };
    await putSession(env, s);
    expect(await getSession(env, 'sess-abc')).toEqual(s);
  });

  test('deleteSession removes record', async () => {
    const s: Session = {
      id: 'sess-x',
      accountId: 'acc-1',
      createdAt: '2026-05-20T08:00:00Z',
      expiresAt: '2026-05-21T08:00:00Z',
    };
    await putSession(env, s);
    await deleteSession(env, 'sess-x');
    expect(await getSession(env, 'sess-x')).toBeNull();
  });

  test('getSession returns null for unknown id', async () => {
    expect(await getSession(env, 'nope')).toBeNull();
  });
});

describe('auth storage — state', () => {
  let env: Env;
  beforeEach(() => {
    env = mockEnv();
  });

  test('consumeState returns and tombstones record', async () => {
    const rec: StateRecord = {
      provider: 'google',
      redirectUri: 'http://test.proxy.local/v1/auth/google/callback',
      deviceId: null,
      createdAt: '2026-05-20T08:00:00Z',
    };
    await putState(env, 'state-1', rec);
    const consumed = await consumeState(env, 'state-1');
    // Возвращается record с consumedAt маркером (best-effort CAS tombstone).
    expect(consumed).toMatchObject(rec);
    expect(consumed?.consumedAt).toBeTypeOf('number');
    // single-use: second consume returns null (consumedAt blocks re-use).
    expect(await consumeState(env, 'state-1')).toBeNull();
  });

  test('consumeState returns null when state never put', async () => {
    expect(await consumeState(env, 'never-stored')).toBeNull();
  });
});
