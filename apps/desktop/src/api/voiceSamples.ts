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
  /** [P4] Slice start_sec в source_call WAV. `null` для legacy rows
   *  (до migration 0017) — UI выключает play кнопку. */
  start_sec: number | null;
  end_sec: number | null;
  /** [P4] `'mic'` либо `'system'`. `null` для legacy. */
  track_kind: string | null;
}

export function listVoiceSamples(contactId: string): Promise<VoiceSampleView[]> {
  return invoke<VoiceSampleView[]>('list_voice_samples', { contactId });
}

export function deleteVoiceSample(id: string): Promise<void> {
  return invoke<void>('delete_voice_sample', { id });
}

/** [P4] Получить WAV bytes короткого slice voice sample (start..end из
 *  выбранной track). Backend возвращает full WAV-encoded buffer (44-byte
 *  RIFF header + i16 PCM samples). Frontend оборачивает в Blob URL для
 *  HTMLAudioElement.src. Errors: `voice_sample_legacy_no_slice` (NULL
 *  metadata) / `voice_sample_source_missing` (WAV file deleted).
 *
 *  Tauri 2 serialize'ит `Vec<u8>` как JSON-массив чисел. Для 5-10 sec
 *  slice @ 16kHz mono i16 = 160–320 KB → acceptable JSON overhead для
 *  click-to-play action. Конвертим обратно в Uint8Array на frontend. */
export async function getVoiceSampleAudio(id: string): Promise<Uint8Array> {
  const arr = await invoke<number[]>('get_voice_sample_audio', { id });
  return new Uint8Array(arr);
}
