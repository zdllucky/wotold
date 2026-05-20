// #45 (M3.6 / M7.4 follow-up): API для просмотра и ручного удаления voice_samples.
//
// Embedding-данные не возвращаются — только метаданные (длина блоба для дебага,
// quality, source_call, created_at).

import { invoke } from '@tauri-apps/api/core';

export interface VoiceSampleView {
  id: string;
  contact_id: string;
  source_call: string | null;
  quality: number | null;
  created_at: string;
  /** Размер embedding-блоба. Реальные значения не раскрываются — биометрия. */
  embedding_bytes: number;
}

export function listVoiceSamples(contactId: string): Promise<VoiceSampleView[]> {
  return invoke<VoiceSampleView[]>('list_voice_samples', { contactId });
}

export function deleteVoiceSample(id: string): Promise<void> {
  return invoke<void>('delete_voice_sample', { id });
}
