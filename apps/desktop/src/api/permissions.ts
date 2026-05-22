import { invoke } from '@tauri-apps/api/core';

export type PermissionStatus =
  | 'granted'
  | 'denied'
  | 'not_determined'
  | 'restricted'
  | 'unknown';

export interface PermissionsStatus {
  microphone: PermissionStatus;
  screen_recording: PermissionStatus;
  accessibility: PermissionStatus;
}

export type PermissionTarget = 'microphone' | 'screen_recording' | 'accessibility' | 'all';
export type SystemPane = 'microphone' | 'screen_recording' | 'accessibility';

export function getAudioPermissions(): Promise<PermissionsStatus> {
  return invoke<PermissionsStatus>('get_audio_permissions');
}

export function requestAudioPermissions(
  target: PermissionTarget,
): Promise<PermissionsStatus> {
  return invoke<PermissionsStatus>('request_audio_permissions', { target });
}

export function openSystemPrivacyPane(pane: SystemPane): Promise<void> {
  return invoke<void>('open_system_privacy_pane', { pane });
}
