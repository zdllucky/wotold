import { Hono } from 'hono';
import type { LlmResponse } from '@wotold/contracts';
import type { Env } from '../lib/env.js';
import { requireDeviceId } from '../middleware/device-id.js';
import { enforceQuota, incUsage } from '../middleware/rate-limit.js';
import { callLlm } from '../lib/llm-backends.js';
import { llmRequestSchema, parseBody } from '../lib/schemas.js';

export const llmRoutes = new Hono<{ Bindings: Env; Variables: { deviceId: string } }>();

llmRoutes.use('*', requireDeviceId);

llmRoutes.post('/', async (c) => {
  const quotaErr = await enforceQuota(c, 'llm_tok');
  if (quotaErr) return quotaErr;

  const parsed = await parseBody(c.req.raw, llmRequestSchema);
  if (!parsed.ok) {
    return c.json(
      { ok: false, code: 'bad_request', message: parsed.message } satisfies LlmResponse,
      400,
    );
  }
  const body = parsed.data;

  const result = await callLlm(c.env, {
    system: body.system,
    input: body.input,
    // nullish (null | undefined) → undefined для downstream (callLlm
    // ожидает Optional<string>, не nullable). Backwards compat: Rust
    // serde Option::None → JSON null, мы принимаем оба и нормализуем.
    model: body.model ?? undefined,
    maxTokens: body.maxTokens ?? undefined,
  });

  if (!result.ok) {
    // [B16 audit P1]: явный whitelist вместо unsafe cast.
    const status: 400 | 502 | 503 =
      result.status === 400 ? 400 : result.status === 503 ? 503 : 502;
    return c.json(
      { ok: false, code: result.code, message: result.message } satisfies LlmResponse,
      status,
    );
  }

  if (result.tokensUsed > 0) {
    await incUsage(c.env, c.get('deviceId'), 'llm_tok', result.tokensUsed);
  }

  return c.json({ ok: true, json: result.parsed } satisfies LlmResponse);
});
