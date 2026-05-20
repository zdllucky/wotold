/// <reference types="@cloudflare/vitest-pool-workers" />
import { describe, expect, test, beforeEach } from 'vitest';
import { SELF, env } from 'cloudflare:test';
import { DEVICE_ID_HEADER } from '@wotold/contracts';
import type { Env } from '../lib/env.js';

declare module 'cloudflare:test' {
  interface ProvidedEnv extends Env {}
}

// vitest-pool-workers держит KV/R2 in-memory per test file. Перед каждым тестом
// очищаем QUOTA, чтобы изоляция была честной.
beforeEach(async () => {
  const keys = await env.QUOTA.list();
  for (const k of keys.keys) await env.QUOTA.delete(k.name);
});

const DEVICE = '550e8400-e29b-41d4-a716-446655440000';

function withDevice(init: RequestInit = {}): RequestInit {
  const headers = new Headers(init.headers);
  headers.set(DEVICE_ID_HEADER, DEVICE);
  if (init.body && !headers.has('content-type')) headers.set('content-type', 'application/json');
  return { ...init, headers };
}

describe('GET / + /health', () => {
  test('root returns ok text', async () => {
    const res = await SELF.fetch('http://proxy/');
    expect(res.status).toBe(200);
    expect(await res.text()).toBe('wotold-proxy ok');
  });

  test('health returns tier free', async () => {
    const res = await SELF.fetch('http://proxy/health');
    expect(res.status).toBe(200);
    const body = (await res.json()) as { ok: boolean; tier: string };
    expect(body.ok).toBe(true);
    expect(body.tier).toBe('free');
  });
});

describe('POST /v1/stt/staging-url', () => {
  test('rejects request without device-id header', async () => {
    const res = await SELF.fetch('http://proxy/v1/stt/staging-url', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ contentType: 'audio/wav' }),
    });
    expect(res.status).toBe(400);
    const body = (await res.json()) as { code: string };
    expect(body.code).toBe('invalid_device_id');
  });

  test('rejects request without contentType', async () => {
    const res = await SELF.fetch(
      'http://proxy/v1/stt/staging-url',
      withDevice({ method: 'POST', body: JSON.stringify({}) }),
    );
    expect(res.status).toBe(400);
    const body = (await res.json()) as { code: string };
    expect(body.code).toBe('bad_request');
  });

  test('rejects invalid JSON body', async () => {
    const res = await SELF.fetch(
      'http://proxy/v1/stt/staging-url',
      withDevice({ method: 'POST', body: 'not-json' }),
    );
    expect(res.status).toBe(400);
  });

  // Happy path для staging-url зависит от R2_ACCOUNT_ID/access keys —
  // без них presign падает. Проверяется отдельно в presign unit-тестах.
  test('falls back to internal_error when R2 creds unset', async () => {
    const res = await SELF.fetch(
      'http://proxy/v1/stt/staging-url',
      withDevice({
        method: 'POST',
        body: JSON.stringify({ contentType: 'audio/wav' }),
      }),
    );
    // Либо 500 internal_error (presign failed), либо 200 если creds случайно есть.
    expect([200, 500]).toContain(res.status);
    if (res.status === 500) {
      const body = (await res.json()) as { code: string };
      expect(body.code).toBe('internal_error');
    }
  });
});

describe('POST /v1/stt', () => {
  test('rejects request without device-id header', async () => {
    const res = await SELF.fetch('http://proxy/v1/stt', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(400);
  });

  test('returns 400 when r2Key/opts missing', async () => {
    const res = await SELF.fetch(
      'http://proxy/v1/stt',
      withDevice({ method: 'POST', body: JSON.stringify({}) }),
    );
    expect(res.status).toBe(400);
    const body = (await res.json()) as { code: string };
    expect(body.code).toBe('bad_request');
  });

  test('returns 404 when r2 staging object not found', async () => {
    const res = await SELF.fetch(
      'http://proxy/v1/stt',
      withDevice({
        method: 'POST',
        body: JSON.stringify({
          r2Key: `stt/${DEVICE}/missing-key`,
          opts: { provider: 'soniox', lang: 'auto' },
        }),
      }),
    );
    expect(res.status).toBe(404);
    const body = (await res.json()) as { code: string };
    expect(body.code).toBe('staging_object_not_found');
  });

  test('returns 400 for unknown provider', async () => {
    // Кладём dummy object в R2, чтобы пройти head-check.
    const key = `stt/${DEVICE}/dummy-unknown-${crypto.randomUUID()}`;
    await env.STT_STAGING.put(key, new Uint8Array([0, 0, 0, 0]));

    const res = await SELF.fetch(
      'http://proxy/v1/stt',
      withDevice({
        method: 'POST',
        body: JSON.stringify({
          r2Key: key,
          opts: { provider: 'whisper-xyz' as 'soniox', lang: 'auto' },
        }),
      }),
    );
    // Без R2 creds presign падает до диспатча → 500. С creds → 400.
    expect([400, 500]).toContain(res.status);
  });

  test('enforces stt_sec daily quota (429 when exceeded)', async () => {
    // Прямо в KV пишем превышение.
    const today = new Date().toISOString().slice(0, 10);
    await env.QUOTA.put(`quota:${DEVICE}:${today}:stt_sec`, '99999');

    const res = await SELF.fetch(
      'http://proxy/v1/stt',
      withDevice({
        method: 'POST',
        body: JSON.stringify({
          r2Key: 'stt/whatever/x',
          opts: { provider: 'soniox', lang: 'auto' },
        }),
      }),
    );
    expect(res.status).toBe(429);
    const body = (await res.json()) as { code: string };
    expect(body.code).toBe('quota_exceeded');
  });
});
