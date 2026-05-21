// [Sec audit P1] IP /16 rate-limit middleware.
//
// Защита от mass-UUID abuse single attacker'а: даже если он сидит и спамит
// случайные device-id (Free-тир обходится переустановкой — R1), один /16
// network не должен пробивать общий лимит запросов в минуту. Удар по
// частной квоте per-device остаётся per-device, но /16-throttle режет
// массовый брутфорс на скорости создания.
//
// Семантика: cf-connecting-ip → /16 prefix (v4: первые 2 октета; v6:
// первые 16 бит = первый hex-блок). KV key `rl:ip16:{prefix}:{minute_bucket}`
// → counter. Превышение лимита → 429 rate_limited.
//
// Окно «минута» — clock-driven (Math.floor(now / 60000)), без атомарности
// (Workers KV нативный CAS отсутствует) — это soft cap, acceptable per
// R1/R7. Под Durable Object перепишется в будущем если станет критично.

import type { Context } from 'hono';
import type { Env } from '../lib/env.js';

/** Дефолт rate limit на /16 network. Покрывает абсолютное большинство
 *  легитимных residential CGN-сетей (~10-50 RPS из одной /16 разумно).
 *  Production-tuning ждёт реальной телеметрии. */
export const DEFAULT_IP16_LIMIT = 120;

/** TTL key'а в KV — 2 минуты, чтобы соседний bucket-rollover не дёрнул
 *  stale value перед тем как expirationTtl сам прибьёт его. */
const KEY_TTL_SECONDS = 120;

/** Извлечь /16 prefix из IP. Для IPv4 — `v4:<oct1>.<oct2>`, для IPv6 —
 *  `v6:<hex0>` (первый 16-bit блок). Невалидный/отсутствующий IP →
 *  `unknown`, чтобы не sharding'овать KV по бесконечному множеству. */
export function ip16Prefix(ip: string | undefined | null): string {
  if (!ip) return 'unknown';
  const trimmed = ip.trim();
  if (!trimmed) return 'unknown';
  if (trimmed.includes(':')) {
    // IPv6 — может быть compressed (`::1`, `2001:db8::1`). Берём первый
    // hex block после удаления пустых сегментов спереди (для `::1`).
    const first = trimmed.split(':').find((s) => s.length > 0);
    if (!first) return 'v6:::';
    // [Sec] Hex-only валидация: блокирует mixed форматы вроде '1.2:3:4'
    // которые иначе превратились бы в 'v6:1.2' (KV-injection risk).
    if (!/^[0-9a-fA-F]+$/.test(first)) return 'unknown';
    return `v6:${first.toLowerCase().slice(0, 4)}`;
  }
  const parts = trimmed.split('.');
  if (parts.length < 2 || !parts[0] || !parts[1]) return 'unknown';
  // Sanitize: только цифры до 3 chars per octet.
  if (!/^\d{1,3}$/.test(parts[0]) || !/^\d{1,3}$/.test(parts[1])) {
    return 'unknown';
  }
  return `v4:${parts[0]}.${parts[1]}`;
}

function rateLimitKey(prefix: string, minuteBucket: number): string {
  return `rl:ip16:${prefix}:${minuteBucket}`;
}

/**
 * Проверить и инкрементировать счётчик запросов на /16-сеть. Возвращает
 * Response при превышении (caller должен return напрямую), иначе null.
 *
 * Передавай `null` lim'ом чтобы пропускать (например для smoke-checks
 * /health). DEV/test без cf-connecting-ip → no-op.
 */
export async function enforceIp16RateLimit(
  c: Context<{ Bindings: Env }>,
  limit: number = DEFAULT_IP16_LIMIT,
): Promise<Response | null> {
  // Cloudflare всегда ставит cf-connecting-ip за edge'ом. Без него (test/dev)
  // — silently allow, чтобы не блокировать локальную разработку.
  const ip = c.req.header('cf-connecting-ip');
  if (!ip) return null;
  const prefix = ip16Prefix(ip);
  if (prefix === 'unknown') return null;

  const minute = Math.floor(Date.now() / 60_000);
  const key = rateLimitKey(prefix, minute);

  // Read current count. Best-effort — без atomic CAS (Workers KV).
  // [Tradeoff] под concurrent burst несколько Worker'ов могут проскочить за
  // лимит на race, но в среднем по минуте — capped. Под Durable Object
  // переписать когда станет реально нужно.
  const current = Number((await c.env.QUOTA.get(key)) ?? '0') || 0;
  if (current >= limit) {
    return c.json(
      {
        ok: false as const,
        code: 'rate_limited' as const,
        message: 'Слишком много запросов с этой подсети — подожди минуту.',
      },
      429,
    );
  }
  // Increment (best-effort write).
  await c.env.QUOTA.put(key, String(current + 1), {
    expirationTtl: KEY_TTL_SECONDS,
  });
  return null;
}
