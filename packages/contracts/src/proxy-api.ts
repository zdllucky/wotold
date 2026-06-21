// Контракт прокси-сервиса. См. раздел 10 паспорта.
//
// R8: аудио НЕ проходит через память воркера. Поток:
//   1. POST /v1/stt/staging-url → presigned R2 PUT
//   2. Клиент загружает аудио прямо в R2
//   3. POST /v1/stt с { r2Key, opts } → воркер передаёт партнёру R2-URL

import type { DiarizedTranscript, TranscriptionProviderId } from './transcript.js';

export interface SttStagingUrlRequest {
  /** Suggested content type, e.g. 'audio/wav'. */
  contentType: string;
}

export interface SttStagingUrlResponse {
  /** Opaque R2 object key chosen by the proxy. */
  r2Key: string;
  /** Presigned PUT URL valid for short window (TTL via STAGING_PRESIGN_TTL_SECONDS). */
  uploadUrl: string;
  /** Headers required on PUT, if any. */
  headers?: Record<string, string>;
  /** UTC ISO of presign expiry. */
  expiresAt: string;
}

export interface SttRequest {
  /** R2 key returned from /v1/stt/staging-url. */
  r2Key: string;
  opts: {
    provider: TranscriptionProviderId;
    /** Must be true; diarization always on (M2.4). */
    diarization: true;
    /** 'auto' or BCP 47 code. */
    lang: 'auto' | string;
  };
}

export type SttResponse =
  | { ok: true; transcript: DiarizedTranscript }
  | { ok: false; code: ProxyErrorCode; message: string };

export interface LlmRequest {
  /** Optional model id; proxy default applied if omitted. */
  model?: string;
  system: string;
  /** Prebuilt input — typically diarized transcript with names. */
  input: string;
  /** Caller hint for max output tokens. */
  maxTokens?: number;
}

export type LlmResponse =
  | { ok: true; json: unknown }
  | { ok: false; code: ProxyErrorCode; message: string };

export interface UsageResponse {
  /** SCAFFOLD — захардкожено 'free' (M9.3). */
  tier: 'free';
  sttSecondsUsed: number;
  /** Дневной лимит STT секунд для текущего тира. 0 = безлимит/не настроен. */
  sttSecondsLimit: number;
  llmTokensUsed: number;
  /** Дневной лимит LLM токенов для текущего тира. 0 = безлимит/не настроен. */
  llmTokensLimit: number;
  /** ISO 8601 UTC. Окончание текущего suток счёта (UTC midnight + 1). */
  periodResetAt: string;
}

export type ProxyErrorCode =
  | 'invalid_device_id'
  | 'quota_exceeded'
  | 'bad_request'
  | 'provider_error'
  | 'staging_object_not_found'
  | 'internal_error'
  /** /16 IP rate-limit (middleware/ip-rate-limit.ts) — 429 при превышении. */
  | 'rate_limited';

/** Header name for device-id (M9.2). */
export const DEVICE_ID_HEADER = 'x-device-id';
