// [Hardening] Zod schemas для boundary'ев proxy — заменяет hand-rolled
// `typeof body.X !== 'string'` validation. Все .parse() → consistent
// `bad_request` 400 response через `parseBody` helper.
//
// Сами schema'ы выровнены под types из @wotold/contracts. Двойная истина —
// shape должен совпадать; при изменении контракта здесь тоже правится
// (TS-проверки в callsite поймают drift).

import { z } from 'zod';

// ─── Common ─────────────────────────────────────────────────────────

/** UUID v4-ish — 8-4-4-4-12 lowercase hex. Используем в auth + device-id. */
export const uuidSchema = z
  .string()
  .regex(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i);

// ─── LLM ────────────────────────────────────────────────────────────

/** /v1/llm POST body. system + input required; model + maxTokens optional. */
export const llmRequestSchema = z.object({
  system: z.string().min(1, 'system required'),
  input: z.string().min(1, 'input required'),
  model: z.string().optional(),
  maxTokens: z.number().int().positive().max(32_000).optional(),
});

export type LlmRequestParsed = z.infer<typeof llmRequestSchema>;

// ─── STT ────────────────────────────────────────────────────────────

/** /v1/stt/staging-url POST body — содержит contentType для R2 presign. */
export const sttStagingUrlRequestSchema = z.object({
  contentType: z.string().min(1, 'contentType required'),
});

/** /v1/stt POST body — r2Key + opts (provider + lang). Lang обязателен по
 *  контракту: `'auto'` либо BCP47-код. STT-партнёры в lib/partners требуют
 *  лежащий string (не undefined). */
export const sttRequestSchema = z.object({
  r2Key: z.string().min(1, 'r2Key required'),
  opts: z.object({
    provider: z.enum(['soniox', 'gladia']),
    lang: z.string().min(1, 'opts.lang required (use "auto")'),
    diarization: z.boolean().optional(),
  }),
});

// ─── Auth ───────────────────────────────────────────────────────────

/** /v1/auth/:provider/start POST body. deviceId должен быть UUID если задан. */
export const authStartRequestSchema = z.object({
  deviceId: uuidSchema.optional(),
  redirectMode: z.enum(['json', 'deeplink']).optional(),
});

// ─── Helper ─────────────────────────────────────────────────────────

/**
 * Распарсить request body против схемы. Возвращает Discriminated union —
 * либо `{ ok: true, data }`, либо `{ ok: false, message }` где message —
 * первый zod issue (user-readable).
 *
 * Caller сам решает как ответить (envelope varies: `{ok:false, code, message}`
 * для stt/llm/usage, `{error: { code, message }}` для auth).
 */
export async function parseBody<T extends z.ZodTypeAny>(
  request: Request,
  schema: T,
): Promise<{ ok: true; data: z.infer<T> } | { ok: false; message: string }> {
  let raw: unknown;
  try {
    raw = await request.json();
  } catch {
    return { ok: false, message: 'invalid JSON body' };
  }
  const result = schema.safeParse(raw);
  if (!result.success) {
    const first = result.error.issues[0];
    const path = first?.path.length ? `${first.path.join('.')}: ` : '';
    return { ok: false, message: `${path}${first?.message ?? 'validation failed'}` };
  }
  return { ok: true, data: result.data };
}
