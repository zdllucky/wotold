import { Hono } from 'hono';
import type { LlmRequest, LlmResponse } from '@wotold/contracts';
import type { Env } from '../lib/env.js';
import { requireDeviceId } from '../middleware/device-id.js';
import { enforceQuota, incUsage } from '../middleware/rate-limit.js';

const ANTHROPIC_URL = 'https://api.anthropic.com/v1/messages';

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

  if (!c.env.ANTHROPIC_API_KEY) {
    return c.json(
      {
        ok: false,
        code: 'provider_error',
        message: 'Anthropic key not configured on proxy',
      } satisfies LlmResponse,
      503,
    );
  }

  const model = body.model ?? c.env.ANTHROPIC_DEFAULT_MODEL;

  const upstream = await fetch(ANTHROPIC_URL, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-api-key': c.env.ANTHROPIC_API_KEY,
      'anthropic-version': '2023-06-01',
    },
    body: JSON.stringify({
      model,
      max_tokens: body.maxTokens ?? 4096,
      system: body.system,
      messages: [{ role: 'user', content: body.input }],
    }),
  });

  if (!upstream.ok) {
    const text = await upstream.text();
    // M9.6 — не логировать входное тело, только статус и краткий хвост ошибки.
    console.error('anthropic upstream error', upstream.status, text.slice(0, 500));
    return c.json(
      {
        ok: false,
        code: 'provider_error',
        message: `upstream ${upstream.status}`,
      } satisfies LlmResponse,
      502,
    );
  }

  const data = (await upstream.json()) as {
    content: Array<{ type: string; text?: string }>;
    usage?: { input_tokens: number; output_tokens: number };
  };

  const textBlock = data.content.find((b) => b.type === 'text')?.text ?? '';

  let parsed: unknown;
  try {
    parsed = JSON.parse(textBlock);
  } catch {
    return c.json(
      {
        ok: false,
        code: 'provider_error',
        message: 'LLM did not return JSON-parseable output',
      } satisfies LlmResponse,
      502,
    );
  }

  const tokensUsed = (data.usage?.input_tokens ?? 0) + (data.usage?.output_tokens ?? 0);
  if (tokensUsed > 0) {
    await incUsage(c.env, c.get('deviceId'), 'llm_tok', tokensUsed);
  }

  return c.json({ ok: true, json: parsed } satisfies LlmResponse);
});
