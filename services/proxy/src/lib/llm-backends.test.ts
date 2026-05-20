import { describe, expect, test, beforeEach, vi, afterEach } from 'vitest';
import type { Env } from './env.js';
import { callLlm, resolveBackend } from './llm-backends.js';

function baseEnv(overrides: Partial<Env> = {}): Env {
  return {
    QUOTA: {} as unknown as Env['QUOTA'],
    AUTH: {} as unknown as Env['AUTH'],
    STT_STAGING: {} as unknown as Env['STT_STAGING'],
    TIER: 'free',
    QUOTA_STT_SECONDS_PER_DAY: '300',
    QUOTA_LLM_TOKENS_PER_DAY: '5000',
    STAGING_PRESIGN_TTL_SECONDS: '900',
    STT_STAGING_BUCKET: 'test',
    ANTHROPIC_DEFAULT_MODEL: 'claude-sonnet-4-6',
    LLM_BACKEND: '',
    GROQ_DEFAULT_MODEL: 'llama-3.3-70b-versatile',
    AUTH_STATE_TTL_SECONDS: '60',
    AUTH_SESSION_TTL_SECONDS: '3600',
    GOOGLE_OAUTH_CLIENT_ID: '',
    APPLE_OAUTH_CLIENT_ID: '',
    MICROSOFT_OAUTH_CLIENT_ID: '',
    PUBLIC_BASE_URL: '',
    ...overrides,
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('resolveBackend', () => {
  test('explicit groq', () => {
    expect(resolveBackend(baseEnv({ LLM_BACKEND: 'groq' }))).toBe('groq');
  });
  test('explicit anthropic wins over auto', () => {
    expect(
      resolveBackend(baseEnv({ LLM_BACKEND: 'anthropic', GROQ_API_KEY: 'g' })),
    ).toBe('anthropic');
  });
  test('auto: groq при наличии GROQ_API_KEY', () => {
    expect(resolveBackend(baseEnv({ GROQ_API_KEY: 'g' }))).toBe('groq');
  });
  test('auto: anthropic при наличии ANTHROPIC_API_KEY', () => {
    expect(resolveBackend(baseEnv({ ANTHROPIC_API_KEY: 'a' }))).toBe('anthropic');
  });
  test('auto: groq предпочтительнее anthropic при наличии обоих', () => {
    expect(resolveBackend(baseEnv({ GROQ_API_KEY: 'g', ANTHROPIC_API_KEY: 'a' }))).toBe('groq');
  });
  test('null если ни одного ключа', () => {
    expect(resolveBackend(baseEnv())).toBeNull();
  });
});

describe('callLlm — Groq backend', () => {
  beforeEach(() => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            choices: [{ message: { content: '{"recap":"ok","items":[]}' } }],
            usage: { total_tokens: 42 },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
      ),
    );
  });

  test('возвращает parsed JSON + tokens', async () => {
    const res = await callLlm(baseEnv({ GROQ_API_KEY: 'gk' }), {
      system: 'Return JSON',
      input: 'transcript',
    });
    expect(res.ok).toBe(true);
    if (!res.ok) return;
    expect(res.parsed).toEqual({ recap: 'ok', items: [] });
    expect(res.tokensUsed).toBe(42);
  });

  test('respects body.model + uses GROQ_DEFAULT_MODEL fallback', async () => {
    const fetchSpy = vi.fn<(url: string, init?: RequestInit) => Promise<Response>>(
      async () => new Response('{"choices":[{"message":{"content":"{}"}}]}', { status: 200 }),
    );
    vi.stubGlobal('fetch', fetchSpy);
    await callLlm(baseEnv({ GROQ_API_KEY: 'gk' }), {
      system: 'JSON',
      input: 'x',
      model: 'mixtral-8x7b-32768',
    });
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const callArgs = fetchSpy.mock.calls[0]!;
    const url = callArgs[0];
    const init = callArgs[1];
    expect(url).toContain('api.groq.com');
    const sent = JSON.parse((init?.body as string) ?? '{}');
    expect(sent.model).toBe('mixtral-8x7b-32768');
    expect(sent.response_format).toEqual({ type: 'json_object' });
  });

  test('upstream 5xx → provider_error 502', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('boom', { status: 503 })));
    const res = await callLlm(baseEnv({ GROQ_API_KEY: 'gk' }), {
      system: 'JSON',
      input: 'x',
    });
    expect(res.ok).toBe(false);
    if (res.ok) return;
    expect(res.status).toBe(502);
    expect(res.code).toBe('provider_error');
  });

  test('не-JSON content → provider_error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(
          JSON.stringify({ choices: [{ message: { content: 'not json' } }] }),
          { status: 200 },
        ),
      ),
    );
    const res = await callLlm(baseEnv({ GROQ_API_KEY: 'gk' }), {
      system: 'JSON',
      input: 'x',
    });
    expect(res.ok).toBe(false);
    if (res.ok) return;
    expect(res.message).toMatch(/JSON-parseable/);
  });

  test('пустой контент → provider_error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ choices: [{ message: { content: '' } }] }), {
          status: 200,
        }),
      ),
    );
    const res = await callLlm(baseEnv({ GROQ_API_KEY: 'gk' }), {
      system: 'JSON',
      input: 'x',
    });
    expect(res.ok).toBe(false);
    if (res.ok) return;
    expect(res.message).toMatch(/empty/);
  });
});

describe('callLlm — Anthropic backend', () => {
  test('parsed JSON + tokens (input+output)', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            content: [{ type: 'text', text: '{"a":1}' }],
            usage: { input_tokens: 10, output_tokens: 5 },
          }),
          { status: 200 },
        ),
      ),
    );
    const res = await callLlm(
      baseEnv({ LLM_BACKEND: 'anthropic', ANTHROPIC_API_KEY: 'ak' }),
      { system: 'JSON', input: 'x' },
    );
    expect(res.ok).toBe(true);
    if (!res.ok) return;
    expect(res.parsed).toEqual({ a: 1 });
    expect(res.tokensUsed).toBe(15);
  });
});

describe('callLlm — no key configured', () => {
  test('503 provider_error', async () => {
    const res = await callLlm(baseEnv(), { system: 'JSON', input: 'x' });
    expect(res.ok).toBe(false);
    if (res.ok) return;
    expect(res.status).toBe(503);
  });
});
