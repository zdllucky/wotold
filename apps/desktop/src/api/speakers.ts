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
}

export function listCallSpeakers(callId: string): Promise<CallSpeakerView[]> {
  return invoke<CallSpeakerView[]>('list_call_speakers', { callId });
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
