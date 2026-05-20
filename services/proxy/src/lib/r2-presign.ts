import { AwsClient } from 'aws4fetch';
import type { Env } from './env.js';

/**
 * Presigned PUT URL для загрузки объекта в R2 через S3-совместимый endpoint.
 * R2 binding не умеет presign — используется aws4fetch + S3 SigV4.
 */
export async function presignR2Put(
  env: Env,
  r2Key: string,
  contentType: string,
  ttlSeconds: number,
): Promise<string> {
  const aws = makeClient(env);
  const url = makeUrl(env, r2Key, ttlSeconds);

  const signed = await aws.sign(
    new Request(url.toString(), {
      method: 'PUT',
      headers: { 'content-type': contentType },
    }),
    { aws: { signQuery: true } },
  );
  return signed.url;
}

/**
 * Presigned GET URL для скачивания объекта из R2 — передаётся партнёру
 * (Soniox/Gladia), чтобы они забрали аудио сами (R8 паспорта: аудио
 * не проходит через память воркера).
 */
export async function presignR2Get(
  env: Env,
  r2Key: string,
  ttlSeconds: number,
): Promise<string> {
  const aws = makeClient(env);
  const url = makeUrl(env, r2Key, ttlSeconds);

  const signed = await aws.sign(
    new Request(url.toString(), { method: 'GET' }),
    { aws: { signQuery: true } },
  );
  return signed.url;
}

function makeClient(env: Env): AwsClient {
  if (!env.R2_ACCOUNT_ID || !env.R2_ACCESS_KEY_ID || !env.R2_SECRET_ACCESS_KEY) {
    throw new Error(
      'R2 S3 credentials not configured: R2_ACCOUNT_ID / R2_ACCESS_KEY_ID / R2_SECRET_ACCESS_KEY',
    );
  }
  return new AwsClient({
    accessKeyId: env.R2_ACCESS_KEY_ID,
    secretAccessKey: env.R2_SECRET_ACCESS_KEY,
    service: 's3',
    region: 'auto',
  });
}

function makeUrl(env: Env, r2Key: string, ttlSeconds: number): URL {
  const url = new URL(
    `https://${env.R2_ACCOUNT_ID}.r2.cloudflarestorage.com/${env.STT_STAGING_BUCKET}/${encodeURIComponent(r2Key)}`,
  );
  url.searchParams.set('X-Amz-Expires', String(ttlSeconds));
  return url;
}
