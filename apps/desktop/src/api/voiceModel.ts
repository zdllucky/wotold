// [B3.7c] Voice embedder model — Tauri commands wrapper.
//
// Управление WeSpeaker ONNX моделью (25 MB) которая включает biometric
// matching. По умолчанию модели нет — pipeline fallback'ит на manual
// confirm. Юзер качает её опт-ин в Settings → Распознавание голоса.
//
// SHA256 захардкожен в backend; mismatch → backend сохраняет partial-файл
// удалённым и Err'ит.

import { invoke } from '@tauri-apps/api/core';

export type VoiceModelStatus =
  | { status: 'missing' }
  | { status: 'valid'; size: number }
  | { status: 'corrupted'; size: number; expected: string; got: string };

export interface VoiceModelInfo {
  url: string;
  sha256: string;
  size_hint: number;
  feature_enabled: boolean;
}

export interface VoiceModelProgress {
  downloaded: number;
  total: number;
  percent: number;
}

export type VoiceModelDoneStatus =
  | { status: 'ok' }
  | { status: 'verify_failed'; expected: string; got: string }
  | { status: 'io_error'; message: string };

export function voiceModelStatus(): Promise<VoiceModelStatus> {
  return invoke<VoiceModelStatus>('voice_model_status');
}

/** Запускает скачивание модели. Прогресс приходит через события
 *  `voice-model:progress` (VoiceModelProgress). Финал — `voice-model:done`
 *  (VoiceModelDoneStatus). Также возвращается Err на network/io fail. */
export function voiceModelDownload(): Promise<void> {
  return invoke<void>('voice_model_download');
}

export function voiceModelDelete(): Promise<void> {
  return invoke<void>('voice_model_delete');
}

export function voiceModelInfo(): Promise<VoiceModelInfo> {
  return invoke<VoiceModelInfo>('voice_model_info');
}
