/// <reference types="@cloudflare/vitest-pool-workers" />
import { beforeEach, describe, expect, test } from 'vitest';
import { SELF, env } from 'cloudflare:test';
import { DEVICE_ID_HEADER, type UsageResponse } from '@wotold/contracts';
import type { Env } from '../lib/env.js';

declare module 'cloudflare:test' {
  interface ProvidedEnv extends Env {}
}

const DEVICE = '550e8400-e29b-41d4-a716-446655440000';

function today(): string {
  return new Date().toISOString().slice(0, 10);
}

beforeEach(async () => {
  const keys = await env.QUOTA.list();
  for (const k of keys.keys) await env.QUOTA.delete(k.name);
});

describe('GET /v1/usage', () => {
  test('returns zero usage + limits from env vars when KV empty', async () => {
    const res = await SELF.fetch('http://proxy/v1/usage', {
      headers: { [DEVICE_ID_HEADER]: DEVICE },
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as UsageResponse;
    expect(body.tier).toBe('free');
    expect(body.sttSecondsUsed).toBe(0);
    expect(body.llmTokensUsed).toBe(0);
    expect(body.sttSecondsLimit).toBeGreaterThan(0);
    expect(body.llmTokensLimit).toBeGreaterThan(0);
    // periodResetAt = ISO UTC midnight tomorrow.
    expect(body.periodResetAt).toMatch(/^\d{4}-\d{2}-\d{2}T00:00:00\.000Z$/);
  });

  test('reflects KV counters в usage fields', async () => {
    const day = today();
    await env.QUOTA.put(`quota:${DEVICE}:${day}:stt_sec`, '120');
    await env.QUOTA.put(`quota:${DEVICE}:${day}:llm_tok`, '5000');

    const res = await SELF.fetch('http://proxy/v1/usage', {
      headers: { [DEVICE_ID_HEADER]: DEVICE },
    });
    const body = (await res.json()) as UsageResponse;
    expect(body.sttSecondsUsed).toBe(120);
    expect(body.llmTokensUsed).toBe(5000);
  });

  test('rejects request without device-id header', async () => {
    const res = await SELF.fetch('http://proxy/v1/usage');
    expect(res.status).toBe(400);
  });
});
