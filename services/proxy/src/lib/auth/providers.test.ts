import { afterEach, describe, expect, test, vi } from 'vitest';
import type { Env } from '../env.js';
import { decodeIdTokenPayload, getAdapter } from './providers.js';

afterEach(() => {
  vi.unstubAllGlobals();
});

function mockEnv(overrides: Partial<Env> = {}): Env {
  return {
    GOOGLE_OAUTH_CLIENT_ID: 'google-cid',
    GOOGLE_OAUTH_CLIENT_SECRET: 'google-secret',
    APPLE_OAUTH_CLIENT_ID: '',
    MICROSOFT_OAUTH_CLIENT_ID: '',
    PUBLIC_BASE_URL: 'http://test',
    AUTH_STATE_TTL_SECONDS: '60',
    AUTH_SESSION_TTL_SECONDS: '3600',
    ...overrides,
  } as Env;
}

function b64urlEncode(s: string): string {
  // btoa не дружит с не-ASCII — нормализуем через UTF-8 bytes.
  const bytes = new TextEncoder().encode(s);
  let bin = '';
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function buildIdToken(payload: Record<string, unknown>): string {
  const header = b64urlEncode(JSON.stringify({ alg: 'RS256', typ: 'JWT' }));
  const body = b64urlEncode(JSON.stringify(payload));
  const sig = 'fakesig';
  return `${header}.${body}.${sig}`;
}

describe('decodeIdTokenPayload', () => {
  test('parses sub/email/name', () => {
    const token = buildIdToken({
      sub: 'google-9000',
      email: 'damir@example.com',
      name: 'Damir',
    });
    const id = decodeIdTokenPayload(token);
    expect(id.providerUserId).toBe('google-9000');
    expect(id.email).toBe('damir@example.com');
    expect(id.displayName).toBe('Damir');
  });

  test('joins given_name+family_name when name missing', () => {
    const token = buildIdToken({
      sub: 's-1',
      given_name: 'Иван',
      family_name: 'Иванов',
    });
    const id = decodeIdTokenPayload(token);
    expect(id.displayName).toBe('Иван Иванов');
  });

  test('throws on malformed token', () => {
    expect(() => decodeIdTokenPayload('not-a-jwt')).toThrow(/invalid id_token format/);
    expect(() => decodeIdTokenPayload('only.two')).toThrow(/invalid id_token format/);
  });

  test('throws when sub missing', () => {
    const token = buildIdToken({ email: 'x@y.z' });
    expect(() => decodeIdTokenPayload(token)).toThrow(/sub/);
  });
});

describe('GoogleAdapter', () => {
  test('buildAuthorizeUrl includes required OAuth params', () => {
    const url = getAdapter('google').buildAuthorizeUrl(mockEnv(), {
      state: 'state-abc',
      redirectUri: 'http://x/v1/auth/google/callback',
    });
    const parsed = new URL(url);
    expect(parsed.origin + parsed.pathname).toBe(
      'https://accounts.google.com/o/oauth2/v2/auth',
    );
    expect(parsed.searchParams.get('client_id')).toBe('google-cid');
    expect(parsed.searchParams.get('state')).toBe('state-abc');
    expect(parsed.searchParams.get('redirect_uri')).toBe(
      'http://x/v1/auth/google/callback',
    );
    expect(parsed.searchParams.get('response_type')).toBe('code');
    expect(parsed.searchParams.get('scope')).toBe('openid email profile');
  });

  test('buildAuthorizeUrl throws when client_id missing', () => {
    expect(() =>
      getAdapter('google').buildAuthorizeUrl(mockEnv({ GOOGLE_OAUTH_CLIENT_ID: '' }), {
        state: 's',
        redirectUri: 'r',
      }),
    ).toThrow(/GOOGLE_OAUTH_CLIENT_ID/);
  });

  test('exchangeCode happy path returns identity', async () => {
    const idToken = buildIdToken({ sub: 'g-1', email: 'a@b', name: 'A' });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ id_token: idToken, access_token: 'at' }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );
    const id = await getAdapter('google').exchangeCode(mockEnv(), {
      code: 'authcode',
      redirectUri: 'http://x/cb',
    });
    expect(id.providerUserId).toBe('g-1');
    expect(id.email).toBe('a@b');
  });

  test('exchangeCode throws on Google token endpoint non-2xx', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response('{"error":"invalid_grant"}', { status: 400 }),
      ),
    );
    await expect(
      getAdapter('google').exchangeCode(mockEnv(), { code: 'x', redirectUri: 'r' }),
    ).rejects.toThrow(/google token 400/);
  });

  test('exchangeCode throws when id_token missing', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ access_token: 'at' }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );
    await expect(
      getAdapter('google').exchangeCode(mockEnv(), { code: 'x', redirectUri: 'r' }),
    ).rejects.toThrow(/id_token missing/);
  });

  test('exchangeCode throws when secret unset', async () => {
    await expect(
      getAdapter('google').exchangeCode(mockEnv({ GOOGLE_OAUTH_CLIENT_SECRET: '' }), {
        code: 'x',
        redirectUri: 'r',
      }),
    ).rejects.toThrow(/credentials not configured/);
  });
});

describe('Apple/Microsoft adapters (stub)', () => {
  test('apple buildAuthorizeUrl throws not_implemented', () => {
    expect(() =>
      getAdapter('apple').buildAuthorizeUrl(mockEnv(), { state: 's', redirectUri: 'r' }),
    ).toThrow(/apple.*not yet implemented/);
  });

  test('microsoft exchangeCode throws not_implemented', async () => {
    await expect(
      getAdapter('microsoft').exchangeCode(mockEnv(), { code: 'x', redirectUri: 'r' }),
    ).rejects.toThrow(/microsoft.*not yet implemented/);
  });
});
