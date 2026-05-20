import { Hono } from 'hono';
import type {
  SttStagingUrlRequest,
  SttStagingUrlResponse,
  SttRequest,
  SttResponse,
} from '@wotold/contracts';
import type { Env } from '../lib/env.js';
import { requireDeviceId } from '../middleware/device-id.js';
import { enforceQuota, incUsage } from '../middleware/rate-limit.js';
import { presignR2Get, presignR2Put } from '../lib/r2-presign.js';
import { transcribeSoniox } from '../lib/partners/soniox.js';
import { transcribeGladia } from '../lib/partners/gladia.js';

// Workers Free CPU/wall лимит ≈ 30s на запрос. Polling 25s + буфер на сеть.
const POLL_BUDGET_MS = 25_000;
// Presigned GET URL для партнёра — TTL под async-окно (30 минут хватит).
const PARTNER_AUDIO_TTL_SECONDS = 1800;
// [B3]: KV TTL для resumable partner jobs. Partner sessions у Soniox/Gladia
// живут дольше — берём 30 минут как safe upper bound для retry окна.
const JOB_CACHE_TTL_SECONDS = 1800;

interface SonioxJobCache {
  jobId: string;
}

interface GladiaJobCache {
  jobId: string;
  resultUrl: string;
}

function sttJobKey(provider: 'soniox' | 'gladia', r2Key: string): string {
  return `stt_job:${provider}:${r2Key}`;
}

export const sttRoutes = new Hono<{ Bindings: Env; Variables: { deviceId: string } }>();

sttRoutes.use('*', requireDeviceId);

// Allowlist content-types для presign — защита от загрузки text/html для
// phishing через R2 presigned URL (audit [P0]). Аудио-форматы которые
// поддерживают наши STT-провайдеры.
const ALLOWED_CONTENT_TYPES = new Set([
  'audio/wav',
  'audio/wave',
  'audio/x-wav',
  'audio/mpeg',
  'audio/mp3',
  'audio/mp4',
  'audio/m4a',
  'audio/x-m4a',
  'audio/ogg',
  'audio/opus',
  'audio/webm',
  'audio/flac',
]);

sttRoutes.post('/staging-url', async (c) => {
  const body = await c.req.json<SttStagingUrlRequest>().catch(() => null);
  if (!body || typeof body.contentType !== 'string') {
    return c.json(
      { ok: false, code: 'bad_request', message: 'contentType required' } satisfies SttResponse,
      400,
    );
  }

  if (!ALLOWED_CONTENT_TYPES.has(body.contentType.toLowerCase())) {
    return c.json(
      {
        ok: false,
        code: 'bad_request',
        message: `contentType '${body.contentType}' не поддерживается. Разрешены аудио-форматы (wav/mp3/m4a/ogg/opus/webm/flac).`,
      } satisfies SttResponse,
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

  // R8: партнёр сам забирает аудио из R2 по presigned URL — байты не
  // проходят через память воркера.
  let audioUrl: string;
  try {
    audioUrl = await presignR2Get(c.env, body.r2Key, PARTNER_AUDIO_TTL_SECONDS);
  } catch (e) {
    console.error('presign GET failed', (e as Error).message);
    return c.json(
      { ok: false, code: 'internal_error', message: 'presign failed' } satisfies SttResponse,
      500,
    );
  }

  const provider = body.opts.provider;
  const lang = body.opts.lang;
  const deadline = Date.now() + POLL_BUDGET_MS;

  try {
    let transcript;
    if (provider === 'soniox') {
      if (!c.env.SONIOX_API_KEY) {
        return c.json(
          {
            ok: false,
            code: 'provider_error',
            message: 'Soniox key not configured on proxy',
          } satisfies SttResponse,
          503,
        );
      }
      // [B3]: попытка resume по KV-кэшу для этого r2Key.
      const cacheKey = sttJobKey('soniox', body.r2Key);
      const cachedRaw = await c.env.QUOTA.get(cacheKey);
      const cached: SonioxJobCache | null = cachedRaw
        ? (JSON.parse(cachedRaw) as SonioxJobCache)
        : null;

      const result = await transcribeSoniox({
        apiKey: c.env.SONIOX_API_KEY,
        audioUrl,
        lang,
        pollDeadlineMs: deadline,
        existingJobId: cached?.jobId,
      });
      // Кэшируем jobId только если job создан в этом вызове ИЛИ был resume но ещё активен —
      // в обоих случаях TTL обновится. На completion удалим.
      if (result.jobCreated) {
        await c.env.QUOTA.put(
          cacheKey,
          JSON.stringify({ jobId: result.jobId } satisfies SonioxJobCache),
          { expirationTtl: JOB_CACHE_TTL_SECONDS },
        );
      } else {
        // Resume + completed → cleanup кэша
        await c.env.QUOTA.delete(cacheKey);
      }
      transcript = result.transcript;
    } else if (provider === 'gladia') {
      if (!c.env.GLADIA_API_KEY) {
        return c.json(
          {
            ok: false,
            code: 'provider_error',
            message: 'Gladia key not configured on proxy',
          } satisfies SttResponse,
          503,
        );
      }
      const cacheKey = sttJobKey('gladia', body.r2Key);
      const cachedRaw = await c.env.QUOTA.get(cacheKey);
      const cached: GladiaJobCache | null = cachedRaw
        ? (JSON.parse(cachedRaw) as GladiaJobCache)
        : null;

      const result = await transcribeGladia({
        apiKey: c.env.GLADIA_API_KEY,
        audioUrl,
        lang,
        pollDeadlineMs: deadline,
        existingJobId: cached?.jobId,
        existingResultUrl: cached?.resultUrl,
      });
      if (result.jobCreated) {
        await c.env.QUOTA.put(
          cacheKey,
          JSON.stringify({
            jobId: result.jobId,
            resultUrl: result.resultUrl,
          } satisfies GladiaJobCache),
          { expirationTtl: JOB_CACHE_TTL_SECONDS },
        );
      } else {
        await c.env.QUOTA.delete(cacheKey);
      }
      transcript = result.transcript;
    } else {
      return c.json(
        {
          ok: false,
          code: 'bad_request',
          message: `unknown provider: ${provider}`,
        } satisfies SttResponse,
        400,
      );
    }

    // M9.6: логируем только метрики, не контент.
    if (transcript.durationSec > 0) {
      await incUsage(
        c.env,
        c.get('deviceId'),
        'stt_sec',
        Math.ceil(transcript.durationSec),
      );
    }

    return c.json({ ok: true, transcript } satisfies SttResponse);
  } catch (e) {
    // R7 + Workers free: при превышении 30s wall time запрос упадёт.
    // Клиенту возвращаем provider_error, он может повторить попытку.
    console.error(`stt ${provider} failed`, (e as Error).message);
    return c.json(
      {
        ok: false,
        code: 'provider_error',
        message: (e as Error).message,
      } satisfies SttResponse,
      502,
    );
  }
});
