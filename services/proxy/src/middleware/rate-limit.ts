import type { Context } from 'hono';
import type { Env } from '../lib/env.js';

const SEC_PER_DAY = 86_400;

function todayUtc(): string {
  return new Date().toISOString().slice(0, 10);
}

export type QuotaKind = 'stt_sec' | 'llm_tok';

function key(deviceId: string, kind: QuotaKind): string {
  return `quota:${deviceId}:${todayUtc()}:${kind}`;
}

export async function readUsage(env: Env, deviceId: string, kind: QuotaKind): Promise<number> {
  const v = await env.QUOTA.get(key(deviceId, kind));
  return v ? Number(v) || 0 : 0;
}

export async function incUsage(
  env: Env,
  deviceId: string,
  kind: QuotaKind,
  delta: number,
): Promise<number> {
  // R1 — гонка приемлема: Free-тир абьюзится переустановкой в любом случае.
  const k = key(deviceId, kind);
  const current = await readUsage(env, deviceId, kind);
  const next = current + delta;
  await env.QUOTA.put(k, String(next), { expirationTtl: SEC_PER_DAY * 2 });
  return next;
}

export function quotaCap(env: Env, kind: QuotaKind): number {
  return kind === 'stt_sec'
    ? Number(env.QUOTA_STT_SECONDS_PER_DAY) || 0
    : Number(env.QUOTA_LLM_TOKENS_PER_DAY) || 0;
}

export function periodResetAt(): string {
  const now = new Date();
  const tomorrow = new Date(
    Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() + 1),
  );
  return tomorrow.toISOString();
}

export async function enforceQuota(
  c: Context<{ Bindings: Env; Variables: { deviceId: string } }>,
  kind: QuotaKind,
): Promise<Response | null> {
  const used = await readUsage(c.env, c.get('deviceId'), kind);
  const cap = quotaCap(c.env, kind);
  if (used >= cap) {
    return c.json(
      { ok: false, code: 'quota_exceeded', message: `${kind} daily quota exceeded` },
      429,
    );
  }
  return null;
}
