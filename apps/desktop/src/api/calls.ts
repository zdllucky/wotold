import { invoke } from '@tauri-apps/api/core';

import type { Call } from './recording';

export interface ActionItem {
  id: string;
  call_id: string;
  text: string;
  owner_contact_id: string | null;
  due: string | null;
  done: boolean;
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

/** Экспортировать звонок (recap + transcript + meta) в один markdown-файл
 *  по выбранному пользователем пути. dest_path берётся из save-dialog'а
 *  на frontend'е. Файл должен иметь расширение `.md`. */
export function exportCallMarkdown(callId: string, destPath: string): Promise<void> {
  return invoke<void>('export_call_markdown', { callId, destPath });
}
