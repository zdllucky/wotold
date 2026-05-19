export interface Env {
  // KV — квота по device-id (M9.3)
  QUOTA: KVNamespace;

  // R2 — стейджинг аудио (R8)
  STT_STAGING: R2Bucket;

  // vars
  TIER: 'free';
  QUOTA_STT_SECONDS_PER_DAY: string;
  QUOTA_LLM_TOKENS_PER_DAY: string;
  STAGING_PRESIGN_TTL_SECONDS: string;
  STT_STAGING_BUCKET: string;
  ANTHROPIC_DEFAULT_MODEL: string;

  // secrets (S1)
  ANTHROPIC_API_KEY?: string;
  SONIOX_API_KEY?: string;
  GLADIA_API_KEY?: string;

  // R2 S3-compat credentials for presigning
  R2_ACCOUNT_ID?: string;
  R2_ACCESS_KEY_ID?: string;
  R2_SECRET_ACCESS_KEY?: string;
}
