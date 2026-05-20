import { describe, expect, test, beforeEach } from 'vitest';
import type { Env } from '../lib/env.js';
import {
  incUsage,
  periodResetAt,
  quotaCap,
  readUsage,
} from './rate-limit.js';

function mockKv() {
  const store = new Map<string, string>();
  return {
    get store() {
      return store;
    },
    async get(key: string) {
      return store.get(key) ?? null;
    },
    async put(key: string, value: string) {
      store.set(key, value);
    },
  };
}

function mockEnv(overrides: Partial<Env> = {}): Env {
  return {
    QUOTA: mockKv() as unknown as Env['QUOTA'],
    QUOTA_STT_SECONDS_PER_DAY: '300',
    QUOTA_LLM_TOKENS_PER_DAY: '50000',
    ...overrides,
  } as Env;
}

const DEVICE = '550e8400-e29b-41d4-a716-446655440000';

describe('rate-limit', () => {
  test('readUsage returns 0 when no key', async () => {
    const env = mockEnv();
    expect(await readUsage(env, DEVICE, 'stt_sec')).toBe(0);
    expect(await readUsage(env, DEVICE, 'llm_tok')).toBe(0);
  });

  test('incUsage adds and persists per device+kind', async () => {
    const env = mockEnv();
    const after = await incUsage(env, DEVICE, 'stt_sec', 30);
    expect(after).toBe(30);
    expect(await readUsage(env, DEVICE, 'stt_sec')).toBe(30);
    // other kind isolated
    expect(await readUsage(env, DEVICE, 'llm_tok')).toBe(0);
  });

  test('incUsage accumulates', async () => {
    const env = mockEnv();
    await incUsage(env, DEVICE, 'llm_tok', 100);
    await incUsage(env, DEVICE, 'llm_tok', 250);
    expect(await readUsage(env, DEVICE, 'llm_tok')).toBe(350);
  });

  test('incUsage isolated per device', async () => {
    const env = mockEnv();
    const other = '11111111-2222-3333-4444-555555555555';
    await incUsage(env, DEVICE, 'stt_sec', 60);
    expect(await readUsage(env, other, 'stt_sec')).toBe(0);
  });

  test('quotaCap reads env caps', () => {
    const env = mockEnv();
    expect(quotaCap(env, 'stt_sec')).toBe(300);
    expect(quotaCap(env, 'llm_tok')).toBe(50000);
  });

  test('quotaCap defaults to 0 for missing/non-numeric', () => {
    const env = mockEnv({
      QUOTA_STT_SECONDS_PER_DAY: '',
      QUOTA_LLM_TOKENS_PER_DAY: 'nope',
    });
    expect(quotaCap(env, 'stt_sec')).toBe(0);
    expect(quotaCap(env, 'llm_tok')).toBe(0);
  });

  test('periodResetAt returns next UTC midnight ISO', () => {
    const iso = periodResetAt();
    expect(iso).toMatch(/^\d{4}-\d{2}-\d{2}T00:00:00\.000Z$/);
    expect(new Date(iso).getTime()).toBeGreaterThan(Date.now());
  });
});
