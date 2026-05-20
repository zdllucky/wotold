import { Hono } from 'hono';
import type { LlmRequest, LlmResponse } from '@wotold/contracts';
import type { Env } from '../lib/env.js';
import { requireDeviceId } from '../middleware/device-id.js';
import { enforceQuota, incUsage } from '../middleware/rate-limit.js';
import { callLlm } from '../lib/llm-backends.js';

export const llmRoutes = new Hono<{ Bindings: Env; Variables: { deviceId: string } }>();

llmRoutes.use('*', requireDeviceId);

llmRoutes.post('/', async (c) => {
  const quotaErr = await enforceQuota(c, 'llm_tok');
  if (quotaErr) return quotaErr;

  const body = await c.req.json<LlmRequest>().catch(() => null);
  if (!body || typeof body.system !== 'string' || typeof body.input !== 'string') {
    return c.json(
      { ok: false, code: 'bad_request', message: 'system and input required' } satisfies LlmResponse,
      400,
    );
  }

  const result = await callLlm(c.env, {
    system: body.system,
    input: body.input,
    model: body.model,
    maxTokens: body.maxTokens,
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
