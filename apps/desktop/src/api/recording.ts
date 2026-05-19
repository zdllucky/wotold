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
  created_at: string;
  updated_at: string;
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
