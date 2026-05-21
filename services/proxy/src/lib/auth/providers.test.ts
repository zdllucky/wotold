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

  // [B16 audit P1] claim validation tests
  test('throws when exp in past', () => {
    const token = buildIdToken({
      sub: 's',
      exp: Math.floor(Date.now() / 1000) - 60,
    });
    expect(() => decodeIdTokenPayload(token)).toThrow(/expired/);
  });

  test('accepts when exp in future', () => {
    const token = buildIdToken({
      sub: 's',
      exp: Math.floor(Date.now() / 1000) + 60,
    });
    expect(() => decodeIdTokenPayload(token)).not.toThrow();
  });

  test('throws when iss mismatch (string)', () => {
    const token = buildIdToken({ sub: 's', iss: 'https://evil.com' });
    expect(() =>
      decodeIdTokenPayload(token, { expectedIssuer: 'https://accounts.google.com' }),
    ).toThrow(/bad iss/);
  });

  test('throws when iss mismatch (array)', () => {
    const token = buildIdToken({ sub: 's', iss: 'https://evil.com' });
    expect(() =>
      decodeIdTokenPayload(token, {
        expectedIssuer: ['https://accounts.google.com', 'accounts.google.com'],
      }),
    ).toThrow(/bad iss/);
  });

  test('accepts iss inside allowed array', () => {
    const token = buildIdToken({ sub: 's', iss: 'accounts.google.com' });
    expect(() =>
      decodeIdTokenPayload(token, {
        expectedIssuer: ['https://accounts.google.com', 'accounts.google.com'],
      }),
    ).not.toThrow();
  });

  test('throws when iss missing but expected', () => {
    const token = buildIdToken({ sub: 's' });
    expect(() =>
      decodeIdTokenPayload(token, { expectedIssuer: 'https://accounts.google.com' }),
    ).toThrow(/bad iss/);
  });

  test('throws when aud mismatch (string)', () => {
    const token = buildIdToken({
      sub: 's',
      iss: 'https://accounts.google.com',
      aud: 'wrong-cid',
    });
    expect(() =>
      decodeIdTokenPayload(token, {
        expectedIssuer: 'https://accounts.google.com',
        expectedAudience: 'right-cid',
      }),
    ).toThrow(/bad aud/);
  });

  test('accepts aud inside array', () => {
    const token = buildIdToken({
      sub: 's',
      iss: 'https://accounts.google.com',
      aud: ['other-cid', 'right-cid'],
    });
    expect(() =>
      decodeIdTokenPayload(token, {
        expectedIssuer: 'https://accounts.google.com',
        expectedAudience: 'right-cid',
      }),
    ).not.toThrow();
  });

  // ─── Negative / edge cases (audit P0 follow-up) ─────────────────

  test('throws on payload with invalid JSON (valid base64, garbage inside)', () => {
    // [Sec] Корректная base64 структура но не-JSON содержимое — атакующий
    // может попробовать подсунуть raw текст. JSON.parse должен бросить.
    const header = b64urlEncode(JSON.stringify({ alg: 'RS256', typ: 'JWT' }));
    const body = b64urlEncode('not json at all {{{');
    expect(() => decodeIdTokenPayload(`${header}.${body}.sig`)).toThrow();
  });

  test('throws on payload with non-base64 chars (corrupted token)', () => {
    // Tampered token с invalid base64 в payload части.
    expect(() => decodeIdTokenPayload('aaa.@@@##.sig')).toThrow();
  });

  test('throws when iss is empty string but expected', () => {
    // [Sec] Provider could омит iss или вернуть пустую строку — оба unsafe.
    const token = buildIdToken({ sub: 's', iss: '' });
    expect(() =>
      decodeIdTokenPayload(token, {
        expectedIssuer: 'https://accounts.google.com',
      }),
    ).toThrow(/bad iss/);
  });

  test('iss comparison is case-sensitive', () => {
    // [Sec] 'Https://accounts.google.com' != 'https://...' — JWT spec
    // требует octet-exact match для StringOrURI claims.
    const token = buildIdToken({
      sub: 's',
      iss: 'HTTPS://accounts.google.com',
    });
    expect(() =>
      decodeIdTokenPayload(token, {
        expectedIssuer: 'https://accounts.google.com',
      }),
    ).toThrow(/bad iss/);
  });

  test('throws when aud array empty', () => {
    // [Sec] aud=[] не должен match'ить ничего.
    const token = buildIdToken({ sub: 's', iss: 'https://accounts.google.com', aud: [] });
    expect(() =>
      decodeIdTokenPayload(token, {
        expectedIssuer: 'https://accounts.google.com',
        expectedAudience: 'cid',
      }),
    ).toThrow(/bad aud/);
  });

  test('accepts when exp is exactly now (boundary)', () => {
    // exp == now: per JWT RFC the token is valid "until" exp; check is `exp < now`
    // (strict less-than), так что exp равно now → still valid. Документируем
    // поведение тестом чтобы случайно не сменить инвариант.
    const token = buildIdToken({ sub: 's', exp: Math.floor(Date.now() / 1000) });
    expect(() => decodeIdTokenPayload(token)).not.toThrow();
  });

  test('throws on exp = 0 (epoch — clearly expired)', () => {
    const token = buildIdToken({ sub: 's', exp: 0 });
    expect(() => decodeIdTokenPayload(token)).toThrow(/expired/);
  });

  test('ignores non-numeric exp (typeof check skips invalid types)', () => {
    // [Note] Текущая семантика — если exp не number, проверки нет
    // (typeof === 'number' filter). Это намеренно для Apple-like provider'ов
    // которые не всегда шлют exp. Документируем тестом — изменение этого
    // поведения нужно делать осознанно.
    const token = buildIdToken({
      sub: 's',
      exp: 'tomorrow' as unknown as number,
    });
    expect(() => decodeIdTokenPayload(token)).not.toThrow();
  });

  test('skips exp check entirely when omitted (Apple)', () => {
    // Apple OIDC может не возвращать exp в id_token — должны принимать.
    const token = buildIdToken({ sub: 'apple-sub' });
    expect(() => decodeIdTokenPayload(token)).not.toThrow();
  });

  test('accepts aud as string (not array)', () => {
    const token = buildIdToken({
      sub: 's',
      iss: 'https://accounts.google.com',
      aud: 'cid-target',
    });
    expect(() =>
      decodeIdTokenPayload(token, {
        expectedIssuer: 'https://accounts.google.com',
        expectedAudience: 'cid-target',
      }),
    ).not.toThrow();
  });

  test('skips aud check when aud absent (some providers)', () => {
    // [Note] options.expectedAudience требует aud claim; если он отсутствует
    // — пропускаем check (текущее поведение). Меняется когда добавим JWKS
    // verification + strict require_aud flag.
    const token = buildIdToken({ sub: 's', iss: 'https://accounts.google.com' });
    expect(() =>
      decodeIdTokenPayload(token, {
        expectedIssuer: 'https://accounts.google.com',
        expectedAudience: 'cid-target',
      }),
    ).not.toThrow();
  });

  test('KNOWN GAP: tampered payload still accepted (JWKS verification not yet implemented)', () => {
    // [Sec audit P1, deferred] Подделанный payload (изменили sub/aud после
    // получения) сейчас проходит — мы не верифицируем подпись против JWKS.
    // Снижение риска: HTTPS к token endpoint защищает от MITM в transit,
    // attacker нужен access к token-endpoint TLS чтобы подсунуть свой id_token.
    // Снимать этот тест когда JWKS verification добавится в next iteration.
    const tampered = buildIdToken({
      sub: 'attacker',
      iss: 'https://accounts.google.com',
      aud: 'google-cid',
      exp: Math.floor(Date.now() / 1000) + 3600,
    });
    expect(() =>
      decodeIdTokenPayload(tampered, {
        expectedIssuer: 'https://accounts.google.com',
        expectedAudience: 'google-cid',
      }),
    ).not.toThrow();
    // ↑ когда JWKS landed — этот expect перевернётся на .toThrow(/signature/i).
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
    const idToken = buildIdToken({
      sub: 'g-1',
      email: 'a@b',
      name: 'A',
      iss: 'https://accounts.google.com',
      aud: 'google-cid',
      exp: Math.floor(Date.now() / 1000) + 3600,
    });
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
