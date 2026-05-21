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
}

export function startRecording(): Promise<Call> {
  return invoke<Call>('start_recording');
}

export function stopRecording(): Promise<Call> {
  return invoke<Call>('stop_recording');
}

export function getRecordingState(): Promise<RecordingState | null> {
  return invoke<RecordingState | null>('get_recording_state');
}

export function listCalls(): Promise<Call[]> {
  return invoke<Call[]>('list_calls');
}
