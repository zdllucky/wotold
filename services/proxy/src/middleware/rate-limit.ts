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
  // [B16 audit P0]: добавили retry-on-conflict через дешёвый CAS-loop через KV
  // (Workers KV не имеет нативного CAS, но мы можем re-read и replay при
  // обнаружении inconsistency в течение N retries). Параллельные writes под
  // одним device-id всё ещё могут терять delta, но это soft-fail для R1.
  const k = key(deviceId, kind);
  for (let attempt = 0; attempt < 3; attempt++) {
    const beforeRead = await readUsage(env, deviceId, kind);
    const next = beforeRead + delta;
    await env.QUOTA.put(k, String(next), { expirationTtl: SEC_PER_DAY * 2 });
    // Verify our write landed (best-effort detection чужого concurrent write).
    const afterRead = await readUsage(env, deviceId, kind);
    if (afterRead >= next) {
      return afterRead;
    }
    // Иначе кто-то перезатёр — retry с новым current.
  }
  // Last attempt без verify.
  return readUsage(env, deviceId, kind);
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
