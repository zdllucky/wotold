// [V6.1] State machine types — call lifecycle.
//
// Mirrors Rust `calls.status` strings + derived UI substages. Не
// изобретать новые значения без правки `packages/contracts` и
// миграции БД.
//
// Backend → UI mapping:
//   DB recording      → UI live
//   DB processing     → UI processing | uploading (substage из pipeline_step)
//   DB ready          → UI ready
//   DB failed         → UI error
//   UI queued         → derivation: status=processing + есть другой processing
//                       начат раньше (UI-only, в БД нет).

export type CallState =
  | 'live' // currently recording
  | 'uploading' // audio uploading to STT provider (R2 → presigned)
  | 'queued' // waiting for another job to finish
  | 'processing' // STT / diarization / recap in progress
  | 'ready' // everything finished — transcript + recap available
  | 'error'; // any terminal failure (audio always preserved locally)

export interface CallProgress {
  /** 0..100 — percent of the overall pipeline (не текущего шага). */
  pct: number;
  /** Which pipeline step we're on (1..N=PIPELINE_STEPS.length). */
  step: 1 | 2 | 3 | 4 | 5;
  /** Human-readable label текущего шага. Localized в caller. */
  stageLabel: string;
  /** Seconds remaining (best estimate). */
  etaSec?: number;
  /** For uploading state — bytes done / total. */
  uploadedBytes?: number;
  uploadTotalBytes?: number;
  /** For queued state — position in queue (1 = next). UI-only. */
  queuePos?: number;
}

export interface CallError {
  /** Short code для stat-tag (STT_TIMEOUT, NET_OFFLINE, etc). */
  code: string;
  /** First-line user-facing message (localized). */
  message: string;
  /** Provider, который вернул failure. */
  provider?: 'soniox' | 'gladia' | 'anthropic' | null;
  /** ISO timestamps для diagnostics панели. */
  firstAttemptAt?: string;
  lastAttemptAt?: string;
  /** Retry counter — caps at PIPELINE_MAX_RETRIES. */
  attempts: number;
  /** Была ли квота списана у юзера? */
  quotaConsumed: boolean;
}

/**
 * Standard 5-step pipeline labels — translation keys, не сырой текст.
 * UI берёт через `t(PIPELINE_STEP_KEYS[step - 1])`.
 *
 * Order = real backend pipeline:
 *   1. Audio upload (mic.wav + system.wav на диск)
 *   2. Diarization (Swift sidecar split mic/system)
 *   3. STT (Soniox/Gladia ASR)
 *   4. Speaker matching (cluster_embedding + cosine)
 *   5. LLM recap + action items (Groq/Anthropic)
 */
export const PIPELINE_STEP_KEYS = [
  'pipeline.step1', // «Загрузили аудио»
  'pipeline.step2', // «Разделили дорожки mic + system»
  'pipeline.step3', // «Распознаём речь»
  'pipeline.step4', // «Соотносим спикеров с контактами»
  'pipeline.step5', // «Готовим рекап и задачи»
] as const;

/** Max retries (matches pipeline::retry::RetryConfig). */
export const PIPELINE_MAX_RETRIES = 3;
