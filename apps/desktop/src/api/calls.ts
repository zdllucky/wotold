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

export type CallArtifactKind = 'recap' | 'transcript';

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

/** Перезапустить полный pipeline (STT + recap) для существующего звонка.
 *  Применяется к failed | ready | processing — берёт mic.wav/system.wav с диска. */
export function reprocessCall(callId: string): Promise<void> {
  return invoke<void>('reprocess_call', { callId });
}
