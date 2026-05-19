import { Hono } from 'hono';
import type {
  SttStagingUrlRequest,
  SttStagingUrlResponse,
  SttRequest,
  SttResponse,
} from '@wotold/contracts';
import type { Env } from '../lib/env.js';
import { requireDeviceId } from '../middleware/device-id.js';
import { enforceQuota } from '../middleware/rate-limit.js';
import { presignR2Put } from '../lib/r2-presign.js';

export const sttRoutes = new Hono<{ Bindings: Env; Variables: { deviceId: string } }>();

sttRoutes.use('*', requireDeviceId);

sttRoutes.post('/staging-url', async (c) => {
  const body = await c.req.json<SttStagingUrlRequest>().catch(() => null);
  if (!body || typeof body.contentType !== 'string') {
    return c.json(
      { ok: false, code: 'bad_request', message: 'contentType required' } satisfies SttResponse,
      400,
    );
  }

  const ttl = Number(c.env.STAGING_PRESIGN_TTL_SECONDS) || 900;
  const r2Key = `stt/${c.get('deviceId')}/${crypto.randomUUID()}`;

  let uploadUrl: string;
  try {
    uploadUrl = await presignR2Put(c.env, r2Key, body.contentType, ttl);
  } catch (e) {
    console.error('presign failed', (e as Error).message);
    return c.json(
      { ok: false, code: 'internal_error', message: 'presign failed' } satisfies SttResponse,
      500,
    );
  }

  const resp: SttStagingUrlResponse = {
    r2Key,
    uploadUrl,
    headers: { 'content-type': body.contentType },
    expiresAt: new Date(Date.now() + ttl * 1000).toISOString(),
  };
  return c.json(resp);
});

sttRoutes.post('/', async (c) => {
  const quotaErr = await enforceQuota(c, 'stt_sec');
  if (quotaErr) return quotaErr;

  const body = await c.req.json<SttRequest>().catch(() => null);
  if (!body || typeof body.r2Key !== 'string' || !body.opts) {
    return c.json(
      { ok: false, code: 'bad_request', message: 'r2Key and opts required' } satisfies SttResponse,
      400,
    );
  }

  // Defensive: stage object must exist (cheap HEAD via R2 binding).
  const head = await c.env.STT_STAGING.head(body.r2Key);
  if (!head) {
    return c.json(
      {
        ok: false,
        code: 'staging_object_not_found',
        message: 'r2 object not found',
      } satisfies SttResponse,
      404,
    );
  }

  // TODO(soniox|gladia): подключить реальный вызов партнёрского STT.
  //   1. Подписать GET-URL для r2Key (TTL под async-окно партнёра)
  //   2. POST к Soniox/Gladia с этим URL + opts (diarization, lang)
  //   3. Дождаться/поллить результат, нормализовать в DiarizedTranscript
  //   4. incUsage(c.env, deviceId, 'stt_sec', durationSec)
  //
  // Ключ из секрета: c.env.SONIOX_API_KEY / c.env.GLADIA_API_KEY (S1).
  // M9.6: не логировать содержимое транскрипта. Метрики — да.

  return c.json(
    {
      ok: false,
      code: 'provider_error',
      message: 'STT provider relay not yet wired',
    } satisfies SttResponse,
    501,
  );
});
