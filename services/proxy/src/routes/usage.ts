import { Hono } from 'hono';
import type { UsageResponse } from '@wotold/contracts';
import type { Env } from '../lib/env.js';
import { requireDeviceId } from '../middleware/device-id.js';
import { readUsage, periodResetAt } from '../middleware/rate-limit.js';

export const usageRoutes = new Hono<{ Bindings: Env; Variables: { deviceId: string } }>();

usageRoutes.use('*', requireDeviceId);

usageRoutes.get('/', async (c) => {
  const deviceId = c.get('deviceId');
  const [sttSecondsUsed, llmTokensUsed] = await Promise.all([
    readUsage(c.env, deviceId, 'stt_sec'),
    readUsage(c.env, deviceId, 'llm_tok'),
  ]);

  const resp: UsageResponse = {
    tier: 'free',
    sttSecondsUsed,
    llmTokensUsed,
    periodResetAt: periodResetAt(),
  };
  return c.json(resp);
});
