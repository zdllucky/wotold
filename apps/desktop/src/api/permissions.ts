import { invoke } from '@tauri-apps/api/core';

export type PermissionStatus =
  | 'granted'
  | 'denied'
  | 'not_determined'
  | 'restricted'
  | 'unknown';

// [perm-usage] Accessibility отсюда убран: он мерился в процессе сайдкара, а не
// приложения, поэтому всегда приезжал `denied`, и ни одна фича его не требует —
// ⌘⇧R висит на `keydown` окна, глобального хоткея у нас нет.
export interface PermissionsStatus {
  microphone: PermissionStatus;
  screen_recording: PermissionStatus;
}

export type PermissionTarget = 'microphone' | 'screen_recording' | 'all';
export type SystemPane = 'microphone' | 'screen_recording';

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

/** Сбрасывает TCC-запись приложения — лечит грант, протухший после обновления. */
export function resetPermission(pane: SystemPane): Promise<void> {
  return invoke<void>('reset_permission', { pane });
}
