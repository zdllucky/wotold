// OIDC provider adapters (#37, M10.1).
//
// Каждый адаптер возвращает authorize URL для start-flow и парсит
// callback (code → token → identity) для callback-flow.
//
// W5 security:
// - state-токен проверяется НЕ здесь — это снаружи в routes/auth.ts
// - PKCE не используем — публичный flow на CF Worker, secret хранится на стороне прокси (S1)
// - ID-token из Google: парсим payload без проверки подписи. JWT signature verification
//   для MVP scaffold отсутствует — провайдер уже доверенный endpoint, IdP HTTPS. Полная
//   JWKS-проверка вынесена в follow-up #38 (M10.2).
// - Apple/Microsoft — stub реализации (X4 manual setup, не в MVP).

import type { Env } from '../env.js';
import type { OidcProvider } from './storage.js';

export interface ProviderIdentity {
  providerUserId: string;
  email: string | null;
  displayName: string | null;
}

export interface AuthorizeUrlInput {
  state: string;
  redirectUri: string;
}

export interface CodeExchangeInput {
  code: string;
  redirectUri: string;
}

export interface ProviderAdapter {
  /** Построить authorize URL для редиректа клиента к IdP. */
  buildAuthorizeUrl(env: Env, input: AuthorizeUrlInput): string;
  /** Обменять authorization code на identity (sub + email). */
  exchangeCode(env: Env, input: CodeExchangeInput): Promise<ProviderIdentity>;
}

class GoogleAdapter implements ProviderAdapter {
  buildAuthorizeUrl(env: Env, input: AuthorizeUrlInput): string {
    if (!env.GOOGLE_OAUTH_CLIENT_ID) {
      throw new Error('GOOGLE_OAUTH_CLIENT_ID not configured');
    }
    const params = new URLSearchParams({
      client_id: env.GOOGLE_OAUTH_CLIENT_ID,
      redirect_uri: input.redirectUri,
      response_type: 'code',
      scope: 'openid email profile',
      state: input.state,
      access_type: 'online',
      prompt: 'select_account',
    });
    return `https://accounts.google.com/o/oauth2/v2/auth?${params.toString()}`;
  }

  async exchangeCode(env: Env, input: CodeExchangeInput): Promise<ProviderIdentity> {
    if (!env.GOOGLE_OAUTH_CLIENT_ID || !env.GOOGLE_OAUTH_CLIENT_SECRET) {
      throw new Error('Google OAuth credentials not configured');
    }
    const body = new URLSearchParams({
      code: input.code,
      client_id: env.GOOGLE_OAUTH_CLIENT_ID,
      client_secret: env.GOOGLE_OAUTH_CLIENT_SECRET,
      redirect_uri: input.redirectUri,
      grant_type: 'authorization_code',
    });
    const resp = await fetch('https://oauth2.googleapis.com/token', {
      method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' },
      body: body.toString(),
    });
    if (!resp.ok) {
      throw new Error(`google token ${resp.status}: ${await safeText(resp)}`);
    }
    const tokens = (await resp.json()) as { id_token?: string; access_token?: string };
    if (!tokens.id_token) {
      throw new Error('google: id_token missing in response');
    }
    return decodeIdTokenPayload(tokens.id_token);
  }
}

class AppleAdapter implements ProviderAdapter {
  buildAuthorizeUrl(_env: Env, _input: AuthorizeUrlInput): string {
    throw new Error('apple: provider not yet implemented (X4 manual setup pending)');
  }
  async exchangeCode(_env: Env, _input: CodeExchangeInput): Promise<ProviderIdentity> {
    throw new Error('apple: provider not yet implemented (X4 manual setup pending)');
  }
}

class MicrosoftAdapter implements ProviderAdapter {
  buildAuthorizeUrl(_env: Env, _input: AuthorizeUrlInput): string {
    throw new Error('microsoft: provider not yet implemented (X4 manual setup pending)');
  }
  async exchangeCode(_env: Env, _input: CodeExchangeInput): Promise<ProviderIdentity> {
    throw new Error('microsoft: provider not yet implemented (X4 manual setup pending)');
  }
}

const ADAPTERS: Record<OidcProvider, ProviderAdapter> = {
  google: new GoogleAdapter(),
  apple: new AppleAdapter(),
  microsoft: new MicrosoftAdapter(),
};

export function getAdapter(provider: OidcProvider): ProviderAdapter {
  return ADAPTERS[provider];
}

// ----- helpers -----

/**
 * Декодирует payload Google ID-token (JWT base64url) БЕЗ проверки подписи.
 * Для production HTTP-API безопасно: запрос идёт прокси → Google over HTTPS,
 * ответ TLS-protected. JWKS-валидация подписи — follow-up #38.
 */
export function decodeIdTokenPayload(idToken: string): ProviderIdentity {
  const parts = idToken.split('.');
  if (parts.length !== 3 || !parts[1]) {
    throw new Error('invalid id_token format');
  }
  const payload = parts[1];
  const pad = '='.repeat((4 - (payload.length % 4)) % 4);
  const b64 = payload.replace(/-/g, '+').replace(/_/g, '/') + pad;
  // atob возвращает byte string; нормализуем через UTF-8 для not-ASCII (cyrillic names).
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const json = JSON.parse(new TextDecoder('utf-8').decode(bytes)) as {
    sub?: string;
    email?: string;
    name?: string;
    given_name?: string;
    family_name?: string;
  };
  if (!json.sub) {
    throw new Error('id_token payload missing sub');
  }
  return {
    providerUserId: json.sub,
    email: json.email ?? null,
    displayName:
      json.name ??
      [json.given_name, json.family_name].filter(Boolean).join(' ').trim() ??
      null,
  };
}

async function safeText(resp: Response): Promise<string> {
  try {
    return (await resp.text()).slice(0, 500);
  } catch {
    return '<unreadable>';
  }
}
