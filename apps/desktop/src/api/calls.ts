import { invoke } from '@tauri-apps/api/core';

import type { Call } from './recording';

export interface ActionItem {
  id: string;
  call_id: string;
  text: string;
  owner_contact_id: string | null;
  due: string | null;
  done: boolean;
  // [M14 T-02 / T-11] V2 enrichment. NULL для legacy v1 rows.
  owner_confidence: number | null;
  due_confidence: number | null;
  /** 'commitment' | 'proposal' | 'idea'. Default 'commitment' для legacy. */
  category: string | null;
  evidence_quote: string | null;
  evidence_speaker: string | null;
  evidence_start_ms: number | null;
}

/** [M14 T-11] Decision row для DecisionsBlock UI. */
export interface Decision {
  id: string;
  call_id: string;
  text: string;
  evidence_quote: string | null;
  evidence_speaker: string | null;
  evidence_start_ms: number | null;
  evidence_end_ms: number | null;
  confidence: number | null;
  order_idx: number;
}

/** [M14 T-11] Open question row для OpenQuestionsBlock UI. */
export interface OpenQuestion {
  id: string;
  call_id: string;
  text: string;
  raised_by: string | null;
  evidence_quote: string | null;
  evidence_speaker: string | null;
  evidence_start_ms: number | null;
  order_idx: number;
}

export function listCallDecisions(callId: string): Promise<Decision[]> {
  return invoke<Decision[]>('list_call_decisions', { callId });
}

export function listCallOpenQuestions(callId: string): Promise<OpenQuestion[]> {
  return invoke<OpenQuestion[]>('list_call_open_questions', { callId });
}

export type CallArtifactKind = 'recap' | 'transcript' | 'raw_stt';

export function getCall(id: string): Promise<Call | null> {
  return invoke<Call | null>('get_call', { id });
}

export function listCallActionItems(callId: string): Promise<ActionItem[]> {
  return invoke<ActionItem[]>('list_call_action_items', { callId });
}

export function readCallArtifact(
  callId: string,
  kind: CallArtifactKind,
): Promise<string | null> {
  return invoke<string | null>('read_call_artifact', { callId, kind });
}

/** C5 (#41): cascade delete — calls row, voice_samples, action_items, audio files. */
export function deleteCall(id: string): Promise<void> {
  return invoke('delete_call', { id });
}

/** M4.5: пересоздать recap.md + action_items без re-STT. */
export function regenerateRecap(callId: string): Promise<void> {
  return invoke<void>('regenerate_recap', { callId });
}

/** [M14 T-17] Lightweight title-only regen. Engine-aware: Local-движок → локальный
 *  Qwen (~5-10s), cloud → Anthropic (мгновенно). Returns new title (persisted in DB). */
export function regenerateTitle(callId: string): Promise<string> {
  return invoke<string>('regenerate_title', { callId });
}

/** Прогресс массового регена пустых рекапов (`recap:bulk_progress`). */
export interface BulkRecapProgress {
  done: number;
  total: number;
  call_id: string;
}

/** Итог массового регена (`recap:bulk_done`). */
export interface BulkRecapDone {
  regenerated: number;
  failed: number;
  cancelled: boolean;
}

/** [Bulk recap] Пересоздать рекапы всех ready-звонков с пустым recap.md.
 *  Возвращает кол-во звонков на обработку; реген идёт в фоне с событиями. */
export function regenerateEmptyRecaps(): Promise<number> {
  return invoke<number>('regenerate_empty_recaps');
}

/** [Bulk recap] Прервать активный массовый реген. */
export function cancelBulkRecap(): Promise<void> {
  return invoke<void>('cancel_bulk_recap');
}

/** [B16]: путь к WAV-файлу звонка для аудиоплеера. */
export function getCallAudioPath(callId: string, kind: 'mic' | 'system'): Promise<string> {
  return invoke<string>('get_call_audio_path', { callId, kind });
}

/** Перезапустить полный pipeline (STT + recap) для существующего звонка.
 *  Применяется к failed | ready | processing — берёт mic.wav/system.wav с диска.
 *  [V8] Spawn'ится в background — promise резолвится сразу. Прогресс через
 *  события `pipeline:started` / `call:progress` / `pipeline:finished`. */
export function reprocessCall(callId: string): Promise<void> {
  return invoke<void>('reprocess_call', { callId });
}

/** [V8] Отменить running reprocess. Идемпотент. После отмены:
 *   - artifacts_intact=true → call.status='ready', старый recap/transcript на месте
 *   - artifacts_intact=false → call.status='failed' с reason «Отменено пользователем»
 *  Эмитит `pipeline:cancelled` событие — фронт перечитывает state. */
export function cancelReprocess(callId: string): Promise<void> {
  return invoke<void>('cancel_reprocess', { callId });
}

/** [V8] Событие `pipeline:cancelled` payload. */
export interface PipelineCancelledEvent {
  call_id: string;
  artifacts_intact: boolean;
}

/** [V9] Количество РЕАЛЬНО работающих pipeline-задач в текущей сессии.
 *  Источник правды — in-memory `pipeline_tasks` registry в AppState, а не
 *  DB filter (там zombie processing rows от crashed sessions дают false
 *  positives).
 *
 *  Использовать вместо `listCalls().filter(status==='processing'|'recording')`
 *  для UI counter badges. listCalls остаётся для отображения списка звонков. */
export function getActivePipelineCount(): Promise<number> {
  return invoke<number>('get_active_pipeline_count');
}

/** Экспортировать звонок (recap + transcript + meta) в один markdown-файл
 *  по выбранному пользователем пути. dest_path берётся из save-dialog'а
 *  на frontend'е. Файл должен иметь расширение `.md`. */
export function exportCallMarkdown(callId: string, destPath: string): Promise<void> {
  return invoke<void>('export_call_markdown', { callId, destPath });
}
