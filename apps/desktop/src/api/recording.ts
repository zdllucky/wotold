import { invoke } from '@tauri-apps/api/core';

export interface Call {
  id: string;
  title: string | null;
  started_at: string;
  ended_at: string | null;
  duration_sec: number | null;
  status: string;
  provider: string | null;
  path_label: string;
  lang_detected: string | null;
  /** M2.7 (#23): UX-readable причина если status=failed. */
  failed_reason: string | null;
  /** [B16]: причина если LLM-recap упал. status может быть 'ready' (транскрипт
   *  есть), но саммари недоступно. UI banner + кнопка retry. */
  recap_failed_reason: string | null;
  /** [V6.2] Pipeline progress fields. NULL когда звонок не в обработке —
   *  UI рендерит ProgressRail только при `status='processing'`. step: 1..5,
   *  pct: 0..100, eta_sec/upload_bytes опционально. */
  pipeline_step: number | null;
  pipeline_pct: number | null;
  pipeline_eta_sec: number | null;
  upload_bytes: number | null;
  /** [W2] RFC3339 timestamp когда юзер нажал pause; null если запись не на
   *  паузе или уже завершена. Только для status='recording'. */
  paused_at: string | null;
  /** [W2] Накопленное время на паузе в миллисекундах. Pipeline и UI вычитают
   *  это из (ended_at - started_at) для фактической длительности аудио. */
  paused_total_ms: number;
  /** [M12-v1.1] Движок обработки. Null для звонков до трекинга — EngineChip
   *  просто не рендерится. */
  processing_via: 'local' | 'cloud_managed' | 'cloud_byo' | null;
  // [M14 T-02 / T-11] V2 summary metadata. NULL для legacy schema_version=1
  // или ещё не обработанных звонков. UI рендерит CallTypeBadge только когда
  // non-null + confidence ≥ 0.5; иначе fallback к engine chip + дате.
  call_type:
    | 'sales_discovery'
    | 'sales_demo'
    | 'product_sync'
    | 'standup'
    | 'customer_interview'
    | 'one_on_one'
    | 'strategy_brainstorm'
    | 'status_update'
    | 'other'
    | null;
  call_type_confidence: number | null;
  /** 1 = legacy markdown only; 2 = full CallSummaryV2 (decisions/open_questions/evidence). */
  summary_schema_version: number | null;
  /** "cloud-managed" | "local-qwen-1.5b" | "local-qwen-3b" | "local-qwen-7b" */
  summary_engine: string | null;
  summary_pipeline_mode: string | null;
  created_at: string;
  updated_at: string;
}

/** [V6.2] Tauri событие `call:progress` — per-step pipeline tick. UI слушает
 *  через `listen('call:progress', ...)` для live ProgressRail без polling'а.
 *  DB также обновлена — reload восстанавливает state. */
export interface CallProgressEvent {
  call_id: string;
  step: number; // 1..5
  pct: number; // 0..100
  eta_sec: number | null;
  upload_bytes: number | null;
}

export interface RecordingState {
  call_id: string;
  started_at: string;
  /** [W2] RFC3339 если запись сейчас на паузе, null иначе. */
  paused_at: string | null;
  /** [W2] Накопленная длительность пауз в мс. UI'у пригодится для elapsed. */
  paused_total_ms: number;
}

export function startRecording(): Promise<Call> {
  return invoke<Call>('start_recording');
}

/** Останавливает запись. `null` = запись короче минимума (30с) — отброшена
 *  (строка звонка + WAV удалены, пайплайн не запущен). */
export function stopRecording(): Promise<Call | null> {
  return invoke<Call | null>('stop_recording');
}

export function getRecordingState(): Promise<RecordingState | null> {
  return invoke<RecordingState | null>('get_recording_state');
}

/** [W2] Пауза активной записи. Бэкенд проставляет `paused_at = now()`. */
export function pauseRecording(): Promise<RecordingState> {
  return invoke<RecordingState>('pause_recording');
}

/** [W2] Возобновление записи с паузы. Накопленное время добавляется к
 *  `paused_total_ms`, `paused_at` очищается. */
export function resumeRecording(): Promise<RecordingState> {
  return invoke<RecordingState>('resume_recording');
}

export function listCalls(): Promise<Call[]> {
  return invoke<Call[]>('list_calls');
}

// ────────────────────────────────────────────────────────────
// [M13.3.1] Chunked pipeline — per-call chunk progress
// ────────────────────────────────────────────────────────────

/** [M13.3.1] Lightweight view над `call_chunks` row для UI ChunkProgressStrip.
 *  Heavy fields (transcript_json / embeddings_json) не передаются — UI они не
 *  нужны, лишний network roundtrip. */
export interface CallChunk {
  chunk_idx: number;
  /** pending | processing | done | failed (status FSM из db::chunks). */
  status: 'pending' | 'processing' | 'done' | 'failed';
  start_ms: number;
  end_ms: number | null;
}

export function listCallChunks(callId: string): Promise<CallChunk[]> {
  return invoke<CallChunk[]>('list_call_chunks', { callId });
}

/** [Tech-debt P0.2] Retry failed chunk — backend `mark_chunk_pending` +
 *  background `chunk_runner::run_chunk`. Status update приходит через
 *  существующий `transcript:chunk_done` event без рефетча. */
export function retryChunk(callId: string, chunkIdx: number): Promise<void> {
  return invoke<void>('retry_chunk', { callId, chunkIdx });
}

/** [M13.2.3] Per-chunk pipeline finished — эмитится из `chunk_runner` на
 *  done/failed. UI patch'ит ChunkProgressStrip без полного refetch'а. */
export interface ChunkDoneEvent {
  call_id: string;
  chunk_idx: number;
  status: 'done' | 'failed';
  segment_count: number;
}

/** [P1.3] Periodic emit во время local LLM recap generation. Backend
 *  `pipeline::recap_progress::with_recap_progress_emitter` шлёт каждые 15s
 *  пока future не resolve'нется. UI рендерит «Пересоздаём… {sec}s» в
 *  HeaderActions. */
export interface RecapProgressEvent {
  call_id: string;
  elapsed_sec: number;
}
export const RECAP_PROGRESS_EVENT = 'recap:progress';

/** [P5.2] Live duration update во время recording — fires на каждый
 *  sidecar `rotated` event (~раз в 10 мин). HomePage / CallDetailPage
 *  patch'ат `call.duration_sec` чтобы не показывать stale «1:56» для
 *  активных 30+ мин записей. */
export interface RecordingDurationEvent {
  call_id: string;
  duration_sec: number;
}
export const RECORDING_DURATION_EVENT = 'recording:duration';
