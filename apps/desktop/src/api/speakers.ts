// #26 (M3.5): фронтенд API для confirm/unbind speaker bindings.
//
// Suggestions приходят из identify_speakers (#25) pipeline, но финальная
// привязка к контакту — ТОЛЬКО через пользовательский confirm (R2 паспорта).

import { invoke } from '@tauri-apps/api/core';

export interface CallSpeakerView {
  id: string;
  call_id: string;
  speaker_tag: string;
  contact_id: string | null;
  contact_display_name: string | null;
  suggestion_contact_id: string | null;
  suggestion_contact_display_name: string | null;
  suggestion_score: number | null;
  /** 'embedding' | 'llm' | 'both' — источник signal для UI debug-вью. */
  suggestion_source: string | null;
  confirmed: boolean;
  /** [V7] RFC3339 timestamp если speaker был привязан автоматически
   *  (suggestion_score >= threshold). NULL = ручное подтверждение или
   *  pending. UI рендерит «↩ отменить» баннер для свежих авто-привязок. */
  auto_bound_at: string | null;
}

/** [V7] Событие `call:auto_bound` — после pipeline matching auto-bind
 *  обнаружил N speaker'ов с score ≥ threshold. UI рендерит баннер
 *  «Авто-привязано: N собеседник(ов) (≥{threshold}%)» с undo-кнопкой. */
export interface CallAutoBoundEvent {
  call_id: string;
  count: number;
  threshold_pct: number;
}

export function listCallSpeakers(callId: string): Promise<CallSpeakerView[]> {
  return invoke<CallSpeakerView[]>('list_call_speakers', { callId });
}

/** [TD-46] Спикеры сразу для списка звонков — один вызов вместо вызова на
 *  строку списка. Звонки без спикеров в ответе отсутствуют. */
export function listCallSpeakersBatch(
  callIds: string[],
): Promise<Record<string, CallSpeakerView[]>> {
  return invoke<Record<string, CallSpeakerView[]>>('list_call_speakers_batch', { callIds });
}

export function confirmCallSpeaker(
  callSpeakerId: string,
  contactId: string,
): Promise<void> {
  return invoke<void>('confirm_call_speaker', { callSpeakerId, contactId });
}

export function unbindCallSpeaker(callSpeakerId: string): Promise<void> {
  return invoke<void>('unbind_call_speaker', { callSpeakerId });
}
