export interface Env {
  // KV — квота по device-id (M9.3)
  QUOTA: KVNamespace;

  // KV — M10 auth (#37): state-токены / sessions / accounts
  AUTH: KVNamespace;

  // R2 — стейджинг аудио (R8)
  STT_STAGING: R2Bucket;

  // vars
  TIER: 'free';
  QUOTA_STT_SECONDS_PER_DAY: string;
  QUOTA_LLM_TOKENS_PER_DAY: string;
  STAGING_PRESIGN_TTL_SECONDS: string;
  STT_STAGING_BUCKET: string;
  ANTHROPIC_DEFAULT_MODEL: string;

  /** Какой LLM-провайдер использует /v1/llm: 'groq' | 'anthropic'.
   *  Если пусто — auto: groq если есть GROQ_API_KEY, иначе anthropic. */
  LLM_BACKEND: string;
  GROQ_DEFAULT_MODEL: string;

  // M10 auth (#37) — vars (публичные).
  AUTH_STATE_TTL_SECONDS: string;
  AUTH_SESSION_TTL_SECONDS: string;
  GOOGLE_OAUTH_CLIENT_ID: string;
  APPLE_OAUTH_CLIENT_ID: string;
  MICROSOFT_OAUTH_CLIENT_ID: string;
  PUBLIC_BASE_URL: string;

  // secrets (S1)
  ANTHROPIC_API_KEY?: string;
  GROQ_API_KEY?: string;
  SONIOX_API_KEY?: string;
  GLADIA_API_KEY?: string;

  // M10 auth (#37) — secrets.
  GOOGLE_OAUTH_CLIENT_SECRET?: string;
  APPLE_OAUTH_CLIENT_SECRET?: string;
  MICROSOFT_OAUTH_CLIENT_SECRET?: string;

  // R2 S3-compat credentials for presigning
  R2_ACCOUNT_ID?: string;
  R2_ACCESS_KEY_ID?: string;
  R2_SECRET_ACCESS_KEY?: string;
}
